use rflux_ir::{Netlist, NodeKind};
use rflux_place::Placement;

/// Configuration for bias distribution grid generation.
pub struct BiasGridConfig {
    /// Grid pitch in X direction (um).
    pub pitch_x_um: f64,
    /// Grid pitch in Y direction (um).
    pub pitch_y_um: f64,
    /// Width of each grid wire (um).
    pub wire_width_um: f64,
    /// Target bias voltage (V).
    pub bias_voltage_v: f64,
}

impl Default for BiasGridConfig {
    fn default() -> Self {
        Self {
            pitch_x_um: 40.0,
            pitch_y_um: 40.0,
            wire_width_um: 0.5,
            bias_voltage_v: 2.5,
        }
    }
}

/// Result of bias grid generation.
#[derive(Debug, Clone, PartialEq)]
pub struct BiasGridReport {
    /// Estimated number of grid cells.
    pub grid_cells: usize,
    /// Total wire length of the bias grid (um).
    pub total_wire_length_um: f64,
    /// Number of nodes connected to the grid.
    pub connected_nodes: usize,
    /// Estimated total bias current (mA).
    pub estimated_total_bias_current_ma: f64,
}

/// Identify nodes that need bias current (all active circuit elements).
pub fn find_bias_loads(netlist: &Netlist) -> Vec<(usize, f64)> {
    let mut loads = Vec::new();
    for (i, node) in netlist.nodes().iter().enumerate() {
        let bias_current_ua = match node.kind {
            NodeKind::CellInstance => 250.0, // typical gate: 250 uA
            NodeKind::MacroCell => 500.0,
            NodeKind::Splitter => 100.0,
            NodeKind::Dff => 300.0,
            NodeKind::Jtl => 150.0,
            NodeKind::Ptl => 50.0,
            NodeKind::Port => 0.0,
        };
        if bias_current_ua > 0.0 {
            loads.push((i, bias_current_ua));
        }
    }
    loads
}

/// Build a simple bias distribution grid over the placement area.
pub fn build_bias_grid(
    netlist: &Netlist,
    placement: &Placement,
    config: &BiasGridConfig,
) -> BiasGridReport {
    let loads = find_bias_loads(netlist);
    let connected_nodes = loads.len();

    // Count grid cells
    let grid_cells_x = (placement.width_um / config.pitch_x_um).ceil() as usize;
    let grid_cells_y = (placement.height_um / config.pitch_y_um).ceil() as usize;
    let grid_cells = grid_cells_x * grid_cells_y;

    // Estimate wire length: horizontal + vertical grid lines
    let h_lines = (grid_cells_y + 1) as f64 * placement.width_um;
    let v_lines = (grid_cells_x + 1) as f64 * placement.height_um;
    let total_wire_length_um = h_lines + v_lines;

    // Estimate total bias current
    let estimated_total_bias_current_ma = loads.iter().map(|(_, ua)| ua).sum::<f64>() / 1000.0;

    BiasGridReport {
        grid_cells,
        total_wire_length_um,
        connected_nodes,
        estimated_total_bias_current_ma,
    }
}
