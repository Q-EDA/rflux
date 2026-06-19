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

fn elaborate_expr(
    netlist: &mut Netlist,
    expr: &Expr,
    wire_map: &mut HashMap<String, Vec<(rflux_ir::NodeId, u16)>>,
    prefix: &str,
) -> Result<(rflux_ir::NodeId, u16), ElabError> {
    match expr {
        Expr::Ident(name) => {
            // Look up the wire and return its source
            if let Some(sources) = wire_map.get(name) {
                if let Some(&(node_id, port)) = sources.first() {
                    return Ok((node_id, port));
                }
            }
            Err(ElabError::Msg(format!("wire '{name}' not found")))
        }
        Expr::Literal(val) => {
            // Create a constant node
            let node_id = netlist.add_node(NodeKind::CellInstance, format!("{prefix}_const_{val}"));
            Ok((node_id, 0))
        }
        Expr::BinOp(op, left, right) => {
            let (left_node, left_port) =
                elaborate_expr(netlist, left, wire_map, &format!("{prefix}_l"))?;
            let (right_node, right_port) =
                elaborate_expr(netlist, right, wire_map, &format!("{prefix}_r"))?;

            let logic_op = match op {
                BinOp::And => LogicOp::And,
                BinOp::Or => LogicOp::Or,
                BinOp::Xor => LogicOp::Xor,
            };

            let node_id = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{prefix}_binop"),
                Some(logic_op),
            );

            // Connect left and right inputs
            let left_from = PinRef {
                node: left_node,
                port: left_port,
            };
            let right_from = PinRef {
                node: right_node,
                port: right_port,
            };
            let left_to = PinRef {
                node: node_id,
                port: 0,
            };
            let right_to = PinRef {
                node: node_id,
                port: 1,
            };

            netlist.connect(left_from, left_to)?;
            netlist.connect(right_from, right_to)?;

            Ok((node_id, 0))
        }
        Expr::UnaryOp(UnaryOp::Not, inner) => {
            let (inner_node, inner_port) =
                elaborate_expr(netlist, inner, wire_map, &format!("{prefix}_not"))?;

            let node_id = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{prefix}_not_gate"),
                Some(LogicOp::Not),
            );

            let from = PinRef {
                node: inner_node,
                port: inner_port,
            };
            let to = PinRef {
                node: node_id,
                port: 0,
            };
            netlist.connect(from, to)?;

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
}
