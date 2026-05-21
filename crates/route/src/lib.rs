use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_place::{Placement, Point};
use rflux_tech::Pdk;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    Jtl,
    Ptl,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteSegment {
    pub start: Point,
    pub end: Point,
    pub layer: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockedRegion {
    pub min_x_um: f64,
    pub max_x_um: f64,
    pub min_y_um: f64,
    pub max_y_um: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetRoute {
    pub from: PinRef,
    pub to: PinRef,
    pub mode: RouteMode,
    pub segments: Vec<RouteSegment>,
    pub direct_length_um: f64,
    pub length_um: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutingReport {
    pub routes: Vec<NetRoute>,
    pub total_length_um: f64,
    pub total_detour_overhead_um: f64,
    pub detoured_routes: usize,
    pub jtl_routes: usize,
    pub ptl_routes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutingConfig {
    pub prefer_ptl_from_length_um: f64,
    pub jtl_layer: u8,
    pub ptl_layer: u8,
    pub blocked_regions: Vec<BlockedRegion>,
    pub detour_margin_um: f64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            prefer_ptl_from_length_um: 60.0,
            jtl_layer: 1,
            ptl_layer: 2,
            blocked_regions: Vec::new(),
            detour_margin_um: 12.0,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouteError {
    #[error("routing requires placement for every node")]
    MissingPlacement,
}

#[derive(Debug, Default)]
pub struct SimpleRouter;

impl SimpleRouter {
    pub fn new() -> Self {
        Self
    }

    pub fn route(
        &self,
        netlist: &Netlist,
        placement: &Placement,
        pdk: &Pdk,
        config: &RoutingConfig,
    ) -> Result<RoutingReport, RouteError> {
        let mut routes = Vec::with_capacity(netlist.edge_count());
        let mut total_length_um = 0.0;
        let mut total_detour_overhead_um = 0.0;
        let mut detoured_routes = 0usize;
        let mut jtl_routes = 0usize;
        let mut ptl_routes = 0usize;

        for (from, to) in netlist.edge_pairs() {
            let source = placement.point_of(from.node).ok_or(RouteError::MissingPlacement)?;
            let sink = placement.point_of(to.node).ok_or(RouteError::MissingPlacement)?;
            let path = choose_route_path(source, sink, config);
            let direct_length_um = manhattan_length(source, sink);
            let length_um = path_length(&path);
            let touches_boundary_port = is_boundary_port(netlist, from.node) || is_boundary_port(netlist, to.node);
            let use_ptl = !touches_boundary_port
                && length_um >= config.prefer_ptl_from_length_um
                && pdk.is_ptl_length_allowed(length_um);
            let mode = if use_ptl { RouteMode::Ptl } else { RouteMode::Jtl };
            let layer = if use_ptl { config.ptl_layer } else { config.jtl_layer };
            let segments = path
                .into_iter()
                .map(|(start, end)| RouteSegment { start, end, layer })
                .collect();

            match mode {
                RouteMode::Jtl => jtl_routes += 1,
                RouteMode::Ptl => ptl_routes += 1,
            }
            total_length_um += length_um;
            let detour_overhead_um = (length_um - direct_length_um).max(0.0);
            total_detour_overhead_um += detour_overhead_um;
            if detour_overhead_um > 0.0 {
                detoured_routes += 1;
            }
            routes.push(NetRoute {
                from,
                to,
                mode,
                segments,
                direct_length_um,
                length_um,
            });
        }

        Ok(RoutingReport {
            routes,
            total_length_um,
            total_detour_overhead_um,
            detoured_routes,
            jtl_routes,
            ptl_routes,
        })
    }
}

fn manhattan_length(a: Point, b: Point) -> f64 {
    (a.x_um - b.x_um).abs() + (a.y_um - b.y_um).abs()
}

fn choose_route_path(source: Point, sink: Point, config: &RoutingConfig) -> Vec<(Point, Point)> {
    shortest_grid_path(source, sink, config)
        .or_else(|| {
            let mut candidates = base_route_candidates(source, sink);
            for region in &config.blocked_regions {
                candidates.extend(detour_candidates(source, sink, *region, config.detour_margin_um));
            }

            let mut best_clear = None::<Vec<(Point, Point)>>;
            let mut best_length = f64::INFINITY;
            for candidate in candidates {
                if path_is_clear(&candidate, &config.blocked_regions) {
                    let length = path_length(&candidate);
                    if length < best_length {
                        best_length = length;
                        best_clear = Some(candidate);
                    }
                }
            }
            best_clear
        })
        .unwrap_or_else(|| base_route_candidates(source, sink).into_iter().next().unwrap_or_default())
}

#[derive(Debug, Clone, Copy)]
struct QueueEntry {
    cost: f64,
    node: usize,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node == other.node
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.node.cmp(&self.node))
    }
}

fn shortest_grid_path(source: Point, sink: Point, config: &RoutingConfig) -> Option<Vec<(Point, Point)>> {
    let points = grid_points(source, sink, config);
    let source_index = points.iter().position(|point| *point == source)?;
    let sink_index = points.iter().position(|point| *point == sink)?;

    let adjacency = build_grid_adjacency(&points, &config.blocked_regions);
    let mut dist = vec![f64::INFINITY; points.len()];
    let mut prev = vec![None::<usize>; points.len()];
    let mut queue = BinaryHeap::<QueueEntry>::new();

    dist[source_index] = 0.0;
    queue.push(QueueEntry {
        cost: 0.0,
        node: source_index,
    });

    while let Some(QueueEntry { cost, node }) = queue.pop() {
        if cost > dist[node] {
            continue;
        }
        if node == sink_index {
            break;
        }

        for &(next, weight) in adjacency.get(&node).map(Vec::as_slice).unwrap_or(&[]) {
            let next_cost = cost + weight;
            if next_cost < dist[next] {
                dist[next] = next_cost;
                prev[next] = Some(node);
                queue.push(QueueEntry {
                    cost: next_cost,
                    node: next,
                });
            }
        }
    }

    if !dist[sink_index].is_finite() {
        return None;
    }

    let mut path_points = Vec::<Point>::new();
    let mut current = sink_index;
    path_points.push(points[current]);
    while let Some(previous) = prev[current] {
        current = previous;
        path_points.push(points[current]);
    }
    path_points.reverse();
    Some(collapse_path_points(&path_points))
}

fn grid_points(source: Point, sink: Point, config: &RoutingConfig) -> Vec<Point> {
    let mut xs = BTreeSet::<i64>::new();
    let mut ys = BTreeSet::<i64>::new();
    insert_axis_point(&mut xs, source.x_um);
    insert_axis_point(&mut xs, sink.x_um);
    insert_axis_point(&mut ys, source.y_um);
    insert_axis_point(&mut ys, sink.y_um);

    for region in &config.blocked_regions {
        insert_axis_point(&mut xs, region.min_x_um - config.detour_margin_um);
        insert_axis_point(&mut xs, region.max_x_um + config.detour_margin_um);
        insert_axis_point(&mut ys, region.min_y_um - config.detour_margin_um);
        insert_axis_point(&mut ys, region.max_y_um + config.detour_margin_um);
    }

    let x_values: Vec<f64> = xs.into_iter().map(from_axis_key).collect();
    let y_values: Vec<f64> = ys.into_iter().map(from_axis_key).collect();
    let mut points = Vec::<Point>::new();
    for &x_um in &x_values {
        for &y_um in &y_values {
            let point = Point { x_um, y_um };
            if point == source || point == sink || point_is_clear(point, &config.blocked_regions) {
                points.push(point);
            }
        }
    }
    points
}

fn build_grid_adjacency(
    points: &[Point],
    blocked_regions: &[BlockedRegion],
) -> HashMap<usize, Vec<(usize, f64)>> {
    let mut adjacency = HashMap::<usize, Vec<(usize, f64)>>::new();

    for axis_is_x in [true, false] {
        let mut groups = HashMap::<i64, Vec<(usize, f64)>>::new();
        for (index, point) in points.iter().enumerate() {
            let key = if axis_is_x { axis_key(point.x_um) } else { axis_key(point.y_um) };
            let coord = if axis_is_x { point.y_um } else { point.x_um };
            groups.entry(key).or_default().push((index, coord));
        }

        for group in groups.values_mut() {
            group.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
            for pair in group.windows(2) {
                let from = pair[0].0;
                let to = pair[1].0;
                let start = points[from];
                let end = points[to];
                if segment_is_clear(start, end, blocked_regions) {
                    let weight = manhattan_length(start, end);
                    adjacency.entry(from).or_default().push((to, weight));
                    adjacency.entry(to).or_default().push((from, weight));
                }
            }
        }
    }

    adjacency
}

fn collapse_path_points(points: &[Point]) -> Vec<(Point, Point)> {
    if points.len() < 2 {
        return Vec::new();
    }

    let mut collapsed = Vec::<Point>::new();
    collapsed.push(points[0]);
    for window in points.windows(3) {
        let a = window[0];
        let b = window[1];
        let c = window[2];
        if (a.x_um == b.x_um && b.x_um == c.x_um) || (a.y_um == b.y_um && b.y_um == c.y_um) {
            continue;
        }
        collapsed.push(b);
    }
    collapsed.push(*points.last().unwrap_or(&points[0]));

    normalize_path(
        collapsed
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect(),
    )
}

fn point_is_clear(point: Point, blocked_regions: &[BlockedRegion]) -> bool {
    blocked_regions.iter().all(|region| {
        point.x_um < region.min_x_um
            || point.x_um > region.max_x_um
            || point.y_um < region.min_y_um
            || point.y_um > region.max_y_um
    })
}

fn segment_is_clear(start: Point, end: Point, blocked_regions: &[BlockedRegion]) -> bool {
    blocked_regions
        .iter()
        .all(|region| !segment_intersects_region(start, end, *region))
}

fn insert_axis_point(set: &mut BTreeSet<i64>, value: f64) {
    set.insert(axis_key(value));
}

fn axis_key(value: f64) -> i64 {
    (value * 1000.0).round() as i64
}

fn from_axis_key(value: i64) -> f64 {
    value as f64 / 1000.0
}

fn base_route_candidates(source: Point, sink: Point) -> Vec<Vec<(Point, Point)>> {
    if source.x_um == sink.x_um || source.y_um == sink.y_um {
        return vec![normalize_path(vec![(source, sink)])];
    }

    let horizontal_first = Point {
        x_um: sink.x_um,
        y_um: source.y_um,
    };
    let vertical_first = Point {
        x_um: source.x_um,
        y_um: sink.y_um,
    };

    vec![
        normalize_path(vec![(source, horizontal_first), (horizontal_first, sink)]),
        normalize_path(vec![(source, vertical_first), (vertical_first, sink)]),
    ]
}

fn detour_candidates(
    source: Point,
    sink: Point,
    region: BlockedRegion,
    detour_margin_um: f64,
) -> Vec<Vec<(Point, Point)>> {
    let above_y = region.max_y_um + detour_margin_um;
    let below_y = region.min_y_um - detour_margin_um;
    let left_x = region.min_x_um - detour_margin_um;
    let right_x = region.max_x_um + detour_margin_um;

    vec![
        normalize_path(vec![
            (source, Point { x_um: source.x_um, y_um: above_y }),
            (Point { x_um: source.x_um, y_um: above_y }, Point { x_um: sink.x_um, y_um: above_y }),
            (Point { x_um: sink.x_um, y_um: above_y }, sink),
        ]),
        normalize_path(vec![
            (source, Point { x_um: source.x_um, y_um: below_y }),
            (Point { x_um: source.x_um, y_um: below_y }, Point { x_um: sink.x_um, y_um: below_y }),
            (Point { x_um: sink.x_um, y_um: below_y }, sink),
        ]),
        normalize_path(vec![
            (source, Point { x_um: left_x, y_um: source.y_um }),
            (Point { x_um: left_x, y_um: source.y_um }, Point { x_um: left_x, y_um: sink.y_um }),
            (Point { x_um: left_x, y_um: sink.y_um }, sink),
        ]),
        normalize_path(vec![
            (source, Point { x_um: right_x, y_um: source.y_um }),
            (Point { x_um: right_x, y_um: source.y_um }, Point { x_um: right_x, y_um: sink.y_um }),
            (Point { x_um: right_x, y_um: sink.y_um }, sink),
        ]),
    ]
}

fn normalize_path(path: Vec<(Point, Point)>) -> Vec<(Point, Point)> {
    path.into_iter()
        .filter(|(start, end)| start.x_um != end.x_um || start.y_um != end.y_um)
        .collect()
}

fn path_is_clear(path: &[(Point, Point)], blocked_regions: &[BlockedRegion]) -> bool {
    path.iter().all(|(start, end)| {
        blocked_regions
            .iter()
            .all(|region| !segment_intersects_region(*start, *end, *region))
    })
}

fn segment_intersects_region(start: Point, end: Point, region: BlockedRegion) -> bool {
    if start.x_um == end.x_um {
        let min_y = start.y_um.min(end.y_um);
        let max_y = start.y_um.max(end.y_um);
        return start.x_um >= region.min_x_um
            && start.x_um <= region.max_x_um
            && ranges_overlap(min_y, max_y, region.min_y_um, region.max_y_um);
    }

    if start.y_um == end.y_um {
        let min_x = start.x_um.min(end.x_um);
        let max_x = start.x_um.max(end.x_um);
        return start.y_um >= region.min_y_um
            && start.y_um <= region.max_y_um
            && ranges_overlap(min_x, max_x, region.min_x_um, region.max_x_um);
    }

    false
}

fn ranges_overlap(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> bool {
    a_max >= b_min && b_max >= a_min
}

fn path_length(path: &[(Point, Point)]) -> f64 {
    path.iter().map(|(start, end)| manhattan_length(*start, *end)).sum()
}

fn is_boundary_port(netlist: &Netlist, node: rflux_ir::NodeId) -> bool {
    matches!(netlist.nodes()[node.0].kind, NodeKind::Port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{NodeKind, PinRef};
    use rflux_place::{LevelizedPlacer, PlacementConfig};
    use rflux_tech::{LengthRange, Pdk};

    #[test]
    fn routes_short_net_with_jtl() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).expect("a to b");

        let placement = LevelizedPlacer::new().place(&netlist, &PlacementConfig::default()).expect("placement");
        let report = SimpleRouter::new()
            .route(&netlist, &placement, &Pdk::minimal("test"), &RoutingConfig::default())
            .expect("route");

        assert_eq!(report.routes.len(), 1);
        assert_eq!(report.routes[0].mode, RouteMode::Jtl);
        assert_eq!(report.routes[0].direct_length_um, report.routes[0].length_um);
        assert_eq!(report.jtl_routes, 1);
    }

    #[test]
    fn routes_long_net_with_ptl_when_allowed() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).expect("a to b");

        let placement = LevelizedPlacer::new()
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 100.0,
                    y_pitch_um: 24.0,
                    fixed_nodes: Vec::new(),
                    blocked_regions: Vec::new(),
                    macro_halo_x_um: 0.0,
                    macro_halo_y_um: 0.0,
                    max_nodes_per_level: 0,
                },
            )
            .expect("placement");
        let report = SimpleRouter::new()
            .route(&netlist, &placement, &Pdk::minimal("test"), &RoutingConfig::default())
            .expect("route");

        assert_eq!(report.routes[0].mode, RouteMode::Ptl);
        assert_eq!(report.detoured_routes, 0);
        assert_eq!(report.ptl_routes, 1);
    }

    #[test]
    fn falls_back_to_jtl_when_ptl_length_is_forbidden() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).expect("a to b");

        let placement = LevelizedPlacer::new()
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 100.0,
                    y_pitch_um: 24.0,
                    fixed_nodes: Vec::new(),
                    blocked_regions: Vec::new(),
                    macro_halo_x_um: 0.0,
                    macro_halo_y_um: 0.0,
                    max_nodes_per_level: 0,
                },
            )
            .expect("placement");
        let mut pdk = Pdk::minimal("test");
        pdk.ptl_forbidden_ranges.push(LengthRange {
            min_um: 80.0,
            max_um: 120.0,
        });

        let report = SimpleRouter::new()
            .route(&netlist, &placement, &pdk, &RoutingConfig::default())
            .expect("route");

        assert_eq!(report.routes[0].mode, RouteMode::Jtl);
        assert_eq!(report.detoured_routes, 0);
        assert_eq!(report.jtl_routes, 1);
    }

    #[test]
    fn keeps_long_boundary_port_access_on_jtl() {
        let mut netlist = Netlist::new();
        let input = netlist.add_node(NodeKind::Port, "input");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        netlist
            .connect(PinRef { node: input, port: 0 }, PinRef { node: gate, port: 0 })
            .expect("input to gate");

        let placement = LevelizedPlacer::new()
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 100.0,
                    y_pitch_um: 24.0,
                    fixed_nodes: Vec::new(),
                    blocked_regions: Vec::new(),
                    macro_halo_x_um: 0.0,
                    macro_halo_y_um: 0.0,
                    max_nodes_per_level: 0,
                },
            )
            .expect("placement");

        let report = SimpleRouter::new()
            .route(&netlist, &placement, &Pdk::minimal("test"), &RoutingConfig::default())
            .expect("route");

        assert_eq!(report.routes[0].mode, RouteMode::Jtl);
        assert_eq!(report.detoured_routes, 0);
        assert_eq!(report.jtl_routes, 1);
        assert_eq!(report.ptl_routes, 0);
    }

    #[test]
    fn detours_around_blocked_region() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).expect("a to b");

        let placement = LevelizedPlacer::new()
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 100.0,
                    y_pitch_um: 24.0,
                    fixed_nodes: Vec::new(),
                    blocked_regions: Vec::new(),
                    macro_halo_x_um: 0.0,
                    macro_halo_y_um: 0.0,
                    max_nodes_per_level: 0,
                },
            )
            .expect("placement");

        let report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig {
                    blocked_regions: vec![BlockedRegion {
                        min_x_um: 40.0,
                        max_x_um: 60.0,
                        min_y_um: -4.0,
                        max_y_um: 4.0,
                    }],
                    ..RoutingConfig::default()
                },
            )
            .expect("route");

        assert!(report.routes[0].segments.len() >= 3);
        assert_eq!(report.detoured_routes, 1);
        assert!(report.total_detour_overhead_um > 0.0);
        assert!(report.routes[0].length_um > report.routes[0].direct_length_um);
        assert!(report.routes[0].length_um > 100.0);
    }

    #[test]
    fn finds_multi_turn_path_around_stacked_obstacles() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 }).expect("a to b");

        let placement = LevelizedPlacer::new()
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 120.0,
                    y_pitch_um: 48.0,
                    fixed_nodes: Vec::new(),
                    blocked_regions: Vec::new(),
                    macro_halo_x_um: 0.0,
                    macro_halo_y_um: 0.0,
                    max_nodes_per_level: 0,
                },
            )
            .expect("placement");

        let report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig {
                    blocked_regions: vec![
                        BlockedRegion {
                            min_x_um: 30.0,
                            max_x_um: 60.0,
                            min_y_um: -4.0,
                            max_y_um: 30.0,
                        },
                        BlockedRegion {
                            min_x_um: 70.0,
                            max_x_um: 100.0,
                            min_y_um: 18.0,
                            max_y_um: 52.0,
                        },
                    ],
                    ..RoutingConfig::default()
                },
            )
            .expect("route");

        assert!(report.routes[0].segments.len() >= 3);
        assert!(report.routes[0].length_um > report.routes[0].direct_length_um);
    }
}
