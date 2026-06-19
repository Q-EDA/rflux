use std::collections::HashMap;

use rflux_ir::{IrError, LogicOp, Netlist, NodeKind, PinRef};

use crate::ast::*;

#[derive(Debug)]
pub enum ElabError {
    Ir(IrError),
    Msg(String),
}

impl From<IrError> for ElabError {
    fn from(e: IrError) -> Self {
        ElabError::Ir(e)
    }
}

impl std::fmt::Display for ElabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElabError::Ir(e) => write!(f, "IR error: {e}"),
            ElabError::Msg(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for ElabError {}

impl ElabError {
    pub fn code(&self) -> &'static str {
        "RFLOW-VERILOG-002"
    }
}

fn gate_name_to_logic_op(name: &str) -> Option<LogicOp> {
    match name.to_lowercase().as_str() {
        "and" => Some(LogicOp::And),
        "or" => Some(LogicOp::Or),
        "not" => Some(LogicOp::Not),
        "buf" => Some(LogicOp::Buf),
        "xor" => Some(LogicOp::Xor),
        "nand" => Some(LogicOp::And),   // AND + NOT
        "nor" => Some(LogicOp::Or),     // OR + NOT
        "xnor" => Some(LogicOp::Xor),   // XOR + NOT
        "mux" => Some(LogicOp::Mux2),
        "dff" => Some(LogicOp::DffEnable),
        _ => None,
    }
}

fn needs_inversion(gate_name: &str) -> bool {
    matches!(
        gate_name.to_lowercase().as_str(),
        "nand" | "nor" | "xnor" | "not"
    )
}

pub fn elaborate_to_ir(
    source: &VerilogSource,
    top_module: &str,
) -> Result<Netlist, ElabError> {
    let module = source
        .modules
        .iter()
        .find(|m| m.name == top_module)
        .ok_or_else(|| ElabError::Msg(format!("module '{top_module}' not found")))?;

    let mut netlist = Netlist::new();
    let mut wire_map: HashMap<String, Vec<(rflux_ir::NodeId, u16)>> = HashMap::new();

    // First pass: create port nodes
    for port_decl in &module.ports {
        let node_id = netlist.add_node(NodeKind::Port, port_decl.name.clone());
        // Port node has a single pin (port 0)
        wire_map
            .entry(port_decl.name.clone())
            .or_default()
            .push((node_id, 0));
    }

    // Second pass: process module items
    for item in &module.items {
        match item {
            ModuleItem::Net(net) => {
                // Register wire name (no node created, just track the name)
                wire_map
                    .entry(net.name.clone())
                    .or_default();
            }
            ModuleItem::Instance(inst) => {
                elaborate_instance(&mut netlist, inst, &mut wire_map)?;
            }
            ModuleItem::Assign(assign) => {
                elaborate_assign(&mut netlist, assign, &mut wire_map)?;
            }
            ModuleItem::AlwaysBlock(always) => {
                elaborate_always(&mut netlist, always, &mut wire_map, &module.name)?;
            }
            ModuleItem::Parameter(_) => {
                // Parameters don't generate nodes
            }
        }
    }

    Ok(netlist)
}

fn elaborate_instance(
    netlist: &mut Netlist,
    inst: &InstanceDecl,
    wire_map: &mut HashMap<String, Vec<(rflux_ir::NodeId, u16)>>,
) -> Result<(), ElabError> {
    let gate_name = &inst.module_name;
    let logic_op = gate_name_to_logic_op(gate_name);
    let invert = needs_inversion(gate_name);

    // Create the gate node
    let node_id = if let Some(op) = logic_op {
        netlist.add_node_with_logic(NodeKind::CellInstance, inst.name.clone(), Some(op))
    } else {
        // Unknown gate type - treat as a generic cell
        netlist.add_node(NodeKind::CellInstance, inst.name.clone())
    };

    // For gates with inversion (nand, nor, xnor, not), we need an extra inverter
    let inv_node_id = if invert && gate_name.to_lowercase() != "not" {
        // Add an inverter after the base gate
        let inv_id = netlist.add_node_with_logic(
            NodeKind::CellInstance,
            format!("{}_inv", inst.name),
            Some(LogicOp::Not),
        );
        Some(inv_id)
    } else {
        None
    };

    // Connect signals
    // Standard gate convention: first connection(s) are inputs, last is output
    // For gates: and/or/xor g(out, in1, in2)
    // The first connection is the output, rest are inputs
    let is_gate = gate_name_to_logic_op(gate_name).is_some();

    if is_gate {
        if inst.connections.is_empty() {
            return Err(ElabError::Msg(format!(
                "gate instance '{}' has no connections",
                inst.name
            )));
        }

        // First connection is output
        let output_conn = &inst.connections[0];
        let output_pin = if let Some(inv_id) = inv_node_id {
            // Output comes from the inverter
            PinRef {
                node: inv_id,
                port: 0,
            }
        } else {
            PinRef {
                node: node_id,
                port: 0,
            }
        };

        wire_map
            .entry(output_conn.signal.clone())
            .or_default()
            .push((output_pin.node, output_pin.port));

        // Remaining connections are inputs
        for (i, conn) in inst.connections[1..].iter().enumerate() {
            let input_pin = PinRef {
                node: node_id,
                port: i as u16,
            };

            // Check if we need to connect from an existing wire
            if let Some(sources) = wire_map.get(&conn.signal) {
                if let Some(&(src_node, src_port)) = sources.first() {
                    let from = PinRef {
                        node: src_node,
                        port: src_port,
                    };
                    netlist.connect(from, input_pin)?;
                }
            }
        }

        // If we have an inverter, connect gate output to inverter input
        if let Some(inv_id) = inv_node_id {
            let from = PinRef {
                node: node_id,
                port: 0,
            };
            let to = PinRef {
                node: inv_id,
                port: 0,
            };
            netlist.connect(from, to)?;
        }
    } else {
        // Module instance - treat as a black box
        // For now, just register the connections in wire_map
        for conn in &inst.connections {
            wire_map
                .entry(conn.signal.clone())
                .or_default()
                .push((node_id, 0));
        }
    }

    Ok(())
}

fn elaborate_assign(
    netlist: &mut Netlist,
    assign: &Assignment,
    wire_map: &mut HashMap<String, Vec<(rflux_ir::NodeId, u16)>>,
) -> Result<(), ElabError> {
    // Create a synthetic gate for the assign expression
    let (node_id, output_port) =
        elaborate_expr(netlist, &assign.expr, wire_map, &assign.target)?;

    // Register the output wire
    wire_map
        .entry(assign.target.clone())
        .or_default()
        .push((node_id, output_port));

    Ok(())
}

fn elaborate_always(
    netlist: &mut Netlist,
    always: &AlwaysBlock,
    wire_map: &mut HashMap<String, Vec<(rflux_ir::NodeId, u16)>>,
    prefix: &str,
) -> Result<(), ElabError> {
    let is_sequential = always
        .sensitivity
        .items
        .iter()
        .any(|item| matches!(item, SensitivityItem::Posedge(_) | SensitivityItem::Negedge(_)));

    if is_sequential {
        elaborate_sequential_statement(netlist, &always.body, wire_map, prefix, &always.sensitivity)
    } else {
        elaborate_combinational_statement(netlist, &always.body, wire_map, prefix)
    }
}

fn elaborate_sequential_statement(
    netlist: &mut Netlist,
    stmt: &Statement,
    wire_map: &mut HashMap<String, Vec<(rflux_ir::NodeId, u16)>>,
    prefix: &str,
    sensitivity: &SensitivityList,
) -> Result<(), ElabError> {
    match stmt {
        Statement::Block(stmts) => {
            for (i, s) in stmts.iter().enumerate() {
                elaborate_sequential_statement(
                    netlist,
                    s,
                    wire_map,
                    &format!("{prefix}_seq_{i}"),
                    sensitivity,
                )?;
            }
            Ok(())
        }
        Statement::If {
            condition,
            then_body,
            else_body,
        } => {
            let target = extract_assign_target(then_body)
                .or_else(|| else_body.as_ref().and_then(|e| extract_assign_target(e)))
                .ok_or_else(|| ElabError::Msg("cannot determine target wire".to_string()))?;

            let (sel_node, sel_port) =
                elaborate_expr(netlist, condition, wire_map, &format!("{prefix}_sel"))?;

            if let Some(else_body) = else_body {
                let (then_node, then_port) = elaborate_expr(
                    netlist,
                    extract_assign_value(then_body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in then branch".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_then"),
                )?;
                let (else_node, else_port) = elaborate_expr(
                    netlist,
                    extract_assign_value(else_body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in else branch".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_else"),
                )?;

                let dff_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    format!("{prefix}_dff"),
                    Some(LogicOp::DffEnable),
                );

                // DffEnable: port 0 = clock, port 1 = enable (sel), port 2 = data (mux output)
                // We need a MUX to select between then/else based on condition
                let mux_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    format!("{prefix}_mux"),
                    Some(LogicOp::Mux2),
                );

                netlist.connect(
                    PinRef {
                        node: sel_node,
                        port: sel_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 0,
                    },
                )?;
                netlist.connect(
                    PinRef {
                        node: then_node,
                        port: then_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 1,
                    },
                )?;
                netlist.connect(
                    PinRef {
                        node: else_node,
                        port: else_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 2,
                    },
                )?;

                // Connect MUX output to DFF data input
                netlist.connect(
                    PinRef {
                        node: mux_node,
                        port: 0,
                    },
                    PinRef {
                        node: dff_node,
                        port: 2,
                    },
                )?;

                // Connect clock
                for item in &sensitivity.items {
                    if let SensitivityItem::Posedge(clk_name) | SensitivityItem::Negedge(clk_name) =
                        item
                    {
                        if let Some(sources) = wire_map.get(clk_name) {
                            if let Some(&(clk_node, clk_port)) = sources.first() {
                                netlist.connect(
                                    PinRef {
                                        node: clk_node,
                                        port: clk_port,
                                    },
                                    PinRef {
                                        node: dff_node,
                                        port: 0,
                                    },
                                )?;
                            }
                        }
                    }
                }

                // Enable = 1 (always enabled for if-else in sequential)
                let enable_node = netlist.add_node(
                    NodeKind::CellInstance,
                    format!("{prefix}_enable_const"),
                );
                netlist.connect(
                    PinRef {
                        node: enable_node,
                        port: 0,
                    },
                    PinRef {
                        node: dff_node,
                        port: 1,
                    },
                )?;

                wire_map
                    .entry(target)
                    .or_default()
                    .push((dff_node, 0));
            } else {
                // if (cond) q <= d; (no else) → DffEnable
                let data_node;
                let data_port;
                if let Some(val) = extract_assign_value(then_body) {
                    let (n, p) =
                        elaborate_expr(netlist, val, wire_map, &format!("{prefix}_data"))?;
                    data_node = n;
                    data_port = p;
                } else {
                    return Err(ElabError::Msg(
                        "expected assignment in then branch".to_string(),
                    ));
                }

                let dff_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    format!("{prefix}_dff"),
                    Some(LogicOp::DffEnable),
                );

                // Connect clock
                for item in &sensitivity.items {
                    if let SensitivityItem::Posedge(clk_name) | SensitivityItem::Negedge(clk_name) =
                        item
                    {
                        if let Some(sources) = wire_map.get(clk_name) {
                            if let Some(&(clk_node, clk_port)) = sources.first() {
                                netlist.connect(
                                    PinRef {
                                        node: clk_node,
                                        port: clk_port,
                                    },
                                    PinRef {
                                        node: dff_node,
                                        port: 0,
                                    },
                                )?;
                            }
                        }
                    }
                }

                // Enable = condition
                netlist.connect(
                    PinRef {
                        node: sel_node,
                        port: sel_port,
                    },
                    PinRef {
                        node: dff_node,
                        port: 1,
                    },
                )?;

                // Data
                netlist.connect(
                    PinRef {
                        node: data_node,
                        port: data_port,
                    },
                    PinRef {
                        node: dff_node,
                        port: 2,
                    },
                )?;

                wire_map
                    .entry(target)
                    .or_default()
                    .push((dff_node, 0));
            }

            Ok(())
        }
        Statement::NonBlockingAssign { target, value } => {
            let (data_node, data_port) =
                elaborate_expr(netlist, value, wire_map, &format!("{prefix}_data"))?;

            let dff_node = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{prefix}_dff"),
                Some(LogicOp::DffEnable),
            );

            // Connect clock
            for item in &sensitivity.items {
                if let SensitivityItem::Posedge(clk_name) | SensitivityItem::Negedge(clk_name) = item
                {
                    if let Some(sources) = wire_map.get(clk_name) {
                        if let Some(&(clk_node, clk_port)) = sources.first() {
                            netlist.connect(
                                PinRef {
                                    node: clk_node,
                                    port: clk_port,
                                },
                                PinRef {
                                    node: dff_node,
                                    port: 0,
                                },
                            )?;
                        }
                    }
                }
            }

            // Enable = always enabled
            let enable_node = netlist.add_node(
                NodeKind::CellInstance,
                format!("{prefix}_enable_const"),
            );
            netlist.connect(
                PinRef {
                    node: enable_node,
                    port: 0,
                },
                PinRef {
                    node: dff_node,
                    port: 1,
                },
            )?;

            // Data
            netlist.connect(
                PinRef {
                    node: data_node,
                    port: data_port,
                },
                PinRef {
                    node: dff_node,
                    port: 2,
                },
            )?;

            wire_map
                .entry(target.clone())
                .or_default()
                .push((dff_node, 0));

            Ok(())
        }
        Statement::BlockingAssign { target, value } => {
            let (node_id, output_port) =
                elaborate_expr(netlist, value, wire_map, &format!("{prefix}_assign"))?;
            wire_map
                .entry(target.clone())
                .or_default()
                .push((node_id, output_port));
            Ok(())
        }
        Statement::Null => Ok(()),
        Statement::Case {
            expr,
            items,
            default,
        } => {
            let target = items
                .first()
                .and_then(|item| extract_assign_target_from_body(&item.body))
                .or_else(|| {
                    default
                        .as_ref()
                        .and_then(|d| extract_assign_target(d))
                })
                .ok_or_else(|| ElabError::Msg("cannot determine target wire".to_string()))?;

            let (sel_node, sel_port) =
                elaborate_expr(netlist, expr, wire_map, &format!("{prefix}_sel"))?;

            // Build a chain of MUXes for the case items
            let mut current_data_node;
            let mut current_data_port;

            // Start with default (or last item)
            if let Some(default_body) = default {
                let (n, p) = elaborate_expr(
                    netlist,
                    extract_assign_value(default_body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in default".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_default"),
                )?;
                current_data_node = n;
                current_data_port = p;
            } else {
                // Use last item as default
                let last = items.last().ok_or_else(|| {
                    ElabError::Msg("case has no items and no default".to_string())
                })?;
                let (n, p) = elaborate_expr(
                    netlist,
                    extract_assign_value_from_body(&last.body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in case item".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_case_last"),
                )?;
                current_data_node = n;
                current_data_port = p;
            }

            // Build MUX chain from back to front
            for item in items.iter().rev() {
                let (val_node, val_port) = elaborate_expr(
                    netlist,
                    extract_assign_value_from_body(&item.body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in case item".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_case_val"),
                )?;

                // For each pattern, create an equality check and MUX
                for pattern in &item.patterns {
                    let (pat_node, pat_port) = elaborate_expr(
                        netlist,
                        pattern,
                        wire_map,
                        &format!("{prefix}_case_pat"),
                    )?;

                    // Equality check: XOR + NOT
                    let xor_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_case_eq_xor"),
                        Some(LogicOp::Xor),
                    );
                    netlist.connect(
                        PinRef {
                            node: sel_node,
                            port: sel_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: pat_node,
                            port: pat_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 1,
                        },
                    )?;

                    let not_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_case_eq_not"),
                        Some(LogicOp::Not),
                    );
                    netlist.connect(
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )?;

                    // MUX: if matches, use val; else use current_data
                    let mux_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_case_mux"),
                        Some(LogicOp::Mux2),
                    );
                    netlist.connect(
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                        PinRef {
                            node: mux_node,
                            port: 0,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: val_node,
                            port: val_port,
                        },
                        PinRef {
                            node: mux_node,
                            port: 1,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: current_data_node,
                            port: current_data_port,
                        },
                        PinRef {
                            node: mux_node,
                            port: 2,
                        },
                    )?;

                    current_data_node = mux_node;
                    current_data_port = 0;
                }
            }

            wire_map
                .entry(target)
                .or_default()
                .push((current_data_node, current_data_port));

            Ok(())
        }
    }
}

fn elaborate_combinational_statement(
    netlist: &mut Netlist,
    stmt: &Statement,
    wire_map: &mut HashMap<String, Vec<(rflux_ir::NodeId, u16)>>,
    prefix: &str,
) -> Result<(), ElabError> {
    match stmt {
        Statement::Block(stmts) => {
            for (i, s) in stmts.iter().enumerate() {
                elaborate_combinational_statement(
                    netlist,
                    s,
                    wire_map,
                    &format!("{prefix}_stmt_{i}"),
                )?;
            }
            Ok(())
        }
        Statement::If {
            condition,
            then_body,
            else_body,
        } => {
            let target = extract_assign_target(then_body)
                .or_else(|| else_body.as_ref().and_then(|e| extract_assign_target(e)))
                .ok_or_else(|| ElabError::Msg("cannot determine target wire".to_string()))?;

            let (sel_node, sel_port) =
                elaborate_expr(netlist, condition, wire_map, &format!("{prefix}_sel"))?;

            if let Some(else_body) = else_body {
                let (then_node, then_port) = elaborate_expr(
                    netlist,
                    extract_assign_value(then_body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in then branch".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_then"),
                )?;
                let (else_node, else_port) = elaborate_expr(
                    netlist,
                    extract_assign_value(else_body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in else branch".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_else"),
                )?;

                let mux_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    format!("{prefix}_mux"),
                    Some(LogicOp::Mux2),
                );

                netlist.connect(
                    PinRef {
                        node: sel_node,
                        port: sel_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 0,
                    },
                )?;
                netlist.connect(
                    PinRef {
                        node: then_node,
                        port: then_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 1,
                    },
                )?;
                netlist.connect(
                    PinRef {
                        node: else_node,
                        port: else_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 2,
                    },
                )?;

                wire_map
                    .entry(target)
                    .or_default()
                    .push((mux_node, 0));
            } else {
                // if (cond) y = val; → MUX(cond, val, y_old) or just assign
                // For simplicity, create a MUX with current value as default
                let (then_node, then_port) = elaborate_expr(
                    netlist,
                    extract_assign_value(then_body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in then branch".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_then"),
                )?;

                // Use existing wire value as default (if available)
                let (else_node, else_port) =
                    if let Some(sources) = wire_map.get(&target) {
                        if let Some(&(n, p)) = sources.first() {
                            (n, p)
                        } else {
                            (then_node, then_port) // fallback
                        }
                    } else {
                        (then_node, then_port) // fallback
                    };

                let mux_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    format!("{prefix}_mux"),
                    Some(LogicOp::Mux2),
                );

                netlist.connect(
                    PinRef {
                        node: sel_node,
                        port: sel_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 0,
                    },
                )?;
                netlist.connect(
                    PinRef {
                        node: then_node,
                        port: then_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 1,
                    },
                )?;
                netlist.connect(
                    PinRef {
                        node: else_node,
                        port: else_port,
                    },
                    PinRef {
                        node: mux_node,
                        port: 2,
                    },
                )?;

                wire_map
                    .entry(target)
                    .or_default()
                    .push((mux_node, 0));
            }

            Ok(())
        }
        Statement::BlockingAssign { target, value } => {
            let (node_id, output_port) =
                elaborate_expr(netlist, value, wire_map, &format!("{prefix}_assign"))?;
            wire_map
                .entry(target.clone())
                .or_default()
                .push((node_id, output_port));
            Ok(())
        }
        Statement::NonBlockingAssign { target, value } => {
            let (node_id, output_port) =
                elaborate_expr(netlist, value, wire_map, &format!("{prefix}_assign"))?;
            wire_map
                .entry(target.clone())
                .or_default()
                .push((node_id, output_port));
            Ok(())
        }
        Statement::Case {
            expr,
            items,
            default,
        } => {
            let target = items
                .first()
                .and_then(|item| extract_assign_target_from_body(&item.body))
                .or_else(|| {
                    default
                        .as_ref()
                        .and_then(|d| extract_assign_target(d))
                })
                .ok_or_else(|| ElabError::Msg("cannot determine target wire".to_string()))?;

            let (sel_node, sel_port) =
                elaborate_expr(netlist, expr, wire_map, &format!("{prefix}_sel"))?;

            let mut current_data_node;
            let mut current_data_port;

            if let Some(default_body) = default {
                let (n, p) = elaborate_expr(
                    netlist,
                    extract_assign_value(default_body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in default".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_default"),
                )?;
                current_data_node = n;
                current_data_port = p;
            } else {
                let last = items.last().ok_or_else(|| {
                    ElabError::Msg("case has no items and no default".to_string())
                })?;
                let (n, p) = elaborate_expr(
                    netlist,
                    extract_assign_value_from_body(&last.body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in case item".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_case_last"),
                )?;
                current_data_node = n;
                current_data_port = p;
            }

            for item in items.iter().rev() {
                let (val_node, val_port) = elaborate_expr(
                    netlist,
                    extract_assign_value_from_body(&item.body).ok_or_else(|| {
                        ElabError::Msg("expected assignment in case item".to_string())
                    })?,
                    wire_map,
                    &format!("{prefix}_case_val"),
                )?;

                for pattern in &item.patterns {
                    let (pat_node, pat_port) = elaborate_expr(
                        netlist,
                        pattern,
                        wire_map,
                        &format!("{prefix}_case_pat"),
                    )?;

                    let xor_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_case_eq_xor"),
                        Some(LogicOp::Xor),
                    );
                    netlist.connect(
                        PinRef {
                            node: sel_node,
                            port: sel_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: pat_node,
                            port: pat_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 1,
                        },
                    )?;

                    let not_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_case_eq_not"),
                        Some(LogicOp::Not),
                    );
                    netlist.connect(
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )?;

                    let mux_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_case_mux"),
                        Some(LogicOp::Mux2),
                    );
                    netlist.connect(
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                        PinRef {
                            node: mux_node,
                            port: 0,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: val_node,
                            port: val_port,
                        },
                        PinRef {
                            node: mux_node,
                            port: 1,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: current_data_node,
                            port: current_data_port,
                        },
                        PinRef {
                            node: mux_node,
                            port: 2,
                        },
                    )?;

                    current_data_node = mux_node;
                    current_data_port = 0;
                }
            }

            wire_map
                .entry(target)
                .or_default()
                .push((current_data_node, current_data_port));

            Ok(())
        }
        Statement::Null => Ok(()),
    }
}

fn extract_assign_target(stmt: &Statement) -> Option<String> {
    match stmt {
        Statement::BlockingAssign { target, .. }
        | Statement::NonBlockingAssign { target, .. } => Some(target.clone()),
        Statement::Block(stmts) => stmts.first().and_then(extract_assign_target),
        _ => None,
    }
}

fn extract_assign_value(stmt: &Statement) -> Option<&Expr> {
    match stmt {
        Statement::BlockingAssign { value, .. }
        | Statement::NonBlockingAssign { value, .. } => Some(value),
        Statement::Block(stmts) => stmts.first().and_then(extract_assign_value),
        _ => None,
    }
}

fn extract_assign_target_from_body(stmt: &Statement) -> Option<String> {
    extract_assign_target(stmt)
}

fn extract_assign_value_from_body(stmt: &Statement) -> Option<&Expr> {
    extract_assign_value(stmt)
}

fn elaborate_expr(
    netlist: &mut Netlist,
    expr: &Expr,
    wire_map: &mut HashMap<String, Vec<(rflux_ir::NodeId, u16)>>,
    prefix: &str,
) -> Result<(rflux_ir::NodeId, u16), ElabError> {
    match expr {
        Expr::Ident(name) => {
            if let Some(sources) = wire_map.get(name) {
                if let Some(&(node_id, port)) = sources.first() {
                    return Ok((node_id, port));
                }
            }
            Err(ElabError::Msg(format!("wire '{name}' not found")))
        }
        Expr::Literal(val) => {
            let node_id =
                netlist.add_node(NodeKind::CellInstance, format!("{prefix}_const_{val}"));
            Ok((node_id, 0))
        }
        Expr::BinOp(op, left, right) => {
            let (left_node, left_port) =
                elaborate_expr(netlist, left, wire_map, &format!("{prefix}_l"))?;
            let (right_node, right_port) =
                elaborate_expr(netlist, right, wire_map, &format!("{prefix}_r"))?;

            let logic_op = match op {
                BinOp::And | BinOp::BitAnd | BinOp::LogicalAnd => LogicOp::And,
                BinOp::Or | BinOp::BitOr | BinOp::LogicalOr => LogicOp::Or,
                BinOp::Xor | BinOp::BitXor => LogicOp::Xor,
                BinOp::Eq => {
                    // a == b → XOR(a, b) + NOT
                    let xor_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_eq_xor"),
                        Some(LogicOp::Xor),
                    );
                    netlist.connect(
                        PinRef {
                            node: left_node,
                            port: left_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: right_node,
                            port: right_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 1,
                        },
                    )?;

                    let not_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_eq_not"),
                        Some(LogicOp::Not),
                    );
                    netlist.connect(
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )?;
                    return Ok((not_node, 0));
                }
                BinOp::Neq => {
                    // a != b → XOR(a, b)
                    let xor_node = netlist.add_node_with_logic(
                        NodeKind::CellInstance,
                        format!("{prefix}_neq"),
                        Some(LogicOp::Xor),
                    );
                    netlist.connect(
                        PinRef {
                            node: left_node,
                            port: left_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: right_node,
                            port: right_port,
                        },
                        PinRef {
                            node: xor_node,
                            port: 1,
                        },
                    )?;
                    return Ok((xor_node, 0));
                }
                BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Mod
                | BinOp::Shl
                | BinOp::Shr => {
                    let node_id = netlist.add_node(
                        NodeKind::CellInstance,
                        format!("{prefix}_op"),
                    );
                    netlist.connect(
                        PinRef {
                            node: left_node,
                            port: left_port,
                        },
                        PinRef {
                            node: node_id,
                            port: 0,
                        },
                    )?;
                    netlist.connect(
                        PinRef {
                            node: right_node,
                            port: right_port,
                        },
                        PinRef {
                            node: node_id,
                            port: 1,
                        },
                    )?;
                    return Ok((node_id, 0));
                }
            };

            let node_id = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{prefix}_binop"),
                Some(logic_op),
            );

            netlist.connect(
                PinRef {
                    node: left_node,
                    port: left_port,
                },
                PinRef {
                    node: node_id,
                    port: 0,
                },
            )?;
            netlist.connect(
                PinRef {
                    node: right_node,
                    port: right_port,
                },
                PinRef {
                    node: node_id,
                    port: 1,
                },
            )?;

            Ok((node_id, 0))
        }
        Expr::UnaryOp(op, inner) => {
            let (inner_node, inner_port) =
                elaborate_expr(netlist, inner, wire_map, &format!("{prefix}_inner"))?;

            let logic_op = match op {
                UnaryOp::Not => LogicOp::Not,
                UnaryOp::Negate | UnaryOp::LogicalNot => LogicOp::Not,
            };

            let node_id = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{prefix}_unary"),
                Some(logic_op),
            );

            netlist.connect(
                PinRef {
                    node: inner_node,
                    port: inner_port,
                },
                PinRef {
                    node: node_id,
                    port: 0,
                },
            )?;

            Ok((node_id, 0))
        }
        Expr::Ternary(cond, then_expr, else_expr) => {
            let (sel_node, sel_port) =
                elaborate_expr(netlist, cond, wire_map, &format!("{prefix}_sel"))?;
            let (then_node, then_port) =
                elaborate_expr(netlist, then_expr, wire_map, &format!("{prefix}_then"))?;
            let (else_node, else_port) =
                elaborate_expr(netlist, else_expr, wire_map, &format!("{prefix}_else"))?;

            let mux_node = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{prefix}_ternary_mux"),
                Some(LogicOp::Mux2),
            );

            netlist.connect(
                PinRef {
                    node: sel_node,
                    port: sel_port,
                },
                PinRef {
                    node: mux_node,
                    port: 0,
                },
            )?;
            netlist.connect(
                PinRef {
                    node: then_node,
                    port: then_port,
                },
                PinRef {
                    node: mux_node,
                    port: 1,
                },
            )?;
            netlist.connect(
                PinRef {
                    node: else_node,
                    port: else_port,
                },
                PinRef {
                    node: mux_node,
                    port: 2,
                },
            )?;

            Ok((mux_node, 0))
        }
        Expr::Concat(_exprs) => {
            let node_id = netlist.add_node(NodeKind::CellInstance, format!("{prefix}_concat"));
            Ok((node_id, 0))
        }
        Expr::BitSelect(inner, _high, _low) => {
            let (inner_node, inner_port) =
                elaborate_expr(netlist, inner, wire_map, &format!("{prefix}_bitsel"))?;
            let node_id = netlist.add_node(NodeKind::CellInstance, format!("{prefix}_bitsel"));
            netlist.connect(
                PinRef {
                    node: inner_node,
                    port: inner_port,
                },
                PinRef {
                    node: node_id,
                    port: 0,
                },
            )?;
            Ok((node_id, 0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_verilog;

    #[test]
    fn elab_error_codes_are_stable() {
        let err = ElabError::Msg("test".to_string());
        assert_eq!(err.code(), "RFLOW-VERILOG-002");
    }

    #[test]
    fn elaborate_simple_and_gate() {
        let input = r#"
module and_gate(a, b, y);
  input a;
  input b;
  output y;
  assign y = a & b;
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "and_gate").unwrap();

        // Should have at least: 3 ports + 1 AND gate = 4 nodes
        assert!(netlist.node_count() >= 4);
        // Should have edges
        assert!(netlist.edge_count() >= 2); // a->and, b->and
    }

    #[test]
    fn elaborate_assign_expression() {
        let input = r#"
module test(a, b, c, y);
  input a, b, c;
  output y;
  assign y = (a & b) | c;
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "test").unwrap();

        // Should have: 4 ports + AND gate + OR gate = 6 nodes
        assert!(netlist.node_count() >= 6);
    }

    #[test]
    fn elaborate_nand_gate() {
        let input = r#"
module nand_gate(a, b, y);
  input a;
  input b;
  output y;
  nand g1(y, a, b);
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "nand_gate").unwrap();

        // NAND = AND + NOT, so should have: 3 ports + AND + NOT = 5 nodes
        assert!(netlist.node_count() >= 5);
    }

    #[test]
    fn elaborate_always_to_mux() {
        let input = r#"
module my_mux(a, b, sel, y);
  input a, b, sel;
  output y;
  always @(*) begin
    if (sel)
      y = b;
    else
      y = a;
  end
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "my_mux").unwrap();

        // Should have: 4 ports + MUX = 5 nodes
        assert!(netlist.node_count() >= 5);
        // Check that a Mux2 node exists
        let has_mux = netlist.nodes().iter().any(|n| n.logic_op == Some(LogicOp::Mux2));
        assert!(has_mux, "expected MUX node");
    }

    #[test]
    fn elaborate_always_to_dff() {
        let input = r#"
module my_dff(clk, d, q);
  input clk, d;
  output q;
  always @(posedge clk) begin
    q <= d;
  end
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "my_dff").unwrap();

        // Should have: 3 ports + DFF + enable const = 5 nodes
        assert!(netlist.node_count() >= 5);
        // Check that a DffEnable node exists
        let has_dff = netlist
            .nodes()
            .iter()
            .any(|n| n.logic_op == Some(LogicOp::DffEnable));
        assert!(has_dff, "expected DFF node");
    }

    #[test]
    fn elaborate_always_dff_enable() {
        let input = r#"
module dff_en(clk, en, d, q);
  input clk, en, d;
  output q;
  always @(posedge clk) begin
    if (en)
      q <= d;
  end
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "dff_en").unwrap();

        // Should have: 4 ports + DFF = 5 nodes
        assert!(netlist.node_count() >= 5);
        let has_dff = netlist
            .nodes()
            .iter()
            .any(|n| n.logic_op == Some(LogicOp::DffEnable));
        assert!(has_dff, "expected DFF node");
    }

    #[test]
    fn elaborate_ternary_to_mux() {
        let input = r#"
module mux_ternary(a, b, sel, y);
  input a, b, sel;
  output y;
  assign y = sel ? b : a;
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "mux_ternary").unwrap();

        let has_mux = netlist.nodes().iter().any(|n| n.logic_op == Some(LogicOp::Mux2));
        assert!(has_mux, "expected MUX node from ternary");
    }

    #[test]
    fn elaborate_equality_to_xnor() {
        let input = r#"
module eq_test(a, b, y);
  input a, b;
  output y;
  assign y = (a == b);
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let netlist = elaborate_to_ir(&source, "eq_test").unwrap();

        // a == b → XOR + NOT = 2 extra nodes + 2 ports
        assert!(netlist.node_count() >= 4);
        let has_xor = netlist.nodes().iter().any(|n| n.logic_op == Some(LogicOp::Xor));
        let has_not = netlist.nodes().iter().any(|n| n.logic_op == Some(LogicOp::Not));
        assert!(has_xor, "expected XOR node for equality");
        assert!(has_not, "expected NOT node for equality");
    }
}
