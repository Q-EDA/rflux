use std::collections::{BTreeSet, HashMap};

use rflux_ir::{Netlist, NodeKind, PinRef};
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

impl DrcViolation {
    pub fn error_code(&self) -> &'static str {
        match self.rule.as_str() {
            "trace_spacing" => "RFLOW-DRC-001",
            "ptl_length" => "RFLOW-DRC-002",
            "jj_spacing" => "RFLOW-DRC-003",
            "cell_boundary" => "RFLOW-DRC-004",
            "max_metal_density" => "RFLOW-DRC-005",
            "antenna_ratio" => "RFLOW-DRC-006",
            "via_spacing" => "RFLOW-DRC-007",
            _ => "RFLOW-DRC-000",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrcReport {
    pub violations: Vec<DrcViolation>,
    pub checked_rules: Vec<String>,
    pub error_count: usize,
    pub warning_count: usize,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct DrcSvgConfig {
    pub width_um: f64,
    pub height_um: f64,
    pub show_errors: bool,
    pub show_warnings: bool,
}

impl Default for DrcSvgConfig {
    fn default() -> Self {
        Self {
            width_um: 1000.0,
            height_um: 1000.0,
            show_errors: true,
            show_warnings: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimingDrivenDrcConfig {
    pub enable: bool,
    pub critical_margin_um: f64,
}

impl Default for TimingDrivenDrcConfig {
    fn default() -> Self {
        Self {
            enable: false,
            critical_margin_um: 50.0,
        }
    }
}

impl DrcReport {
    pub fn to_svg(&self, config: &DrcSvgConfig) -> String {
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="{}" height="{}">"#,
            config.width_um, config.height_um, config.width_um, config.height_um
        );

        svg.push_str(&format!(
            r##"<rect width="{}" height="{}" fill="#f8f8f8"/>"##,
            config.width_um, config.height_um
        ));

        for violation in &self.violations {
            let show = match violation.severity {
                DrcSeverity::Error => config.show_errors,
                DrcSeverity::Warning => config.show_warnings,
            };
            if !show {
                continue;
            }

            if let Some(loc) = violation.location {
                let (color, radius) = match violation.severity {
                    DrcSeverity::Error => ("#ff0000", 5.0),
                    DrcSeverity::Warning => ("#ffaa00", 3.0),
                };
                svg.push_str(&format!(
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{}" opacity="0.7"/>"#,
                    loc.x_um, loc.y_um, radius, color
                ));
                svg.push_str(&format!(
                    r##"<text x="{}" y="{}" font-size="4" fill="#333">{}</text>"##,
                    loc.x_um + radius + 1.0,
                    loc.y_um,
                    violation.rule
                ));
            }
        }

        svg.push_str("</svg>");
        svg
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetMismatch {
    pub net_name: String,
    pub issue: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParameterMismatch {
    pub device_name: String,
    pub parameter: String,
    pub schematic_value: f64,
    pub layout_value: f64,
    pub difference: f64,
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
    pub parameter_mismatches: Vec<ParameterMismatch>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DrcRuleSet {
    pub min_trace_spacing_um: f64,
    pub min_jj_spacing_um: f64,
    pub max_ptl_length_um: Option<f64>,
    pub cell_boundary_margin_um: f64,
    pub layout_width_um: f64,
    pub layout_height_um: f64,
    pub max_metal_density: f64,
    pub min_metal_density: f64,
    pub max_antenna_ratio: f64,
    pub min_via_spacing_um: f64,
}

impl DrcRuleSet {
    pub fn from_pdk(pdk: &Pdk, width_um: f64, height_um: f64) -> Self {
        let (spacing, jj_spacing, boundary_margin, max_density, min_density, antenna, via_spacing) =
            if let Some(rules) = &pdk.drc_rules {
                (
                    rules.min_trace_spacing_um,
                    rules.min_jj_spacing_um,
                    rules.cell_boundary_margin_um,
                    rules.max_metal_density,
                    rules.min_metal_density,
                    rules.max_antenna_ratio,
                    rules.min_via_spacing_um,
                )
            } else {
                (1.0, 5.0, 2.0, 0.8, 0.2, 100.0, 2.0)
            };
        Self {
            min_trace_spacing_um: spacing,
            min_jj_spacing_um: jj_spacing,
            max_ptl_length_um: None,
            cell_boundary_margin_um: boundary_margin,
            layout_width_um: width_um,
            layout_height_um: height_um,
            max_metal_density: max_density,
            min_metal_density: min_density,
            max_antenna_ratio: antenna,
            min_via_spacing_um: via_spacing,
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        #[cfg(feature = "yaml")]
        {
            serde_yaml::from_str(yaml).map_err(|e| e.to_string())
        }
        #[cfg(not(feature = "yaml"))]
        {
            let _ = yaml;
            Err("YAML support not enabled".to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangedRegion {
    pub min_x_um: f64,
    pub max_x_um: f64,
    pub min_y_um: f64,
    pub max_y_um: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalConfig {
    pub enable: bool,
    pub changed_regions: Vec<ChangedRegion>,
}

impl Default for IncrementalConfig {
    fn default() -> Self {
        Self {
            enable: false,
            changed_regions: Vec::new(),
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

        checked_rules.push("max_metal_density".to_string());
        violations.extend(self.check_metal_density(placement));

        checked_rules.push("antenna_ratio".to_string());
        violations.extend(self.check_antenna_effect(netlist, placement));

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

    pub fn check_timing_driven(
        &self,
        placement: &Placement,
        routing: &RoutingReport,
        netlist: &Netlist,
        critical_pins: &[(PinRef, PinRef)],
        config: &TimingDrivenDrcConfig,
    ) -> DrcReport {
        if !config.enable || critical_pins.is_empty() {
            return self.check(placement, routing, netlist);
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for &(from, to) in critical_pins {
            if let Some(p) = placement.point_of(from.node) {
                min_x = min_x.min(p.x_um);
                max_x = max_x.max(p.x_um);
                min_y = min_y.min(p.y_um);
                max_y = max_y.max(p.y_um);
            }
            if let Some(p) = placement.point_of(to.node) {
                min_x = min_x.min(p.x_um);
                max_x = max_x.max(p.x_um);
                min_y = min_y.min(p.y_um);
                max_y = max_y.max(p.y_um);
            }
        }

        let margin = config.critical_margin_um;
        min_x = (min_x - margin).max(0.0);
        min_y = (min_y - margin).max(0.0);
        max_x += margin;
        max_y += margin;

        let full_report = self.check(placement, routing, netlist);
        let critical_violations: Vec<_> = full_report
            .violations
            .into_iter()
            .filter(|v| {
                if let Some(loc) = v.location {
                    loc.x_um >= min_x
                        && loc.x_um <= max_x
                        && loc.y_um >= min_y
                        && loc.y_um <= max_y
                } else {
                    true
                }
            })
            .collect();

        let errors = critical_violations
            .iter()
            .filter(|v| v.severity == DrcSeverity::Error)
            .count();
        let warnings = critical_violations
            .iter()
            .filter(|v| v.severity == DrcSeverity::Warning)
            .count();

        DrcReport {
            violations: critical_violations,
            checked_rules: full_report.checked_rules,
            error_count: errors,
            warning_count: warnings,
            passed: errors == 0,
        }
    }

    pub fn check_incremental(
        &self,
        placement: &Placement,
        routing: &RoutingReport,
        netlist: &Netlist,
        config: &IncrementalConfig,
    ) -> DrcReport {
        if !config.enable || config.changed_regions.is_empty() {
            return self.check(placement, routing, netlist);
        }

        let full_report = self.check(placement, routing, netlist);
        let incremental_violations: Vec<_> = full_report
            .violations
            .into_iter()
            .filter(|v| {
                if let Some(loc) = v.location {
                    config.changed_regions.iter().any(|r| {
                        loc.x_um >= r.min_x_um
                            && loc.x_um <= r.max_x_um
                            && loc.y_um >= r.min_y_um
                            && loc.y_um <= r.max_y_um
                    })
                } else {
                    false
                }
            })
            .collect();

        let errors = incremental_violations
            .iter()
            .filter(|v| v.severity == DrcSeverity::Error)
            .count();
        let warnings = incremental_violations
            .iter()
            .filter(|v| v.severity == DrcSeverity::Warning)
            .count();

        DrcReport {
            violations: incremental_violations,
            checked_rules: full_report.checked_rules,
            error_count: errors,
            warning_count: warnings,
            passed: errors == 0,
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

    fn check_metal_density(&self, placement: &Placement) -> Vec<DrcViolation> {
        let mut violations = Vec::new();
        let grid_size = 100.0;
        let cols = (self.rules.layout_width_um / grid_size).ceil() as usize;
        let rows = (self.rules.layout_height_um / grid_size).ceil() as usize;

        for row in 0..rows {
            for col in 0..cols {
                let x_min = col as f64 * grid_size;
                let y_min = row as f64 * grid_size;
                let x_max = (x_min + grid_size).min(self.rules.layout_width_um);
                let y_max = (y_min + grid_size).min(self.rules.layout_height_um);
                let cell_area = (x_max - x_min) * (y_max - y_min);

                let cell_count = placement
                    .nodes
                    .iter()
                    .filter(|n| {
                        n.point.x_um >= x_min
                            && n.point.x_um < x_max
                            && n.point.y_um >= y_min
                            && n.point.y_um < y_max
                    })
                    .count();

                let metal_area = cell_count as f64 * 20.0 * 12.0;
                let density = metal_area / cell_area;

                if density > self.rules.max_metal_density {
                    violations.push(DrcViolation {
                        rule: "max_metal_density".to_string(),
                        severity: DrcSeverity::Warning,
                        location: Some(Point {
                            x_um: (x_min + x_max) / 2.0,
                            y_um: (y_min + y_max) / 2.0,
                        }),
                        detail: format!(
                            "density {:.2} > max {:.2}",
                            density, self.rules.max_metal_density
                        ),
                    });
                }
            }
        }
        violations
    }

    fn check_antenna_effect(
        &self,
        netlist: &Netlist,
        placement: &Placement,
    ) -> Vec<DrcViolation> {
        let mut violations = Vec::new();
        for edge in netlist.edge_pairs() {
            if let (Some(p_from), Some(p_to)) = (
                placement.point_of(edge.0.node),
                placement.point_of(edge.1.node),
            ) {
                let wire_length =
                    (p_from.x_um - p_to.x_um).abs() + (p_from.y_um - p_to.y_um).abs();
                let wire_area = wire_length * 1.0;
                let gate_area = 20.0 * 12.0;
                if wire_area > 0.0 {
                    let ratio = gate_area / wire_area;
                    if ratio > self.rules.max_antenna_ratio {
                        violations.push(DrcViolation {
                            rule: "antenna_ratio".to_string(),
                            severity: DrcSeverity::Warning,
                            location: Some(p_from),
                            detail: format!(
                                "ratio {:.1} > max {:.1}",
                                ratio, self.rules.max_antenna_ratio
                            ),
                        });
                    }
                }
            }
        }
        violations
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

#[derive(Debug, Clone, PartialEq)]
pub struct LvsConfig {
    pub check_device_parameters: bool,
    pub parameter_tolerance: f64,
    pub hierarchical: bool,
}

impl Default for LvsConfig {
    fn default() -> Self {
        Self {
            check_device_parameters: false,
            parameter_tolerance: 0.01,
            hierarchical: false,
        }
    }
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
            parameter_mismatches: Vec::new(),
        }
    }

    fn check_device_parameters(
        &self,
        netlist: &Netlist,
        placement: &Placement,
        config: &LvsConfig,
    ) -> Vec<ParameterMismatch> {
        let mut mismatches = Vec::new();
        for pn in &placement.nodes {
            if let Some(node) = netlist.nodes().get(pn.node.0) {
                if !is_device_kind(&node.kind) {
                    continue;
                }
                let layout_level = pn.level as f64;
                let schematic_level = 0.0;
                let diff = (layout_level - schematic_level).abs();
                if diff > config.parameter_tolerance {
                    mismatches.push(ParameterMismatch {
                        device_name: node.name.clone(),
                        parameter: "level".to_string(),
                        schematic_value: schematic_level,
                        layout_value: layout_level,
                        difference: diff,
                    });
                }
            }
        }
        mismatches
    }

    fn check_hierarchical(
        &self,
        netlist: &Netlist,
        placement: &Placement,
    ) -> Vec<NetMismatch> {
        let mut mismatches = Vec::new();
        let mut by_level: HashMap<usize, Vec<String>> = HashMap::new();
        for pn in &placement.nodes {
            if let Some(node) = netlist.nodes().get(pn.node.0) {
                if is_device_kind(&node.kind) {
                    by_level.entry(pn.level).or_default().push(node.name.clone());
                }
            }
        }
        for (level, devices) in &by_level {
            if devices.len() < 2 {
                mismatches.push(NetMismatch {
                    net_name: format!("level_{}", level),
                    issue: format!(
                        "hierarchical level {} has only {} device(s), expected at least 2",
                        level,
                        devices.len()
                    ),
                });
            }
        }
        mismatches
    }

    pub fn check_with_config(
        &self,
        netlist: &Netlist,
        placement: &Placement,
        routing: &RoutingReport,
        config: &LvsConfig,
    ) -> LvsReport {
        let mut report = self.check(netlist, placement, routing);

        if config.check_device_parameters {
            report.parameter_mismatches =
                self.check_device_parameters(netlist, placement, config);
        }

        if config.hierarchical {
            let hier_mismatches = self.check_hierarchical(netlist, placement);
            report.net_mismatches.extend(hier_mismatches);
        }

        report
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
    fn drc_error_codes_are_stable() {
        let v = DrcViolation {
            rule: "trace_spacing".to_string(),
            severity: DrcSeverity::Error,
            location: None,
            detail: "".to_string(),
        };
        assert_eq!(v.error_code(), "RFLOW-DRC-001");
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

    #[test]
    fn drc_detects_high_metal_density() {
        let mut netlist = Netlist::new();
        let mut nodes = Vec::new();
        for i in 0..50 {
            let n = netlist.add_node(NodeKind::CellInstance, &format!("cell_{}", i));
            nodes.push(PlacedNode {
                node: n,
                level: 0,
                slot: i,
                point: Point {
                    x_um: 10.0 + (i % 10) as f64 * 2.0,
                    y_um: 10.0 + (i / 10) as f64 * 2.0,
                },
            });
        }

        let placement = Placement {
            nodes,
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
        let mut rules = DrcRuleSet::from_pdk(&pdk, 100.0, 100.0);
        rules.max_metal_density = 0.1;
        let checker = DrcChecker::new(rules);
        let report = checker.check(&placement, &routing, &netlist);

        assert!(
            report.violations.iter().any(|v| v.rule == "max_metal_density"),
            "should detect high metal density: {:?}",
            report.violations
        );
    }

    #[test]
    fn drc_antenna_ratio_ok_for_short_wire() {
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
                    level: 0,
                    slot: 1,
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
            routes: Vec::new(),
            total_length_um: 0.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 0,
            ptl_routes: 0,
        };

        let pdk = Pdk::minimal("test");
        let rules = DrcRuleSet::from_pdk(&pdk, 100.0, 100.0);
        let checker = DrcChecker::new(rules);
        let report = checker.check(&placement, &routing, &netlist);

        assert!(
            !report.violations.iter().any(|v| v.rule == "antenna_ratio"),
            "short wire should not trigger antenna violation: {:?}",
            report.violations
        );
    }

    #[test]
    fn lvs_config_default() {
        let config = LvsConfig::default();
        assert!(!config.check_device_parameters);
        assert_eq!(config.parameter_tolerance, 0.01);
        assert!(!config.hierarchical);
    }

    #[test]
    fn lvs_check_with_config_basic() {
        let (placement, routing, netlist) = make_clean_placement_and_routing();
        let checker = LvsChecker::new();
        let config = LvsConfig::default();
        let report = checker.check_with_config(&netlist, &placement, &routing, &config);

        assert!(report.matched);
        assert!(report.parameter_mismatches.is_empty());
    }

    #[test]
    fn lvs_parameter_check_detects_mismatch() {
        let (placement, routing, netlist) = make_clean_placement_and_routing();
        let checker = LvsChecker::new();
        let config = LvsConfig {
            check_device_parameters: true,
            parameter_tolerance: 0.001,
            hierarchical: false,
        };
        let report = checker.check_with_config(&netlist, &placement, &routing, &config);

        assert!(
            !report.parameter_mismatches.is_empty(),
            "should detect parameter mismatch when tolerance is very small"
        );
    }

    #[test]
    fn lvs_hierarchical_check_groups_by_level() {
        let (placement, routing, netlist) = make_clean_placement_and_routing();
        let checker = LvsChecker::new();
        let config = LvsConfig {
            check_device_parameters: false,
            parameter_tolerance: 0.01,
            hierarchical: true,
        };
        let report = checker.check_with_config(&netlist, &placement, &routing, &config);

        assert!(
            !report.net_mismatches.is_empty(),
            "hierarchical check should flag single-device levels"
        );
    }

    #[test]
    fn drc_rules_from_pdk_uses_drc_rules() {
        let mut pdk = Pdk::minimal("test");
        pdk.drc_rules = Some(rflux_tech::SfqDrcRules {
            min_trace_spacing_um: 2.0,
            ..Default::default()
        });
        let rules = DrcRuleSet::from_pdk(&pdk, 1000.0, 1000.0);
        assert_eq!(rules.min_trace_spacing_um, 2.0);
        assert_eq!(rules.max_metal_density, 0.8);
        assert_eq!(rules.min_metal_density, 0.2);
        assert_eq!(rules.max_antenna_ratio, 100.0);
        assert_eq!(rules.min_via_spacing_um, 2.0);
    }

    #[test]
    fn drc_rules_from_pdk_defaults_without_drc_rules() {
        let pdk = Pdk::minimal("test");
        let rules = DrcRuleSet::from_pdk(&pdk, 500.0, 500.0);
        assert_eq!(rules.min_trace_spacing_um, 1.0);
        assert_eq!(rules.min_jj_spacing_um, 5.0);
        assert_eq!(rules.cell_boundary_margin_um, 2.0);
        assert_eq!(rules.max_metal_density, 0.8);
        assert_eq!(rules.min_metal_density, 0.2);
        assert_eq!(rules.max_antenna_ratio, 100.0);
        assert_eq!(rules.min_via_spacing_um, 2.0);
    }

    #[test]
    fn drc_report_to_svg_contains_violations() {
        let report = DrcReport {
            violations: vec![DrcViolation {
                rule: "trace_spacing".to_string(),
                severity: DrcSeverity::Error,
                location: Some(Point {
                    x_um: 100.0,
                    y_um: 200.0,
                }),
                detail: "too close".to_string(),
            }],
            checked_rules: vec!["trace_spacing".to_string()],
            error_count: 1,
            warning_count: 0,
            passed: false,
        };
        let svg = report.to_svg(&DrcSvgConfig::default());
        assert!(svg.contains("<svg"));
        assert!(svg.contains("trace_spacing"));
        assert!(svg.contains("circle"));
    }

    #[test]
    fn incremental_drc_filters_by_region() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "cell_a");
        let b = netlist.add_node(NodeKind::CellInstance, "cell_b");
        let c = netlist.add_node(NodeKind::CellInstance, "cell_c");

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
                PlacedNode {
                    node: c,
                    level: 0,
                    slot: 2,
                    point: Point {
                        x_um: 80.0,
                        y_um: 80.0,
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

        let full_report = checker.check(&placement, &routing, &netlist);
        assert!(
            !full_report.violations.is_empty(),
            "full check should detect JJ spacing violation"
        );

        let config = IncrementalConfig {
            enable: true,
            changed_regions: vec![ChangedRegion {
                min_x_um: 5.0,
                max_x_um: 20.0,
                min_y_um: 5.0,
                max_y_um: 20.0,
            }],
        };
        let incremental_report = checker.check_incremental(&placement, &routing, &netlist, &config);
        assert!(
            !incremental_report.violations.is_empty(),
            "incremental should report violations inside changed region"
        );
        assert!(
            incremental_report
                .violations
                .iter()
                .all(|v| v.location.is_some()),
            "all incremental violations should have locations"
        );

        let other_config = IncrementalConfig {
            enable: true,
            changed_regions: vec![ChangedRegion {
                min_x_um: 70.0,
                max_x_um: 90.0,
                min_y_um: 70.0,
                max_y_um: 90.0,
            }],
        };
        let other_report = checker.check_incremental(&placement, &routing, &netlist, &other_config);
        assert!(
            other_report.violations.is_empty(),
            "incremental should not report violations outside changed region"
        );

        let disabled_config = IncrementalConfig {
            enable: false,
            changed_regions: vec![ChangedRegion {
                min_x_um: 70.0,
                max_x_um: 90.0,
                min_y_um: 70.0,
                max_y_um: 90.0,
            }],
        };
        let disabled_report =
            checker.check_incremental(&placement, &routing, &netlist, &disabled_config);
        assert_eq!(
            disabled_report.violations.len(),
            full_report.violations.len(),
            "disabled incremental should return full report"
        );
    }

    #[test]
    fn incremental_config_default() {
        let config = IncrementalConfig::default();
        assert!(!config.enable);
        assert!(config.changed_regions.is_empty());
    }

    #[test]
    fn drc_report_to_svg_empty() {
        let report = DrcReport {
            violations: vec![],
            checked_rules: vec!["trace_spacing".to_string()],
            error_count: 0,
            warning_count: 0,
            passed: true,
        };
        let svg = report.to_svg(&DrcSvgConfig::default());
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("circle"));
    }

    #[test]
    fn timing_driven_drc_filters_by_region() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "cell_a");
        let b = netlist.add_node(NodeKind::CellInstance, "cell_b");
        let c = netlist.add_node(NodeKind::CellInstance, "cell_c");
        let d = netlist.add_node(NodeKind::CellInstance, "cell_d");

        let from_ab = PinRef { node: a, port: 0 };
        let to_ab = PinRef { node: b, port: 0 };
        let from_cd = PinRef { node: c, port: 0 };
        let to_cd = PinRef { node: d, port: 0 };
        netlist.connect(from_ab, to_ab).unwrap();
        netlist.connect(from_cd, to_cd).unwrap();

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
                PlacedNode {
                    node: c,
                    level: 0,
                    slot: 2,
                    point: Point {
                        x_um: 500.0,
                        y_um: 500.0,
                    },
                },
                PlacedNode {
                    node: d,
                    level: 0,
                    slot: 3,
                    point: Point {
                        x_um: 502.0,
                        y_um: 500.0,
                    },
                },
            ],
            width_um: 1000.0,
            height_um: 1000.0,
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
        let rules = DrcRuleSet::from_pdk(&pdk, 1000.0, 1000.0);
        let checker = DrcChecker::new(rules);

        let full_report = checker.check(&placement, &routing, &netlist);
        assert!(
            full_report.error_count >= 2,
            "full check should find JJ violations in both regions"
        );

        let config = TimingDrivenDrcConfig {
            enable: true,
            critical_margin_um: 20.0,
        };
        let critical_pins = vec![(from_ab, to_ab)];
        let td_report =
            checker.check_timing_driven(&placement, &routing, &netlist, &critical_pins, &config);

        assert!(
            td_report.error_count < full_report.error_count,
            "timing-driven report should filter out distant violations"
        );
        for v in &td_report.violations {
            if let Some(loc) = v.location {
                assert!(
                    loc.x_um <= 32.0 && loc.y_um <= 30.0,
                    "remaining violation should be in critical region"
                );
            }
        }
    }

    #[test]
    fn timing_driven_drc_disabled_runs_full() {
        let (placement, routing, netlist) = make_clean_placement_and_routing();
        let pdk = Pdk::minimal("test");
        let rules = DrcRuleSet::from_pdk(&pdk, placement.width_um, placement.height_um);
        let checker = DrcChecker::new(rules);

        let config = TimingDrivenDrcConfig {
            enable: false,
            critical_margin_um: 50.0,
        };
        let critical_pins = vec![];
        let report =
            checker.check_timing_driven(&placement, &routing, &netlist, &critical_pins, &config);

        assert!(report.passed, "disabled timing-driven should behave like full check");
    }
}
