pub mod gds;
pub mod layout_svg;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::path::Path;

use libreda_db::chip::Chip;
use libreda_db::prelude::HierarchyBase;
use libreda_lefdef::def_ast::DEF;
use libreda_lefdef::def_parser::read_def_bytes;
use libreda_lefdef::def_writer::write_def;
use libreda_lefdef::export::export_db_to_def;
use libreda_lefdef::export::DEFExportOptions;
use libreda_lefdef::import::import_def_into_db;
use libreda_lefdef::import::lef_to_db;
use libreda_lefdef::import::DEFImportOptions;
use libreda_lefdef::lef_ast::LEF;
use libreda_lefdef::lef_parser::read_lef_bytes;
use rflux_ir::{LogicOp, Netlist, NodeKind, PinRef};
use rflux_tech::Pdk;
use serde_json::{json, Value};
use thiserror::Error;

const IR_JSON_SCHEMA_VERSION: u64 = 1;
const PDK_JSON_SCHEMA_VERSION: u64 = 1;
const IR_JSON_KIND: &str = "rflux_ir_netlist";
const PDK_JSON_KIND: &str = "rflux_pdk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Supported input formats for netlist parsing.
pub enum NetlistInputFormat {
    IrJson,
    Bench,
    Verilog,
    Spice,
    Blif,
    Edif,
}

#[derive(Debug, Error)]
/// Errors that can occur during file I/O, parsing, and schema validation.
///
/// Each variant maps to a structured error code (e.g. RFLOW-SCHEMA-001).
pub enum IoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json parse error: {0}")]
    Json(String),
    #[error("lef/def error: {0}")]
    LefDef(String),
    #[error("top cell not found")]
    TopCellNotFound,
    #[error("bench parse error: {0}")]
    BenchParse(String),
    #[error("spice parse error: {0}")]
    SpiceParse(String),
    #[error("blif parse error: {0}")]
    BlifParse(String),
    #[error("edif parse error: {0}")]
    EdifParse(String),
    #[error("lef writing is not supported by libreda-lefdef")]
    LefWriteUnsupported,
    #[error("unsupported {kind} schema version {version}")]
    UnsupportedSchemaVersion { kind: &'static str, version: u64 },
    #[error("invalid {kind} JSON envelope: {detail}")]
    InvalidJsonEnvelope {
        kind: &'static str,
        detail: &'static str,
    },
    #[error("expected {expected_kind} JSON envelope, found {found_kind}")]
    UnexpectedJsonKind {
        expected_kind: &'static str,
        found_kind: String,
    },
}

#[derive(Debug, Clone)]
struct BenchNamedSignal {
    name: String,
    line: usize,
}

fn format_location_detail(
    detail: impl Into<String>,
    line: Option<usize>,
    column: Option<usize>,
) -> String {
    let detail = detail.into();
    match (line, column) {
        (Some(line), Some(column)) if line > 0 && column > 0 => {
            format!("at line {line}, column {column}: {detail}")
        }
        (Some(line), _) if line > 0 => format!("at line {line}: {detail}"),
        _ => detail,
    }
}

fn json_error(error: serde_json::Error) -> IoError {
    IoError::Json(format_location_detail(
        error.to_string(),
        Some(error.line()),
        Some(error.column()),
    ))
}

fn bench_parse_error(detail: impl Into<String>) -> IoError {
    IoError::BenchParse(detail.into())
}

fn bench_parse_error_at_line(line: usize, detail: impl Into<String>) -> IoError {
    IoError::BenchParse(format_location_detail(detail, Some(line), None))
}

impl IoError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            IoError::Io(_) => "RFLOW-INPUT-001",
            IoError::Json(_) => "RFLOW-INPUT-002",
            IoError::LefDef(_) => "RFLOW-INPUT-002",
            IoError::TopCellNotFound => "RFLOW-INPUT-002",
            IoError::BenchParse(_) => "RFLOW-INPUT-002",
            IoError::SpiceParse(_) => "RFLOW-INPUT-002",
            IoError::BlifParse(_) => "RFLOW-INPUT-002",
            IoError::EdifParse(_) => "RFLOW-INPUT-002",
            IoError::LefWriteUnsupported => "RFLOW-LIMIT-001",
            IoError::UnsupportedSchemaVersion { .. } => "RFLOW-SCHEMA-001",
            IoError::InvalidJsonEnvelope { .. } => "RFLOW-SCHEMA-002",
            IoError::UnexpectedJsonKind { .. } => "RFLOW-SCHEMA-003",
        }
    }

    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            IoError::Io(_) => "Check that the input path exists and is readable, then retry.",
            IoError::Json(_) => "Validate the JSON syntax and field types against the current file contract.",
            IoError::LefDef(_) => "Validate the LEF/DEF syntax and ensure the file matches the supported subset.",
            IoError::TopCellNotFound => {
                "Specify a valid top cell name or ensure the design has a unique top-level cell."
            }
            IoError::BenchParse(_) => {
                "Keep the file within the supported bench subset and ensure signals are declared before use."
            }
            IoError::SpiceParse(_) => {
                "Check the SPICE netlist syntax. Supported elements: X (subckt), J (jj), R, L, C, T (tline), K (mutual), V (voltage source), I (current source)."
            }
            IoError::BlifParse(_) => {
                "Check the BLIF netlist syntax. Supported directives: .model, .inputs, .outputs, .names, .latch, .end."
            }
            IoError::EdifParse(_) => {
                "Check the EDIF netlist syntax. Supported constructs: edif, library, cell, view, interface, port, design, instance, cellRef, libraryRef, connect, portRef."
            }
            IoError::LefWriteUnsupported => {
                "Use DEF export for now; LEF writing is not part of the supported output surface yet."
            }
            IoError::UnsupportedSchemaVersion { .. } => {
                "Regenerate the file with the current toolchain or run a compatible rflux version."
            }
            IoError::InvalidJsonEnvelope { .. } => {
                "Ensure the JSON envelope contains schema_version, kind, and payload."
            }
            IoError::UnexpectedJsonKind { .. } => {
                "Use the correct file type for this command, or regenerate the file with the matching writer."
            }
        }
    }
}

pub fn write_ir_json(path: impl AsRef<Path>, netlist: &Netlist) -> Result<(), IoError> {
    let content = serde_json::to_string_pretty(&json!({
        "schema_version": IR_JSON_SCHEMA_VERSION,
        "kind": IR_JSON_KIND,
        "payload": netlist,
    }))
    .map_err(json_error)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn read_ir_json(path: impl AsRef<Path>) -> Result<Netlist, IoError> {
    let content = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content).map_err(json_error)?;
    if let Some(payload) = extract_versioned_payload(&json, IR_JSON_KIND, IR_JSON_SCHEMA_VERSION)? {
        return serde_json::from_value(payload).map_err(json_error);
    }
    serde_json::from_value(json).map_err(json_error)
}

pub fn read_bench_netlist(path: impl AsRef<Path>) -> Result<Netlist, IoError> {
    let content = fs::read_to_string(path)?;
    parse_bench_netlist(&content)
}

pub fn detect_netlist_input_format(path: impl AsRef<Path>) -> NetlistInputFormat {
    let ext = path
        .as_ref()
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    if ext.eq_ignore_ascii_case("bench") {
        NetlistInputFormat::Bench
    } else if ext.eq_ignore_ascii_case("v") {
        NetlistInputFormat::Verilog
    } else if ext.eq_ignore_ascii_case("cir") || ext.eq_ignore_ascii_case("sp") || ext.eq_ignore_ascii_case("spice") {
        NetlistInputFormat::Spice
    } else if ext.eq_ignore_ascii_case("blif") {
        NetlistInputFormat::Blif
    } else if ext.eq_ignore_ascii_case("edif") || ext.eq_ignore_ascii_case("edf") {
        NetlistInputFormat::Edif
    } else {
        NetlistInputFormat::IrJson
    }
}

pub fn read_netlist(path: impl AsRef<Path>) -> Result<Netlist, IoError> {
    let path = path.as_ref();
    read_netlist_as(path, detect_netlist_input_format(path))
}

pub fn read_netlist_as(
    path: impl AsRef<Path>,
    format: NetlistInputFormat,
) -> Result<Netlist, IoError> {
    match format {
        NetlistInputFormat::IrJson => read_ir_json(path),
        NetlistInputFormat::Bench => read_bench_netlist(path),
        NetlistInputFormat::Verilog => {
            let content = fs::read_to_string(path)?;
            let src = rflux_verilog::parse_verilog(&content)
                .map_err(|e| IoError::BenchParse(e.to_string()))?;
            rflux_verilog::elaborate_to_ir(&src, "top")
                .map_err(|e| IoError::BenchParse(e.to_string()))
        }
        NetlistInputFormat::Spice => {
            let content = fs::read_to_string(path)?;
            let devices = rflux_sim::parse_spice_netlist(&content)
                .map_err(|e| IoError::SpiceParse(e.to_string()))?;
            spice_to_ir(&devices)
        }
        NetlistInputFormat::Blif => {
            let content = fs::read_to_string(path)?;
            parse_blif(&content).map(|(netlist, _name)| netlist)
        }
        NetlistInputFormat::Edif => {
            let content = fs::read_to_string(path)?;
            parse_edif(&content).map(|(netlist, _name)| netlist)
        }
    }
}

pub fn spice_to_ir(devices: &[rflux_sim::SpiceDevice]) -> Result<Netlist, IoError> {
    use std::collections::HashMap;
    let mut netlist = Netlist::new();
    let mut node_map: HashMap<String, rflux_ir::NodeId> = HashMap::new();

    for device in devices {
        match &device.kind {
            rflux_sim::SpiceDeviceKind::SubcktInstance(subckt) => {
                let logic_op = match subckt.as_str() {
                    "and" => Some(LogicOp::And),
                    "or" => Some(LogicOp::Or),
                    "not" | "buf" => Some(LogicOp::Not),
                    "xor" => Some(LogicOp::Xor),
                    "nand" => Some(LogicOp::And),
                    "nor" => Some(LogicOp::Or),
                    "xnor" => Some(LogicOp::Xor),
                    "mux" => Some(LogicOp::Mux2),
                    "dff" => Some(LogicOp::DffEnable),
                    _ => None,
                };
                let kind = if subckt == "dff" {
                    NodeKind::Dff
                } else {
                    NodeKind::CellInstance
                };
                let node = netlist.add_node_with_logic(kind, &device.name, logic_op);
                node_map.insert(device.name.clone(), node);
            }
            rflux_sim::SpiceDeviceKind::Jj => {
                let node = netlist.add_node(NodeKind::CellInstance, &device.name);
                node_map.insert(device.name.clone(), node);
            }
            _ => {}
        }
    }

    let mut net_devices: HashMap<String, Vec<(rflux_ir::NodeId, usize)>> = HashMap::new();
    for device in devices {
        if let Some(&node_id) = node_map.get(&device.name) {
            for (i, net_name) in device.connections.iter().enumerate() {
                net_devices
                    .entry(net_name.clone())
                    .or_default()
                    .push((node_id, i));
            }
        }
    }

    for (_net, devs) in &net_devices {
        if devs.len() >= 2 {
            let (src_node, src_port) = devs[0];
            for &(dst_node, dst_port) in &devs[1..] {
                let _ = netlist.connect(
                    PinRef {
                        node: src_node,
                        port: src_port as u16,
                    },
                    PinRef {
                        node: dst_node,
                        port: dst_port as u16,
                    },
                );
            }
        }
    }

    Ok(netlist)
}

pub fn read_blif(path: impl AsRef<Path>) -> Result<(Netlist, String), IoError> {
    let content = fs::read_to_string(path)?;
    parse_blif(&content)
}

pub fn parse_blif(content: &str) -> Result<(Netlist, String), IoError> {
    let mut netlist = Netlist::new();
    let mut model_name = String::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut wire_map: HashMap<String, rflux_ir::NodeId> = HashMap::new();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        i += 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with(".model ") {
            model_name = line[7..].trim().to_string();
        } else if line.starts_with(".inputs ") {
            inputs = line[8..].split_whitespace().map(String::from).collect();
        } else if line.starts_with(".outputs ") {
            outputs = line[9..].split_whitespace().map(String::from).collect();
        } else if line.starts_with(".names ") {
            let parts: Vec<&str> = line[7..].split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let (input_names, output_name) = if parts.len() == 1 {
                (Vec::new(), parts[0].to_string())
            } else {
                (
                    parts[..parts.len() - 1]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    parts.last().unwrap().to_string(),
                )
            };

            let mut truth_table_rows = Vec::new();
            while i < lines.len() {
                let next = lines[i].trim();
                if next.is_empty() || next.starts_with('.') {
                    break;
                }
                truth_table_rows.push(next.to_string());
                i += 1;
            }

            let logic_op = determine_blif_gate_op(&input_names, &truth_table_rows);
            let kind = NodeKind::CellInstance;
            let node = netlist.add_node_with_logic(kind, format!("gate_{}", output_name), logic_op);
            wire_map.insert(output_name, node);

            for (idx, input_name) in input_names.iter().enumerate() {
                let input_node =
                    blif_get_or_create_wire(&mut netlist, &mut wire_map, input_name);
                let _ = netlist.connect(
                    PinRef {
                        node: input_node,
                        port: 0,
                    },
                    PinRef {
                        node,
                        port: (idx + 1) as u16,
                    },
                );
            }
        } else if line.starts_with(".latch ") {
            let parts: Vec<&str> = line[7..].split_whitespace().collect();
            if parts.len() >= 2 {
                let input_name = parts[0].to_string();
                let output_name = parts[1].to_string();
                let node = netlist.add_node_with_logic(
                    NodeKind::Dff,
                    format!("latch_{}", output_name),
                    Some(LogicOp::DffEnable),
                );
                wire_map.insert(output_name, node);
                let input_node =
                    blif_get_or_create_wire(&mut netlist, &mut wire_map, &input_name);
                let _ = netlist.connect(
                    PinRef {
                        node: input_node,
                        port: 0,
                    },
                    PinRef {
                        node,
                        port: 1,
                    },
                );
            }
        } else if line.starts_with(".end") {
            break;
        }
    }

    for input in &inputs {
        blif_get_or_create_wire(&mut netlist, &mut wire_map, input);
    }
    for output in &outputs {
        blif_get_or_create_wire(&mut netlist, &mut wire_map, output);
    }

    Ok((netlist, model_name))
}

fn blif_get_or_create_wire(
    netlist: &mut Netlist,
    wire_map: &mut HashMap<String, rflux_ir::NodeId>,
    name: &str,
) -> rflux_ir::NodeId {
    if let Some(&node) = wire_map.get(name) {
        node
    } else {
        let node = netlist.add_node(NodeKind::Port, name.to_string());
        wire_map.insert(name.to_string(), node);
        node
    }
}

fn determine_blif_gate_op(inputs: &[String], truth_table: &[String]) -> Option<LogicOp> {
    if inputs.is_empty() {
        return Some(LogicOp::Buf);
    }

    let has_only_one_true_minterm = truth_table.len() == 1
        && truth_table[0].contains('1')
        && !truth_table[0].starts_with('0')
        || (truth_table.len() == 1 && truth_table[0].len() >= 2);

    if inputs.len() == 1 {
        if truth_table.len() == 1 {
            let row = &truth_table[0];
            if row.starts_with('0') {
                return Some(LogicOp::Not);
            }
            if row.starts_with('1') {
                return Some(LogicOp::Buf);
            }
        }
        return Some(LogicOp::Buf);
    }

    let _ = has_only_one_true_minterm;
    Some(LogicOp::And)
}

pub fn read_edif(path: impl AsRef<Path>) -> Result<(Netlist, String), IoError> {
    let content = fs::read_to_string(path)?;
    parse_edif(&content)
}

pub fn parse_edif(content: &str) -> Result<(Netlist, String), IoError> {
    let tokens = tokenize_edif(content);
    let mut pos = 0;

    let edif_block = find_edif_block(&tokens, &mut pos)?;
    let design_name = edif_block.name.clone();

    let mut cell_defs: HashMap<String, Vec<EdifPort>> = HashMap::new();
    for lib in &edif_block.libraries {
        for cell in &lib.cells {
            cell_defs.insert(cell.name.clone(), cell.ports.clone());
        }
    }

    let mut netlist = Netlist::new();
    let mut wire_map: HashMap<String, rflux_ir::NodeId> = HashMap::new();
    let mut port_index: HashMap<String, HashMap<String, usize>> = HashMap::new();

    for (cell_name, ports) in &cell_defs {
        let mut pin_map = HashMap::new();
        for (i, port) in ports.iter().enumerate() {
            pin_map.insert(port.name.clone(), i);
        }
        port_index.insert(cell_name.clone(), pin_map);
    }

    for inst in &edif_block.instances {
        let ports = cell_defs.get(&inst.cell_ref).cloned().unwrap_or_default();
        let logic_op = match inst.cell_ref.as_str() {
            "AND2" | "AND" => Some(LogicOp::And),
            "OR2" | "OR" => Some(LogicOp::Or),
            "NOT" | "INV" => Some(LogicOp::Not),
            "XOR2" | "XOR" => Some(LogicOp::Xor),
            "NAND2" | "NAND" => Some(LogicOp::And),
            "NOR2" | "NOR" => Some(LogicOp::Or),
            "XNOR2" | "XNOR" => Some(LogicOp::Xor),
            "MUX2" | "MUX" => Some(LogicOp::Mux2),
            "DFF" => Some(LogicOp::DffEnable),
            "BUF" | "BUFF" => Some(LogicOp::Buf),
            _ => None,
        };
        let kind = if inst.cell_ref == "DFF" {
            NodeKind::Dff
        } else {
            NodeKind::CellInstance
        };
        let node = netlist.add_node_with_logic(kind, &inst.name, logic_op);
        wire_map.insert(inst.name.clone(), node);

        for (i, _port) in ports.iter().enumerate() {
            let pin_key = format!("{}.{}", inst.name, i);
            wire_map.entry(pin_key).or_insert(node);
        }
    }

    for conn in &edif_block.connections {
        let src = edif_resolve_wire(&mut netlist, &mut wire_map, &conn.src_net);
        let dst = edif_resolve_wire(&mut netlist, &mut wire_map, &conn.dst_net);
        if src != dst {
            let src_port = conn.src_port.unwrap_or(0);
            let dst_port = conn.dst_port.unwrap_or(0);
            let _ = netlist.connect(
                PinRef {
                    node: src,
                    port: src_port as u16,
                },
                PinRef {
                    node: dst,
                    port: dst_port as u16,
                },
            );
        }
    }

    Ok((netlist, design_name))
}

fn edif_resolve_wire(
    netlist: &mut Netlist,
    wire_map: &mut HashMap<String, rflux_ir::NodeId>,
    name: &str,
) -> rflux_ir::NodeId {
    if let Some(&node) = wire_map.get(name) {
        node
    } else {
        let node = netlist.add_node(NodeKind::Port, name.to_string());
        wire_map.insert(name.to_string(), node);
        node
    }
}

#[derive(Debug, Clone)]
struct EdifPort {
    name: String,
    _direction: String,
}

#[derive(Debug, Clone)]
struct EdifCell {
    name: String,
    ports: Vec<EdifPort>,
}

#[derive(Debug, Clone)]
struct EdifLibrary {
    _name: String,
    cells: Vec<EdifCell>,
}

#[derive(Debug, Clone)]
struct EdifInstance {
    name: String,
    cell_ref: String,
    _connections: Vec<EdifConnect>,
}

#[derive(Debug, Clone)]
struct EdifConnect {
    _src_net: String,
    _src_port: Option<usize>,
    _dst_net: String,
    _dst_port: Option<usize>,
}

#[derive(Debug, Clone)]
struct EdifConnection {
    src_net: String,
    src_port: Option<usize>,
    dst_net: String,
    dst_port: Option<usize>,
}

#[derive(Debug)]
struct EdifDesign {
    name: String,
    libraries: Vec<EdifLibrary>,
    instances: Vec<EdifInstance>,
    connections: Vec<EdifConnection>,
}

fn find_edif_block(tokens: &[String], pos: &mut usize) -> Result<EdifDesign, IoError> {
    while *pos < tokens.len() {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "edif" {
                *pos += 1;
                let name = if *pos < tokens.len() {
                    tokens[*pos].clone()
                } else {
                    return Err(IoError::EdifParse("missing edif name".into()));
                };
                *pos += 1;

                let mut libraries = Vec::new();
                let mut design_cell_ref = String::new();
                let mut instances = Vec::new();
                let mut connections = Vec::new();

                while *pos < tokens.len() && tokens[*pos] != ")" {
                    if tokens[*pos] == "(" {
                        *pos += 1;
                        if *pos < tokens.len() {
                            match tokens[*pos].as_str() {
                                "library" => {
                                    *pos += 1;
                                    let lib = parse_edif_library(tokens, pos)?;
                                    libraries.push(lib);
                                }
                                "design" => {
                                    *pos += 1;
                                    parse_edif_design_block(
                                        tokens,
                                        pos,
                                        &mut design_cell_ref,
                                        &mut instances,
                                        &mut connections,
                                    )?;
                                }
                                _ => skip_edif_form(tokens, pos)?,
                            }
                        }
                    } else {
                        *pos += 1;
                    }
                }
                if *pos < tokens.len() {
                    *pos += 1;
                }

                return Ok(EdifDesign {
                    name,
                    libraries,
                    instances,
                    connections,
                });
            }
        } else {
            *pos += 1;
        }
    }
    Err(IoError::EdifParse("no (edif ...) block found".into()))
}

fn parse_edif_library(tokens: &[String], pos: &mut usize) -> Result<EdifLibrary, IoError> {
    let name = if *pos < tokens.len() {
        tokens[*pos].clone()
    } else {
        return Err(IoError::EdifParse("missing library name".into()));
    };
    *pos += 1;

    let mut cells = Vec::new();

    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "cell" {
                *pos += 1;
                let cell = parse_edif_cell(tokens, pos)?;
                cells.push(cell);
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }

    Ok(EdifLibrary { _name: name, cells })
}

fn parse_edif_cell(tokens: &[String], pos: &mut usize) -> Result<EdifCell, IoError> {
    let name = if *pos < tokens.len() {
        tokens[*pos].clone()
    } else {
        return Err(IoError::EdifParse("missing cell name".into()));
    };
    *pos += 1;

    let mut ports = Vec::new();

    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "view" {
                *pos += 1;
                parse_edif_view(tokens, pos, &mut ports)?;
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }

    Ok(EdifCell { name, ports })
}

fn parse_edif_view(
    tokens: &[String],
    pos: &mut usize,
    ports: &mut Vec<EdifPort>,
) -> Result<(), IoError> {
    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "interface" {
                *pos += 1;
                parse_edif_interface(tokens, pos, ports)?;
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }
    Ok(())
}

fn parse_edif_interface(
    tokens: &[String],
    pos: &mut usize,
    ports: &mut Vec<EdifPort>,
) -> Result<(), IoError> {
    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "port" {
                *pos += 1;
                let port = parse_edif_port(tokens, pos)?;
                ports.push(port);
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }
    Ok(())
}

fn parse_edif_port(tokens: &[String], pos: &mut usize) -> Result<EdifPort, IoError> {
    let mut direction = String::from("input");
    let mut name = String::new();

    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "direction" {
                *pos += 1;
                if *pos < tokens.len() {
                    direction = tokens[*pos].clone();
                    *pos += 1;
                }
                skip_to_close(tokens, pos)?;
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else if name.is_empty() {
            name = tokens[*pos].clone();
            *pos += 1;
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }

    if name.is_empty() {
        return Err(IoError::EdifParse("port missing name".into()));
    }

    Ok(EdifPort { name, _direction: direction })
}

fn parse_edif_design_block(
    tokens: &[String],
    pos: &mut usize,
    design_cell_ref: &mut String,
    instances: &mut Vec<EdifInstance>,
    connections: &mut Vec<EdifConnection>,
) -> Result<(), IoError> {
    if *pos < tokens.len() && tokens[*pos] != "(" {
        *design_cell_ref = tokens[*pos].clone();
        *pos += 1;
    }

    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() {
                match tokens[*pos].as_str() {
                    "cellRef" => {
                        *pos += 1;
                        if *pos < tokens.len() {
                            *design_cell_ref = tokens[*pos].clone();
                            *pos += 1;
                        }
                        skip_to_close(tokens, pos)?;
                    }
                    "instance" => {
                        *pos += 1;
                        let inst = parse_edif_instance(tokens, pos)?;
                        instances.push(inst);
                    }
                    "net" => {
                        *pos += 1;
                        parse_edif_net(tokens, pos, connections)?;
                    }
                    _ => skip_edif_form(tokens, pos)?,
                }
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }

    Ok(())
}

fn parse_edif_instance(tokens: &[String], pos: &mut usize) -> Result<EdifInstance, IoError> {
    let name = if *pos < tokens.len() {
        tokens[*pos].clone()
    } else {
        return Err(IoError::EdifParse("missing instance name".into()));
    };
    *pos += 1;

    let mut cell_ref = String::new();
    let mut connects = Vec::new();

    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() {
                match tokens[*pos].as_str() {
                    "cellRef" => {
                        *pos += 1;
                        if *pos < tokens.len() {
                            cell_ref = tokens[*pos].clone();
                            *pos += 1;
                        }
                        skip_to_close(tokens, pos)?;
                    }
                    "connect" => {
                        *pos += 1;
                        let conn = parse_edif_connect(tokens, pos)?;
                        connects.push(conn);
                    }
                    _ => skip_edif_form(tokens, pos)?,
                }
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }

    Ok(EdifInstance {
        name,
        cell_ref,
        _connections: connects,
    })
}

fn parse_edif_connect(tokens: &[String], pos: &mut usize) -> Result<EdifConnect, IoError> {
    let _net_name = if *pos < tokens.len() {
        tokens[*pos].clone()
    } else {
        return Err(IoError::EdifParse("missing connect net name".into()));
    };
    *pos += 1;

    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "portRef" {
                *pos += 1;
                if *pos < tokens.len() {
                    *pos += 1;
                }
                skip_to_close(tokens, pos)?;
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }

    Ok(EdifConnect {
        _src_net: String::new(),
        _src_port: None,
        _dst_net: String::new(),
        _dst_port: None,
    })
}

fn parse_edif_net(
    tokens: &[String],
    pos: &mut usize,
    connections: &mut Vec<EdifConnection>,
) -> Result<(), IoError> {
    let _net_name = if *pos < tokens.len() {
        tokens[*pos].clone()
    } else {
        return Err(IoError::EdifParse("missing net name".into()));
    };
    *pos += 1;

    let mut joined_refs: Vec<(String, Option<String>)> = Vec::new();

    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "joined" {
                *pos += 1;
                parse_edif_joined(tokens, pos, &mut joined_refs)?;
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }

    for i in 0..joined_refs.len() {
        for j in (i + 1)..joined_refs.len() {
            let (ref inst_a, ref port_a) = joined_refs[i];
            let (ref inst_b, ref port_b) = joined_refs[j];
            let port_a_idx = port_a.as_ref().and_then(|p| p.parse::<usize>().ok());
            let port_b_idx = port_b.as_ref().and_then(|p| p.parse::<usize>().ok());
            connections.push(EdifConnection {
                src_net: inst_a.clone(),
                src_port: port_a_idx,
                dst_net: inst_b.clone(),
                dst_port: port_b_idx,
            });
        }
    }

    Ok(())
}

fn parse_edif_joined(
    tokens: &[String],
    pos: &mut usize,
    refs: &mut Vec<(String, Option<String>)>,
) -> Result<(), IoError> {
    while *pos < tokens.len() && tokens[*pos] != ")" {
        if tokens[*pos] == "(" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "portRef" {
                *pos += 1;
                let port_name = if *pos < tokens.len() && tokens[*pos] != "(" && tokens[*pos] != ")" {
                    let n = tokens[*pos].clone();
                    *pos += 1;
                    n
                } else {
                    String::new()
                };
                let mut inst_ref = None;
                while *pos < tokens.len() && tokens[*pos] != ")" {
                    if tokens[*pos] == "(" {
                        *pos += 1;
                        if *pos < tokens.len() && tokens[*pos] == "instanceRef" {
                            *pos += 1;
                            if *pos < tokens.len() {
                                inst_ref = Some(tokens[*pos].clone());
                                *pos += 1;
                            }
                            skip_to_close(tokens, pos)?;
                        } else {
                            skip_edif_form(tokens, pos)?;
                        }
                    } else {
                        *pos += 1;
                    }
                }
                if *pos < tokens.len() {
                    *pos += 1;
                }
                if let Some(inst) = inst_ref {
                    refs.push((inst, Some(port_name)));
                } else {
                    refs.push((port_name, None));
                }
            } else {
                skip_edif_form(tokens, pos)?;
            }
        } else {
            *pos += 1;
        }
    }
    if *pos < tokens.len() {
        *pos += 1;
    }
    Ok(())
}

fn skip_to_close(tokens: &[String], pos: &mut usize) -> Result<(), IoError> {
    let mut depth = 0;
    while *pos < tokens.len() {
        if tokens[*pos] == "(" {
            depth += 1;
        } else if tokens[*pos] == ")" {
            if depth == 0 {
                *pos += 1;
                return Ok(());
            }
            depth -= 1;
        }
        *pos += 1;
    }
    Ok(())
}

fn skip_edif_form(tokens: &[String], pos: &mut usize) -> Result<(), IoError> {
    let mut depth = 1;
    while *pos < tokens.len() {
        if tokens[*pos] == "(" {
            depth += 1;
        } else if tokens[*pos] == ")" {
            depth -= 1;
            if depth == 0 {
                *pos += 1;
                return Ok(());
            }
        }
        *pos += 1;
    }
    Ok(())
}

fn tokenize_edif(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_comment = false;
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if in_comment {
            if ch == '\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '(' | ')' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push(ch.to_string());
            }
            ' ' | '\n' | '\r' | '\t' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '"' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                let mut s = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    s.push(chars[i]);
                    i += 1;
                }
                tokens.push(s);
            }
            ';' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                in_comment = true;
            }
            _ => current.push(ch),
        }
        i += 1;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub fn write_pdk_json(path: impl AsRef<Path>, pdk: &Pdk) -> Result<(), IoError> {
    let content = serde_json::to_string_pretty(&json!({
        "schema_version": PDK_JSON_SCHEMA_VERSION,
        "kind": PDK_JSON_KIND,
        "payload": pdk,
    }))
    .map_err(json_error)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn read_pdk_json(path: impl AsRef<Path>) -> Result<Pdk, IoError> {
    let content = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content).map_err(json_error)?;
    if let Some(payload) = extract_versioned_payload(&json, PDK_JSON_KIND, PDK_JSON_SCHEMA_VERSION)?
    {
        return serde_json::from_value(payload).map_err(json_error);
    }
    serde_json::from_value(json).map_err(json_error)
}

#[cfg(feature = "yaml")]
pub fn read_pdk_yaml(path: impl AsRef<Path>) -> Result<Pdk, IoError> {
    let content = fs::read_to_string(path)?;
    Pdk::from_yaml(&content).map_err(|e| IoError::Json(e.to_string()))
}

#[cfg(feature = "yaml")]
pub fn write_pdk_yaml(path: impl AsRef<Path>, pdk: &Pdk) -> Result<(), IoError> {
    let content = pdk.to_yaml().map_err(|e| IoError::Json(e.to_string()))?;
    fs::write(path, content)?;
    Ok(())
}

pub fn read_pdk_auto(path: impl AsRef<Path>) -> Result<Pdk, IoError> {
    let path_ref = path.as_ref();
    #[cfg(feature = "yaml")]
    {
        if matches!(
            path_ref.extension().and_then(|e| e.to_str()),
            Some("yaml" | "yml")
        ) {
            let content = fs::read_to_string(path_ref)?;
            return Pdk::from_yaml(&content).map_err(|e| IoError::Json(e.to_string()));
        }
    }
    read_pdk_json(path_ref)
}

fn extract_versioned_payload(
    json: &Value,
    expected_kind: &'static str,
    expected_schema_version: u64,
) -> Result<Option<Value>, IoError> {
    let Some(object) = json.as_object() else {
        return Ok(None);
    };

    let looks_like_envelope = object.contains_key("schema_version")
        || object.contains_key("kind")
        || object.contains_key("payload");
    if !looks_like_envelope {
        return Ok(None);
    }

    let schema_version = object.get("schema_version").and_then(Value::as_u64).ok_or(
        IoError::InvalidJsonEnvelope {
            kind: expected_kind,
            detail: "missing schema_version",
        },
    )?;
    if schema_version != expected_schema_version {
        return Err(IoError::UnsupportedSchemaVersion {
            kind: expected_kind,
            version: schema_version,
        });
    }

    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(IoError::InvalidJsonEnvelope {
            kind: expected_kind,
            detail: "missing kind",
        })?;
    if kind != expected_kind {
        return Err(IoError::UnexpectedJsonKind {
            expected_kind,
            found_kind: kind.to_string(),
        });
    }

    object
        .get("payload")
        .cloned()
        .ok_or(IoError::InvalidJsonEnvelope {
            kind: expected_kind,
            detail: "missing payload",
        })
        .map(Some)
}

#[derive(Debug, Clone)]
struct BenchGateSpec {
    output: String,
    op: String,
    inputs: Vec<String>,
    line: usize,
}

fn parse_bench_netlist(text: &str) -> Result<Netlist, IoError> {
    let mut input_names = Vec::new();
    let mut output_names = Vec::new();
    let mut gates = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(name) = parse_bench_decl(trimmed, "INPUT", line_number)? {
            input_names.push(BenchNamedSignal {
                name: name.to_string(),
                line: line_number,
            });
            continue;
        }
        if let Some(name) = parse_bench_decl(trimmed, "OUTPUT", line_number)? {
            output_names.push(BenchNamedSignal {
                name: name.to_string(),
                line: line_number,
            });
            continue;
        }

        gates.push(parse_bench_gate(trimmed, line_number)?);
    }

    ensure_unique_bench_signal_names(&input_names)?;
    ensure_unique_bench_signal_names(&output_names)?;
    let gates = order_bench_gates(gates, &input_names)?;

    let mut netlist = Netlist::new();
    let mut signal_driver = HashMap::new();
    let mut next_output_port = HashMap::<String, u16>::new();

    for name in input_names {
        let node_id = netlist.add_node(NodeKind::Port, &name.name);
        signal_driver.insert(name.name, node_id);
    }

    for gate in gates {
        let expected_inputs = bench_expected_inputs(&gate.op).ok_or_else(|| {
            bench_parse_error_at_line(gate.line, format!(
                "unsupported gate op '{}'; supported ops are AND/OR/XOR/XNOR/NOT/NAND/NOR/BUF/BUFF/MUX/DFF/DFFE/MAJ/AOI21/OAI21/AOI22/OAI22/AOI31/OAI31/AOI211/OAI211/AOI311/OAI311/AOI321/OAI321/AOI221/OAI221/AOI222/OAI222/AOI322/OAI322/AOI421/OAI421/AOI422/OAI422/AOI431/OAI431/AOI432/OAI432/AOI433/OAI433/AOI441/OAI441/AOI442/OAI442/AOI443/OAI443/AOI444/OAI444/AOI2221/OAI2221",
                gate.op
            ))
        })?;
        if gate.inputs.len() != expected_inputs {
            return Err(bench_parse_error_at_line(
                gate.line,
                format!(
                    "gate '{}' op {} expects {} input(s), got {}",
                    gate.output,
                    gate.op,
                    expected_inputs,
                    gate.inputs.len()
                ),
            ));
        }

        let driver_node = match gate.op.as_str() {
            "DFF" => {
                let dff_node = netlist.add_node(NodeKind::Dff, &gate.output);
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &gate.inputs,
                    dff_node,
                )?;
                dff_node
            }
            "DFFE" => {
                let dffe_node = netlist.add_node_with_logic(
                    NodeKind::Dff,
                    &gate.output,
                    Some(LogicOp::DffEnable),
                );
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &gate.inputs,
                    dffe_node,
                )?;
                dffe_node
            }
            "XNOR" => {
                let inner_name = format!("{}__bench_xnor_inner", gate.output);
                let xor_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &inner_name,
                    Some(LogicOp::Xor),
                );
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &gate.inputs,
                    xor_node,
                )?;
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );
                netlist
                    .connect(
                        PinRef {
                            node: xor_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "NAND" => {
                let inner_name = format!("{}__bench_nand_inner", gate.output);
                let and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &inner_name,
                    Some(LogicOp::And),
                );
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &gate.inputs,
                    and_node,
                )?;
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );
                netlist
                    .connect(
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "NOR" => {
                let inner_name = format!("{}__bench_nor_inner", gate.output);
                let or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &inner_name,
                    Some(LogicOp::Or),
                );
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &gate.inputs,
                    or_node,
                )?;
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );
                netlist
                    .connect(
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "MAJ" => {
                let left_and_name = format!("{}__bench_maj_ab", gate.output);
                let mid_and_name = format!("{}__bench_maj_ac", gate.output);
                let right_and_name = format!("{}__bench_maj_bc", gate.output);
                let left_or_name = format!("{}__bench_maj_or0", gate.output);

                let ab_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_and_name,
                    Some(LogicOp::And),
                );
                let ac_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &mid_and_name,
                    Some(LogicOp::And),
                );
                let bc_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &right_and_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_or_name,
                    Some(LogicOp::Or),
                );
                let maj_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Or),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    ab_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[2].clone()],
                    ac_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[1].clone(), gate.inputs[2].clone()],
                    bc_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: ab_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: ac_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: maj_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: bc_node,
                            port: 0,
                        },
                        PinRef {
                            node: maj_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                maj_node
            }
            "AOI21" => {
                let and_name = format!("{}__bench_aoi21_and", gate.output);
                let or_name = format!("{}__bench_aoi21_or", gate.output);
                let and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and_name,
                    Some(LogicOp::And),
                );
                let or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI21" => {
                let or_name = format!("{}__bench_oai21_or", gate.output);
                let and_name = format!("{}__bench_oai21_and", gate.output);
                let or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or_name,
                    Some(LogicOp::Or),
                );
                let and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI22" => {
                let left_and_name = format!("{}__bench_aoi22_and0", gate.output);
                let right_and_name = format!("{}__bench_aoi22_and1", gate.output);
                let or_name = format!("{}__bench_aoi22_or", gate.output);

                let left_and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_and_name,
                    Some(LogicOp::And),
                );
                let right_and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &right_and_name,
                    Some(LogicOp::And),
                );
                let or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    left_and_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    right_and_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: left_and_node,
                            port: 0,
                        },
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: right_and_node,
                            port: 0,
                        },
                        PinRef {
                            node: or_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI22" => {
                let left_or_name = format!("{}__bench_oai22_or0", gate.output);
                let right_or_name = format!("{}__bench_oai22_or1", gate.output);
                let and_name = format!("{}__bench_oai22_and", gate.output);

                let left_or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_or_name,
                    Some(LogicOp::Or),
                );
                let right_or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &right_or_name,
                    Some(LogicOp::Or),
                );
                let and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    left_or_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    right_or_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: left_or_node,
                            port: 0,
                        },
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: right_or_node,
                            port: 0,
                        },
                        PinRef {
                            node: and_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI31" => {
                let and0_name = format!("{}__bench_aoi31_and0", gate.output);
                let and1_name = format!("{}__bench_aoi31_and1", gate.output);
                let or_name = format!("{}__bench_aoi31_or", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI31" => {
                let or0_name = format!("{}__bench_oai31_or0", gate.output);
                let or1_name = format!("{}__bench_oai31_or1", gate.output);
                let and_name = format!("{}__bench_oai31_and", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI211" => {
                let and_name = format!("{}__bench_aoi211_and", gate.output);
                let or0_name = format!("{}__bench_aoi211_or0", gate.output);
                let or1_name = format!("{}__bench_aoi211_or1", gate.output);

                let and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI211" => {
                let or_name = format!("{}__bench_oai211_or", gate.output);
                let and0_name = format!("{}__bench_oai211_and0", gate.output);
                let and1_name = format!("{}__bench_oai211_and1", gate.output);

                let or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI311" => {
                let and0_name = format!("{}__bench_aoi311_and0", gate.output);
                let and1_name = format!("{}__bench_aoi311_and1", gate.output);
                let or0_name = format!("{}__bench_aoi311_or0", gate.output);
                let or1_name = format!("{}__bench_aoi311_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fifth_input = signal_driver.get(&gate.inputs[4]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[4]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: fifth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[4]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI311" => {
                let or0_name = format!("{}__bench_oai311_or0", gate.output);
                let or1_name = format!("{}__bench_oai311_or1", gate.output);
                let and0_name = format!("{}__bench_oai311_and0", gate.output);
                let and1_name = format!("{}__bench_oai311_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fifth_input = signal_driver.get(&gate.inputs[4]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[4]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: fifth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[4]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI321" => {
                let and0_name = format!("{}__bench_aoi321_and0", gate.output);
                let and1_name = format!("{}__bench_aoi321_and1", gate.output);
                let and2_name = format!("{}__bench_aoi321_and2", gate.output);
                let or0_name = format!("{}__bench_aoi321_or0", gate.output);
                let or1_name = format!("{}__bench_aoi321_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fifth_input = signal_driver.get(&gate.inputs[4]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[4]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: fifth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[4]),
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let sixth_input = signal_driver.get(&gate.inputs[5]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[5]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: sixth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[5]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI321" => {
                let or0_name = format!("{}__bench_oai321_or0", gate.output);
                let or1_name = format!("{}__bench_oai321_or1", gate.output);
                let or2_name = format!("{}__bench_oai321_or2", gate.output);
                let and0_name = format!("{}__bench_oai321_and0", gate.output);
                let and1_name = format!("{}__bench_oai321_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                let third_input = signal_driver.get(&gate.inputs[2]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[2]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: third_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fourth_input =
                    signal_driver.get(&gate.inputs[3]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[3]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: fourth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fifth_input = signal_driver.get(&gate.inputs[4]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[4]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: fifth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[4]),
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let sixth_input = signal_driver.get(&gate.inputs[5]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[5]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: sixth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[5]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI322" => {
                let and0_name = format!("{}__bench_aoi322_and0", gate.output);
                let and1_name = format!("{}__bench_aoi322_and1", gate.output);
                let and2_name = format!("{}__bench_aoi322_and2", gate.output);
                let or0_name = format!("{}__bench_aoi322_or0", gate.output);
                let or1_name = format!("{}__bench_aoi322_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[
                        gate.inputs[0].clone(),
                        gate.inputs[1].clone(),
                        gate.inputs[2].clone(),
                    ],
                    and0_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[3].clone(), gate.inputs[4].clone()],
                    and1_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[5].clone(), gate.inputs[6].clone()],
                    and2_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI322" => {
                let or0_name = format!("{}__bench_oai322_or0", gate.output);
                let or1_name = format!("{}__bench_oai322_or1", gate.output);
                let or2_name = format!("{}__bench_oai322_or2", gate.output);
                let and0_name = format!("{}__bench_oai322_and0", gate.output);
                let and1_name = format!("{}__bench_oai322_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[
                        gate.inputs[0].clone(),
                        gate.inputs[1].clone(),
                        gate.inputs[2].clone(),
                    ],
                    or0_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[3].clone(), gate.inputs[4].clone()],
                    or1_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[5].clone(), gate.inputs[6].clone()],
                    or2_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI421" => {
                let and0_name = format!("{}__bench_aoi421_and0", gate.output);
                let and1_name = format!("{}__bench_aoi421_and1", gate.output);
                let and2_name = format!("{}__bench_aoi421_and2", gate.output);
                let and3_name = format!("{}__bench_aoi421_and3", gate.output);
                let or0_name = format!("{}__bench_aoi421_or0", gate.output);
                let or1_name = format!("{}__bench_aoi421_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI421" => {
                let or0_name = format!("{}__bench_oai421_or0", gate.output);
                let or1_name = format!("{}__bench_oai421_or1", gate.output);
                let or2_name = format!("{}__bench_oai421_or2", gate.output);
                let or3_name = format!("{}__bench_oai421_or3", gate.output);
                let and0_name = format!("{}__bench_oai421_and0", gate.output);
                let and1_name = format!("{}__bench_oai421_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI422" => {
                let and0_name = format!("{}__bench_aoi422_and0", gate.output);
                let and1_name = format!("{}__bench_aoi422_and1", gate.output);
                let and2_name = format!("{}__bench_aoi422_and2", gate.output);
                let and3_name = format!("{}__bench_aoi422_and3", gate.output);
                let and4_name = format!("{}__bench_aoi422_and4", gate.output);
                let or0_name = format!("{}__bench_aoi422_or0", gate.output);
                let or1_name = format!("{}__bench_aoi422_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[6].clone(), gate.inputs[7].clone()],
                    and4_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI422" => {
                let or0_name = format!("{}__bench_oai422_or0", gate.output);
                let or1_name = format!("{}__bench_oai422_or1", gate.output);
                let or2_name = format!("{}__bench_oai422_or2", gate.output);
                let or3_name = format!("{}__bench_oai422_or3", gate.output);
                let or4_name = format!("{}__bench_oai422_or4", gate.output);
                let and0_name = format!("{}__bench_oai422_and0", gate.output);
                let and1_name = format!("{}__bench_oai422_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[6].clone(), gate.inputs[7].clone()],
                    or4_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI431" => {
                let and0_name = format!("{}__bench_aoi431_and0", gate.output);
                let and1_name = format!("{}__bench_aoi431_and1", gate.output);
                let and2_name = format!("{}__bench_aoi431_and2", gate.output);
                let and3_name = format!("{}__bench_aoi431_and3", gate.output);
                let and4_name = format!("{}__bench_aoi431_and4", gate.output);
                let or0_name = format!("{}__bench_aoi431_or0", gate.output);
                let or1_name = format!("{}__bench_aoi431_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI431" => {
                let or0_name = format!("{}__bench_oai431_or0", gate.output);
                let or1_name = format!("{}__bench_oai431_or1", gate.output);
                let or2_name = format!("{}__bench_oai431_or2", gate.output);
                let or3_name = format!("{}__bench_oai431_or3", gate.output);
                let or4_name = format!("{}__bench_oai431_or4", gate.output);
                let and0_name = format!("{}__bench_oai431_and0", gate.output);
                let and1_name = format!("{}__bench_oai431_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI432" => {
                let and0_name = format!("{}__bench_aoi432_and0", gate.output);
                let and1_name = format!("{}__bench_aoi432_and1", gate.output);
                let and2_name = format!("{}__bench_aoi432_and2", gate.output);
                let and3_name = format!("{}__bench_aoi432_and3", gate.output);
                let and4_name = format!("{}__bench_aoi432_and4", gate.output);
                let and5_name = format!("{}__bench_aoi432_and5", gate.output);
                let or0_name = format!("{}__bench_aoi432_or0", gate.output);
                let or1_name = format!("{}__bench_aoi432_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let and5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and5_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[7].clone(), gate.inputs[8].clone()],
                    and5_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI432" => {
                let or0_name = format!("{}__bench_oai432_or0", gate.output);
                let or1_name = format!("{}__bench_oai432_or1", gate.output);
                let or2_name = format!("{}__bench_oai432_or2", gate.output);
                let or3_name = format!("{}__bench_oai432_or3", gate.output);
                let or4_name = format!("{}__bench_oai432_or4", gate.output);
                let or5_name = format!("{}__bench_oai432_or5", gate.output);
                let and0_name = format!("{}__bench_oai432_and0", gate.output);
                let and1_name = format!("{}__bench_oai432_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let or5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or5_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[7].clone(), gate.inputs[8].clone()],
                    or5_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI221" => {
                let left_and_name = format!("{}__bench_aoi221_and0", gate.output);
                let right_and_name = format!("{}__bench_aoi221_and1", gate.output);
                let left_or_name = format!("{}__bench_aoi221_or0", gate.output);
                let right_or_name = format!("{}__bench_aoi221_or1", gate.output);

                let left_and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_and_name,
                    Some(LogicOp::And),
                );
                let right_and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &right_and_name,
                    Some(LogicOp::And),
                );
                let left_or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_or_name,
                    Some(LogicOp::Or),
                );
                let right_or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &right_or_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    left_and_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    right_and_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: left_and_node,
                            port: 0,
                        },
                        PinRef {
                            node: left_or_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: right_and_node,
                            port: 0,
                        },
                        PinRef {
                            node: left_or_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fifth_input = signal_driver.get(&gate.inputs[4]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[4]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: left_or_node,
                            port: 0,
                        },
                        PinRef {
                            node: right_or_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: fifth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[4]),
                        },
                        PinRef {
                            node: right_or_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: right_or_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI221" => {
                let left_or_name = format!("{}__bench_oai221_or0", gate.output);
                let right_or_name = format!("{}__bench_oai221_or1", gate.output);
                let left_and_name = format!("{}__bench_oai221_and0", gate.output);
                let right_and_name = format!("{}__bench_oai221_and1", gate.output);

                let left_or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_or_name,
                    Some(LogicOp::Or),
                );
                let right_or_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &right_or_name,
                    Some(LogicOp::Or),
                );
                let left_and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &left_and_name,
                    Some(LogicOp::And),
                );
                let right_and_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &right_and_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    left_or_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    right_or_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: left_or_node,
                            port: 0,
                        },
                        PinRef {
                            node: left_and_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: right_or_node,
                            port: 0,
                        },
                        PinRef {
                            node: left_and_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let fifth_input = signal_driver.get(&gate.inputs[4]).copied().ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "signal '{}' used before definition",
                        gate.inputs[4]
                    ))
                })?;
                netlist
                    .connect(
                        PinRef {
                            node: left_and_node,
                            port: 0,
                        },
                        PinRef {
                            node: right_and_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: fifth_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[4]),
                        },
                        PinRef {
                            node: right_and_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: right_and_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI222" => {
                let and0_name = format!("{}__bench_aoi222_and0", gate.output);
                let and1_name = format!("{}__bench_aoi222_and1", gate.output);
                let and2_name = format!("{}__bench_aoi222_and2", gate.output);
                let or0_name = format!("{}__bench_aoi222_or0", gate.output);
                let or1_name = format!("{}__bench_aoi222_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    and1_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and2_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI222" => {
                let or0_name = format!("{}__bench_oai222_or0", gate.output);
                let or1_name = format!("{}__bench_oai222_or1", gate.output);
                let or2_name = format!("{}__bench_oai222_or2", gate.output);
                let and0_name = format!("{}__bench_oai222_and0", gate.output);
                let and1_name = format!("{}__bench_oai222_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    or1_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or2_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI433" => {
                let and0_name = format!("{}__bench_aoi433_and0", gate.output);
                let and1_name = format!("{}__bench_aoi433_and1", gate.output);
                let and2_name = format!("{}__bench_aoi433_and2", gate.output);
                let and3_name = format!("{}__bench_aoi433_and3", gate.output);
                let and4_name = format!("{}__bench_aoi433_and4", gate.output);
                let and5_name = format!("{}__bench_aoi433_and5", gate.output);
                let and6_name = format!("{}__bench_aoi433_and6", gate.output);
                let or0_name = format!("{}__bench_aoi433_or0", gate.output);
                let or1_name = format!("{}__bench_aoi433_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let and5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and5_name,
                    Some(LogicOp::And),
                );
                let and6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and6_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[7].clone(), gate.inputs[8].clone()],
                    and5_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                        PinRef {
                            node: and6_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[9]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[9]),
                        },
                        PinRef {
                            node: and6_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and6_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI433" => {
                let or0_name = format!("{}__bench_oai433_or0", gate.output);
                let or1_name = format!("{}__bench_oai433_or1", gate.output);
                let or2_name = format!("{}__bench_oai433_or2", gate.output);
                let or3_name = format!("{}__bench_oai433_or3", gate.output);
                let or4_name = format!("{}__bench_oai433_or4", gate.output);
                let or5_name = format!("{}__bench_oai433_or5", gate.output);
                let or6_name = format!("{}__bench_oai433_or6", gate.output);
                let and0_name = format!("{}__bench_oai433_and0", gate.output);
                let and1_name = format!("{}__bench_oai433_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let or5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or5_name,
                    Some(LogicOp::Or),
                );
                let or6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or6_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[7].clone(), gate.inputs[8].clone()],
                    or5_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                        PinRef {
                            node: or6_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[9]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[9]),
                        },
                        PinRef {
                            node: or6_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or6_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI441" => {
                let and0_name = format!("{}__bench_aoi441_and0", gate.output);
                let and1_name = format!("{}__bench_aoi441_and1", gate.output);
                let and2_name = format!("{}__bench_aoi441_and2", gate.output);
                let and3_name = format!("{}__bench_aoi441_and3", gate.output);
                let and4_name = format!("{}__bench_aoi441_and4", gate.output);
                let and5_name = format!("{}__bench_aoi441_and5", gate.output);
                let or0_name = format!("{}__bench_aoi441_or0", gate.output);
                let or1_name = format!("{}__bench_aoi441_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let and5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and5_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: and5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[8]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[8]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI441" => {
                let or0_name = format!("{}__bench_oai441_or0", gate.output);
                let or1_name = format!("{}__bench_oai441_or1", gate.output);
                let or2_name = format!("{}__bench_oai441_or2", gate.output);
                let or3_name = format!("{}__bench_oai441_or3", gate.output);
                let or4_name = format!("{}__bench_oai441_or4", gate.output);
                let or5_name = format!("{}__bench_oai441_or5", gate.output);
                let and0_name = format!("{}__bench_oai441_and0", gate.output);
                let and1_name = format!("{}__bench_oai441_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let or5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or5_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: or5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[8]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[8]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI442" => {
                let and0_name = format!("{}__bench_aoi442_and0", gate.output);
                let and1_name = format!("{}__bench_aoi442_and1", gate.output);
                let and2_name = format!("{}__bench_aoi442_and2", gate.output);
                let and3_name = format!("{}__bench_aoi442_and3", gate.output);
                let and4_name = format!("{}__bench_aoi442_and4", gate.output);
                let and5_name = format!("{}__bench_aoi442_and5", gate.output);
                let and6_name = format!("{}__bench_aoi442_and6", gate.output);
                let or0_name = format!("{}__bench_aoi442_or0", gate.output);
                let or1_name = format!("{}__bench_aoi442_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let and5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and5_name,
                    Some(LogicOp::And),
                );
                let and6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and6_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: and5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[8].clone(), gate.inputs[9].clone()],
                    and6_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and6_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI442" => {
                let or0_name = format!("{}__bench_oai442_or0", gate.output);
                let or1_name = format!("{}__bench_oai442_or1", gate.output);
                let or2_name = format!("{}__bench_oai442_or2", gate.output);
                let or3_name = format!("{}__bench_oai442_or3", gate.output);
                let or4_name = format!("{}__bench_oai442_or4", gate.output);
                let or5_name = format!("{}__bench_oai442_or5", gate.output);
                let or6_name = format!("{}__bench_oai442_or6", gate.output);
                let and0_name = format!("{}__bench_oai442_and0", gate.output);
                let and1_name = format!("{}__bench_oai442_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let or5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or5_name,
                    Some(LogicOp::Or),
                );
                let or6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or6_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: or5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[8].clone(), gate.inputs[9].clone()],
                    or6_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or6_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI443" => {
                let and0_name = format!("{}__bench_aoi443_and0", gate.output);
                let and1_name = format!("{}__bench_aoi443_and1", gate.output);
                let and2_name = format!("{}__bench_aoi443_and2", gate.output);
                let and3_name = format!("{}__bench_aoi443_and3", gate.output);
                let and4_name = format!("{}__bench_aoi443_and4", gate.output);
                let and5_name = format!("{}__bench_aoi443_and5", gate.output);
                let and6_name = format!("{}__bench_aoi443_and6", gate.output);
                let and7_name = format!("{}__bench_aoi443_and7", gate.output);
                let or0_name = format!("{}__bench_aoi443_or0", gate.output);
                let or1_name = format!("{}__bench_aoi443_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let and5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and5_name,
                    Some(LogicOp::And),
                );
                let and6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and6_name,
                    Some(LogicOp::And),
                );
                let and7_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and7_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: and5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[8].clone(), gate.inputs[9].clone()],
                    and6_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and6_node,
                            port: 0,
                        },
                        PinRef {
                            node: and7_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[10]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[10]),
                        },
                        PinRef {
                            node: and7_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and7_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI443" => {
                let or0_name = format!("{}__bench_oai443_or0", gate.output);
                let or1_name = format!("{}__bench_oai443_or1", gate.output);
                let or2_name = format!("{}__bench_oai443_or2", gate.output);
                let or3_name = format!("{}__bench_oai443_or3", gate.output);
                let or4_name = format!("{}__bench_oai443_or4", gate.output);
                let or5_name = format!("{}__bench_oai443_or5", gate.output);
                let or6_name = format!("{}__bench_oai443_or6", gate.output);
                let or7_name = format!("{}__bench_oai443_or7", gate.output);
                let and0_name = format!("{}__bench_oai443_and0", gate.output);
                let and1_name = format!("{}__bench_oai443_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let or5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or5_name,
                    Some(LogicOp::Or),
                );
                let or6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or6_name,
                    Some(LogicOp::Or),
                );
                let or7_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or7_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: or5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[8].clone(), gate.inputs[9].clone()],
                    or6_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or6_node,
                            port: 0,
                        },
                        PinRef {
                            node: or7_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[10]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[10]),
                        },
                        PinRef {
                            node: or7_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or7_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI444" => {
                let and0_name = format!("{}__bench_aoi444_and0", gate.output);
                let and1_name = format!("{}__bench_aoi444_and1", gate.output);
                let and2_name = format!("{}__bench_aoi444_and2", gate.output);
                let and3_name = format!("{}__bench_aoi444_and3", gate.output);
                let and4_name = format!("{}__bench_aoi444_and4", gate.output);
                let and5_name = format!("{}__bench_aoi444_and5", gate.output);
                let and6_name = format!("{}__bench_aoi444_and6", gate.output);
                let and7_name = format!("{}__bench_aoi444_and7", gate.output);
                let and8_name = format!("{}__bench_aoi444_and8", gate.output);
                let or0_name = format!("{}__bench_aoi444_or0", gate.output);
                let or1_name = format!("{}__bench_aoi444_or1", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let and3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and3_name,
                    Some(LogicOp::And),
                );
                let and4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and4_name,
                    Some(LogicOp::And),
                );
                let and5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and5_name,
                    Some(LogicOp::And),
                );
                let and6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and6_name,
                    Some(LogicOp::And),
                );
                let and7_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and7_name,
                    Some(LogicOp::And),
                );
                let and8_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and8_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and3_node,
                            port: 0,
                        },
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and4_node,
                            port: 0,
                        },
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: and5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[8].clone(), gate.inputs[9].clone()],
                    and6_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and6_node,
                            port: 0,
                        },
                        PinRef {
                            node: and7_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[10]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[10]),
                        },
                        PinRef {
                            node: and7_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and7_node,
                            port: 0,
                        },
                        PinRef {
                            node: and8_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[11]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[11]),
                        },
                        PinRef {
                            node: and8_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and5_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and8_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI444" => {
                let or0_name = format!("{}__bench_oai444_or0", gate.output);
                let or1_name = format!("{}__bench_oai444_or1", gate.output);
                let or2_name = format!("{}__bench_oai444_or2", gate.output);
                let or3_name = format!("{}__bench_oai444_or3", gate.output);
                let or4_name = format!("{}__bench_oai444_or4", gate.output);
                let or5_name = format!("{}__bench_oai444_or5", gate.output);
                let or6_name = format!("{}__bench_oai444_or6", gate.output);
                let or7_name = format!("{}__bench_oai444_or7", gate.output);
                let or8_name = format!("{}__bench_oai444_or8", gate.output);
                let and0_name = format!("{}__bench_oai444_and0", gate.output);
                let and1_name = format!("{}__bench_oai444_and1", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let or3_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or3_name,
                    Some(LogicOp::Or),
                );
                let or4_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or4_name,
                    Some(LogicOp::Or),
                );
                let or5_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or5_name,
                    Some(LogicOp::Or),
                );
                let or6_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or6_name,
                    Some(LogicOp::Or),
                );
                let or7_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or7_name,
                    Some(LogicOp::Or),
                );
                let or8_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or8_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[2]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[2]),
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[3]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[3]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or3_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or3_node,
                            port: 0,
                        },
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[6]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or4_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or4_node,
                            port: 0,
                        },
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[7]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[7]),
                        },
                        PinRef {
                            node: or5_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[8].clone(), gate.inputs[9].clone()],
                    or6_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or6_node,
                            port: 0,
                        },
                        PinRef {
                            node: or7_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[10]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[10]),
                        },
                        PinRef {
                            node: or7_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or7_node,
                            port: 0,
                        },
                        PinRef {
                            node: or8_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: signal_driver[&gate.inputs[11]],
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[11]),
                        },
                        PinRef {
                            node: or8_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or5_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or8_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "AOI2221" => {
                let and0_name = format!("{}__bench_aoi2221_and0", gate.output);
                let and1_name = format!("{}__bench_aoi2221_and1", gate.output);
                let and2_name = format!("{}__bench_aoi2221_and2", gate.output);
                let or0_name = format!("{}__bench_aoi2221_or0", gate.output);
                let or1_name = format!("{}__bench_aoi2221_or1", gate.output);
                let or2_name = format!("{}__bench_aoi2221_or2", gate.output);

                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    and0_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    and1_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    and2_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: or1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let seventh_input =
                    signal_driver.get(&gate.inputs[6]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[6]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: seventh_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: or2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            "OAI2221" => {
                let or0_name = format!("{}__bench_oai2221_or0", gate.output);
                let or1_name = format!("{}__bench_oai2221_or1", gate.output);
                let or2_name = format!("{}__bench_oai2221_or2", gate.output);
                let and0_name = format!("{}__bench_oai2221_and0", gate.output);
                let and1_name = format!("{}__bench_oai2221_and1", gate.output);
                let and2_name = format!("{}__bench_oai2221_and2", gate.output);

                let or0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or0_name,
                    Some(LogicOp::Or),
                );
                let or1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or1_name,
                    Some(LogicOp::Or),
                );
                let or2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &or2_name,
                    Some(LogicOp::Or),
                );
                let and0_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and0_name,
                    Some(LogicOp::And),
                );
                let and1_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and1_name,
                    Some(LogicOp::And),
                );
                let and2_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &and2_name,
                    Some(LogicOp::And),
                );
                let not_node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(LogicOp::Not),
                );

                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[0].clone(), gate.inputs[1].clone()],
                    or0_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[2].clone(), gate.inputs[3].clone()],
                    or1_node,
                )?;
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &[gate.inputs[4].clone(), gate.inputs[5].clone()],
                    or2_node,
                )?;
                netlist
                    .connect(
                        PinRef {
                            node: or0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and0_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and0_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: or2_node,
                            port: 0,
                        },
                        PinRef {
                            node: and1_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                let seventh_input =
                    signal_driver.get(&gate.inputs[6]).copied().ok_or_else(|| {
                        IoError::BenchParse(format!(
                            "signal '{}' used before definition",
                            gate.inputs[6]
                        ))
                    })?;
                netlist
                    .connect(
                        PinRef {
                            node: and1_node,
                            port: 0,
                        },
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: seventh_input,
                            port: alloc_bench_output_port(&mut next_output_port, &gate.inputs[6]),
                        },
                        PinRef {
                            node: and2_node,
                            port: 1,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                netlist
                    .connect(
                        PinRef {
                            node: and2_node,
                            port: 0,
                        },
                        PinRef {
                            node: not_node,
                            port: 0,
                        },
                    )
                    .map_err(|error| IoError::BenchParse(error.to_string()))?;
                not_node
            }
            _ => {
                let logic_op = bench_logic_op(&gate.op).ok_or_else(|| {
                    IoError::BenchParse(format!(
                        "unsupported gate op '{}'; supported ops are AND/OR/XOR/XNOR/NOT/NAND/NOR/BUF/BUFF/MUX/DFF/MAJ/AOI21/OAI21/AOI22/OAI22/AOI31/OAI31/AOI211/OAI211/AOI311/OAI311/AOI321/OAI321/AOI221/OAI221/AOI222/OAI222/AOI322/OAI322/AOI421/OAI421/AOI422/OAI422/AOI431/OAI431/AOI432/OAI432/AOI433/OAI433/AOI441/OAI441/AOI442/OAI442/AOI443/OAI443/AOI444/OAI444/AOI2221/OAI2221",
                        gate.op
                    ))
                })?;
                let node_id = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    &gate.output,
                    Some(logic_op),
                );
                connect_bench_gate_inputs(
                    &mut netlist,
                    &mut next_output_port,
                    &signal_driver,
                    &gate.inputs,
                    node_id,
                )?;
                node_id
            }
        };
        signal_driver.insert(gate.output, driver_node);
    }

    for output_name in output_names {
        let Some(&src_node) = signal_driver.get(&output_name.name) else {
            return Err(bench_parse_error_at_line(
                output_name.line,
                format!("output signal '{}' has no driver", output_name.name),
            ));
        };
        let output_node = netlist.add_node(NodeKind::Port, &output_name.name);
        let src_pin = PinRef {
            node: src_node,
            port: alloc_bench_output_port(&mut next_output_port, &output_name.name),
        };
        let dst_pin = PinRef {
            node: output_node,
            port: 0,
        };
        netlist
            .connect(src_pin, dst_pin)
            .map_err(|error| IoError::BenchParse(error.to_string()))?;
    }

    Ok(netlist)
}

fn order_bench_gates(
    gates: Vec<BenchGateSpec>,
    input_names: &[BenchNamedSignal],
) -> Result<Vec<BenchGateSpec>, IoError> {
    let input_set = input_names
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<HashSet<_>>();
    let mut gate_outputs = HashSet::new();
    for gate in &gates {
        if input_set.contains(&gate.output) {
            return Err(bench_parse_error_at_line(
                gate.line,
                format!("signal '{}' defined more than once", gate.output),
            ));
        }
        if !gate_outputs.insert(gate.output.clone()) {
            return Err(bench_parse_error_at_line(
                gate.line,
                format!("signal '{}' defined more than once", gate.output),
            ));
        }
    }

    for gate in &gates {
        for input in &gate.inputs {
            if !input_names.iter().any(|name| &name.name == input) && !gate_outputs.contains(input)
            {
                return Err(bench_parse_error_at_line(
                    gate.line,
                    format!("signal '{input}' used before definition"),
                ));
            }
        }
    }

    let mut known_signals = input_names
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<HashSet<_>>();
    let mut remaining = gates.into_iter().map(Some).collect::<Vec<_>>();
    let mut ordered = Vec::new();

    while ordered.len() < remaining.len() {
        let mut progress = false;
        for gate in &mut remaining {
            let Some(candidate) = gate.as_ref() else {
                continue;
            };
            if candidate
                .inputs
                .iter()
                .all(|input| known_signals.contains(input))
            {
                let resolved = gate.take().expect("gate should exist");
                known_signals.insert(resolved.output.clone());
                ordered.push(resolved);
                progress = true;
            }
        }

        if !progress {
            let line = remaining
                .iter()
                .flatten()
                .map(|gate| gate.line)
                .min()
                .unwrap_or(0);
            return Err(bench_parse_error_at_line(
                line,
                "bench gate dependency cycle or self-reference detected",
            ));
        }
    }

    Ok(ordered)
}

fn ensure_unique_bench_signal_names(names: &[BenchNamedSignal]) -> Result<(), IoError> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(&name.name) {
            return Err(bench_parse_error_at_line(
                name.line,
                format!("signal '{}' defined more than once", name.name),
            ));
        }
    }
    Ok(())
}

fn parse_bench_decl<'a>(
    line: &'a str,
    keyword: &str,
    line_number: usize,
) -> Result<Option<&'a str>, IoError> {
    let Some((prefix, rest)) = line.split_once('(') else {
        return Ok(None);
    };
    if !prefix.trim().eq_ignore_ascii_case(keyword) {
        return Ok(None);
    }
    let Some(name) = rest.strip_suffix(')') else {
        return Err(bench_parse_error_at_line(
            line_number,
            format!("invalid {keyword} declaration: {line}"),
        ));
    };
    let name = name.trim();
    validate_bench_signal_name(name)?;
    Ok(Some(name))
}

fn parse_bench_gate(line: &str, line_number: usize) -> Result<BenchGateSpec, IoError> {
    let Some((lhs, rhs)) = line.split_once('=') else {
        return Err(bench_parse_error_at_line(
            line_number,
            format!("unsupported line format: {line}"),
        ));
    };
    let output = lhs.trim();
    validate_bench_signal_name(output)?;

    let rhs = rhs.trim();
    let Some(open_idx) = rhs.find('(') else {
        return Err(bench_parse_error_at_line(
            line_number,
            format!("unsupported line format: {line}"),
        ));
    };
    let Some(arg_text) = rhs.strip_suffix(')') else {
        return Err(bench_parse_error_at_line(
            line_number,
            format!("unsupported line format: {line}"),
        ));
    };
    let op = arg_text[..open_idx].trim();
    validate_bench_op_name(op)?;

    let args = arg_text[open_idx + 1..]
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            validate_bench_signal_name(segment)?;
            Ok(segment.to_string())
        })
        .collect::<Result<Vec<_>, IoError>>()?;

    Ok(BenchGateSpec {
        output: output.to_string(),
        op: op.to_ascii_uppercase(),
        inputs: args,
        line: line_number,
    })
}

fn validate_bench_signal_name(identifier: &str) -> Result<(), IoError> {
    if identifier.is_empty() {
        return Err(bench_parse_error("empty identifier"));
    }
    if identifier
        .chars()
        .any(|ch| ch.is_ascii_whitespace() || matches!(ch, '(' | ')' | ',' | '=' | '#'))
    {
        return Err(bench_parse_error(format!(
            "invalid identifier '{identifier}'"
        )));
    }
    Ok(())
}

fn validate_bench_op_name(identifier: &str) -> Result<(), IoError> {
    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        return Err(bench_parse_error("empty identifier"));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(bench_parse_error(format!(
            "invalid identifier '{identifier}'"
        )));
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return Err(bench_parse_error(format!(
            "invalid identifier '{identifier}'"
        )));
    }
    Ok(())
}

fn connect_bench_gate_inputs(
    netlist: &mut Netlist,
    next_output_port: &mut HashMap<String, u16>,
    signal_driver: &HashMap<String, rflux_ir::NodeId>,
    inputs: &[String],
    target_node: rflux_ir::NodeId,
) -> Result<(), IoError> {
    for (port, input_signal) in inputs.iter().enumerate() {
        let Some(&src_node) = signal_driver.get(input_signal) else {
            return Err(IoError::BenchParse(format!(
                "signal '{input_signal}' used before definition"
            )));
        };
        let src_pin = PinRef {
            node: src_node,
            port: alloc_bench_output_port(next_output_port, input_signal),
        };
        let dst_pin = PinRef {
            node: target_node,
            port: port as u16,
        };
        netlist
            .connect(src_pin, dst_pin)
            .map_err(|error| IoError::BenchParse(error.to_string()))?;
    }
    Ok(())
}

fn bench_logic_op(op: &str) -> Option<LogicOp> {
    match op {
        "AND" => Some(LogicOp::And),
        "OR" => Some(LogicOp::Or),
        "XOR" => Some(LogicOp::Xor),
        "NOT" => Some(LogicOp::Not),
        "BUF" | "BUFF" => Some(LogicOp::Buf),
        "MUX" => Some(LogicOp::Mux2),
        _ => None,
    }
}

fn bench_expected_inputs(op: &str) -> Option<usize> {
    match op {
        "BUF" | "BUFF" | "NOT" => Some(1),
        "MUX" | "MAJ" | "AOI21" | "OAI21" | "DFFE" => Some(3),
        "AOI22" | "OAI22" | "AOI31" | "OAI31" | "AOI211" | "OAI211" => Some(4),
        "AOI311" | "OAI311" | "AOI221" | "OAI221" => Some(5),
        "AOI321" | "OAI321" | "AOI222" | "OAI222" => Some(6),
        "AOI322" | "OAI322" | "AOI421" | "OAI421" | "AOI2221" | "OAI2221" => Some(7),
        "AOI422" | "OAI422" | "AOI431" | "OAI431" => Some(8),
        "AOI432" | "OAI432" | "AOI441" | "OAI441" => Some(9),
        "AOI433" | "OAI433" | "AOI442" | "OAI442" => Some(10),
        "AOI443" | "OAI443" => Some(11),
        "AOI444" | "OAI444" => Some(12),
        "AND" | "OR" | "XOR" | "XNOR" | "NAND" | "NOR" | "DFF" => Some(2),
        _ => None,
    }
}

fn alloc_bench_output_port(next_output_port: &mut HashMap<String, u16>, signal: &str) -> u16 {
    let entry = next_output_port.entry(signal.to_string()).or_insert(0);
    let current = *entry;
    *entry += 1;
    current
}

pub fn read_lef(path: impl AsRef<Path>) -> Result<LEF, IoError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    read_lef_bytes(&mut reader).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn read_def(path: impl AsRef<Path>) -> Result<DEF, IoError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    read_def_bytes(&mut reader).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn read_lef_to_chip(path: impl AsRef<Path>) -> Result<Chip, IoError> {
    let lef = read_lef(path)?;
    lef_to_db::<Chip, i32>(&lef).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn import_def_into_chip(
    def_path: impl AsRef<Path>,
    lef: Option<&LEF>,
    chip: &mut Chip,
) -> Result<(), IoError> {
    let def = read_def(def_path)?;
    let options = DEFImportOptions::<Chip>::default();
    import_def_into_db::<Chip, i32>(&options, lef, &def, chip)
        .map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn read_lef_def_to_chip(
    lef_path: impl AsRef<Path>,
    def_path: impl AsRef<Path>,
) -> Result<Chip, IoError> {
    let lef = read_lef(lef_path)?;
    let mut chip = lef_to_db::<Chip, i32>(&lef).map_err(|e| IoError::LefDef(e.to_string()))?;
    import_def_into_chip(def_path, Some(&lef), &mut chip)?;
    Ok(chip)
}

pub fn write_def_from_chip(
    path: impl AsRef<Path>,
    chip: &Chip,
    top_cell_name: Option<&str>,
) -> Result<(), IoError> {
    let top_cell = if let Some(name) = top_cell_name {
        chip.cell_by_name(name).ok_or(IoError::TopCellNotFound)?
    } else {
        chip.each_cell()
            .find(|c| chip.num_cell_references(c) == 0)
            .ok_or(IoError::TopCellNotFound)?
    };

    let options = DEFExportOptions::<Chip>::default();
    let mut def = DEF::default();
    export_db_to_def::<Chip, i32>(&options, chip, &top_cell, &mut def)
        .map_err(|e| IoError::LefDef(e.to_string()))?;

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    write_def(&mut writer, &def).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn write_lef_from_chip(_path: impl AsRef<Path>, _chip: &Chip) -> Result<(), IoError> {
    Err(IoError::LefWriteUnsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rflux_ir::NodeKind;

    fn unique_test_path(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        env::temp_dir().join(format!("rflux-io-{name}-{stamp}.json"))
    }

    #[test]
    fn write_ir_json_wraps_payload_with_schema_metadata() {
        let path = unique_test_path("ir-envelope");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "in");

        write_ir_json(&path, &netlist).expect("ir json should write");
        let raw = fs::read_to_string(&path).expect("ir json should exist");
        let json: Value = serde_json::from_str(&raw).expect("ir json should parse");

        assert_eq!(json["schema_version"], IR_JSON_SCHEMA_VERSION);
        assert_eq!(json["kind"], IR_JSON_KIND);
        assert!(json.get("payload").is_some());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_ir_json_accepts_legacy_unversioned_payload() {
        let path = unique_test_path("ir-legacy");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "legacy");
        fs::write(
            &path,
            serde_json::to_string_pretty(&netlist).expect("legacy netlist should serialize"),
        )
        .expect("legacy ir json should write");

        let loaded = read_ir_json(&path).expect("legacy ir json should remain readable");

        assert_eq!(loaded.node_count(), 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_ir_json_rejects_unsupported_schema_version() {
        let path = unique_test_path("ir-bad-version");
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 99,
                "kind": IR_JSON_KIND,
                "payload": Netlist::new(),
            }))
            .expect("bad ir json should serialize"),
        )
        .expect("bad ir json should write");

        let error = read_ir_json(&path).expect_err("unsupported schema version should be rejected");

        assert!(error
            .to_string()
            .contains("unsupported rflux_ir_netlist schema version 99"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_ir_json_reports_line_and_column_for_malformed_json() {
        let path = unique_test_path("ir-malformed-json-location");
        fs::write(&path, "{\n  \"nodes\": [\n    \n").expect("malformed ir json should write");

        let error = read_ir_json(&path).expect_err("malformed json should be rejected");

        assert!(error.to_string().contains("json parse error: at line"));
        assert!(error.to_string().contains("column"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn write_and_read_pdk_json_roundtrip_with_schema_metadata() {
        let path = unique_test_path("pdk-envelope");
        let pdk = Pdk::minimal("minimal-sfq");

        write_pdk_json(&path, &pdk).expect("pdk json should write");
        let raw = fs::read_to_string(&path).expect("pdk json should exist");
        let json: Value = serde_json::from_str(&raw).expect("pdk json should parse");
        let loaded = read_pdk_json(&path).expect("pdk json should roundtrip");

        assert_eq!(json["schema_version"], PDK_JSON_SCHEMA_VERSION);
        assert_eq!(json["kind"], PDK_JSON_KIND);
        assert_eq!(
            serde_json::to_value(&loaded).expect("loaded pdk should serialize"),
            serde_json::to_value(&pdk).expect("pdk should serialize")
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn unsupported_schema_version_maps_to_schema_error_code() {
        let error = IoError::UnsupportedSchemaVersion {
            kind: IR_JSON_KIND,
            version: 2,
        };

        assert_eq!(error.code(), "RFLOW-SCHEMA-001");
        assert!(error.suggestion().contains("Regenerate the file"));
    }

    #[test]
    fn invalid_json_envelope_maps_to_schema_contract_error_code() {
        let error = IoError::InvalidJsonEnvelope {
            kind: PDK_JSON_KIND,
            detail: "missing payload",
        };

        assert_eq!(error.code(), "RFLOW-SCHEMA-002");
        assert!(error
            .suggestion()
            .contains("schema_version, kind, and payload"));
    }

    #[test]
    fn read_bench_netlist_loads_supported_quaigh_subset() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nOUTPUT(y)\nmid = XOR(a, b)\ny = BUF(mid)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("bench netlist should load");

        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
        assert!(matches!(netlist.nodes()[2].logic_op, Some(LogicOp::Xor)));
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::Buf)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_netlist_auto_detects_bench_inputs() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-auto-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nOUTPUT(y)\ny = BUF(a)\n").expect("bench should write");

        assert_eq!(
            detect_netlist_input_format(&path),
            NetlistInputFormat::Bench
        );
        let netlist = read_netlist(&path).expect("bench netlist should auto-load");

        assert_eq!(netlist.node_count(), 3);
        assert_eq!(netlist.edge_count(), 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_netlist_defaults_to_ir_json_inputs() {
        let path = unique_test_path("netlist-auto-ir");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "in");
        write_ir_json(&path, &netlist).expect("ir json should write");

        assert_eq!(
            detect_netlist_input_format(&path),
            NetlistInputFormat::IrJson
        );
        let loaded = read_netlist(&path).expect("ir json should auto-load");

        assert_eq!(loaded.node_count(), 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_rejects_unsupported_gate_ops() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-bad-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nOUTPUT(y)\ny = XAND(a, a)\n").expect("bench should write");

        let error = read_bench_netlist(&path).expect_err("unsupported op should fail");

        assert!(error.to_string().contains("unsupported gate op 'XAND'"));
        assert_eq!(error.code(), "RFLOW-INPUT-002");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_reports_source_line_for_unsupported_gate_ops() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-bad-line-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nOUTPUT(y)\ny = XAND(a, a)\n").expect("bench should write");

        let error = read_bench_netlist(&path).expect_err("unsupported op should fail");

        assert!(error.to_string().contains("bench parse error: at line 3:"));
        assert!(error.to_string().contains("unsupported gate op 'XAND'"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_bracketed_signal_names() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-bracketed-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a[0])\nINPUT(b[0])\nsum[0] = XOR(a[0], b[0])\nOUTPUT(sum[0])\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("bracketed bench names should load");

        assert_eq!(netlist.node_count(), 4);
        assert_eq!(netlist.edge_count(), 3);
        assert_eq!(netlist.nodes()[0].name, "a[0]");
        assert_eq!(netlist.nodes()[1].name, "b[0]");
        assert_eq!(netlist.nodes()[2].name, "sum[0]");
        assert_eq!(netlist.nodes()[3].name, "sum[0]");
        assert!(matches!(netlist.nodes()[2].logic_op, Some(LogicOp::Xor)));
        assert!(matches!(netlist.nodes()[3].kind, NodeKind::Port));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_forward_gate_references() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-forward-ref-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\ny = BUF(n0)\nn0 = AND(a, b)\nOUTPUT(y)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("forward-referenced gates should load");

        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
        assert!(matches!(netlist.nodes()[2].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::Buf)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_rejects_gate_dependency_cycles() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-cycle-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nn0 = BUF(n1)\nn1 = BUF(n0)\nOUTPUT(n0)\n")
            .expect("bench should write");

        let error = read_bench_netlist(&path).expect_err("cycle should fail");

        assert!(error
            .to_string()
            .contains("bench gate dependency cycle or self-reference detected"));
        assert_eq!(error.code(), "RFLOW-INPUT-002");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_rejects_duplicate_gate_outputs() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-duplicate-output-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nn0 = AND(a, b)\nn0 = OR(a, b)\nOUTPUT(n0)\n",
        )
        .expect("bench should write");

        let error = read_bench_netlist(&path).expect_err("duplicate output should fail");

        assert!(error
            .to_string()
            .contains("signal 'n0' defined more than once"));
        assert_eq!(error.code(), "RFLOW-INPUT-002");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_rejects_duplicate_input_declarations() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-duplicate-input-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(a)\nOUTPUT(a)\n").expect("bench should write");

        let error = read_bench_netlist(&path).expect_err("duplicate INPUT should fail");

        assert!(error
            .to_string()
            .contains("signal 'a' defined more than once"));
        assert_eq!(error.code(), "RFLOW-INPUT-002");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_rejects_duplicate_output_declarations() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-duplicate-output-decl-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nOUTPUT(y)\nOUTPUT(y)\ny = BUF(a)\n")
            .expect("bench should write");

        let error = read_bench_netlist(&path).expect_err("duplicate OUTPUT should fail");

        assert!(error
            .to_string()
            .contains("signal 'y' defined more than once"));
        assert_eq!(error.code(), "RFLOW-INPUT-002");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_rejects_gate_outputs_that_redefine_inputs() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-input-redefinition-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\na = AND(a, b)\nOUTPUT(a)\n")
            .expect("bench should write");

        let error = read_bench_netlist(&path).expect_err("gate output should not redefine INPUT");

        assert!(error
            .to_string()
            .contains("signal 'a' defined more than once"));
        assert_eq!(error.code(), "RFLOW-INPUT-002");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_input_output_passthrough() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-passthrough-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nOUTPUT(a)\n").expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("bench passthrough should load");

        assert_eq!(netlist.node_count(), 2);
        assert_eq!(netlist.edge_count(), 1);
        assert!(matches!(netlist.nodes()[0].kind, NodeKind::Port));
        assert!(matches!(netlist.nodes()[1].kind, NodeKind::Port));
        assert_eq!(netlist.nodes()[0].name, "a");
        assert_eq!(netlist.nodes()[1].name, "a");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_dff_gate() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-dff-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(d)\nINPUT(clk)\nOUTPUT(q)\nq = DFF(d, clk)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("DFF bench should load");

        assert_eq!(netlist.node_count(), 4);
        assert_eq!(netlist.edge_count(), 3);
        assert!(matches!(netlist.nodes()[2].kind, NodeKind::Dff));
        assert_eq!(netlist.nodes()[2].name, "q");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_dffe_gate() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-dffe-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(d)\nINPUT(en)\nINPUT(clk)\nOUTPUT(q)\nq = DFFE(d, en, clk)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("DFFE bench should load");

        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
        assert!(matches!(netlist.nodes()[3].kind, NodeKind::Dff));
        assert!(matches!(
            netlist.nodes()[3].logic_op,
            Some(LogicOp::DffEnable)
        ));
        assert_eq!(netlist.nodes()[3].name, "q");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_not_gate() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-not-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nOUTPUT(y)\ninv = NOT(a)\ny = BUF(inv)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("NOT gate bench should load");

        assert_eq!(netlist.node_count(), 4);
        assert_eq!(netlist.edge_count(), 3);
        assert!(matches!(netlist.nodes()[1].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_nand_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-nand-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nOUTPUT(y)\ny = NAND(a, b)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("NAND bench should load");

        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
        assert!(matches!(netlist.nodes()[2].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_nor_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-nor-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nOUTPUT(y)\ny = NOR(a, b)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("NOR bench should load");

        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
        assert!(matches!(netlist.nodes()[2].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_xnor_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-xnor-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nOUTPUT(y)\ny = XNOR(a, b)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("XNOR bench should load");

        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
        assert!(matches!(netlist.nodes()[2].logic_op, Some(LogicOp::Xor)));
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_maj_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-maj-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nOUTPUT(y)\ny = MAJ(a, b, c)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("MAJ bench should load");

        assert_eq!(netlist.node_count(), 9);
        assert_eq!(netlist.edge_count(), 11);
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi21_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi21-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nOUTPUT(y)\ny = AOI21(a, b, c)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI21 bench should load");

        assert_eq!(netlist.node_count(), 7);
        assert_eq!(netlist.edge_count(), 6);
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai21_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai21-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nOUTPUT(y)\ny = OAI21(a, b, c)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI21 bench should load");

        assert_eq!(netlist.node_count(), 7);
        assert_eq!(netlist.edge_count(), 6);
        assert!(matches!(netlist.nodes()[3].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi22_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi22-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nOUTPUT(y)\ny = AOI22(a, b, c, d)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI22 bench should load");

        assert_eq!(netlist.node_count(), 9);
        assert_eq!(netlist.edge_count(), 8);
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai22_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai22-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nOUTPUT(y)\ny = OAI22(a, b, c, d)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI22 bench should load");

        assert_eq!(netlist.node_count(), 9);
        assert_eq!(netlist.edge_count(), 8);
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi31_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi31-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nOUTPUT(y)\ny = AOI31(a, b, c, d)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI31 bench should load");

        assert_eq!(netlist.node_count(), 9);
        assert_eq!(netlist.edge_count(), 8);
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai31_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai31-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nOUTPUT(y)\ny = OAI31(a, b, c, d)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI31 bench should load");

        assert_eq!(netlist.node_count(), 9);
        assert_eq!(netlist.edge_count(), 8);
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi211_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi211-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nOUTPUT(y)\ny = AOI211(a, b, c, d)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI211 bench should load");

        assert_eq!(netlist.node_count(), 9);
        assert_eq!(netlist.edge_count(), 8);
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai211_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai211-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nOUTPUT(y)\ny = OAI211(a, b, c, d)\n",
        )
        .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI211 bench should load");

        assert_eq!(netlist.node_count(), 9);
        assert_eq!(netlist.edge_count(), 8);
        assert!(matches!(netlist.nodes()[4].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi311_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi311-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nOUTPUT(y)\ny = AOI311(a, b, c, d, e)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI311 bench should load");

        assert_eq!(netlist.node_count(), 11);
        assert_eq!(netlist.edge_count(), 10);
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai311_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai311-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nOUTPUT(y)\ny = OAI311(a, b, c, d, e)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI311 bench should load");

        assert_eq!(netlist.node_count(), 11);
        assert_eq!(netlist.edge_count(), 10);
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi321_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi321-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nOUTPUT(y)\ny = AOI321(a, b, c, d, e, f)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI321 bench should load");

        assert_eq!(netlist.node_count(), 13);
        assert_eq!(netlist.edge_count(), 12);
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai321_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai321-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nOUTPUT(y)\ny = OAI321(a, b, c, d, e, f)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI321 bench should load");

        assert_eq!(netlist.node_count(), 13);
        assert_eq!(netlist.edge_count(), 12);
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi322_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi322-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nOUTPUT(y)\ny = AOI322(a, b, c, d, e, f, g)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI322 bench should load");

        assert_eq!(netlist.node_count(), 14);
        assert_eq!(netlist.edge_count(), 13);
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai322_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai322-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nOUTPUT(y)\ny = OAI322(a, b, c, d, e, f, g)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI322 bench should load");

        assert_eq!(netlist.node_count(), 14);
        assert_eq!(netlist.edge_count(), 13);
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi421_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi421-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nOUTPUT(y)\ny = AOI421(a, b, c, d, e, f, g)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI421 bench should load");

        assert_eq!(netlist.node_count(), 15);
        assert_eq!(netlist.edge_count(), 14);
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai421_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai421-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nOUTPUT(y)\ny = OAI421(a, b, c, d, e, f, g)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI421 bench should load");

        assert_eq!(netlist.node_count(), 15);
        assert_eq!(netlist.edge_count(), 14);
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi422_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi422-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nOUTPUT(y)\ny = AOI422(a, b, c, d, e, f, g, h)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI422 bench should load");

        assert_eq!(netlist.node_count(), 17);
        assert_eq!(netlist.edge_count(), 16);
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai422_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai422-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nOUTPUT(y)\ny = OAI422(a, b, c, d, e, f, g, h)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI422 bench should load");

        assert_eq!(netlist.node_count(), 17);
        assert_eq!(netlist.edge_count(), 16);
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi431_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi431-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nOUTPUT(y)\ny = AOI431(a, b, c, d, e, f, g, h)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI431 bench should load");

        assert_eq!(netlist.node_count(), 17);
        assert_eq!(netlist.edge_count(), 16);
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai431_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai431-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nOUTPUT(y)\ny = OAI431(a, b, c, d, e, f, g, h)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI431 bench should load");

        assert_eq!(netlist.node_count(), 17);
        assert_eq!(netlist.edge_count(), 16);
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi432_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi432-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nOUTPUT(y)\ny = AOI432(a, b, c, d, e, f, g, h, i)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI432 bench should load");

        assert_eq!(netlist.node_count(), 19);
        assert_eq!(netlist.edge_count(), 18);
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai432_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai432-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nOUTPUT(y)\ny = OAI432(a, b, c, d, e, f, g, h, i)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI432 bench should load");

        assert_eq!(netlist.node_count(), 19);
        assert_eq!(netlist.edge_count(), 18);
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi433_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi433-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nOUTPUT(y)\ny = AOI433(a, b, c, d, e, f, g, h, i, j)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI433 bench should load");

        assert_eq!(netlist.node_count(), 21);
        assert_eq!(netlist.edge_count(), 20);
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai433_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai433-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nOUTPUT(y)\ny = OAI433(a, b, c, d, e, f, g, h, i, j)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI433 bench should load");

        assert_eq!(netlist.node_count(), 21);
        assert_eq!(netlist.edge_count(), 20);
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi441_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi441-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nOUTPUT(y)\ny = AOI441(a, b, c, d, e, f, g, h, i)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI441 bench should load");

        assert_eq!(netlist.node_count(), 19);
        assert_eq!(netlist.edge_count(), 18);
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai441_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai441-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nOUTPUT(y)\ny = OAI441(a, b, c, d, e, f, g, h, i)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI441 bench should load");

        assert_eq!(netlist.node_count(), 19);
        assert_eq!(netlist.edge_count(), 18);
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi442_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi442-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nOUTPUT(y)\ny = AOI442(a, b, c, d, e, f, g, h, i, j)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI442 bench should load");

        assert_eq!(netlist.node_count(), 21);
        assert_eq!(netlist.edge_count(), 20);
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai442_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai442-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nOUTPUT(y)\ny = OAI442(a, b, c, d, e, f, g, h, i, j)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI442 bench should load");

        assert_eq!(netlist.node_count(), 21);
        assert_eq!(netlist.edge_count(), 20);
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi443_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi443-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\nOUTPUT(y)\ny = AOI443(a, b, c, d, e, f, g, h, i, j, k)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI443 bench should load");

        assert_eq!(netlist.node_count(), 23);
        assert_eq!(netlist.edge_count(), 22);
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[20].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[21].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai443_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai443-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\nOUTPUT(y)\ny = OAI443(a, b, c, d, e, f, g, h, i, j, k)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI443 bench should load");

        assert_eq!(netlist.node_count(), 23);
        assert_eq!(netlist.edge_count(), 22);
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[20].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[21].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi444_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi444-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\nINPUT(l)\nOUTPUT(y)\ny = AOI444(a, b, c, d, e, f, g, h, i, j, k, l)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI444 bench should load");

        assert_eq!(netlist.node_count(), 25);
        assert_eq!(netlist.edge_count(), 24);
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[20].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[21].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[22].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[23].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai444_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai444-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\nINPUT(l)\nOUTPUT(y)\ny = OAI444(a, b, c, d, e, f, g, h, i, j, k, l)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI444 bench should load");

        assert_eq!(netlist.node_count(), 25);
        assert_eq!(netlist.edge_count(), 24);
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[14].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[15].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[16].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[17].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[18].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[19].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[20].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[21].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[22].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[23].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi221_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi221-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nOUTPUT(y)\ny = AOI221(a, b, c, d, e)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI221 bench should load");

        assert_eq!(netlist.node_count(), 11);
        assert_eq!(netlist.edge_count(), 10);
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai221_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai221-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nOUTPUT(y)\ny = OAI221(a, b, c, d, e)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI221 bench should load");

        assert_eq!(netlist.node_count(), 11);
        assert_eq!(netlist.edge_count(), 10);
        assert!(matches!(netlist.nodes()[5].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi222_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi222-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nOUTPUT(y)\ny = AOI222(a, b, c, d, e, f)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI222 bench should load");

        assert_eq!(netlist.node_count(), 13);
        assert_eq!(netlist.edge_count(), 12);
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai222_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai222-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nOUTPUT(y)\ny = OAI222(a, b, c, d, e, f)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI222 bench should load");

        assert_eq!(netlist.node_count(), 13);
        assert_eq!(netlist.edge_count(), 12);
        assert!(matches!(netlist.nodes()[6].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_aoi2221_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-aoi2221-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nOUTPUT(y)\ny = AOI2221(a, b, c, d, e, f, g)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("AOI2221 bench should load");

        assert_eq!(netlist.node_count(), 15);
        assert_eq!(netlist.edge_count(), 14);
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_bench_netlist_supports_oai2221_lowering() {
        let path = env::temp_dir().join(format!(
            "rflux-io-bench-oai2221-{}.bench",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(&path, "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nOUTPUT(y)\ny = OAI2221(a, b, c, d, e, f, g)\n")
            .expect("bench should write");

        let netlist = read_bench_netlist(&path).expect("OAI2221 bench should load");

        assert_eq!(netlist.node_count(), 15);
        assert_eq!(netlist.edge_count(), 14);
        assert!(matches!(netlist.nodes()[7].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[8].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[9].logic_op, Some(LogicOp::Or)));
        assert!(matches!(netlist.nodes()[10].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[11].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[12].logic_op, Some(LogicOp::And)));
        assert!(matches!(netlist.nodes()[13].logic_op, Some(LogicOp::Not)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_blif_simple_and_gate() {
        let blif = r#".model top
.inputs a b
.outputs y
.names a b y
11 1
.end
"#;
        let (netlist, name) = parse_blif(blif).unwrap();
        assert_eq!(name, "top");
        assert!(netlist.node_count() >= 3);
    }

    #[test]
    fn parse_blif_with_latch() {
        let blif = r#".model top
.inputs d clk
.outputs q
.latch d q
.end
"#;
        let (netlist, _) = parse_blif(blif).unwrap();
        assert!(netlist.node_count() >= 3);
    }

    #[test]
    fn parse_blif_not_gate() {
        let blif = r#".model top
.inputs a
.outputs y
.names a y
0 1
.end
"#;
        let (netlist, _) = parse_blif(blif).unwrap();
        assert!(netlist.node_count() >= 2);
    }

    #[test]
    fn parse_blif_comment_and_blank_lines() {
        let blif = r#"# comment
.model top
.inputs a b
.outputs y

.names a b y
11 1
# another comment
.end
"#;
        let (netlist, name) = parse_blif(blif).unwrap();
        assert_eq!(name, "top");
        assert!(netlist.node_count() >= 3);
    }

    #[test]
    fn detect_blif_format_by_extension() {
        let path = Path::new("test.blif");
        assert_eq!(detect_netlist_input_format(path), NetlistInputFormat::Blif);
    }

    #[test]
    fn read_blif_from_file() {
        let path = env::temp_dir().join(format!(
            "rflux-io-blif-{}.blif",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            ".model top\n.inputs a b\n.outputs y\n.names a b y\n11 1\n.end\n",
        )
        .expect("blif should write");

        let (netlist, name) = read_blif(&path).expect("blif should load");
        assert_eq!(name, "top");
        assert!(netlist.node_count() >= 3);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_edif_simple() {
        let edif = r#"(edif my_design
  (library work
    (cell AND2
      (view netlist
        (viewType netlist)
        (interface
          (port (direction input) a)
          (port (direction input) b)
          (port (direction output) y)
        )
      )
    )
  )
)
"#;
        let (netlist, name) = parse_edif(edif).unwrap();
        assert_eq!(name, "my_design");
        assert_eq!(netlist.node_count(), 0);
    }

    #[test]
    fn parse_edif_with_instance() {
        let edif = r#"(edif test_design
  (library work
    (cell AND2
      (view netlist
        (viewType netlist)
        (interface
          (port (direction input) a)
          (port (direction input) b)
          (port (direction output) y)
        )
      )
    )
  )
  (design top
    (cellRef AND2 (libraryRef work))
    (instance g1 (cellRef AND2 (libraryRef work))
      (connect a (portRef a))
      (connect b (portRef b))
      (connect y (portRef y))
    )
  )
)
"#;
        let (netlist, name) = parse_edif(edif).unwrap();
        assert_eq!(name, "test_design");
        assert!(netlist.node_count() >= 1);
    }

    #[test]
    fn parse_edif_with_comments() {
        let edif = r#"; this is a comment
(edif my_design
  ; another comment
  (library work
    (cell BUF
      (view netlist
        (viewType netlist)
        (interface
          (port (direction input) a)
          (port (direction output) y)
        )
      )
    )
  )
)
"#;
        let (netlist, name) = parse_edif(edif).unwrap();
        assert_eq!(name, "my_design");
        assert_eq!(netlist.node_count(), 0);
    }

    #[test]
    fn detect_edif_format_by_extension() {
        assert_eq!(
            detect_netlist_input_format(Path::new("test.edif")),
            NetlistInputFormat::Edif
        );
        assert_eq!(
            detect_netlist_input_format(Path::new("test.edf")),
            NetlistInputFormat::Edif
        );
    }

    #[test]
    fn read_edif_from_file() {
        let path = env::temp_dir().join(format!(
            "rflux-io-edif-{}.edif",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            "(edif my_design\n  (library work\n    (cell AND2\n      (view netlist\n        (viewType netlist)\n        (interface\n          (port (direction input) a)\n          (port (direction input) b)\n          (port (direction output) y)\n        )\n      )\n    )\n  )\n)\n",
        )
        .expect("edif should write");

        let (netlist, name) = read_edif(&path).expect("edif should load");
        assert_eq!(name, "my_design");
        assert_eq!(netlist.node_count(), 0);

        let _ = fs::remove_file(path);
    }
}
