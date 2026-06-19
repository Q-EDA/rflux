#![allow(clippy::float_cmp)]
use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_place::{Placement, Point};
use rflux_tech::{InterconnectKind, Pdk};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub congestion_weight: f64,
    pub coupling_weight: f64,
    pub ptl_reflection_risk_weight: f64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            prefer_ptl_from_length_um: 60.0,
            jtl_layer: 1,
            ptl_layer: 2,
            blocked_regions: Vec::new(),
            detour_margin_um: 12.0,
            congestion_weight: 0.0,
            coupling_weight: 0.0,
            ptl_reflection_risk_weight: 0.0,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct CongestionMap {
    edge_usage: HashMap<(i64, i64, i64, i64), usize>,
}

impl CongestionMap {
    fn increment(&mut self, start: Point, end: Point) {
        let key = Self::edge_key(start, end);
        *self.edge_usage.entry(key).or_insert(0) += 1;
    }

    fn usage(&self, start: Point, end: Point) -> usize {
        let key = Self::edge_key(start, end);
        *self.edge_usage.get(&key).unwrap_or(&0)
    }

    fn edge_key(a: Point, b: Point) -> (i64, i64, i64, i64) {
        (
            axis_key(a.x_um),
            axis_key(a.y_um),
            axis_key(b.x_um),
            axis_key(b.y_um),
        )
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouteError {
    #[error("routing requires placement for every node")]
    MissingPlacement,
}

impl RouteError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            RouteError::MissingPlacement => "RFLOW-FLOW-003",
        }
    }

    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            RouteError::MissingPlacement => {
                "Ensure all nodes have placement coordinates before routing."
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoutingCache {
    cached_routes: HashMap<(PinRef, PinRef), NetRoute>,
}

impl RoutingCache {
    pub fn new() -> Self {
        Self {
            cached_routes: HashMap::new(),
        }
    }

    pub fn from_report(report: &RoutingReport) -> Self {
        let mut cache = Self::new();
        for route in &report.routes {
            cache
                .cached_routes
                .insert((route.from, route.to), route.clone());
        }
        cache
    }

    pub fn get(&self, from: PinRef, to: PinRef) -> Option<&NetRoute> {
        self.cached_routes.get(&(from, to))
    }

    pub fn insert(&mut self, route: NetRoute) {
        self.cached_routes
            .insert((route.from, route.to), route);
    }

    pub fn invalidate(&mut self, from: PinRef, to: PinRef) {
        self.cached_routes.remove(&(from, to));
    }

    pub fn len(&self) -> usize {
        self.cached_routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cached_routes.is_empty()
    }
}

impl Default for RoutingCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct SimpleRouter;

impl SimpleRouter {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[allow(dead_code)]
    fn route_single_net(
        &self,
        from: PinRef,
        to: PinRef,
        placement: &Placement,
        pdk: &Pdk,
        config: &RoutingConfig,
    ) -> Option<NetRoute> {
        let source = placement.point_of(from.node)?;
        let sink = placement.point_of(to.node)?;
        let path = choose_route_path(source, sink, config, &CongestionMap::default(), None);
        let direct_length_um = manhattan_length(source, sink);
        let length_um = path_length(&path);
        let touches_boundary_port = false;
        let ptl_allowed = !touches_boundary_port
            && length_um >= config.prefer_ptl_from_length_um
            && pdk.is_ptl_length_allowed(length_um);
        let reflection_risk = if ptl_allowed && config.ptl_reflection_risk_weight > 0.0 {
            pdk.ptl_reflection_coefficient(length_um)
        } else {
            0.0
        };
        let use_ptl = ptl_allowed && reflection_risk < 0.3;
        let mode = if use_ptl {
            RouteMode::Ptl
        } else {
            RouteMode::Jtl
        };
        let layer = if use_ptl {
            config.ptl_layer
        } else {
            config.jtl_layer
        };
        let segments: Vec<RouteSegment> = path
            .iter()
            .map(|(start, end)| RouteSegment {
                start: *start,
                end: *end,
                layer,
            })
            .collect();

        Some(NetRoute {
            from,
            to,
            mode,
            segments,
            direct_length_um,
            length_um,
        })
    }

    pub fn route_with_cache(
        &self,
        netlist: &Netlist,
        placement: &Placement,
        pdk: &Pdk,
        config: &RoutingConfig,
        cache: &RoutingCache,
    ) -> Result<(RoutingReport, RoutingCache), RouteError> {
        let mut routes = Vec::with_capacity(netlist.edge_count());
        let mut new_cache = cache.clone();
        let mut total_length_um = 0.0;
        let mut total_detour_overhead_um = 0.0;
        let mut detoured_routes = 0usize;
        let mut jtl_routes = 0usize;
        let mut ptl_routes = 0usize;

        for (from, to) in netlist.edge_pairs() {
            if let Some(cached) = cache.get(from, to) {
                let route = cached.clone();
                total_length_um += route.length_um;
                let detour_overhead_um =
                    (route.length_um - route.direct_length_um).max(0.0);
                total_detour_overhead_um += detour_overhead_um;
                if detour_overhead_um > 0.0 {
                    detoured_routes += 1;
                }
                match route.mode {
                    RouteMode::Jtl => jtl_routes += 1,
                    RouteMode::Ptl => ptl_routes += 1,
                }
                routes.push(route);
            } else {
                let source = placement
                    .point_of(from.node)
                    .ok_or(RouteError::MissingPlacement)?;
                let sink = placement
                    .point_of(to.node)
                    .ok_or(RouteError::MissingPlacement)?;
                let path = choose_route_path(
                    source,
                    sink,
                    config,
                    &CongestionMap::default(),
                    None,
                );
                let direct_length_um = manhattan_length(source, sink);
                let length_um = path_length(&path);
                let touches_boundary_port =
                    is_boundary_port(netlist, from.node) || is_boundary_port(netlist, to.node);
                let ptl_allowed = !touches_boundary_port
                    && length_um >= config.prefer_ptl_from_length_um
                    && pdk.is_ptl_length_allowed(length_um);
                let reflection_risk = if ptl_allowed && config.ptl_reflection_risk_weight > 0.0 {
                    pdk.ptl_reflection_coefficient(length_um)
                } else {
                    0.0
                };
                let use_ptl = ptl_allowed && reflection_risk < 0.3;
                let mode = if use_ptl {
                    RouteMode::Ptl
                } else {
                    RouteMode::Jtl
                };
                let layer = if use_ptl {
                    config.ptl_layer
                } else {
                    config.jtl_layer
                };
                let segments: Vec<RouteSegment> = path
                    .iter()
                    .map(|(start, end)| RouteSegment {
                        start: *start,
                        end: *end,
                        layer,
                    })
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
                let route = NetRoute {
                    from,
                    to,
                    mode,
                    segments,
                    direct_length_um,
                    length_um,
                };
                new_cache.insert(route.clone());
                routes.push(route);
            }
        }

        let report = RoutingReport {
            routes,
            total_length_um,
            total_detour_overhead_um,
            detoured_routes,
            jtl_routes,
            ptl_routes,
        };
        Ok((report, new_cache))
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
        let mut congestion = CongestionMap::default();
        let coupling_radius_um = 10.0;

        for (from, to) in netlist.edge_pairs() {
            let source = placement
                .point_of(from.node)
                .ok_or(RouteError::MissingPlacement)?;
            let sink = placement
                .point_of(to.node)
                .ok_or(RouteError::MissingPlacement)?;
            let coupling_map = if config.coupling_weight > 0.0 && !routes.is_empty() {
                Some(CouplingMap::build(&routes, coupling_radius_um))
            } else {
                None
            };
            let path = choose_route_path(source, sink, config, &congestion, coupling_map.as_ref());
            let direct_length_um = manhattan_length(source, sink);
            let length_um = path_length(&path);
            let touches_boundary_port =
                is_boundary_port(netlist, from.node) || is_boundary_port(netlist, to.node);
            let ptl_allowed = !touches_boundary_port
                && length_um >= config.prefer_ptl_from_length_um
                && pdk.is_ptl_length_allowed(length_um);
            let reflection_risk = if ptl_allowed && config.ptl_reflection_risk_weight > 0.0 {
                pdk.ptl_reflection_coefficient(length_um)
            } else {
                0.0
            };
            let use_ptl = ptl_allowed && reflection_risk < 0.3;
            let mode = if use_ptl {
                RouteMode::Ptl
            } else {
                RouteMode::Jtl
            };
            let layer = if use_ptl {
                config.ptl_layer
            } else {
                config.jtl_layer
            };
            let segments: Vec<RouteSegment> = path
                .iter()
                .map(|(start, end)| {
                    congestion.increment(*start, *end);
                    RouteSegment {
                        start: *start,
                        end: *end,
                        layer,
                    }
                })
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

fn choose_route_path(
    source: Point,
    sink: Point,
    config: &RoutingConfig,
    congestion: &CongestionMap,
    coupling_map: Option<&CouplingMap>,
) -> Vec<(Point, Point)> {
    shortest_grid_path(source, sink, config, congestion, coupling_map)
        .or_else(|| {
            let mut candidates = base_route_candidates(source, sink);
            for region in &config.blocked_regions {
                candidates.extend(detour_candidates(
                    source,
                    sink,
                    *region,
                    config.detour_margin_um,
                ));
            }

            let mut best_clear = None::<Vec<(Point, Point)>>;
            let mut best_cost = f64::INFINITY;
            for candidate in candidates {
                if path_is_clear(&candidate, &config.blocked_regions) {
                    let length = path_length(&candidate);
                    let congestion_cost: f64 = candidate
                        .iter()
                        .map(|(start, end)| congestion.usage(*start, *end) as f64 * config.congestion_weight)
                        .sum();
                    let coupling_cost: f64 = if let Some(cm) = coupling_map {
                        candidate
                            .iter()
                            .map(|(start, end)| {
                                cm.estimate_segment_coupling(*start, *end, config.jtl_layer)
                                    * config.coupling_weight
                            })
                            .sum()
                    } else {
                        0.0
                    };
                    let cost = length + congestion_cost + coupling_cost;
                    if cost < best_cost {
                        best_cost = cost;
                        best_clear = Some(candidate);
                    }
                }
            }
            best_clear
        })
        .unwrap_or_else(|| {
            base_route_candidates(source, sink)
                .into_iter()
                .next()
                .unwrap_or_default()
        })
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

fn shortest_grid_path(
    source: Point,
    sink: Point,
    config: &RoutingConfig,
    congestion: &CongestionMap,
    coupling_map: Option<&CouplingMap>,
) -> Option<Vec<(Point, Point)>> {
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

        #[allow(clippy::map_unwrap_or)]
        let adjacent_nodes = adjacency.get(&node).map(Vec::as_slice).unwrap_or(&[]);
        for &(next, weight) in adjacent_nodes {
            let congestion_penalty =
                congestion.usage(points[node], points[next]) as f64 * config.congestion_weight;
            let coupling_penalty = if let Some(cm) = coupling_map {
                cm.estimate_segment_coupling(points[node], points[next], config.jtl_layer)
                    * config.coupling_weight
            } else {
                0.0
            };
            let next_cost = cost + weight + congestion_penalty + coupling_penalty;
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
            let key = if axis_is_x {
                axis_key(point.x_um)
            } else {
                axis_key(point.y_um)
            };
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
            (
                source,
                Point {
                    x_um: source.x_um,
                    y_um: above_y,
                },
            ),
            (
                Point {
                    x_um: source.x_um,
                    y_um: above_y,
                },
                Point {
                    x_um: sink.x_um,
                    y_um: above_y,
                },
            ),
            (
                Point {
                    x_um: sink.x_um,
                    y_um: above_y,
                },
                sink,
            ),
        ]),
        normalize_path(vec![
            (
                source,
                Point {
                    x_um: source.x_um,
                    y_um: below_y,
                },
            ),
            (
                Point {
                    x_um: source.x_um,
                    y_um: below_y,
                },
                Point {
                    x_um: sink.x_um,
                    y_um: below_y,
                },
            ),
            (
                Point {
                    x_um: sink.x_um,
                    y_um: below_y,
                },
                sink,
            ),
        ]),
        normalize_path(vec![
            (
                source,
                Point {
                    x_um: left_x,
                    y_um: source.y_um,
                },
            ),
            (
                Point {
                    x_um: left_x,
                    y_um: source.y_um,
                },
                Point {
                    x_um: left_x,
                    y_um: sink.y_um,
                },
            ),
            (
                Point {
                    x_um: left_x,
                    y_um: sink.y_um,
                },
                sink,
            ),
        ]),
        normalize_path(vec![
            (
                source,
                Point {
                    x_um: right_x,
                    y_um: source.y_um,
                },
            ),
            (
                Point {
                    x_um: right_x,
                    y_um: source.y_um,
                },
                Point {
                    x_um: right_x,
                    y_um: sink.y_um,
                },
            ),
            (
                Point {
                    x_um: right_x,
                    y_um: sink.y_um,
                },
                sink,
            ),
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
    path.iter()
        .map(|(start, end)| manhattan_length(*start, *end))
        .sum()
}

fn is_boundary_port(netlist: &Netlist, node: rflux_ir::NodeId) -> bool {
    matches!(netlist.nodes()[node.0].kind, NodeKind::Port)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CouplingNeighbor {
    pub route_index: usize,
    pub parallel_length_um: f64,
    pub distance_um: f64,
    pub coupling_coefficient: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetCouplingInfo {
    pub total_coupling_score: f64,
    pub neighbors: Vec<CouplingNeighbor>,
    pub max_coefficient: f64,
}

#[derive(Debug, Clone)]
struct SpatialBin {
    segments: Vec<BinSegment>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct BinSegment {
    route_index: usize,
    segment_index: usize,
    start: Point,
    end: Point,
    layer: u8,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CouplingMap {
    bin_size_um: f64,
    bins: HashMap<(i64, i64), SpatialBin>,
    coupling_radius_um: f64,
    per_net: Vec<NetCouplingInfo>,
}

impl CouplingMap {
    pub fn build(routes: &[NetRoute], coupling_radius_um: f64) -> Self {
        let bin_size_um = coupling_radius_um.max(1.0);
        let mut bins: HashMap<(i64, i64), SpatialBin> = HashMap::new();

        for (route_idx, route) in routes.iter().enumerate() {
            for (seg_idx, seg) in route.segments.iter().enumerate() {
                let bin_entries = bins_covered_by_segment(seg, bin_size_um);
                for bin_key in bin_entries {
                    bins
                        .entry(bin_key)
                        .or_insert_with(|| SpatialBin {
                            segments: Vec::new(),
                        })
                        .segments
                        .push(BinSegment {
                            route_index: route_idx,
                            segment_index: seg_idx,
                            start: seg.start,
                            end: seg.end,
                            layer: seg.layer,
                        });
                }
            }
        }

        let mut per_net = Vec::with_capacity(routes.len());
        for (route_idx, route) in routes.iter().enumerate() {
            let info = Self::compute_net_coupling(route_idx, route, &bins, coupling_radius_um, bin_size_um);
            per_net.push(info);
        }

        CouplingMap {
            bin_size_um,
            bins,
            coupling_radius_um,
            per_net,
        }
    }

    fn compute_net_coupling(
        route_index: usize,
        route: &NetRoute,
        bins: &HashMap<(i64, i64), SpatialBin>,
        radius_um: f64,
        bin_size_um: f64,
    ) -> NetCouplingInfo {
        let mut neighbors: Vec<CouplingNeighbor> = Vec::new();
        let mut neighbor_map: HashMap<usize, (f64, f64, f64)> = HashMap::new();

        for seg in &route.segments {
            let candidate_bins = bins_covered_by_segment(seg, bin_size_um);
            for bin_key in candidate_bins {
                let Some(bin) = bins.get(&bin_key) else {
                    continue;
                };
                for other in &bin.segments {
                    if other.route_index == route_index {
                        continue;
                    }
                    if other.layer != seg.layer {
                        continue;
                    }

                    let (parallel_len, distance) =
                        parallel_length_and_distance(seg.start, seg.end, other.start, other.end);
                    if distance > radius_um || parallel_len < 0.1 {
                        continue;
                    }

                    let coeff = coupling_coefficient(parallel_len, distance);
                    let entry = neighbor_map
                        .entry(other.route_index)
                        .or_insert((0.0, f64::INFINITY, 0.0));
                    entry.0 += parallel_len;
                    entry.1 = entry.1.min(distance);
                    entry.2 = entry.2.max(coeff);
                }
            }
        }

        for (nbr_idx, (parallel_len, distance, coeff)) in neighbor_map {
            neighbors.push(CouplingNeighbor {
                route_index: nbr_idx,
                parallel_length_um: parallel_len,
                distance_um: distance,
                coupling_coefficient: coeff,
            });
        }
        neighbors.sort_by(|a, b| {
            b.coupling_coefficient
                .partial_cmp(&a.coupling_coefficient)
                .unwrap_or(Ordering::Equal)
        });

        let total_score: f64 = neighbors.iter().map(|n| n.coupling_coefficient).sum();
        let max_coeff = neighbors
            .iter()
            .map(|n| n.coupling_coefficient)
            .fold(0.0_f64, f64::max);

        NetCouplingInfo {
            total_coupling_score: total_score,
            neighbors,
            max_coefficient: max_coeff,
        }
    }

    pub fn net_info(&self, route_index: usize) -> Option<&NetCouplingInfo> {
        self.per_net.get(route_index)
    }

    pub fn total_coupling_score(&self) -> f64 {
        self.per_net.iter().map(|n| n.total_coupling_score).sum()
    }

    pub fn high_coupling_nets(&self, threshold: f64) -> usize {
        self.per_net
            .iter()
            .filter(|n| n.max_coefficient > threshold)
            .count()
    }

    pub fn coupling_delay_ps(&self, route_index: usize, base_delay_ps: f64, coefficient: f64) -> f64 {
        let coeff = if coefficient > 0.0 { coefficient } else { 0.05 };
        let Some(info) = self.per_net.get(route_index) else {
            return 0.0;
        };
        base_delay_ps * info.total_coupling_score * coeff
    }

    pub fn coupling_sigma_ps(&self, route_index: usize, base_delay_ps: f64, coefficient: f64) -> f64 {
        let coeff = if coefficient > 0.0 { coefficient } else { 0.02 };
        let Some(info) = self.per_net.get(route_index) else {
            return 0.0;
        };
        base_delay_ps * info.max_coefficient * coeff
    }

    pub fn estimate_segment_coupling(&self, start: Point, end: Point, layer: u8) -> f64 {
        let seg = RouteSegment { start, end, layer };
        let candidate_bins = bins_covered_by_segment(&seg, self.bin_size_um);
        let mut total = 0.0;
        for bin_key in candidate_bins {
            if let Some(bin) = self.bins.get(&bin_key) {
                for other in &bin.segments {
                    if other.layer != layer {
                        continue;
                    }
                    let (parallel_len, distance) =
                        parallel_length_and_distance(start, end, other.start, other.end);
                    if distance > self.coupling_radius_um || parallel_len < 0.1 {
                        continue;
                    }
                    total += coupling_coefficient(parallel_len, distance);
                }
            }
        }
        total
    }
}

fn bins_covered_by_segment(seg: &RouteSegment, bin_size_um: f64) -> Vec<(i64, i64)> {
    let min_x = seg.start.x_um.min(seg.end.x_um);
    let max_x = seg.start.x_um.max(seg.end.x_um);
    let min_y = seg.start.y_um.min(seg.end.y_um);
    let max_y = seg.start.y_um.max(seg.end.y_um);

    let bx_min = (min_x / bin_size_um).floor() as i64;
    let bx_max = (max_x / bin_size_um).floor() as i64;
    let by_min = (min_y / bin_size_um).floor() as i64;
    let by_max = (max_y / bin_size_um).floor() as i64;

    let mut bins = Vec::new();
    for bx in bx_min..=bx_max {
        for by in by_min..=by_max {
            bins.push((bx, by));
        }
    }
    bins
}

fn parallel_length_and_distance(
    a_start: Point,
    a_end: Point,
    b_start: Point,
    b_end: Point,
) -> (f64, f64) {
    let a_horiz = (a_start.y_um - a_end.y_um).abs() < 0.001;
    let b_horiz = (b_start.y_um - b_end.y_um).abs() < 0.001;
    let a_vert = (a_start.x_um - a_end.x_um).abs() < 0.001;
    let b_vert = (b_start.x_um - b_end.x_um).abs() < 0.001;

    if a_horiz && b_horiz {
        let distance = (a_start.y_um - b_start.y_um).abs();
        let overlap = segment_overlap_1d(
            a_start.x_um.min(a_end.x_um),
            a_start.x_um.max(a_end.x_um),
            b_start.x_um.min(b_end.x_um),
            b_start.x_um.max(b_end.x_um),
        );
        (overlap, distance)
    } else if a_vert && b_vert {
        let distance = (a_start.x_um - b_start.x_um).abs();
        let overlap = segment_overlap_1d(
            a_start.y_um.min(a_end.y_um),
            a_start.y_um.max(a_end.y_um),
            b_start.y_um.min(b_end.y_um),
            b_start.y_um.max(b_end.y_um),
        );
        (overlap, distance)
    } else {
        let dx = a_start.x_um - b_start.x_um;
        let dy = a_start.y_um - b_start.y_um;
        let distance = (dx * dx + dy * dy).sqrt();
        (0.0, distance)
    }
}

fn segment_overlap_1d(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    let overlap_start = a_min.max(b_min);
    let overlap_end = a_max.min(b_max);
    (overlap_end - overlap_start).max(0.0)
}

fn coupling_coefficient(parallel_length_um: f64, distance_um: f64) -> f64 {
    if distance_um < 0.001 {
        return 1.0;
    }
    let length_factor = (parallel_length_um / 10.0).min(5.0);
    let distance_factor = 1.0 / (1.0 + (distance_um / 5.0).powi(2));
    (length_factor * distance_factor).min(1.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReflectionSite {
    pub route_index: usize,
    pub segment_index: usize,
    pub from_mode: RouteMode,
    pub to_mode: RouteMode,
    pub boundary_coefficient: f64,
    pub resonance_coefficient: f64,
    pub reflected_delay_ps: f64,
    pub position: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteReflectionInfo {
    pub sites: Vec<ReflectionSite>,
    pub total_reflection_energy: f64,
    pub max_coefficient: f64,
    pub has_risk: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReflectionReport {
    pub per_route: Vec<RouteReflectionInfo>,
    pub total_risk_routes: usize,
    pub total_boundary_sites: usize,
    pub max_reflection_energy: f64,
}

#[derive(Debug, Clone)]
pub struct ReflectionAnalyzer {
    risk_threshold: f64,
}

impl ReflectionAnalyzer {
    pub fn new(risk_threshold: f64) -> Self {
        Self { risk_threshold }
    }

    pub fn analyze(&self, routes: &[NetRoute], pdk: &Pdk, config: &RoutingConfig) -> ReflectionReport {
        let mut per_route = Vec::with_capacity(routes.len());
        let mut total_risk_routes = 0usize;
        let mut total_boundary_sites = 0usize;
        let mut max_energy = 0.0_f64;

        for (route_idx, route) in routes.iter().enumerate() {
            let info = self.analyze_route(route_idx, route, pdk, config);
            if info.has_risk {
                total_risk_routes += 1;
            }
            total_boundary_sites += info.sites.len();
            max_energy = max_energy.max(info.total_reflection_energy);
            per_route.push(info);
        }

        ReflectionReport {
            per_route,
            total_risk_routes,
            total_boundary_sites,
            max_reflection_energy: max_energy,
        }
    }

    fn analyze_route(&self, route_idx: usize, route: &NetRoute, pdk: &Pdk, config: &RoutingConfig) -> RouteReflectionInfo {
        let mut sites = Vec::new();
        let mut total_energy = 0.0_f64;
        let mut max_coeff = 0.0_f64;

        for (seg_idx, window) in route.segments.windows(2).enumerate() {
            let from_layer = window[0].layer;
            let to_layer = window[1].layer;
            if from_layer == to_layer {
                continue;
            }

            let from_mode = layer_to_interconnect_kind(from_layer, config);
            let to_mode = layer_to_interconnect_kind(to_layer, config);
            if from_mode == to_mode {
                continue;
            }

            let boundary_gamma = pdk.boundary_reflection_coefficient(from_mode, to_mode);
            let boundary_coeff = boundary_gamma.abs();
            let transition_point = window[0].end;
            let delay_ps = self.reflected_delay_ps(&window[0], pdk, config);

            total_energy += boundary_coeff * boundary_coeff;
            max_coeff = max_coeff.max(boundary_coeff);

            if boundary_coeff > self.risk_threshold {
                sites.push(ReflectionSite {
                    route_index: route_idx,
                    segment_index: seg_idx,
                    from_mode: interconnect_to_route_mode(from_mode),
                    to_mode: interconnect_to_route_mode(to_mode),
                    boundary_coefficient: boundary_coeff,
                    resonance_coefficient: 0.0,
                    reflected_delay_ps: delay_ps,
                    position: transition_point,
                });
            }
        }

        for seg in &route.segments {
            if layer_to_interconnect_kind(seg.layer, config) == InterconnectKind::Ptl {
                let length_um = manhattan_length(seg.start, seg.end);
                let resonance = pdk.ptl_reflection_coefficient(length_um);
                if resonance > self.risk_threshold {
                    let delay_ps = 2.0 * length_um * pdk.ptl_propagation_delay_ps_per_um;
                    total_energy += resonance * resonance;
                    max_coeff = max_coeff.max(resonance);
                    sites.push(ReflectionSite {
                        route_index: route_idx,
                        segment_index: 0,
                        from_mode: RouteMode::Ptl,
                        to_mode: RouteMode::Ptl,
                        boundary_coefficient: 0.0,
                        resonance_coefficient: resonance,
                        reflected_delay_ps: delay_ps,
                        position: seg.start,
                    });
                }
            }
        }

        let has_risk = !sites.is_empty();

        RouteReflectionInfo {
            sites,
            total_reflection_energy: total_energy,
            max_coefficient: max_coeff,
            has_risk,
        }
    }

    fn reflected_delay_ps(&self, seg: &RouteSegment, pdk: &Pdk, config: &RoutingConfig) -> f64 {
        let length_um = manhattan_length(seg.start, seg.end);
        let delay_per_um = match layer_to_interconnect_kind(seg.layer, config) {
            InterconnectKind::Jtl => pdk.jtl_propagation_delay_ps_per_um,
            InterconnectKind::Ptl => pdk.ptl_propagation_delay_ps_per_um,
        };
        2.0 * length_um * delay_per_um
    }
}

fn layer_to_interconnect_kind(layer: u8, config: &RoutingConfig) -> InterconnectKind {
    if layer == config.ptl_layer {
        InterconnectKind::Ptl
    } else {
        InterconnectKind::Jtl
    }
}

fn interconnect_to_route_mode(kind: InterconnectKind) -> RouteMode {
    match kind {
        InterconnectKind::Jtl => RouteMode::Jtl,
        InterconnectKind::Ptl => RouteMode::Ptl,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{NodeId, NodeKind, PinRef};
    use rflux_place::{LevelizedPlacer, PlacementConfig};
    use rflux_tech::{LengthRange, Pdk};

    #[test]
    fn routes_short_net_with_jtl() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

        let placement = LevelizedPlacer::new()
            .place(&netlist, &PlacementConfig::default())
            .expect("placement");
        let report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");

        assert_eq!(report.routes.len(), 1);
        assert_eq!(report.routes[0].mode, RouteMode::Jtl);
        assert_eq!(
            report.routes[0].direct_length_um,
            report.routes[0].length_um
        );
        assert_eq!(report.jtl_routes, 1);
    }

    #[test]
    fn routes_long_net_with_ptl_when_allowed() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

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
                &RoutingConfig::default(),
            )
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
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

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
                &RoutingConfig::default(),
            )
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
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

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
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

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

    #[test]
    fn empty_netlist_produces_empty_report() {
        let netlist = Netlist::new();
        let placement = Placement {
            nodes: Vec::new(),
            width_um: 0.0,
            height_um: 0.0,
        };
        let report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");
        assert!(report.routes.is_empty());
        assert_eq!(report.total_length_um, 0.0);
        assert_eq!(report.jtl_routes, 0);
        assert_eq!(report.ptl_routes, 0);
    }

    #[test]
    fn default_routing_config_has_expected_values() {
        let config = RoutingConfig::default();
        assert_eq!(config.prefer_ptl_from_length_um, 60.0);
        assert_eq!(config.jtl_layer, 1);
        assert_eq!(config.ptl_layer, 2);
        assert!(config.blocked_regions.is_empty());
        assert_eq!(config.detour_margin_um, 12.0);
    }

    #[test]
    fn router_default_creates_valid_instance() {
        let router = SimpleRouter::default();
        let netlist = Netlist::new();
        let placement = Placement {
            nodes: Vec::new(),
            width_um: 0.0,
            height_um: 0.0,
        };
        let report = router
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");
        assert!(report.routes.is_empty());
    }

    #[test]
    fn uses_custom_layer_indices() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

        let placement = LevelizedPlacer::new()
            .place(&netlist, &PlacementConfig::default())
            .expect("placement");
        let report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig {
                    jtl_layer: 5,
                    ptl_layer: 7,
                    ..RoutingConfig::default()
                },
            )
            .expect("route");

        assert_eq!(report.routes[0].segments[0].layer, 5);
    }

    #[test]
    fn reports_correct_route_counts_for_multi_net() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        let d = netlist.add_node(NodeKind::Port, "d");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: d, port: 0 })
            .expect("c to d");

        let placement = LevelizedPlacer::new()
            .place(&netlist, &PlacementConfig::default())
            .expect("placement");
        let report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");

        assert_eq!(report.routes.len(), 2);
        assert_eq!(report.jtl_routes, 2);
        assert_eq!(report.ptl_routes, 0);
        assert!(report.total_length_um > 0.0);
    }

    #[test]
    fn reports_missing_placement_for_unplaced_node() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

        let placement = Placement {
            nodes: Vec::new(),
            width_um: 0.0,
            height_um: 0.0,
        };
        let err = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .unwrap_err();
        assert_eq!(err, RouteError::MissingPlacement);
    }

    #[test]
    fn route_error_codes_are_stable() {
        assert_eq!(
            RouteError::MissingPlacement.code(),
            "RFLOW-FLOW-003"
        );
        assert!(!RouteError::MissingPlacement.suggestion().is_empty());
    }

    #[test]
    fn congestion_weight_defaults_to_zero() {
        let config = RoutingConfig::default();
        assert_eq!(config.congestion_weight, 0.0);
    }

    #[test]
    fn congestion_aware_routing_avoids_shared_path() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        let d = netlist.add_node(NodeKind::Port, "d");
        let e = netlist.add_node(NodeKind::CellInstance, "e");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: d, port: 0 })
            .expect("c to d");
        netlist
            .connect(PinRef { node: b, port: 1 }, PinRef { node: e, port: 0 })
            .expect("b to e");

        let placement = LevelizedPlacer::new()
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 80.0,
                    y_pitch_um: 24.0,
                    ..PlacementConfig::default()
                },
            )
            .expect("placement");

        let report_no_congestion = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");

        let report_with_congestion = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig {
                    congestion_weight: 50.0,
                    ..RoutingConfig::default()
                },
            )
            .expect("route");

        assert_eq!(report_no_congestion.routes.len(), 3);
        assert_eq!(report_with_congestion.routes.len(), 3);

        assert!(report_no_congestion.total_length_um > 0.0);
        assert!(report_with_congestion.total_length_um > 0.0);
    }

    #[test]
    fn congestion_map_tracks_edge_usage() {
        let mut congestion = CongestionMap::default();
        let a = Point {
            x_um: 0.0,
            y_um: 0.0,
        };
        let b = Point {
            x_um: 40.0,
            y_um: 0.0,
        };

        assert_eq!(congestion.usage(a, b), 0);
        congestion.increment(a, b);
        assert_eq!(congestion.usage(a, b), 1);
        congestion.increment(a, b);
        assert_eq!(congestion.usage(a, b), 2);
        assert_eq!(congestion.usage(b, a), 0);
    }

    #[test]
    fn default_routing_config_has_congestion_weight() {
        let config = RoutingConfig::default();
        assert_eq!(config.congestion_weight, 0.0);
        assert_eq!(config.prefer_ptl_from_length_um, 60.0);
    }

    #[test]
    fn coupling_map_empty_for_no_routes() {
        let routes: Vec<NetRoute> = Vec::new();
        let cmap = CouplingMap::build(&routes, 10.0);
        assert_eq!(cmap.total_coupling_score(), 0.0);
        assert_eq!(cmap.high_coupling_nets(0.1), 0);
    }

    #[test]
    fn coupling_map_single_route_has_no_neighbors() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Jtl,
            segments: vec![RouteSegment {
                start: Point { x_um: 0.0, y_um: 0.0 },
                end: Point { x_um: 40.0, y_um: 0.0 },
                layer: 1,
            }],
            direct_length_um: 40.0,
            length_um: 40.0,
        }];
        let cmap = CouplingMap::build(&routes, 10.0);
        assert_eq!(cmap.total_coupling_score(), 0.0);
        let info = cmap.net_info(0).expect("should have info");
        assert!(info.neighbors.is_empty());
    }

    #[test]
    fn coupling_map_detects_parallel_routes() {
        let routes = vec![
            NetRoute {
                from: PinRef { node: NodeId(0), port: 0 },
                to: PinRef { node: NodeId(1), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
            NetRoute {
                from: PinRef { node: NodeId(2), port: 0 },
                to: PinRef { node: NodeId(3), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 4.0 },
                    end: Point { x_um: 40.0, y_um: 4.0 },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
        ];
        let cmap = CouplingMap::build(&routes, 10.0);
        let info0 = cmap.net_info(0).expect("should have info for route 0");
        assert_eq!(info0.neighbors.len(), 1);
        assert_eq!(info0.neighbors[0].route_index, 1);
        assert!(info0.neighbors[0].coupling_coefficient > 0.0);
        assert!(info0.total_coupling_score > 0.0);
    }

    #[test]
    fn coupling_map_ignores_different_layers() {
        let routes = vec![
            NetRoute {
                from: PinRef { node: NodeId(0), port: 0 },
                to: PinRef { node: NodeId(1), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
            NetRoute {
                from: PinRef { node: NodeId(2), port: 0 },
                to: PinRef { node: NodeId(3), port: 0 },
                mode: RouteMode::Ptl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 4.0 },
                    end: Point { x_um: 40.0, y_um: 4.0 },
                    layer: 2,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
        ];
        let cmap = CouplingMap::build(&routes, 10.0);
        assert_eq!(cmap.total_coupling_score(), 0.0);
    }

    #[test]
    fn coupling_map_ignores_distant_routes() {
        let routes = vec![
            NetRoute {
                from: PinRef { node: NodeId(0), port: 0 },
                to: PinRef { node: NodeId(1), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
            NetRoute {
                from: PinRef { node: NodeId(2), port: 0 },
                to: PinRef { node: NodeId(3), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 50.0 },
                    end: Point { x_um: 40.0, y_um: 50.0 },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
        ];
        let cmap = CouplingMap::build(&routes, 10.0);
        assert_eq!(cmap.total_coupling_score(), 0.0);
    }

    #[test]
    fn coupling_coefficient_decreases_with_distance() {
        let close = coupling_coefficient(20.0, 2.0);
        let medium = coupling_coefficient(20.0, 10.0);
        let far = coupling_coefficient(20.0, 30.0);
        assert!(close > medium);
        assert!(medium > far);
    }

    #[test]
    fn coupling_coefficient_increases_with_parallel_length() {
        let short = coupling_coefficient(5.0, 5.0);
        let long = coupling_coefficient(50.0, 5.0);
        assert!(long > short);
    }

    #[test]
    fn coupling_map_high_coupling_nets_threshold() {
        let routes = vec![
            NetRoute {
                from: PinRef { node: NodeId(0), port: 0 },
                to: PinRef { node: NodeId(1), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
            NetRoute {
                from: PinRef { node: NodeId(2), port: 0 },
                to: PinRef { node: NodeId(3), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point { x_um: 0.0, y_um: 3.0 },
                    end: Point { x_um: 40.0, y_um: 3.0 },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            },
        ];
        let cmap = CouplingMap::build(&routes, 10.0);
        assert_eq!(cmap.high_coupling_nets(0.01), 2);
        assert_eq!(cmap.high_coupling_nets(1.5), 0);
    }

    #[test]
    fn coupling_map_per_net_info_returns_none_for_out_of_bounds() {
        let routes: Vec<NetRoute> = Vec::new();
        let cmap = CouplingMap::build(&routes, 10.0);
        assert!(cmap.net_info(99).is_none());
    }

    #[test]
    fn segment_overlap_1d_basic_cases() {
        assert_eq!(segment_overlap_1d(0.0, 10.0, 5.0, 15.0), 5.0);
        assert_eq!(segment_overlap_1d(0.0, 10.0, 20.0, 30.0), 0.0);
        assert_eq!(segment_overlap_1d(0.0, 10.0, 0.0, 10.0), 10.0);
    }

    #[test]
    fn reflection_analyzer_empty_routes() {
        let routes: Vec<NetRoute> = Vec::new();
        let pdk = Pdk::minimal("test");
        let config = RoutingConfig::default();
        let analyzer = ReflectionAnalyzer::new(0.1);
        let report = analyzer.analyze(&routes, &pdk, &config);
        assert_eq!(report.total_risk_routes, 0);
        assert_eq!(report.total_boundary_sites, 0);
        assert_eq!(report.max_reflection_energy, 0.0);
    }

    #[test]
    fn reflection_analyzer_single_jtl_route_no_risk() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Jtl,
            segments: vec![RouteSegment {
                start: Point { x_um: 0.0, y_um: 0.0 },
                end: Point { x_um: 40.0, y_um: 0.0 },
                layer: 1,
            }],
            direct_length_um: 40.0,
            length_um: 40.0,
        }];
        let pdk = Pdk::minimal("test");
        let config = RoutingConfig::default();
        let analyzer = ReflectionAnalyzer::new(0.1);
        let report = analyzer.analyze(&routes, &pdk, &config);
        assert_eq!(report.total_risk_routes, 0);
        assert_eq!(report.total_boundary_sites, 0);
    }

    #[test]
    fn reflection_analyzer_detects_jtl_ptl_boundary() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Jtl,
            segments: vec![
                RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 1,
                },
                RouteSegment {
                    start: Point { x_um: 40.0, y_um: 0.0 },
                    end: Point { x_um: 80.0, y_um: 0.0 },
                    layer: 2,
                },
            ],
            direct_length_um: 80.0,
            length_um: 80.0,
        }];
        let pdk = Pdk::minimal("test");
        let config = RoutingConfig::default();
        let analyzer = ReflectionAnalyzer::new(0.01);
        let report = analyzer.analyze(&routes, &pdk, &config);

        assert_eq!(report.total_risk_routes, 1);
        assert!(report.total_boundary_sites >= 1);
        let boundary_site = report.per_route[0]
            .sites
            .iter()
            .find(|s| s.boundary_coefficient > 0.0)
            .expect("should have a boundary site");
        assert_eq!(boundary_site.route_index, 0);
        assert_eq!(boundary_site.from_mode, RouteMode::Jtl);
        assert_eq!(boundary_site.to_mode, RouteMode::Ptl);
        assert!(boundary_site.boundary_coefficient > 0.0);
        assert!(boundary_site.reflected_delay_ps > 0.0);
    }

    #[test]
    fn reflection_analyzer_detects_ptl_jtl_boundary() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Ptl,
            segments: vec![
                RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 2,
                },
                RouteSegment {
                    start: Point { x_um: 40.0, y_um: 0.0 },
                    end: Point { x_um: 80.0, y_um: 0.0 },
                    layer: 1,
                },
            ],
            direct_length_um: 80.0,
            length_um: 80.0,
        }];
        let pdk = Pdk::minimal("test");
        let config = RoutingConfig::default();
        let analyzer = ReflectionAnalyzer::new(0.01);
        let report = analyzer.analyze(&routes, &pdk, &config);

        assert_eq!(report.total_risk_routes, 1);
        let site = &report.per_route[0].sites[0];
        assert_eq!(site.from_mode, RouteMode::Ptl);
        assert_eq!(site.to_mode, RouteMode::Jtl);
    }

    #[test]
    fn reflection_analyzer_same_layer_no_boundary() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Jtl,
            segments: vec![
                RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 1,
                },
                RouteSegment {
                    start: Point { x_um: 40.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 24.0 },
                    layer: 1,
                },
            ],
            direct_length_um: 64.0,
            length_um: 64.0,
        }];
        let pdk = Pdk::minimal("test");
        let config = RoutingConfig::default();
        let analyzer = ReflectionAnalyzer::new(0.1);
        let report = analyzer.analyze(&routes, &pdk, &config);

        assert_eq!(report.total_boundary_sites, 0);
    }

    #[test]
    fn reflection_analyzer_high_threshold_filters_sites() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Jtl,
            segments: vec![
                RouteSegment {
                    start: Point { x_um: 0.0, y_um: 0.0 },
                    end: Point { x_um: 40.0, y_um: 0.0 },
                    layer: 1,
                },
                RouteSegment {
                    start: Point { x_um: 40.0, y_um: 0.0 },
                    end: Point { x_um: 80.0, y_um: 0.0 },
                    layer: 2,
                },
            ],
            direct_length_um: 80.0,
            length_um: 80.0,
        }];
        let pdk = Pdk::minimal("test");
        let config = RoutingConfig::default();

        let low = ReflectionAnalyzer::new(0.01);
        let high = ReflectionAnalyzer::new(0.99);
        let r_low = low.analyze(&routes, &pdk, &config);
        let r_high = high.analyze(&routes, &pdk, &config);

        assert!(r_low.total_boundary_sites >= r_high.total_boundary_sites);
    }

    #[test]
    fn reflection_analyzer_ptl_resonance_detection() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Ptl,
            segments: vec![RouteSegment {
                start: Point { x_um: 0.0, y_um: 0.0 },
                end: Point { x_um: 500.0, y_um: 0.0 },
                layer: 2,
            }],
            direct_length_um: 500.0,
            length_um: 500.0,
        }];
        let pdk = Pdk::minimal("test");
        let config = RoutingConfig::default();
        let analyzer = ReflectionAnalyzer::new(0.1);
        let report = analyzer.analyze(&routes, &pdk, &config);

        assert!(report.per_route[0].sites.iter().any(|s| s.resonance_coefficient > 0.0));
        assert!(report.per_route[0].has_risk);
    }

    #[test]
    fn boundary_reflection_coefficient_symmetric() {
        let pdk = Pdk::minimal("test");
        let gamma_jtl_ptl = pdk.boundary_reflection_coefficient(InterconnectKind::Jtl, InterconnectKind::Ptl);
        let gamma_ptl_jtl = pdk.boundary_reflection_coefficient(InterconnectKind::Ptl, InterconnectKind::Jtl);
        assert!((gamma_jtl_ptl + gamma_ptl_jtl).abs() < 1e-10);
    }

    #[test]
    fn boundary_reflection_coefficient_same_mode_zero() {
        let pdk = Pdk::minimal("test");
        assert_eq!(pdk.boundary_reflection_coefficient(InterconnectKind::Jtl, InterconnectKind::Jtl), 0.0);
        assert_eq!(pdk.boundary_reflection_coefficient(InterconnectKind::Ptl, InterconnectKind::Ptl), 0.0);
    }

    #[test]
    fn coupling_weight_affects_routing() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        let d = netlist.add_node(NodeKind::CellInstance, "d");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: d, port: 0 })
            .expect("c to d");

        let placement = LevelizedPlacer::new()
            .place(
                &netlist,
                &PlacementConfig {
                    x_pitch_um: 80.0,
                    y_pitch_um: 24.0,
                    ..PlacementConfig::default()
                },
            )
            .expect("placement");

        let report_no_coupling = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");

        let report_with_coupling = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig {
                    coupling_weight: 10.0,
                    ..RoutingConfig::default()
                },
            )
            .expect("route");

        assert_eq!(report_no_coupling.routes.len(), 2);
        assert_eq!(report_with_coupling.routes.len(), 2);
        assert!(report_no_coupling.total_length_um > 0.0);
        assert!(report_with_coupling.total_length_um >= report_no_coupling.total_length_um);
    }

    #[test]
    fn coupling_weight_zero_is_default() {
        let config = RoutingConfig::default();
        assert_eq!(config.coupling_weight, 0.0);
    }

    #[test]
    fn estimate_segment_coupling_returns_zero_when_empty() {
        let routes: Vec<NetRoute> = Vec::new();
        let cmap = CouplingMap::build(&routes, 10.0);
        let score = cmap.estimate_segment_coupling(
            Point { x_um: 0.0, y_um: 0.0 },
            Point { x_um: 40.0, y_um: 0.0 },
            1,
        );
        assert_eq!(score, 0.0);
    }

    #[test]
    fn estimate_segment_coupling_detects_nearby_route() {
        let routes = vec![NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode: RouteMode::Jtl,
            segments: vec![RouteSegment {
                start: Point { x_um: 0.0, y_um: 0.0 },
                end: Point { x_um: 40.0, y_um: 0.0 },
                layer: 1,
            }],
            direct_length_um: 40.0,
            length_um: 40.0,
        }];
        let cmap = CouplingMap::build(&routes, 10.0);
        let close = cmap.estimate_segment_coupling(
            Point { x_um: 0.0, y_um: 3.0 },
            Point { x_um: 40.0, y_um: 3.0 },
            1,
        );
        let far = cmap.estimate_segment_coupling(
            Point { x_um: 0.0, y_um: 50.0 },
            Point { x_um: 40.0, y_um: 50.0 },
            1,
        );
        assert!(close > 0.0);
        assert_eq!(far, 0.0);
    }

    #[test]
    fn routing_cache_stores_and_retrieves() {
        let mut cache = RoutingCache::new();
        let from = PinRef { node: NodeId(0), port: 0 };
        let to = PinRef { node: NodeId(1), port: 0 };
        let route = NetRoute {
            from,
            to,
            mode: RouteMode::Jtl,
            segments: vec![],
            direct_length_um: 10.0,
            length_um: 10.0,
        };
        cache.insert(route);
        assert!(cache.get(from, to).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn routing_cache_returns_none_for_missing() {
        let cache = RoutingCache::new();
        let from = PinRef { node: NodeId(0), port: 0 };
        let to = PinRef { node: NodeId(1), port: 0 };
        assert!(cache.get(from, to).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn routing_cache_invalidate_removes_entry() {
        let mut cache = RoutingCache::new();
        let from = PinRef { node: NodeId(0), port: 0 };
        let to = PinRef { node: NodeId(1), port: 0 };
        cache.insert(NetRoute {
            from,
            to,
            mode: RouteMode::Jtl,
            segments: vec![],
            direct_length_um: 10.0,
            length_um: 10.0,
        });
        assert_eq!(cache.len(), 1);
        cache.invalidate(from, to);
        assert!(cache.get(from, to).is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn routing_cache_from_report() {
        let report = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef { node: NodeId(0), port: 0 },
                to: PinRef { node: NodeId(1), port: 0 },
                mode: RouteMode::Jtl,
                segments: vec![],
                direct_length_um: 10.0,
                length_um: 10.0,
            }],
            total_length_um: 10.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };
        let cache = RoutingCache::from_report(&report);
        assert_eq!(cache.len(), 1);
        let from = PinRef { node: NodeId(0), port: 0 };
        let to = PinRef { node: NodeId(1), port: 0 };
        assert!(cache.get(from, to).is_some());
    }

    #[test]
    fn route_with_cache_reuses_cached_route() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");

        let placement = LevelizedPlacer::new()
            .place(&netlist, &PlacementConfig::default())
            .expect("placement");

        let first_report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");

        let cache = RoutingCache::from_report(&first_report);
        let (second_report, new_cache) = SimpleRouter::new()
            .route_with_cache(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
                &cache,
            )
            .expect("route_with_cache");

        assert_eq!(second_report.routes.len(), 1);
        assert_eq!(second_report.routes[0].mode, RouteMode::Jtl);
        assert_eq!(new_cache.len(), 1);
    }

    #[test]
    fn route_with_cache_routes_uncached_nets() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::CellInstance, "c");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");
        netlist
            .connect(PinRef { node: b, port: 1 }, PinRef { node: c, port: 0 })
            .expect("b to c");

        let placement = LevelizedPlacer::new()
            .place(&netlist, &PlacementConfig::default())
            .expect("placement");

        let first_report = SimpleRouter::new()
            .route(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
            )
            .expect("route");

        let mut cache = RoutingCache::from_report(&first_report);
        cache.invalidate(
            PinRef { node: b, port: 1 },
            PinRef { node: c, port: 0 },
        );
        assert_eq!(cache.len(), 1);

        let (second_report, new_cache) = SimpleRouter::new()
            .route_with_cache(
                &netlist,
                &placement,
                &Pdk::minimal("test"),
                &RoutingConfig::default(),
                &cache,
            )
            .expect("route_with_cache");

        assert_eq!(second_report.routes.len(), 2);
        assert_eq!(new_cache.len(), 2);
    }
}
