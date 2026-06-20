//! WebAssembly bindings for rflux SFQ EDA toolkit
//!
//! This crate provides JavaScript/TypeScript bindings for rflux,
//! enabling SFQ circuit design, synthesis, placement, and routing
//! in web browsers or Node.js environments.

use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

// When the `console_error_panic_hook` feature is enabled, we can
// use `console_error_panic_hook` set the panic hook to log panics
// in the browser console.
#[cfg(feature = "console_error_panic_hook")]
fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

// When the feature isn't enabled, we just use an empty no-op.
#[cfg(not(feature = "console_error_panic_hook"))]
fn init_panic_hook() {}

/// Initialize rflux-wasm
///
/// Sets up panic hooks and logging.
#[wasm_bindgen(start)]
pub fn init() {
    init_panic_hook();
    #[cfg(feature = "console_log")]
    console_log::init_with_level(log::Level::Info).ok();
}

// =============================================================================
// Circuit builder
// =============================================================================

/// A SFQ circuit builder for use in JavaScript
#[wasm_bindgen]
pub struct Circuit {
    netlist: rflux_ir::Netlist,
}

#[wasm_bindgen]
impl Circuit {
    /// Create a new empty circuit
    #[wasm_bindgen(constructor)]
    pub fn new() -> Circuit {
        init_panic_hook();
        Circuit {
            netlist: rflux_ir::Netlist::new(),
        }
    }

    /// Add a node to the circuit
    ///
    /// Kind can be "port", "cell_instance", "macro_cell",
    /// "splitter", "dff", "jtl", or "ptl"
    pub fn add_node(&mut self, kind: &str, name: String, logic_op: Option<String>) -> usize {
        let node_kind = match kind {
            "port" => rflux_ir::NodeKind::Port,
            "cell_instance" | "cell" => rflux_ir::NodeKind::CellInstance,
            "macro_cell" | "macro" => rflux_ir::NodeKind::MacroCell,
            "splitter" => rflux_ir::NodeKind::Splitter,
            "dff" => rflux_ir::NodeKind::Dff,
            "jtl" => rflux_ir::NodeKind::Jtl,
            "ptl" => rflux_ir::NodeKind::Ptl,
            _ => panic!("unknown node kind: {}", kind),
        };

        let logic_op = logic_op.and_then(|op| match op.to_lowercase().as_str() {
            "buf" => Some(rflux_ir::LogicOp::Buf),
            "not" | "inv" => Some(rflux_ir::LogicOp::Not),
            "and" => Some(rflux_ir::LogicOp::And),
            "or" => Some(rflux_ir::LogicOp::Or),
            "xor" => Some(rflux_ir::LogicOp::Xor),
            "mux2" | "mux" => Some(rflux_ir::LogicOp::Mux2),
            "dffenable" | "dffe" => Some(rflux_ir::LogicOp::DffEnable),
            _ => None,
        });

        self.netlist.add_node_with_logic(node_kind, name, logic_op).0
    }

    /// Connect two nodes
    pub fn connect(&mut self, from_node: usize, from_port: u16, to_node: usize, to_port: u16) -> Result<(), JsValue> {
        let from = rflux_ir::PinRef {
            node: rflux_ir::NodeId(from_node),
            port: from_port,
        };
        let to = rflux_ir::PinRef {
            node: rflux_ir::NodeId(to_node),
            port: to_port,
        };

        self.netlist.connect(from, to)
            .map_err(|e| JsValue::from_str(&format!("Connection error: {}", e)))
    }

    /// Get the number of nodes
    pub fn node_count(&self) -> usize {
        self.netlist.node_count()
    }

    /// Get the number of edges
    pub fn edge_count(&self) -> usize {
        self.netlist.edge_count()
    }

    /// Serialize the circuit to JSON
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.netlist)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Deserialize a circuit from JSON
    pub fn from_json(json: &str) -> Result<Circuit, JsValue> {
        let netlist: rflux_ir::Netlist = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&format!("Deserialization error: {}", e)))?;
        Ok(Circuit { netlist })
    }
}

impl Default for Circuit {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Simple compile functions with JSON input/output
// =============================================================================

/// Compile options as JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileOptions {
    /// Clock period in picoseconds (default: 120 ps)
    #[serde(default = "default_clock_period")]
    pub clock_period_ps: f64,
}

fn default_clock_period() -> f64 { 120.0 }

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            clock_period_ps: 120.0,
        }
    }
}

/// Compile result as JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    /// Whether compilation succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(default)]
    pub error: Option<String>,
    /// Placement statistics
    #[serde(default)]
    pub placement: Option<PlacementStats>,
    /// Routing statistics
    #[serde(default)]
    pub routing: Option<RoutingStats>,
    /// Timing statistics
    #[serde(default)]
    pub timing: Option<TimingStats>,
}

/// Placement statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementStats {
    /// Number of placed nodes
    pub placed_nodes: usize,
    /// Layout width in micrometers
    pub width_um: f64,
    /// Layout height in micrometers
    pub height_um: f64,
}

/// Routing statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStats {
    /// Number of routed nets
    pub routed_nets: usize,
    /// Total wire length in micrometers
    pub total_length_um: f64,
    /// Number of JTL routes
    pub jtl_routes: usize,
    /// Number of PTL routes
    pub ptl_routes: usize,
}

/// Timing statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingStats {
    /// Worst setup slack in picoseconds
    pub worst_setup_slack_ps: f64,
    /// Worst hold slack in picoseconds
    pub worst_hold_slack_ps: f64,
    /// Critical path delay in picoseconds
    pub critical_path_delay_ps: f64,
    /// Whether timing closure was achieved
    pub timing_closed: bool,
}

/// Compile a circuit from JSON, returning JSON result
#[wasm_bindgen]
pub fn compile_json(circuit_json: &str, options_json: Option<String>) -> Result<String, JsValue> {
    let netlist: rflux_ir::Netlist = serde_json::from_str(circuit_json)
        .map_err(|e| JsValue::from_str(&format!("Circuit JSON parse error: {}", e)))?;

    let options = if let Some(json) = options_json {
        serde_json::from_str(&json)
            .map_err(|e| JsValue::from_str(&format!("Options JSON parse error: {}", e)))?
    } else {
        CompileOptions::default()
    };

    let pdk = rflux_tech::Pdk::minimal("wasm-flow-pdk");

    let mut config = rflux_flow::FlowConfig::default();
    config.timing.clock_period_ps = options.clock_period_ps;

    let mut runner = rflux_flow::FlowRunner::new();

    let result = runner.compile_layout(&mut netlist.clone(), &pdk, &config);

    let compile_result = match result {
        Ok(report) => CompileResult {
            success: true,
            error: None,
            placement: Some(PlacementStats {
                placed_nodes: report.placement.placed_nodes,
                width_um: report.placement.width_um,
                height_um: report.placement.height_um,
            }),
            routing: Some(RoutingStats {
                routed_nets: report.routing.routed_nets,
                total_length_um: report.routing.total_length_um,
                jtl_routes: report.routing.jtl_routes,
                ptl_routes: report.routing.ptl_routes,
            }),
            timing: Some(TimingStats {
                worst_setup_slack_ps: report.timing.worst_setup_slack_ps,
                worst_hold_slack_ps: report.timing.worst_hold_slack_ps,
                critical_path_delay_ps: report.timing.critical_path_delay_ps,
                timing_closed: report.timing_closure.closed,
            }),
        },
        Err(e) => CompileResult {
            success: false,
            error: Some(format!("{}", e)),
            placement: None,
            routing: None,
            timing: None,
        },
    };

    serde_json::to_string(&compile_result)
        .map_err(|e| JsValue::from_str(&format!("Result serialization error: {}", e)))
}

// =============================================================================
// Utilities
// =============================================================================

/// Get rflux version
#[wasm_bindgen]
pub fn version() -> String {
    "0.1.0".to_string()
}

/// Get a simple example circuit as JSON
#[wasm_bindgen]
pub fn example_circuit_json() -> String {
    let mut circuit = Circuit::new();

    // Create a simple XOR circuit
    let a = circuit.add_node("port", "a".to_string(), None);
    let b = circuit.add_node("port", "b".to_string(), None);
    let xor = circuit.add_node("cell", "xor0".to_string(), Some("xor".to_string()));
    let out = circuit.add_node("port", "out".to_string(), None);

    let _ = circuit.connect(a, 0, xor, 0);
    let _ = circuit.connect(b, 0, xor, 1);
    let _ = circuit.connect(xor, 0, out, 0);

    circuit.to_json().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_create_circuit() {
        let c = Circuit::new();
        assert_eq!(c.node_count(), 0);
        assert_eq!(c.edge_count(), 0);
    }

    #[test]
    fn can_add_nodes_and_connect() {
        let mut c = Circuit::new();
        let a = c.add_node("port", "a".to_string(), None);
        let b = c.add_node("port", "b".to_string(), None);
        assert_eq!(c.node_count(), 2);

        let result = c.connect(a, 0, b, 0);
        assert!(result.is_ok());
        assert_eq!(c.edge_count(), 1);
    }

    #[test]
    fn example_circuit_is_valid() {
        let json = example_circuit_json();
        let c = Circuit::from_json(&json);
        assert!(c.is_ok());
        let c = c.unwrap();
        assert_eq!(c.node_count(), 4);
        assert_eq!(c.edge_count(), 3);
    }
}
