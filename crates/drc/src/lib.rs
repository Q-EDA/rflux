use std::collections::{BTreeSet, HashMap};

use rflux_ir::{Netlist, NodeKind};
use rflux_place::{Placement, Point};
use rflux_route::{NetRoute, RouteMode, RouteSegment, RoutingReport};
use rflux_tech::Pdk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrcSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrcViolation {
    pub rule: String,
    pub severity: DrcSeverity,
    pub location: Option<Point>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrcReport {
    pub violations: Vec<DrcViolation>,
    pub checked_rules: Vec<String>,
    pub error_count: usize,
    pub warning_count: usize,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetMismatch {
    pub net_name: String,
    pub issue: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LvsReport {
    pub matched: bool,
    pub device_count_mismatch: bool,
    pub connectivity_mismatch: bool,
    pub missing_devices: Vec<String>,
    pub extra_devices: Vec<String>,
    pub net_mismatches: Vec<NetMismatch>,
    pub checked_nets: usize,
    pub matched_nets: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrcRuleSet {
    pub min_trace_spacing_um: f64,
    pub min_jj_spacing_um: f64,
    pub max_ptl_length_um: Option<f64>,
    pub cell_boundary_margin_um: f64,
    pub layout_width_um: f64,
    pub layout_height_um: f64,
}

impl DrcRuleSet {
    pub fn from_pdk(pdk: &Pdk, width_um: f64, height_um: f64) -> Self {
        let (spacing, jj_spacing, boundary_margin) = if let Some(rules) = &pdk.drc_rules {
            (
                rules.min_trace_spacing_um,
                rules.min_jj_spacing_um,
                rules.cell_boundary_margin_um,
            )
        } else {
            (1.0, 5.0, 2.0)
        };
        Self {
            min_trace_spacing_um: spacing,
            min_jj_spacing_um: jj_spacing,
            max_ptl_length_um: None,
            cell_boundary_margin_um: boundary_margin,
            layout_width_um: width_um,
            layout_height_um: height_um,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DrcChecker {
    rules: DrcRuleSet,
}

impl DrcChecker {
    pub fn new(rules: DrcRuleSet) -> Self {
        Self { rules }
    }

    pub fn check(
        &self,
        placement: &Placement,
        routing: &RoutingReport,
        netlist: &Netlist,
    ) -> DrcReport {
        let mut violations = Vec::new();
        let mut checked_rules = Vec::new();

        checked_rules.push("trace_spacing".to_string());
        self.check_trace_spacing(routing, &mut violations);

        checked_rules.push("ptl_length".to_string());
        self.check_ptl_length(routing, &mut violations);

        checked_rules.push("jj_spacing".to_string());
        self.check_jj_spacing(placement, netlist, &mut violations);

        checked_rules.push("cell_boundary".to_string());
        self.check_cell_boundary(placement, netlist, &mut violations);

        let error_count = violations
            .iter()
            .filter(|v| v.severity == DrcSeverity::Error)
            .count();
        let warning_count = violations
            .iter()
            .filter(|v| v.severity == DrcSeverity::Warning)
            .count();

        DrcReport {
            violations,
            checked_rules,
            error_count,
            warning_count,
            passed: error_count == 0,
        }
    }

    fn check_trace_spacing(&self, routing: &RoutingReport, violations: &mut Vec<DrcViolation>) {
        let min_spacing = self.rules.min_trace_spacing_um;
        for i in 0..routing.routes.len() {
            for j in (i + 1)..routing.routes.len() {
                let r1 = &routing.routes[i];
                let r2 = &routing.routes[j];
                if same_net(r1, r2) {
                    continue;
                }
                for s1 in &r1.segments {
                    for s2 in &r2.segments {
                        if s1.layer != s2.layer {
                            continue;
                        }
                        let dist = segment_distance(s1, s2);
                        if dist < min_spacing {
                            let mid = midpoint(s1);
                            violations.push(DrcViolation {
                                rule: "trace_spacing".to_string(),
                                severity: DrcSeverity::Error,
                                location: Some(mid),
                                detail: format!(
                                    "Trace spacing {:.3} um < {:.3} um between nets on layer {}",
                                    dist, min_spacing, s1.layer
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    fn check_ptl_length(&self, routing: &RoutingReport, violations: &mut Vec<DrcViolation>) {
        let Some(max_length) = self.rules.max_ptl_length_um else {
            return;
        };
        for route in &routing.routes {
            if route.mode != RouteMode::Ptl {
                continue;
            }
            if route.length_um > max_length {
                let mid = route.segments.first().map(midpoint);
                violations.push(DrcViolation {
                    rule: "ptl_length".to_string(),
                    severity: DrcSeverity::Warning,
                    location: mid,
                    detail: format!(
                        "PTL route length {:.3} um > max {:.3} um",
                        route.length_um, max_length
                    ),
                });
            }
        }
    }

    fn check_jj_spacing(
        &self,
        placement: &Placement,
        netlist: &Netlist,
        violations: &mut Vec<DrcViolation>,
    ) {
        let min_spacing = self.rules.min_jj_spacing_um;
        let jj_nodes: Vec<_> = placement
            .nodes
            .iter()
            .filter(|pn| {
                netlist.nodes().get(pn.node.0).is_some_and(|n| {
                    matches!(
                        n.kind,
                        NodeKind::CellInstance | NodeKind::Dff | NodeKind::MacroCell
                    )
                })
            })
            .collect();

        for i in 0..jj_nodes.len() {
            for j in (i + 1)..jj_nodes.len() {
                let a = &jj_nodes[i];
                let b = &jj_nodes[j];
                let dx = (a.point.x_um - b.point.x_um).abs();
                let dy = (a.point.y_um - b.point.y_um).abs();
                let dist = dx + dy;
                if dist < min_spacing {
                    violations.push(DrcViolation {
                        rule: "jj_spacing".to_string(),
                        severity: DrcSeverity::Error,
                        location: Some(midpoint_placed(a.point, b.point)),
                        detail: format!(
                            "JJ spacing {:.3} um < {:.3} um between {:?} and {:?}",
                            dist, min_spacing, a.node, b.node
                        ),
                    });
                }
            }
        }
    }

    fn check_cell_boundary(
        &self,
        placement: &Placement,
        netlist: &Netlist,
        violations: &mut Vec<DrcViolation>,
    ) {
        let margin = self.rules.cell_boundary_margin_um;
        let w = self.rules.layout_width_um;
        let h = self.rules.layout_height_um;
        for pn in &placement.nodes {
            let node = match netlist.nodes().get(pn.node.0) {
                Some(n) => n,
                None => continue,
            };
            if matches!(node.kind, NodeKind::Port) {
                continue;
            }
            let p = pn.point;
            if p.x_um < margin
                || p.x_um > w - margin
                || p.y_um < margin
                || p.y_um > h - margin
            {
                violations.push(DrcViolation {
                    rule: "cell_boundary".to_string(),
                    severity: DrcSeverity::Warning,
                    location: Some(p),
                    detail: format!(
                        "Node {:?} at ({:.1}, {:.1}) within {:.1} um of layout boundary",
                        pn.node, p.x_um, p.y_um, margin
                    ),
                });
            }
        }
    }
}

fn same_net(a: &NetRoute, b: &NetRoute) -> bool {
    a.from.node == b.from.node && a.to.node == b.to.node
}

fn segment_distance(s1: &RouteSegment, s2: &RouteSegment) -> f64 {
    let d1 = point_distance(s1.start, s2.start);
    let d2 = point_distance(s1.start, s2.end);
    let d3 = point_distance(s1.end, s2.start);
    let d4 = point_distance(s1.end, s2.end);
    d1.min(d2).min(d3).min(d4)
}

fn point_distance(a: Point, b: Point) -> f64 {
    (a.x_um - b.x_um).abs() + (a.y_um - b.y_um).abs()
}

fn midpoint(seg: &RouteSegment) -> Point {
    Point {
        x_um: (seg.start.x_um + seg.end.x_um) / 2.0,
        y_um: (seg.start.y_um + seg.end.y_um) / 2.0,
    }
}

fn midpoint_placed(a: Point, b: Point) -> Point {
    Point {
        x_um: (a.x_um + b.x_um) / 2.0,
        y_um: (a.y_um + b.y_um) / 2.0,
    }
}

fn is_device_kind(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::CellInstance | NodeKind::Dff | NodeKind::MacroCell | NodeKind::Splitter
    )
}

#[derive(Debug, Clone, Default)]
pub struct LvsChecker;

impl LvsChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn check(
        &self,
        netlist: &Netlist,
        placement: &Placement,
        routing: &RoutingReport,
    ) -> LvsReport {
        let mut schematic_devices: HashMap<String, usize> = HashMap::new();
        for node in netlist.nodes() {
            if is_device_kind(&node.kind) {
                *schematic_devices.entry(node.name.clone()).or_insert(0) += 1;
            }
        }

        let mut layout_devices: HashMap<String, usize> = HashMap::new();
        for pn in &placement.nodes {
            if let Some(node) = netlist.nodes().get(pn.node.0) {
                if is_device_kind(&node.kind) {
                    *layout_devices.entry(node.name.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut missing_devices = Vec::new();
        let mut extra_devices = Vec::new();

        for (name, count) in &schematic_devices {
            let layout_count = layout_devices.get(name).copied().unwrap_or(0);
            if layout_count < *count {
                missing_devices.push(name.clone());
            }
        }
        for (name, count) in &layout_devices {
            let schematic_count = schematic_devices.get(name).copied().unwrap_or(0);
            if schematic_count < *count {
                extra_devices.push(name.clone());
            }
        }

        let device_count_mismatch = !missing_devices.is_empty() || !extra_devices.is_empty();

        let mut schematic_nets: HashMap<String, BTreeSet<usize>> = HashMap::new();
        for (from, to) in netlist.edge_pairs() {
            let from_node = &netlist.nodes()[from.node.0];
            let to_node = &netlist.nodes()[to.node.0];
            let net_name = format!(
                "{}:{}_{}:{}",
                from_node.name, from.port, to_node.name, to.port
            );
            let pins = schematic_nets.entry(net_name).or_default();
            pins.insert(from.node.0);
            pins.insert(to.node.0);
        }

        let mut layout_nets: HashMap<String, BTreeSet<usize>> = HashMap::new();
        for route in &routing.routes {
            let from_node = &netlist.nodes()[route.from.node.0];
            let to_node = &netlist.nodes()[route.to.node.0];
            let net_name = format!(
                "{}:{}_{}:{}",
                from_node.name, route.from.port, to_node.name, route.to.port
            );
            let pins = layout_nets.entry(net_name).or_default();
            pins.insert(route.from.node.0);
            pins.insert(route.to.node.0);
        }

        let checked_nets = schematic_nets.len().max(layout_nets.len());
        let mut matched_nets = 0;
        let mut net_mismatches = Vec::new();

        for (net_name, schematic_pins) in &schematic_nets {
            if let Some(layout_pins) = layout_nets.get(net_name) {
                if schematic_pins == layout_pins {
                    matched_nets += 1;
                } else {
                    net_mismatches.push(NetMismatch {
                        net_name: net_name.clone(),
                        issue: "connectivity mismatch".to_string(),
                    });
                }
            } else {
                net_mismatches.push(NetMismatch {
                    net_name: net_name.clone(),
                    issue: "missing from layout".to_string(),
                });
            }
        }
        for net_name in layout_nets.keys() {
            if !schematic_nets.contains_key(net_name) {
                net_mismatches.push(NetMismatch {
                    net_name: net_name.clone(),
                    issue: "extra in layout".to_string(),
                });
            }
        }

        let connectivity_mismatch = !net_mismatches.is_empty();
        let matched = !device_count_mismatch && !connectivity_mismatch;

        LvsReport {
            matched,
            device_count_mismatch,
            connectivity_mismatch,
            missing_devices,
            extra_devices,
            net_mismatches,
            checked_nets,
            matched_nets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{Netlist, NodeKind, PinRef};
    use rflux_place::{Placement, PlacedNode, Point};
    use rflux_route::{NetRoute, RouteMode, RouteSegment, RoutingReport};
    use rflux_tech::Pdk;

    fn make_clean_placement_and_routing() -> (Placement, RoutingReport, Netlist) {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "cell_a");
        let b = netlist.add_node(NodeKind::CellInstance, "cell_b");
        let from = PinRef { node: a, port: 0 };
        let to = PinRef { node: b, port: 0 };
        netlist.connect(from, to).unwrap();

        let placement = Placement {
            nodes: vec![
                PlacedNode {
                    node: a,
                    level: 0,
                    slot: 0,
                    point: Point {
                        x_um: 10.0,
                        y_um: 10.0,
                    },
                },
                PlacedNode {
                    node: b,
                    level: 1,
                    slot: 0,
                    point: Point {
                        x_um: 50.0,
                        y_um: 10.0,
                    },
                },
            ],
            width_um: 100.0,
            height_um: 100.0,
        };

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from,
                to,
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point {
                        x_um: 10.0,
                        y_um: 10.0,
                    },
                    end: Point {
                        x_um: 50.0,
                        y_um: 10.0,
                    },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        (placement, routing, netlist)
    }

    #[test]
    fn drc_clean_layout_passes() {
        let (placement, routing, netlist) = make_clean_placement_and_routing();
        let pdk = Pdk::minimal("test");
        let rules = DrcRuleSet::from_pdk(&pdk, placement.width_um, placement.height_um);
        let checker = DrcChecker::new(rules);
        let report = checker.check(&placement, &routing, &netlist);

        assert!(
            report.passed,
            "clean layout should pass DRC: {:?}",
            report.violations
        );
        assert_eq!(report.error_count, 0);
    }

    #[test]
    fn drc_detects_jj_spacing_violation() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "cell_a");
        let b = netlist.add_node(NodeKind::CellInstance, "cell_b");

        let placement = Placement {
            nodes: vec![
                PlacedNode {
                    node: a,
                    level: 0,
                    slot: 0,
                    point: Point {
                        x_um: 10.0,
                        y_um: 10.0,
                    },
                },
                PlacedNode {
                    node: b,
                    level: 0,
                    slot: 1,
                    point: Point {
                        x_um: 12.0,
                        y_um: 10.0,
                    },
                },
            ],
            width_um: 100.0,
            height_um: 100.0,
        };

        let routing = RoutingReport {
            routes: Vec::new(),
            total_length_um: 0.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 0,
            ptl_routes: 0,
        };

        let pdk = Pdk::minimal("test");
        let rules = DrcRuleSet::from_pdk(&pdk, placement.width_um, placement.height_um);
        let checker = DrcChecker::new(rules);
        let report = checker.check(&placement, &routing, &netlist);

        assert!(!report.passed, "should detect JJ spacing violation");
        assert!(report.error_count > 0);
        assert!(report.violations.iter().any(|v| v.rule == "jj_spacing"));
    }

    #[test]
    fn lvs_matching_layout_passes() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "cell_a");
        let b = netlist.add_node(NodeKind::CellInstance, "cell_b");
        let from = PinRef { node: a, port: 0 };
        let to = PinRef { node: b, port: 0 };
        netlist.connect(from, to).unwrap();

        let placement = Placement {
            nodes: vec![
                PlacedNode {
                    node: a,
                    level: 0,
                    slot: 0,
                    point: Point {
                        x_um: 10.0,
                        y_um: 10.0,
                    },
                },
                PlacedNode {
                    node: b,
                    level: 1,
                    slot: 0,
                    point: Point {
                        x_um: 50.0,
                        y_um: 10.0,
                    },
                },
            ],
            width_um: 100.0,
            height_um: 100.0,
        };

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from,
                to,
                mode: RouteMode::Jtl,
                segments: vec![RouteSegment {
                    start: Point {
                        x_um: 10.0,
                        y_um: 10.0,
                    },
                    end: Point {
                        x_um: 50.0,
                        y_um: 10.0,
                    },
                    layer: 1,
                }],
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let checker = LvsChecker::new();
        let report = checker.check(&netlist, &placement, &routing);

        assert!(
            report.matched,
            "matching layout should pass LVS: {:?}",
            report
        );
        assert!(!report.device_count_mismatch);
        assert!(!report.connectivity_mismatch);
    }
}
