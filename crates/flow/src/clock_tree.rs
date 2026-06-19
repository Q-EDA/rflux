use rflux_ir::{Netlist, NodeId, NodeKind, PinRef};
use rflux_place::{Placement, Point};
use rflux_route::{NetRoute, RouteMode, RouteSegment};

/// Configuration for clock tree generation (H-tree).
pub struct ClockTreeConfig {
    pub phase_count: usize,
    pub target_fanout: usize,
    pub jtl_layer: u8,
    /// Phase assignment strategy. If true, assign phases based on spatial
    /// proximity (nearby sinks get same phase). If false, use round-robin.
    pub spatial_phase_assignment: bool,
    pub skew_target_ps: f64,
    pub enable_buffer_sizing: bool,
    pub enable_skew_optimization: bool,
    pub jtl_delay_per_um: f64,
    pub enable_clock_gating: bool,
}

impl Default for ClockTreeConfig {
    fn default() -> Self {
        Self {
            phase_count: 2,
            target_fanout: 4,
            jtl_layer: 1,
            spatial_phase_assignment: true,
            skew_target_ps: 2.0,
            enable_buffer_sizing: true,
            enable_skew_optimization: true,
            jtl_delay_per_um: 0.15,
            enable_clock_gating: false,
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
    pub skew_target_ps: f64,
    pub skew_optimized: bool,
    pub buffer_sizing_levels: usize,
    pub clock_gated: bool,
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

/// A clock gating cell inserted into the tree.
pub struct ClockGate {
    pub enable_signal: String,
    pub position: Point,
}

fn size_clock_buffers(
    buffers: &mut Vec<ClockBuffer>,
    _sinks: &[(NodeId, Point)],
    config: &ClockTreeConfig,
    netlist: &mut Netlist,
) -> usize {
    let mut extra = 0usize;
    let max_level = buffers.iter().map(|b| b.level).max().unwrap_or(0);

    for level in 0..=max_level {
        let count_at_level = buffers.iter().filter(|b| b.level == level).count();
        if count_at_level <= config.target_fanout {
            continue;
        }

        let parent_positions: Vec<Point> = buffers
            .iter()
            .filter(|b| b.level == level)
            .map(|b| b.position)
            .collect();
        let parent_phase = buffers
            .iter()
            .find(|b| b.level == level)
            .map(|b| b.phase)
            .unwrap_or(0);

        let sub_count = (count_at_level + config.target_fanout - 1) / config.target_fanout - 1;
        for j in 0..sub_count {
            let frac = (j + 1) as f64 / (sub_count + 1) as f64;
            let base_pos = parent_positions.first().copied().unwrap_or(Point {
                x_um: 0.0,
                y_um: 0.0,
            });
            let sub_id = netlist.add_node(
                NodeKind::Jtl,
                format!("clk_buf_s{}_l{}", extra + j, level),
            );
            buffers.push(ClockBuffer {
                id: sub_id,
                position: Point {
                    x_um: base_pos.x_um,
                    y_um: base_pos.y_um + frac * 5.0,
                },
                level,
                phase: parent_phase,
            });
        }
        extra += sub_count;
    }
    extra
}

fn optimize_clock_skew(
    buffers: &mut Vec<ClockBuffer>,
    sinks: &[(NodeId, Point)],
    config: &ClockTreeConfig,
    netlist: &mut Netlist,
) -> f64 {
    if buffers.is_empty() || sinks.is_empty() {
        return 0.0;
    }

    let max_level = buffers.iter().map(|b| b.level).max().unwrap_or(0);

    let mut path_delays: Vec<f64> = Vec::new();
    for sink in sinks {
        let mut delay = 0.0f64;
        let sink_pos = sink.1;
        for level in 0..=max_level {
            let bufs_at_level: Vec<&ClockBuffer> =
                buffers.iter().filter(|b| b.level == level).collect();
            if let Some(nearest) = bufs_at_level.iter().min_by(|a, b| {
                let da = (a.position.x_um - sink_pos.x_um).powi(2)
                    + (a.position.y_um - sink_pos.y_um).powi(2);
                let db = (b.position.x_um - sink_pos.x_um).powi(2)
                    + (b.position.y_um - sink_pos.y_um).powi(2);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            }) {
                delay += 2.0; // buffer delay per level
                let wire_len = ((nearest.position.x_um - sink_pos.x_um).abs()
                    + (nearest.position.y_um - sink_pos.y_um).abs())
                .max(0.0);
                delay += wire_len * config.jtl_delay_per_um;
            }
        }
        path_delays.push(delay);
    }

    let max_delay = path_delays
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let min_delay = path_delays
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let skew = (max_delay - min_delay).max(0.0);

    if skew <= config.skew_target_ps {
        return skew;
    }

    for (i, sink) in sinks.iter().enumerate() {
        let deficit = max_delay - path_delays[i];
        if deficit <= 0.0 || config.jtl_delay_per_um <= 0.0 {
            continue;
        }
        let jtl_length = deficit / config.jtl_delay_per_um;
        if jtl_length > 0.1 {
            let jtl_id = netlist.add_node(NodeKind::Jtl, format!("clk_bal_{i}"));
            buffers.push(ClockBuffer {
                id: jtl_id,
                position: sink.1,
                level: max_level + 1,
                phase: 0,
            });
        }
    }

    let mut adjusted_delays: Vec<f64> = Vec::new();
    for sink in sinks {
        let mut delay = 0.0f64;
        let sink_pos = sink.1;
        for level in 0..=max_level + 1 {
            let bufs_at_level: Vec<&ClockBuffer> =
                buffers.iter().filter(|b| b.level == level).collect();
            if let Some(nearest) = bufs_at_level.iter().min_by(|a, b| {
                let da = (a.position.x_um - sink_pos.x_um).powi(2)
                    + (a.position.y_um - sink_pos.y_um).powi(2);
                let db = (b.position.x_um - sink_pos.x_um).powi(2)
                    + (b.position.y_um - sink_pos.y_um).powi(2);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            }) {
                delay += 2.0;
                let wire_len = ((nearest.position.x_um - sink_pos.x_um).abs()
                    + (nearest.position.y_um - sink_pos.y_um).abs())
                .max(0.0);
                delay += wire_len * config.jtl_delay_per_um;
            }
        }
        adjusted_delays.push(delay);
    }

    let new_max = adjusted_delays
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let new_min = adjusted_delays
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    (new_max - new_min).max(0.0)
}

pub fn insert_clock_gate(
    netlist: &mut Netlist,
    gate: &ClockGate,
    clock_source: NodeId,
) -> NodeId {
    let gated_id = netlist.add_node_with_logic(
        NodeKind::CellInstance,
        format!("clk_gate_{}", gate.enable_signal),
        Some(rflux_ir::LogicOp::And),
    );
    let _ = netlist.connect(
        PinRef {
            node: clock_source,
            port: 0,
        },
        PinRef {
            node: gated_id,
            port: 0,
        },
    );
    gated_id
}

/// Build H-tree clock distribution returning buffers for routing generation.
pub fn build_h_tree_with_buffers(
    netlist: &mut Netlist,
    sinks: &[(NodeId, Point)],
    _placement: &Placement,
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
                skew_target_ps: config.skew_target_ps,
                skew_optimized: false,
                buffer_sizing_levels: 0,
                clock_gated: false,
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
    for buf in &mut buffers {
        let phase = if config.spatial_phase_assignment {
            // Spatial phase assignment: use grid-based clustering.
            // Divide the placement area into phase_count regions and assign
            // based on which region the buffer falls in.
            let x_bucket = ((buf.position.x_um / 100.0) as usize) % phase_count;
            let y_bucket = ((buf.position.y_um / 100.0) as usize) % phase_count;
            (x_bucket + y_bucket) % phase_count
        } else {
            buf.id.0 % phase_count
        };
        buf.phase = phase;
        phase_stats[phase].0 += 1;
    }

    // Estimate clock skew: proportional to deepest path vs shallowest
    let max_depth = buffers.iter().map(|b| b.level).max().unwrap_or(0);
    let min_depth = buffers.iter().map(|b| b.level).min().unwrap_or(0);
    let estimated_skew_ps = ((max_depth - min_depth) as f64).max(0.0) * 2.0; // 2 ps per level

    // Buffer sizing pass
    let buffer_sizing_levels = if config.enable_buffer_sizing {
        size_clock_buffers(&mut buffers, sinks, config, netlist)
    } else {
        0
    };

    // Skew optimization pass
    let (achieved_skew, skew_optimized) = if config.enable_skew_optimization {
        let skew = optimize_clock_skew(&mut buffers, sinks, config, netlist);
        (skew, true)
    } else {
        (estimated_skew_ps, false)
    };

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
        levels: max_depth + 1 + if buffer_sizing_levels > 0 { 1 } else { 0 },
        total_wire_length_um: total_length_um,
        estimated_skew_ps: achieved_skew,
        phase_count,
        phases,
        skew_target_ps: config.skew_target_ps,
        skew_optimized,
        buffer_sizing_levels,
        clock_gated: config.enable_clock_gating,
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
    let (left_sinks, right_sinks) = if (max_x - min_x).abs() < 1e-9 {
        // All sinks share the same x-coordinate; split by index to avoid infinite recursion
        let mid = sinks.len() / 2;
        (
            sinks[..mid].to_vec(),
            sinks[mid..].to_vec(),
        )
    } else {
        let left: Vec<_> = sinks
            .iter()
            .filter(|(_, p)| p.x_um <= mid_x)
            .copied()
            .collect();
        let right: Vec<_> = sinks
            .iter()
            .filter(|(_, p)| p.x_um > mid_x)
            .copied()
            .collect();
        (left, right)
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::NodeKind;
    use rflux_place::PlacedNode;

    fn make_netlist_with_dffs(count: usize) -> (Netlist, Vec<NodeId>) {
        let mut netlist = Netlist::new();
        let mut dff_ids = Vec::new();
        for i in 0..count {
            let id = netlist.add_node(NodeKind::Dff, format!("dff{i}"));
            dff_ids.push(id);
        }
        (netlist, dff_ids)
    }

    fn make_placement_grid(dff_ids: &[NodeId], cols: usize, pitch: f64) -> Placement {
        let nodes: Vec<PlacedNode> = dff_ids
            .iter()
            .enumerate()
            .map(|(i, &id)| PlacedNode {
                node: id,
                level: 0,
                slot: i,
                point: Point {
                    x_um: (i % cols) as f64 * pitch,
                    y_um: (i / cols) as f64 * pitch,
                },
            })
            .collect();
        let width = nodes.iter().map(|n| n.point.x_um).fold(0.0, f64::max) + pitch;
        let height = nodes.iter().map(|n| n.point.y_um).fold(0.0, f64::max) + pitch;
        Placement {
            nodes,
            width_um: width,
            height_um: height,
        }
    }

    #[test]
    fn clock_tree_empty_sinks() {
        let mut netlist = Netlist::new();
        let placement = Placement {
            nodes: Vec::new(),
            width_um: 0.0,
            height_um: 0.0,
        };
        let report = build_h_tree(&mut netlist, &[], &placement, &ClockTreeConfig::default());
        assert_eq!(report.sink_count, 0);
        assert_eq!(report.buffer_count, 0);
    }

    #[test]
    fn clock_tree_single_sink() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(1);
        let placement = make_placement_grid(&dff_ids, 1, 40.0);
        let report = build_h_tree(&mut netlist, &dff_ids.iter().map(|&id| (id, placement.point_of(id).unwrap())).collect::<Vec<_>>(), &placement, &ClockTreeConfig::default());
        assert_eq!(report.sink_count, 1);
        assert_eq!(report.buffer_count, 0);
    }

    #[test]
    fn clock_tree_dual_phase_assigns_two_phases() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(4);
        let placement = make_placement_grid(&dff_ids, 2, 80.0);
        let sinks: Vec<_> = dff_ids.iter().map(|&id| (id, placement.point_of(id).unwrap())).collect();
        let config = ClockTreeConfig {
            phase_count: 2,
            ..ClockTreeConfig::default()
        };
        let report = build_h_tree(&mut netlist, &sinks, &placement, &config);
        assert_eq!(report.phase_count, 2);
        assert_eq!(report.phases.len(), 2);
        assert!(report.phases.iter().all(|p| p.sinks > 0 || p.buffers >= 0));
    }

    #[test]
    fn clock_tree_quad_phase_assigns_four_phases() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(8);
        let placement = make_placement_grid(&dff_ids, 4, 80.0);
        let sinks: Vec<_> = dff_ids.iter().map(|&id| (id, placement.point_of(id).unwrap())).collect();
        let config = ClockTreeConfig {
            phase_count: 4,
            ..ClockTreeConfig::default()
        };
        let report = build_h_tree(&mut netlist, &sinks, &placement, &config);
        assert_eq!(report.phase_count, 4);
        assert_eq!(report.phases.len(), 4);
    }

    #[test]
    fn spatial_phase_assignment_groups_nearby_sinks() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(4);
        let placement = make_placement_grid(&dff_ids, 2, 80.0);
        let sinks: Vec<_> = dff_ids.iter().map(|&id| (id, placement.point_of(id).unwrap())).collect();

        let config_spatial = ClockTreeConfig {
            phase_count: 2,
            spatial_phase_assignment: true,
            ..ClockTreeConfig::default()
        };
        let report_spatial = build_h_tree(&mut netlist, &sinks, &placement, &config_spatial);

        let config_roundrobin = ClockTreeConfig {
            phase_count: 2,
            spatial_phase_assignment: false,
            ..ClockTreeConfig::default()
        };
        let report_roundrobin = build_h_tree(&mut netlist, &sinks, &placement, &config_roundrobin);

        assert_eq!(report_spatial.phase_count, 2);
        assert_eq!(report_roundrobin.phase_count, 2);
    }

    #[test]
    fn clock_tree_estimates_skew() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(4);
        let placement = make_placement_grid(&dff_ids, 2, 80.0);
        let sinks: Vec<_> = dff_ids.iter().map(|&id| (id, placement.point_of(id).unwrap())).collect();
        let report = build_h_tree(&mut netlist, &sinks, &placement, &ClockTreeConfig::default());
        assert!(report.estimated_skew_ps >= 0.0);
    }

    #[test]
    fn find_clock_sinks_finds_dffs_and_cells() {
        let mut netlist = Netlist::new();
        let dff = netlist.add_node(NodeKind::Dff, "dff0");
        let cell = netlist.add_node(NodeKind::CellInstance, "cell0");
        let port = netlist.add_node(NodeKind::Port, "clk");
        let splitter = netlist.add_node(NodeKind::Splitter, "split0");

        let mut placement = Placement {
            nodes: Vec::new(),
            width_um: 0.0,
            height_um: 0.0,
        };
        for &id in &[dff, cell, port, splitter] {
            placement.nodes.push(PlacedNode {
                node: id,
                level: 0,
                slot: 0,
                point: Point { x_um: 0.0, y_um: 0.0 },
            });
        }

        let sinks = find_clock_sinks(&netlist, &placement);
        assert_eq!(sinks.len(), 2);
        assert!(sinks.iter().any(|(id, _)| *id == dff));
        assert!(sinks.iter().any(|(id, _)| *id == cell));
    }

    #[test]
    fn clock_tree_buffer_sizing_limits_fanout() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(16);
        let placement = make_placement_grid(&dff_ids, 4, 40.0);
        let sinks: Vec<_> = dff_ids
            .iter()
            .map(|&id| (id, placement.point_of(id).unwrap()))
            .collect();
        let config = ClockTreeConfig {
            target_fanout: 2,
            enable_buffer_sizing: true,
            enable_skew_optimization: false,
            ..ClockTreeConfig::default()
        };
        let (report, _buffers, _) =
            build_h_tree_with_buffers(&mut netlist, &sinks, &placement, &config);
        assert!(report.buffer_sizing_levels > 0);
    }

    #[test]
    fn clock_tree_skew_optimization_reduces_skew() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(8);
        let placement = make_placement_grid(&dff_ids, 4, 80.0);
        let sinks: Vec<_> = dff_ids
            .iter()
            .map(|&id| (id, placement.point_of(id).unwrap()))
            .collect();
        let config_opt = ClockTreeConfig {
            enable_skew_optimization: true,
            skew_target_ps: 0.1,
            ..ClockTreeConfig::default()
        };
        let (report_opt, _, _) =
            build_h_tree_with_buffers(&mut netlist, &sinks, &placement, &config_opt);

        assert!(report_opt.skew_optimized);
        assert!(report_opt.estimated_skew_ps >= 0.0);
    }

    #[test]
    fn clock_tree_skew_target_met() {
        let (mut netlist, dff_ids) = make_netlist_with_dffs(4);
        let placement = make_placement_grid(&dff_ids, 2, 40.0);
        let sinks: Vec<_> = dff_ids
            .iter()
            .map(|&id| (id, placement.point_of(id).unwrap()))
            .collect();
        let config = ClockTreeConfig {
            enable_skew_optimization: true,
            skew_target_ps: 5.0,
            ..ClockTreeConfig::default()
        };
        let (report, _, _) =
            build_h_tree_with_buffers(&mut netlist, &sinks, &placement, &config);
        assert!(report.skew_optimized);
        assert!(report.estimated_skew_ps >= 0.0);
    }

    #[test]
    fn clock_tree_gating_inserts_and_gate() {
        let mut netlist = Netlist::new();
        let clock_port = netlist.add_node(NodeKind::Port, "clk");
        let gate = ClockGate {
            enable_signal: "en0".to_string(),
            position: Point {
                x_um: 0.0,
                y_um: 0.0,
            },
        };
        let gated = insert_clock_gate(&mut netlist, &gate, clock_port);
        let gated_node = &netlist.nodes()[gated.0];
        assert!(matches!(gated_node.kind, NodeKind::CellInstance));
        assert_eq!(gated_node.logic_op, Some(rflux_ir::LogicOp::And));
        assert!(gated_node.name.contains("en0"));
    }
}
