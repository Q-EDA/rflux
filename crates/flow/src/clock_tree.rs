use rflux_ir::{Netlist, NodeId, NodeKind, PinRef};
use rflux_place::{Placement, Point};
use rflux_route::{NetRoute, RouteMode, RouteSegment};

/// Configuration for clock tree generation (H-tree).
pub struct ClockTreeConfig {
    pub phase_count: usize,
    pub target_fanout: usize,
    pub jtl_layer: u8,
}

impl Default for ClockTreeConfig {
    fn default() -> Self {
        Self {
            phase_count: 2,
            target_fanout: 4,
            jtl_layer: 1,
        }
    }
}

/// Per-phase breakdown in clock tree report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockPhaseReport {
    pub phase: usize,
    pub sinks: usize,
    pub buffers: usize,
}

/// Result of clock tree generation.
#[derive(Debug, Clone, PartialEq)]
pub struct ClockTreeReport {
    pub sink_count: usize,
    pub buffer_count: usize,
    pub levels: usize,
    pub total_wire_length_um: f64,
    pub estimated_skew_ps: f64,
    pub phase_count: usize,
    pub phases: Vec<ClockPhaseReport>,
}

/// A clock buffer node placed during H-tree generation.
#[derive(Debug, Clone, Copy)]
pub struct ClockBuffer {
    pub id: NodeId,
    pub position: Point,
    pub level: usize,
    pub phase: usize,
}

/// Identify all nodes that need a clock signal in SFQ.
pub fn find_clock_sinks(netlist: &Netlist, placement: &Placement) -> Vec<(NodeId, Point)> {
    netlist
        .nodes()
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Dff | NodeKind::CellInstance))
        .filter_map(|n| placement.point_of(n.id).map(|p| (n.id, p)))
        .collect()
}

/// Build an H-tree clock distribution network.
///
/// Places clock buffer nodes at H-tree split points, generates routing
/// segments between them, and assigns clock phases.
pub fn build_h_tree(
    netlist: &mut Netlist,
    sinks: &[(NodeId, Point)],
    placement: &Placement,
    config: &ClockTreeConfig,
) -> ClockTreeReport {
    let (report, _, _) = build_h_tree_with_buffers(netlist, sinks, placement, config);
    report
}

/// Build H-tree clock distribution returning buffers for routing generation.
pub fn build_h_tree_with_buffers(
    netlist: &mut Netlist,
    sinks: &[(NodeId, Point)],
    placement: &Placement,
    config: &ClockTreeConfig,
) -> (ClockTreeReport, Vec<ClockBuffer>, Vec<(NodeId, PinRef)>) {
    if sinks.is_empty() {
        return (
            ClockTreeReport {
                sink_count: 0,
                buffer_count: 0,
                levels: 0,
                total_wire_length_um: 0.0,
                estimated_skew_ps: 0.0,
                phase_count: config.phase_count,
                phases: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
    }

    let mut buffers: Vec<ClockBuffer> = Vec::new();
    let mut total_length_um = 0.0;
    let phase_count = config.phase_count.max(1);

    // Recursive H-tree builder
    build_h_tree_level(netlist, sinks, &mut buffers, &mut total_length_um, 0);

    // Assign phases to buffers and compute per-phase stats
    let mut phase_stats = vec![(0usize, 0usize); phase_count];
    for buf in &buffers {
        let phase = buf.id.0 % phase_count; // deterministic phase assignment
        phase_stats[phase].0 += 1;
    }

    // Estimate clock skew: proportional to deepest path vs shallowest
    let max_depth = buffers.iter().map(|b| b.level).max().unwrap_or(0);
    let min_depth = buffers.iter().map(|b| b.level).min().unwrap_or(0);
    let estimated_skew_ps = ((max_depth - min_depth) as f64).max(0.0) * 2.0; // 2 ps per level

    // Distribute sinks to phases
    let mut sink_phases = vec![0usize; phase_count];
    for (i, _) in sinks.iter().enumerate() {
        sink_phases[i % phase_count] += 1;
    }

    let phases: Vec<ClockPhaseReport> = (0..phase_count)
        .map(|p| ClockPhaseReport {
            phase: p,
            sinks: sink_phases[p],
            buffers: phase_stats[p].0,
        })
        .collect();

    let report = ClockTreeReport {
        sink_count: sinks.len(),
        buffer_count: buffers.len(),
        levels: max_depth + 1,
        total_wire_length_um: total_length_um,
        estimated_skew_ps,
        phase_count,
        phases,
    };

    // Collect driver pins for each buffer (for routing insertion)
    let clock_inputs = netlist
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            matches!(n.kind, NodeKind::Port) && n.name.to_lowercase().contains("clock")
        })
        .map(|(i, _)| {
            (
                NodeId(i),
                PinRef {
                    node: NodeId(i),
                    port: 0,
                },
            )
        })
        .collect::<Vec<_>>();

    (report, buffers, clock_inputs)
}

fn build_h_tree_level(
    netlist: &mut Netlist,
    sinks: &[(NodeId, Point)],
    buffers: &mut Vec<ClockBuffer>,
    total_length_um: &mut f64,
    level: usize,
) -> Option<NodeId> {
    if sinks.is_empty() {
        return None;
    }
    if sinks.len() <= 2 {
        // For 1-2 sinks, no buffer needed; return the sink node if exactly 1
        if sinks.len() == 1 {
            return Some(sinks[0].0);
        }
        return None;
    }

    // Find center of sinks
    let min_x = sinks
        .iter()
        .map(|(_, p)| p.x_um)
        .fold(f64::INFINITY, f64::min);
    let max_x = sinks
        .iter()
        .map(|(_, p)| p.x_um)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = sinks
        .iter()
        .map(|(_, p)| p.y_um)
        .fold(f64::INFINITY, f64::min);
    let max_y = sinks
        .iter()
        .map(|(_, p)| p.y_um)
        .fold(f64::NEG_INFINITY, f64::max);
    let center = Point {
        x_um: (min_x + max_x) / 2.0,
        y_um: (min_y + max_y) / 2.0,
    };

    // Insert clock buffer at center
    let buf_id = netlist.add_node(NodeKind::Jtl, format!("clk_buf_l{level}"));
    buffers.push(ClockBuffer {
        id: buf_id,
        position: center,
        level,
        phase: 0,
    });

    // Split sinks into left/right halves
    let mid_x = (min_x + max_x) / 2.0;
    let left_sinks: Vec<_> = sinks
        .iter()
        .filter(|(_, p)| p.x_um <= mid_x)
        .copied()
        .collect();
    let right_sinks: Vec<_> = sinks
        .iter()
        .filter(|(_, p)| p.x_um > mid_x)
        .copied()
        .collect();

    // Add wire length from center to both halves
    for group in [&left_sinks, &right_sinks] {
        if group.is_empty() {
            continue;
        }
        let g_min_x = group
            .iter()
            .map(|(_, p)| p.x_um)
            .fold(f64::INFINITY, f64::min);
        let g_max_x = group
            .iter()
            .map(|(_, p)| p.x_um)
            .fold(f64::NEG_INFINITY, f64::max);
        let g_center = (g_min_x + g_max_x) / 2.0;
        let wire_length = (center.x_um - g_center).abs();
        *total_length_um += wire_length;

        build_h_tree_level(netlist, group, buffers, total_length_um, level + 1);
    }

    Some(buf_id)
}

/// Return a set of routing segments for the clock tree.
pub fn clock_tree_routes(_report: &ClockTreeReport, buffers: &[ClockBuffer]) -> Vec<NetRoute> {
    if buffers.is_empty() {
        return Vec::new();
    }

    let mut routes = Vec::new();
    for pair in buffers.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let direct_length =
            (a.position.x_um - b.position.x_um).abs() + (a.position.y_um - b.position.y_um).abs();

        routes.push(NetRoute {
            from: PinRef {
                node: a.id,
                port: 0,
            },
            to: PinRef {
                node: b.id,
                port: 0,
            },
            mode: RouteMode::Jtl,
            segments: vec![RouteSegment {
                start: a.position,
                end: b.position,
                layer: 1,
            }],
            direct_length_um: direct_length,
            length_um: direct_length,
        });
    }
    routes
}
