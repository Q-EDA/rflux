use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rflux_ir::{Netlist, NodeId, NodeKind};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x_um: f64,
    pub y_um: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedNode {
    pub node: NodeId,
    pub level: usize,
    pub slot: usize,
    pub point: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub nodes: Vec<PlacedNode>,
    pub width_um: f64,
    pub height_um: f64,
}

impl Placement {
    #[must_use]
    pub fn point_of(&self, node: NodeId) -> Option<Point> {
        self.nodes
            .iter()
            .find(|placed| placed.node == node)
            .map(|placed| placed.point)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacementConfig {
    pub x_pitch_um: f64,
    pub y_pitch_um: f64,
    pub fixed_nodes: Vec<FixedNodePlacement>,
    pub blocked_regions: Vec<BlockedRegion>,
    pub macro_halo_x_um: f64,
    pub macro_halo_y_um: f64,
    pub max_nodes_per_level: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedNodePlacement {
    pub node: NodeId,
    pub point: Point,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockedRegion {
    pub min_x_um: f64,
    pub max_x_um: f64,
    pub min_y_um: f64,
    pub max_y_um: f64,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            x_pitch_um: 40.0,
            y_pitch_um: 24.0,
            fixed_nodes: Vec::new(),
            blocked_regions: Vec::new(),
            macro_halo_x_um: 40.0,
            macro_halo_y_um: 24.0,
            max_nodes_per_level: 0,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlaceError {
    #[error("placement requires an acyclic netlist")]
    Cycle,
}

impl PlaceError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            PlaceError::Cycle => "RFLOW-FLOW-002",
        }
    }

    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            PlaceError::Cycle => {
                "The netlist contains a cycle. Placement requires a directed acyclic graph."
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct LevelizedPlacer;

impl LevelizedPlacer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    pub fn place(
        &self,
        netlist: &Netlist,
        config: &PlacementConfig,
    ) -> Result<Placement, PlaceError> {
        let levels = compute_node_levels(netlist)?;
        let (indegree, outdegree) = degree_maps(netlist);
        let output_port_level = netlist
            .nodes()
            .iter()
            .filter(|node| {
                !(matches!(node.kind, NodeKind::Port)
                    && indegree[node.id.0] > 0
                    && outdegree[node.id.0] == 0)
            })
            .map(|node| levels[node.id.0])
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut fixed_reservations = fixed_slot_map(config);
        reserve_blocked_slots(config, &mut fixed_reservations);
        let mut occupied_slots = BTreeMap::<usize, BTreeSet<usize>>::new();
        let mut nodes_per_level = BTreeMap::<usize, usize>::new();
        let mut max_level = fixed_reservations.keys().copied().max().unwrap_or(0);
        for node in netlist.nodes() {
            let level = default_level_for_node(
                node.kind.clone(),
                node.id,
                &levels,
                &indegree,
                &outdegree,
                output_port_level,
            );
            max_level = max_level.max(level);
        }
        let mut next_slot_per_level = vec![0usize; max_level + 1];
        let mut nodes = Vec::with_capacity(netlist.node_count());
        let mut width_um = 0.0_f64;
        let mut height_um = 0.0_f64;

        for node in netlist.nodes() {
            let (level, slot) =
                if let Some((fixed_level, fixed_slot)) = find_fixed_slot(config, node.id) {
                    release_fixed_reservation(&mut fixed_reservations, fixed_level, fixed_slot);
                    let slot = legalize_fixed_slot(&mut occupied_slots, fixed_level, fixed_slot);
                    (fixed_level, slot)
                } else {
                    let min_level = default_level_for_node(
                        node.kind.clone(),
                        node.id,
                        &levels,
                        &indegree,
                        &outdegree,
                        output_port_level,
                    );
                    let level = legalize_level_for_congestion(config, &nodes_per_level, min_level);
                    ensure_level_capacity(&mut next_slot_per_level, level);
                    let slot = next_available_slot(
                        &fixed_reservations,
                        &mut occupied_slots,
                        &mut next_slot_per_level,
                        level,
                    );
                    (level, slot)
                };
            let point = Point {
                x_um: level as f64 * config.x_pitch_um,
                y_um: slot as f64 * config.y_pitch_um,
            };
            width_um = width_um.max(point.x_um);
            height_um = height_um.max(point.y_um);
            nodes.push(PlacedNode {
                node: node.id,
                level,
                slot,
                point,
            });
            *nodes_per_level.entry(level).or_default() += 1;
            if matches!(node.kind, NodeKind::MacroCell) {
                reserve_macro_halo(config, &mut fixed_reservations, point);
            }
        }

        Ok(Placement {
            nodes,
            width_um: if netlist.node_count() == 0 {
                0.0
            } else {
                width_um + config.x_pitch_um
            },
            height_um: if netlist.node_count() == 0 {
                0.0
            } else {
                height_um + config.y_pitch_um
            },
        })
    }
}

fn fixed_slot_map(config: &PlacementConfig) -> BTreeMap<usize, BTreeMap<usize, usize>> {
    let mut reserved_slots = BTreeMap::<usize, BTreeMap<usize, usize>>::new();
    for fixed in &config.fixed_nodes {
        let level = snap_to_grid(fixed.point.x_um, config.x_pitch_um);
        let slot = snap_to_grid(fixed.point.y_um, config.y_pitch_um);
        *reserved_slots
            .entry(level)
            .or_default()
            .entry(slot)
            .or_default() += 1;
    }
    reserved_slots
}

fn reserve_blocked_slots(
    config: &PlacementConfig,
    reserved_slots: &mut BTreeMap<usize, BTreeMap<usize, usize>>,
) {
    for region in &config.blocked_regions {
        reserve_region_slots(config, reserved_slots, *region);
    }
}

fn reserve_macro_halo(
    config: &PlacementConfig,
    reserved_slots: &mut BTreeMap<usize, BTreeMap<usize, usize>>,
    point: Point,
) {
    if config.macro_halo_x_um <= 0.0 && config.macro_halo_y_um <= 0.0 {
        return;
    }

    reserve_region_slots(
        config,
        reserved_slots,
        BlockedRegion {
            min_x_um: (point.x_um - config.macro_halo_x_um).max(0.0),
            max_x_um: point.x_um + config.macro_halo_x_um,
            min_y_um: (point.y_um - config.macro_halo_y_um).max(0.0),
            max_y_um: point.y_um + config.macro_halo_y_um,
        },
    );
}

fn reserve_region_slots(
    config: &PlacementConfig,
    reserved_slots: &mut BTreeMap<usize, BTreeMap<usize, usize>>,
    region: BlockedRegion,
) {
    let min_level = snap_to_grid(region.min_x_um, config.x_pitch_um);
    let max_level = snap_to_grid(region.max_x_um, config.x_pitch_um);
    let min_slot = snap_to_grid(region.min_y_um, config.y_pitch_um);
    let max_slot = snap_to_grid(region.max_y_um, config.y_pitch_um);

    for level in min_level..=max_level {
        for slot in min_slot..=max_slot {
            *reserved_slots
                .entry(level)
                .or_default()
                .entry(slot)
                .or_default() += 1;
        }
    }
}

fn find_fixed_slot(config: &PlacementConfig, node: NodeId) -> Option<(usize, usize)> {
    config
        .fixed_nodes
        .iter()
        .find(|fixed| fixed.node == node)
        .map(|fixed| {
            (
                snap_to_grid(fixed.point.x_um, config.x_pitch_um),
                snap_to_grid(fixed.point.y_um, config.y_pitch_um),
            )
        })
}

fn next_available_slot(
    fixed_reservations: &BTreeMap<usize, BTreeMap<usize, usize>>,
    occupied_slots: &mut BTreeMap<usize, BTreeSet<usize>>,
    next_slot_per_level: &mut [usize],
    level: usize,
) -> usize {
    let mut slot = next_slot_per_level[level];
    while occupied_slots
        .get(&level)
        .is_some_and(|slots| slots.contains(&slot))
        || fixed_reservations
            .get(&level)
            .and_then(|slots| slots.get(&slot))
            .copied()
            .unwrap_or(0)
            > 0
    {
        slot += 1;
    }
    occupied_slots.entry(level).or_default().insert(slot);
    next_slot_per_level[level] = slot + 1;
    slot
}

fn legalize_fixed_slot(
    occupied_slots: &mut BTreeMap<usize, BTreeSet<usize>>,
    level: usize,
    requested_slot: usize,
) -> usize {
    let mut slot = requested_slot;
    while occupied_slots
        .get(&level)
        .is_some_and(|slots| slots.contains(&slot))
    {
        slot += 1;
    }
    occupied_slots.entry(level).or_default().insert(slot);
    slot
}

fn release_fixed_reservation(
    fixed_reservations: &mut BTreeMap<usize, BTreeMap<usize, usize>>,
    level: usize,
    slot: usize,
) {
    let mut remove_level = false;
    if let Some(slots) = fixed_reservations.get_mut(&level) {
        if let Some(count) = slots.get_mut(&slot) {
            if *count > 1 {
                *count -= 1;
            } else {
                slots.remove(&slot);
            }
        }
        remove_level = slots.is_empty();
    }
    if remove_level {
        fixed_reservations.remove(&level);
    }
}

fn snap_to_grid(value_um: f64, pitch_um: f64) -> usize {
    if pitch_um <= 0.0 {
        return 0;
    }
    (value_um / pitch_um).round().max(0.0) as usize
}

fn degree_maps(netlist: &Netlist) -> (Vec<usize>, Vec<usize>) {
    let node_count = netlist.node_count();
    let mut indegree = vec![0usize; node_count];
    let mut outdegree = vec![0usize; node_count];
    for (from, to) in netlist.edge_pairs() {
        indegree[to.node.0] += 1;
        outdegree[from.node.0] += 1;
    }
    (indegree, outdegree)
}

fn default_level_for_node(
    kind: NodeKind,
    node: NodeId,
    levels: &[usize],
    indegree: &[usize],
    outdegree: &[usize],
    output_port_level: usize,
) -> usize {
    match kind {
        NodeKind::Port if indegree[node.0] == 0 && outdegree[node.0] > 0 => 0,
        NodeKind::Port if indegree[node.0] > 0 && outdegree[node.0] == 0 => output_port_level,
        _ => levels[node.0],
    }
}

fn legalize_level_for_congestion(
    config: &PlacementConfig,
    nodes_per_level: &BTreeMap<usize, usize>,
    min_level: usize,
) -> usize {
    if config.max_nodes_per_level == 0 {
        return min_level;
    }

    let mut level = min_level;
    while nodes_per_level.get(&level).copied().unwrap_or(0) >= config.max_nodes_per_level {
        level += 1;
    }
    level
}

fn ensure_level_capacity(next_slot_per_level: &mut Vec<usize>, level: usize) {
    if next_slot_per_level.len() <= level {
        next_slot_per_level.resize(level + 1, 0);
    }
}

fn compute_node_levels(netlist: &Netlist) -> Result<Vec<usize>, PlaceError> {
    let node_count = netlist.node_count();
    let mut indegree = vec![0usize; node_count];
    let mut adjacency = vec![Vec::<usize>::new(); node_count];
    for (from, to) in netlist.edge_pairs() {
        indegree[to.node.0] += 1;
        adjacency[from.node.0].push(to.node.0);
    }

    let mut queue = VecDeque::new();
    for (node, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(node);
        }
    }

    let mut levels = vec![0usize; node_count];
    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        let next_level = levels[node] + 1;
        for succ in &adjacency[node] {
            if next_level > levels[*succ] {
                levels[*succ] = next_level;
            }
            indegree[*succ] -= 1;
            if indegree[*succ] == 0 {
                queue.push_back(*succ);
            }
        }
    }

    if visited != node_count {
        return Err(PlaceError::Cycle);
    }

    Ok(levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{NodeKind, PinRef};

    #[test]
    fn places_nodes_by_topological_level() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 })
            .expect("b to c");

        let placement = placer
            .place(&netlist, &PlacementConfig::default())
            .expect("placement should succeed");

        assert_eq!(placement.point_of(a).expect("a point").x_um, 0.0);
        assert_eq!(placement.point_of(b).expect("b point").x_um, 40.0);
        assert_eq!(placement.point_of(c).expect("c point").x_um, 80.0);
    }

    #[test]
    fn rejects_cycles() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");
        netlist
            .connect(PinRef { node: b, port: 1 }, PinRef { node: a, port: 1 })
            .expect("b to a");

        let err = placer
            .place(&netlist, &PlacementConfig::default())
            .expect_err("cycle should fail");
        assert_eq!(err, PlaceError::Cycle);
    }

    #[test]
    fn honors_fixed_node_constraints() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::MacroCell, "b");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    fixed_nodes: vec![FixedNodePlacement {
                        node: b,
                        point: Point {
                            x_um: 120.0,
                            y_um: 48.0,
                        },
                    }],
                    blocked_regions: Vec::new(),
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");

        assert_eq!(
            placement.point_of(a).expect("a point"),
            Point {
                x_um: 0.0,
                y_um: 0.0
            }
        );
        assert_eq!(
            placement.point_of(b).expect("b point"),
            Point {
                x_um: 120.0,
                y_um: 48.0
            }
        );
    }

    #[test]
    fn legalizes_conflicting_fixed_slots() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::MacroCell, "a");
        let b = netlist.add_node(NodeKind::MacroCell, "b");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    fixed_nodes: vec![
                        FixedNodePlacement {
                            node: a,
                            point: Point {
                                x_um: 80.0,
                                y_um: 24.0,
                            },
                        },
                        FixedNodePlacement {
                            node: b,
                            point: Point {
                                x_um: 80.0,
                                y_um: 24.0,
                            },
                        },
                    ],
                    blocked_regions: Vec::new(),
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");

        assert_eq!(
            placement.point_of(a).expect("a point"),
            Point {
                x_um: 80.0,
                y_um: 24.0
            }
        );
        assert_eq!(
            placement.point_of(b).expect("b point"),
            Point {
                x_um: 80.0,
                y_um: 48.0
            }
        );
    }

    #[test]
    fn keeps_output_ports_on_right_boundary() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let input = netlist.add_node(NodeKind::Port, "input");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        let output = netlist.add_node(NodeKind::Port, "output");

        netlist
            .connect(
                PinRef {
                    node: input,
                    port: 0,
                },
                PinRef {
                    node: gate,
                    port: 0,
                },
            )
            .expect("input to gate");
        netlist
            .connect(
                PinRef {
                    node: gate,
                    port: 0,
                },
                PinRef {
                    node: output,
                    port: 0,
                },
            )
            .expect("gate to output");

        let placement = placer
            .place(&netlist, &PlacementConfig::default())
            .expect("placement should succeed");

        assert_eq!(placement.point_of(input).expect("input point").x_um, 0.0);
        assert_eq!(placement.point_of(gate).expect("gate point").x_um, 40.0);
        assert_eq!(placement.point_of(output).expect("output point").x_um, 80.0);
    }

    #[test]
    fn keeps_fixed_output_ports_at_requested_boundary_slot() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let input = netlist.add_node(NodeKind::Port, "input");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        let output = netlist.add_node(NodeKind::Port, "output");

        netlist
            .connect(
                PinRef {
                    node: input,
                    port: 0,
                },
                PinRef {
                    node: gate,
                    port: 0,
                },
            )
            .expect("input to gate");
        netlist
            .connect(
                PinRef {
                    node: gate,
                    port: 0,
                },
                PinRef {
                    node: output,
                    port: 0,
                },
            )
            .expect("gate to output");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    fixed_nodes: vec![FixedNodePlacement {
                        node: output,
                        point: Point {
                            x_um: 120.0,
                            y_um: 48.0,
                        },
                    }],
                    blocked_regions: Vec::new(),
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");

        assert_eq!(
            placement.point_of(output).expect("output point"),
            Point {
                x_um: 120.0,
                y_um: 48.0
            }
        );
        assert_eq!(placement.width_um, 160.0);
    }

    #[test]
    fn avoids_blocked_regions_during_slot_assignment() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    blocked_regions: vec![BlockedRegion {
                        min_x_um: 0.0,
                        max_x_um: 0.0,
                        min_y_um: 0.0,
                        max_y_um: 24.0,
                    }],
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");

        assert_eq!(
            placement.point_of(a).expect("a point"),
            Point {
                x_um: 0.0,
                y_um: 48.0
            }
        );
        assert_eq!(
            placement.point_of(b).expect("b point"),
            Point {
                x_um: 0.0,
                y_um: 72.0
            }
        );
    }

    #[test]
    fn reserves_macro_halo_for_following_nodes() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let macro_node = netlist.add_node(NodeKind::MacroCell, "macro");
        let follower = netlist.add_node(NodeKind::CellInstance, "follower");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    macro_halo_x_um: 40.0,
                    macro_halo_y_um: 24.0,
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");

        assert_eq!(
            placement.point_of(macro_node).expect("macro point"),
            Point {
                x_um: 0.0,
                y_um: 0.0
            }
        );
        assert_eq!(
            placement.point_of(follower).expect("follower point"),
            Point {
                x_um: 0.0,
                y_um: 48.0
            }
        );
    }

    #[test]
    fn spills_nodes_to_later_levels_when_congested() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    max_nodes_per_level: 1,
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");

        assert_eq!(placement.point_of(a).expect("a point").x_um, 0.0);
        assert_eq!(placement.point_of(b).expect("b point").x_um, 40.0);
        assert_eq!(placement.point_of(c).expect("c point").x_um, 80.0);
    }

    #[test]
    fn keeps_fixed_nodes_inside_blocked_regions_when_explicitly_requested() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::MacroCell, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    fixed_nodes: vec![FixedNodePlacement {
                        node: a,
                        point: Point {
                            x_um: 0.0,
                            y_um: 0.0,
                        },
                    }],
                    blocked_regions: vec![BlockedRegion {
                        min_x_um: 0.0,
                        max_x_um: 0.0,
                        min_y_um: 0.0,
                        max_y_um: 24.0,
                    }],
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");

        assert_eq!(
            placement.point_of(a).expect("a point"),
            Point {
                x_um: 0.0,
                y_um: 0.0
            }
        );
        assert_eq!(
            placement.point_of(b).expect("b point"),
            Point {
                x_um: 0.0,
                y_um: 48.0
            }
        );
    }

    #[test]
    fn empty_netlist_produces_zero_size_placement() {
        let placer = LevelizedPlacer::new();
        let netlist = Netlist::new();
        let placement = placer
            .place(&netlist, &PlacementConfig::default())
            .expect("empty placement should succeed");
        assert!(placement.nodes.is_empty());
        assert_eq!(placement.width_um, 0.0);
        assert_eq!(placement.height_um, 0.0);
    }

    #[test]
    fn single_node_occupies_origin() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let placement = placer
            .place(&netlist, &PlacementConfig::default())
            .expect("placement should succeed");
        assert_eq!(
            placement.point_of(a).expect("a point"),
            Point {
                x_um: 0.0,
                y_um: 0.0
            }
        );
    }

    #[test]
    fn point_of_returns_none_for_unknown_node() {
        let placer = LevelizedPlacer::new();
        let netlist = Netlist::new();
        let placement = placer
            .place(&netlist, &PlacementConfig::default())
            .expect("placement should succeed");
        assert!(placement.point_of(rflux_ir::NodeId(999)).is_none());
    }

    #[test]
    fn default_placement_config_has_expected_values() {
        let config = PlacementConfig::default();
        assert_eq!(config.x_pitch_um, 40.0);
        assert_eq!(config.y_pitch_um, 24.0);
        assert!(config.fixed_nodes.is_empty());
        assert!(config.blocked_regions.is_empty());
        assert_eq!(config.macro_halo_x_um, 40.0);
        assert_eq!(config.macro_halo_y_um, 24.0);
        assert_eq!(config.max_nodes_per_level, 0);
    }

    #[test]
    fn custom_pitch_scales_placement() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef { node: b, port: 0 },
            )
            .expect("a to b");

        let placement = placer
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 80.0,
                    y_pitch_um: 48.0,
                    ..PlacementConfig::default()
                },
            )
            .expect("placement should succeed");
        assert_eq!(placement.point_of(a).expect("a").x_um, 0.0);
        assert_eq!(placement.point_of(b).expect("b").x_um, 80.0);
    }

    #[test]
    fn branching_netlist_spreads_across_levels() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let input = netlist.add_node(NodeKind::Port, "in");
        let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
        let g2 = netlist.add_node(NodeKind::CellInstance, "g2");
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: input, port: 0 },
                PinRef { node: g1, port: 0 },
            )
            .unwrap();
        netlist
            .connect(
                PinRef { node: input, port: 1 },
                PinRef { node: g2, port: 0 },
            )
            .unwrap();
        netlist
            .connect(
                PinRef { node: g1, port: 0 },
                PinRef { node: out, port: 0 },
            )
            .unwrap();
        netlist
            .connect(
                PinRef { node: g2, port: 0 },
                PinRef { node: out, port: 1 },
            )
            .unwrap();

        let placement = placer
            .place(&netlist, &PlacementConfig::default())
            .expect("placement should succeed");
        assert_eq!(placement.point_of(input).expect("input").x_um, 0.0);
        assert_eq!(placement.width_um, 120.0);
    }

    #[test]
    fn place_error_codes_are_stable() {
        assert_eq!(PlaceError::Cycle.code(), "RFLOW-FLOW-002");
        assert!(!PlaceError::Cycle.suggestion().is_empty());
    }
}
