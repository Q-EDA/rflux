use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write as _;
use std::path::Path;

use rflux_ir::{Netlist, NodeId, NodeKind, PinRef};
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

    pub fn write_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        for node in &self.nodes {
            writeln!(
                file,
                "{} {} {} {}",
                node.node.0, node.level, node.slot, node.point.x_um
            )?;
        }
        Ok(())
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

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionConfig {
    pub max_partition_size: usize,
    pub overlap_margin_um: f64,
    pub enable_partitioning: bool,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            max_partition_size: 500,
            overlap_margin_um: 20.0,
            enable_partitioning: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaConfig {
    pub initial_temperature: f64,
    pub cooling_rate: f64,
    pub min_temperature: f64,
    pub moves_per_temp: usize,
    pub cost_weight_hpwl: f64,
    pub cost_weight_congestion: f64,
    pub cost_weight_timing: f64,
}

impl Default for SaConfig {
    fn default() -> Self {
        Self {
            initial_temperature: 1000.0,
            cooling_rate: 0.95,
            min_temperature: 1.0,
            moves_per_temp: 1000,
            cost_weight_hpwl: 1.0,
            cost_weight_congestion: 0.5,
            cost_weight_timing: 0.0,
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

pub struct PartitionPlacer {
    config: PlacementConfig,
    partition_config: PartitionConfig,
}

impl PartitionPlacer {
    pub fn new(config: PlacementConfig, partition_config: PartitionConfig) -> Self {
        Self {
            config,
            partition_config,
        }
    }

    pub fn place(&self, netlist: &Netlist) -> Result<Placement, PlaceError> {
        if !self.partition_config.enable_partitioning {
            let placer = LevelizedPlacer::new();
            return placer.place(netlist, &self.config);
        }

        let levels = compute_node_levels_grouped(netlist)?;
        let max_size = self.partition_config.max_partition_size;

        let needs_partitioning = levels.iter().any(|(_, nodes)| nodes.len() > max_size);

        if !needs_partitioning {
            let placer = LevelizedPlacer::new();
            return placer.place(netlist, &self.config);
        }

        let mut partitions: Vec<Vec<NodeId>> = Vec::new();
        for (_, mut level_nodes) in levels {
            while level_nodes.len() > max_size {
                let partition: Vec<NodeId> = level_nodes.drain(..max_size).collect();
                partitions.push(partition);
            }
            if !level_nodes.is_empty() {
                partitions.push(level_nodes);
            }
        }

        let mut all_placed = Vec::new();
        let mut y_offset = 0.0f64;

        for partition in &partitions {
            let sub_netlist = extract_subgraph(netlist, partition);
            let placer = LevelizedPlacer::new();
            let sub_placement = placer.place(&sub_netlist, &self.config)?;

            for mut placed in sub_placement.nodes {
                placed.point.y_um += y_offset;
                all_placed.push(placed);
            }
            y_offset += sub_placement.height_um + self.partition_config.overlap_margin_um;
        }

        let width = all_placed
            .iter()
            .map(|p| p.point.x_um)
            .fold(0.0f64, f64::max)
            + self.config.x_pitch_um;

        Ok(Placement {
            nodes: all_placed,
            width_um: width,
            height_um: y_offset,
        })
    }
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn gen_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn gen_range(&mut self, low: usize, high: usize) -> usize {
        (self.gen_u64() as usize) % (high - low) + low
    }

    fn gen_f64(&mut self) -> f64 {
        (self.gen_u64() as f64) / (u64::MAX as f64)
    }

    fn gen_bool(&mut self) -> bool {
        self.gen_u64().is_multiple_of(2)
    }
}

pub struct SaPlacer {
    config: PlacementConfig,
    sa_config: SaConfig,
}

impl SaPlacer {
    pub fn new(config: PlacementConfig, sa_config: SaConfig) -> Self {
        Self { config, sa_config }
    }

    pub fn place(&self, netlist: &Netlist) -> Result<Placement, PlaceError> {
        self.place_with_critical_nets(netlist, &[])
    }

    pub fn place_with_critical_nets(
        &self,
        netlist: &Netlist,
        critical_nets: &[(PinRef, PinRef)],
    ) -> Result<Placement, PlaceError> {
        let initial = LevelizedPlacer::new().place(netlist, &self.config)?;

        if netlist.node_count() < 3 {
            return Ok(initial);
        }

        let optimized = self.simulated_annealing_with_timing(netlist, initial, critical_nets);
        Ok(optimized)
    }

    fn simulated_annealing_with_timing(
        &self,
        netlist: &Netlist,
        initial: Placement,
        critical_nets: &[(PinRef, PinRef)],
    ) -> Placement {
        let mut current = initial.clone();
        let mut current_cost = self.total_cost_with_timing(netlist, &current, critical_nets);
        let mut best = current.clone();
        let mut best_cost = current_cost;
        let mut temp = self.sa_config.initial_temperature;
        let mut rng = SimpleRng::new(42);

        while temp > self.sa_config.min_temperature {
            for _ in 0..self.sa_config.moves_per_temp {
                let mut candidate = current.clone();
                self.random_move(&mut candidate, &mut rng);
                let candidate_cost = self.total_cost_with_timing(netlist, &candidate, critical_nets);
                let delta = candidate_cost - current_cost;

                if delta < 0.0 || rng.gen_f64() < (-delta / temp).exp() {
                    current = candidate;
                    current_cost = candidate_cost;
                    if current_cost < best_cost {
                        best = current.clone();
                        best_cost = current_cost;
                    }
                }
            }
            temp *= self.sa_config.cooling_rate;
        }

        best
    }

    fn total_cost_with_timing(&self, netlist: &Netlist, placement: &Placement, critical_nets: &[(PinRef, PinRef)]) -> f64 {
        let hpwl = self.compute_hpwl(netlist, placement);
        let congestion = self.compute_congestion(placement);
        let timing = self.compute_timing_penalty(netlist, placement, critical_nets);
        self.sa_config.cost_weight_hpwl * hpwl
            + self.sa_config.cost_weight_congestion * congestion
            + self.sa_config.cost_weight_timing * timing
    }

    fn compute_hpwl(&self, netlist: &Netlist, placement: &Placement) -> f64 {
        let mut total_hpwl = 0.0;
        for (from, to) in netlist.edge_pairs() {
            if let (Some(p_from), Some(p_to)) =
                (placement.point_of(from.node), placement.point_of(to.node))
            {
                let dx = (p_from.x_um - p_to.x_um).abs();
                let dy = (p_from.y_um - p_to.y_um).abs();
                total_hpwl += dx + dy;
            }
        }
        total_hpwl
    }

    fn compute_congestion(&self, placement: &Placement) -> f64 {
        let mut level_counts = BTreeMap::<usize, usize>::new();
        for placed in &placement.nodes {
            *level_counts.entry(placed.level).or_default() += 1;
        }
        let max_per_level = if self.config.max_nodes_per_level > 0 {
            self.config.max_nodes_per_level as f64
        } else {
            10.0
        };
        level_counts
            .values()
            .map(|&count| {
                let excess = count as f64 - max_per_level;
                if excess > 0.0 {
                    excess * excess
                } else {
                    0.0
                }
            })
            .sum()
    }

    fn compute_timing_penalty(
        &self,
        _netlist: &Netlist,
        placement: &Placement,
        critical_nets: &[(PinRef, PinRef)],
    ) -> f64 {
        if critical_nets.is_empty() || self.sa_config.cost_weight_timing <= 0.0 {
            return 0.0;
        }
        let mut total_penalty = 0.0;
        for &(from, to) in critical_nets {
            if let (Some(p_from), Some(p_to)) = (placement.point_of(from.node), placement.point_of(to.node)) {
                let dx = (p_from.x_um - p_to.x_um).abs();
                let dy = (p_from.y_um - p_to.y_um).abs();
                total_penalty += dx + dy;
            }
        }
        total_penalty
    }

    fn random_move(&self, placement: &mut Placement, rng: &mut SimpleRng) {
        let n = placement.nodes.len();
        if n < 2 {
            return;
        }

        let move_type = rng.gen_range(0, 3);
        match move_type {
            0 => {
                let i = rng.gen_range(0, n);
                let j = rng.gen_range(0, n);
                if i != j && !self.is_fixed(placement.nodes[i].node) && !self.is_fixed(placement.nodes[j].node) {
                    let temp_point = placement.nodes[i].point;
                    placement.nodes[i].point = placement.nodes[j].point;
                    placement.nodes[j].point = temp_point;
                    let temp_level = placement.nodes[i].level;
                    let temp_slot = placement.nodes[i].slot;
                    placement.nodes[i].level = placement.nodes[j].level;
                    placement.nodes[i].slot = placement.nodes[j].slot;
                    placement.nodes[j].level = temp_level;
                    placement.nodes[j].slot = temp_slot;
                }
            }
            1 => {
                let i = rng.gen_range(0, n);
                if !self.is_fixed(placement.nodes[i].node) {
                    let delta = if rng.gen_bool() { 1 } else { -1 };
                    let new_slot = (placement.nodes[i].slot as i32 + delta).max(0) as usize;
                    placement.nodes[i].slot = new_slot;
                    placement.nodes[i].point.y_um = new_slot as f64 * self.config.y_pitch_um;
                }
            }
            _ => {
                let i = rng.gen_range(0, n);
                if !self.is_fixed(placement.nodes[i].node) {
                    let delta = if rng.gen_bool() { 1 } else { -1 };
                    let new_level = (placement.nodes[i].level as i32 + delta).max(0) as usize;
                    placement.nodes[i].level = new_level;
                    placement.nodes[i].point.x_um = new_level as f64 * self.config.x_pitch_um;
                }
            }
        }
    }

    fn is_fixed(&self, node: NodeId) -> bool {
        self.config.fixed_nodes.iter().any(|f| f.node == node)
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

fn compute_node_levels_grouped(netlist: &Netlist) -> Result<Vec<(usize, Vec<NodeId>)>, PlaceError> {
    let levels = compute_node_levels(netlist)?;
    let mut grouped: BTreeMap<usize, Vec<NodeId>> = BTreeMap::new();
    for node in netlist.nodes() {
        grouped.entry(levels[node.id.0]).or_default().push(node.id);
    }
    Ok(grouped.into_iter().collect())
}

fn extract_subgraph(netlist: &Netlist, nodes: &[NodeId]) -> Netlist {
    let node_set: std::collections::HashSet<NodeId> = nodes.iter().copied().collect();
    let mut sub = Netlist::new();

    let mut id_map: std::collections::HashMap<NodeId, NodeId> = std::collections::HashMap::new();
    for &node_id in &node_set {
        if let Some(node) = netlist.nodes().get(node_id.0) {
            let new_id = sub.add_node(node.kind.clone(), node.name.clone());
            id_map.insert(node_id, new_id);
        }
    }

    for (from, to) in netlist.edge_pairs() {
        if node_set.contains(&from.node) && node_set.contains(&to.node) {
            let new_from = PinRef {
                node: id_map[&from.node],
                port: from.port,
            };
            let new_to = PinRef {
                node: id_map[&to.node],
                port: to.port,
            };
            let _ = sub.connect(new_from, new_to);
        }
    }

    sub
}

// ---------------------------------------------------------------------------
// P1-5: Quick layout estimation
// ---------------------------------------------------------------------------

/// Result of quick layout estimation (P1-5).
#[derive(Debug, Clone)]
pub struct LayoutEstimate {
    /// Estimated total width (um).
    pub width_um: f64,
    /// Estimated total height (um).
    pub height_um: f64,
    /// Estimated total area (um²).
    pub area_um2: f64,
    /// Estimated average wire length (um) based on HPWL.
    pub estimated_avg_wire_length_um: f64,
    /// Number of nodes placed.
    pub placed_nodes: usize,
}

/// Quick layout estimation without full simulated annealing (P1-5).
///
/// Uses levelized placement with a fixed pitch to estimate area
/// and wire length.  Runs in O(n) time, suitable for synthesis-
/// stage feasibility checks.
pub fn estimate_layout(netlist: &Netlist) -> LayoutEstimate {
    let placer = LevelizedPlacer::new();
    let config = PlacementConfig::default();
    let placement = placer.place(netlist, &config).unwrap_or(Placement {
        nodes: Vec::new(),
        width_um: 0.0,
        height_um: 0.0,
    });

    let width = placement.width_um;
    let height = placement.height_um;
    let area = width * height;

    // Estimate average wire length from HPWL of edges
    let mut total_wire = 0.0f64;
    let mut edge_count = 0usize;
    for (from, to) in netlist.edge_pairs() {
        if let (Some(p_from), Some(p_to)) = (
            placement.point_of(from.node),
            placement.point_of(to.node),
        ) {
            let hpwl = (p_from.x_um - p_to.x_um).abs() + (p_from.y_um - p_to.y_um).abs();
            total_wire += hpwl;
            edge_count += 1;
        }
    }
    let avg_wire = if edge_count > 0 {
        total_wire / edge_count as f64
    } else {
        0.0
    };

    LayoutEstimate {
        width_um: width,
        height_um: height,
        area_um2: area,
        estimated_avg_wire_length_um: avg_wire,
        placed_nodes: placement.nodes.len(),
    }
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

    #[test]
    fn partition_placer_small_circuit_no_partition() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .unwrap();
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 })
            .unwrap();

        let placer = PartitionPlacer::new(
            PlacementConfig::default(),
            PartitionConfig {
                max_partition_size: 500,
                enable_partitioning: true,
                ..Default::default()
            },
        );
        let placement = placer.place(&netlist).unwrap();
        assert_eq!(placement.nodes.len(), 3);
    }

    #[test]
    fn partition_placer_disabled_uses_levelized() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .unwrap();

        let placer = PartitionPlacer::new(
            PlacementConfig::default(),
            PartitionConfig {
                enable_partitioning: false,
                ..Default::default()
            },
        );
        let placement = placer.place(&netlist).unwrap();
        assert_eq!(placement.nodes.len(), 2);
        assert_eq!(placement.point_of(a).unwrap().x_um, 0.0);
        assert_eq!(placement.point_of(b).unwrap().x_um, 40.0);
    }

    #[test]
    fn partition_placer_splits_large_level() {
        let mut netlist = Netlist::new();
        for i in 0..4 {
            netlist.add_node(NodeKind::CellInstance, format!("g{i}"));
        }

        let placer = PartitionPlacer::new(
            PlacementConfig::default(),
            PartitionConfig {
                max_partition_size: 2,
                overlap_margin_um: 10.0,
                enable_partitioning: true,
            },
        );
        let placement = placer.place(&netlist).unwrap();
        assert_eq!(placement.nodes.len(), 4);
        assert!(placement.height_um > 0.0);
    }

    #[test]
    fn partition_placer_empty_netlist() {
        let netlist = Netlist::new();
        let placer = PartitionPlacer::new(
            PlacementConfig::default(),
            PartitionConfig {
                enable_partitioning: true,
                ..Default::default()
            },
        );
        let placement = placer.place(&netlist).unwrap();
        assert!(placement.nodes.is_empty());
        assert_eq!(placement.width_um, 0.0);
        assert_eq!(placement.height_um, 0.0);
    }

    #[test]
    fn partition_config_default_values() {
        let config = PartitionConfig::default();
        assert_eq!(config.max_partition_size, 500);
        assert_eq!(config.overlap_margin_um, 20.0);
        assert!(!config.enable_partitioning);
    }

    #[test]
    fn extract_subgraph_preserves_edges() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .unwrap();
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 })
            .unwrap();

        let sub = extract_subgraph(&netlist, &[a, b]);
        assert_eq!(sub.node_count(), 2);
        assert_eq!(sub.edge_count(), 1);
    }

    #[test]
    fn sa_placer_improves_hpwl() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        let d = netlist.add_node(NodeKind::Port, "d");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).ok();
        netlist.connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 }).ok();
        netlist.connect(PinRef { node: c, port: 0 }, PinRef { node: d, port: 0 }).ok();

        let config = PlacementConfig::default();
        let sa_config = SaConfig {
            moves_per_temp: 100,
            initial_temperature: 100.0,
            ..Default::default()
        };

        let placer = SaPlacer::new(config, sa_config);
        let placement = placer.place(&netlist).unwrap();
        assert_eq!(placement.nodes.len(), 4);
    }

    #[test]
    fn sa_placer_respects_fixed_nodes() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).ok();
        netlist.connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 }).ok();

        let fixed_point = Point { x_um: 200.0, y_um: 216.0 };
        let config = PlacementConfig {
            fixed_nodes: vec![FixedNodePlacement { node: b, point: fixed_point }],
            ..PlacementConfig::default()
        };
        let sa_config = SaConfig {
            moves_per_temp: 100,
            initial_temperature: 100.0,
            ..Default::default()
        };

        let placer = SaPlacer::new(config, sa_config);
        let placement = placer.place(&netlist).unwrap();
        let placed_b = placement.nodes.iter().find(|p| p.node == b).unwrap();
        assert_eq!(placed_b.point, fixed_point);
    }

    #[test]
    fn sa_placer_empty_netlist() {
        let netlist = Netlist::new();
        let config = PlacementConfig::default();
        let sa_config = SaConfig::default();
        let placer = SaPlacer::new(config, sa_config);
        let placement = placer.place(&netlist).unwrap();
        assert_eq!(placement.nodes.len(), 0);
    }

    #[test]
    fn sa_config_default_values() {
        let config = SaConfig::default();
        assert_eq!(config.initial_temperature, 1000.0);
        assert_eq!(config.cooling_rate, 0.95);
        assert_eq!(config.min_temperature, 1.0);
        assert_eq!(config.moves_per_temp, 1000);
        assert_eq!(config.cost_weight_hpwl, 1.0);
        assert_eq!(config.cost_weight_congestion, 0.5);
        assert_eq!(config.cost_weight_timing, 0.0);
    }

    #[test]
    fn sa_placer_timing_weight_affects_placement() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        let d = netlist.add_node(NodeKind::Port, "d");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).ok();
        netlist.connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 }).ok();
        netlist.connect(PinRef { node: c, port: 0 }, PinRef { node: d, port: 0 }).ok();

        let config = PlacementConfig::default();
        let critical_nets = vec![
            (PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }),
            (PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 }),
        ];

        let sa_config_no_timing = SaConfig {
            cost_weight_timing: 0.0,
            moves_per_temp: 100,
            ..Default::default()
        };
        let placer_no_timing = SaPlacer::new(config.clone(), sa_config_no_timing);
        let placement_no_timing = placer_no_timing.place_with_critical_nets(&netlist, &critical_nets).unwrap();

        let sa_config_timing = SaConfig {
            cost_weight_timing: 2.0,
            moves_per_temp: 100,
            ..Default::default()
        };
        let placer_timing = SaPlacer::new(config, sa_config_timing);
        let placement_timing = placer_timing.place_with_critical_nets(&netlist, &critical_nets).unwrap();

        assert_eq!(placement_no_timing.nodes.len(), 4);
        assert_eq!(placement_timing.nodes.len(), 4);
    }

    #[test]
    fn sa_placer_timing_weight_zero_is_default() {
        let config = SaConfig::default();
        assert_eq!(config.cost_weight_timing, 0.0);
    }

    #[test]
    fn placement_write_to_file() {
        let placer = LevelizedPlacer::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .unwrap();

        let placement = placer
            .place(&netlist, &PlacementConfig::default())
            .expect("placement should succeed");

        let dir = std::env::temp_dir().join("rflux_place_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_placement.txt");
        placement.write_to_file(&path).expect("write should succeed");

        let contents = std::fs::read_to_string(&path).expect("read should succeed");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("0 "));

        let _ = std::fs::remove_file(&path);
    }

    // --- P1-5: estimate_layout tests ---

    #[test]
    fn estimate_layout_empty_netlist() {
        let netlist = Netlist::new();
        let est = estimate_layout(&netlist);
        assert_eq!(est.placed_nodes, 0);
        assert_eq!(est.width_um, 0.0);
    }

    #[test]
    fn estimate_layout_simple_netlist() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).unwrap();
        netlist.connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 }).unwrap();

        let est = estimate_layout(&netlist);
        assert!(est.width_um > 0.0);
        assert!(est.height_um > 0.0);
        assert!(est.area_um2 > 0.0);
        assert_eq!(est.placed_nodes, 3);
    }
}
