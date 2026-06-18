use std::collections::VecDeque;

use rflux_ir::{Netlist, NodeId, NodeKind, PinRef};
use rflux_route::{CouplingMap, RouteMode, RoutingReport};
use rflux_tech::{CharacterizationArtifactMetadata, InterconnectKind, Pdk, SfCell, SfCellKind};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
/// Timing constraint attached to a specific node.
pub struct NodeTimingConstraint {
    pub node: NodeId,
    pub input_arrival_ps: Option<f64>,
    pub required_ps: Option<f64>,
    pub clock_domain: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
/// Timing constraint attached to a specific pin.
pub struct PinTimingConstraint {
    pub pin: PinRef,
    pub input_arrival_ps: Option<f64>,
    pub required_ps: Option<f64>,
    pub clock_domain: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
/// Constraint defining a single clock domain.
pub struct ClockDomainConstraint {
    pub id: usize,
    pub period_ps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
/// Kind of clock-domain crossing constraint.
pub enum CrossingConstraintKind {
    FalsePath,
    MaxDelay,
    Multicycle,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
/// Constraint for paths that cross clock domains.
pub struct CrossingConstraint {
    pub from_domain: usize,
    pub to_domain: usize,
    pub kind: CrossingConstraintKind,
    pub value_ps: Option<f64>,
    pub cycles: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Configuration for static timing analysis.
pub struct TimingConfig {
    pub clock_period_ps: f64,
    pub input_arrival_ps: f64,
    pub sfq_phase_count: usize,
    pub sfq_pulse_window_ps: f64,
    pub node_constraints: Vec<NodeTimingConstraint>,
    pub pin_constraints: Vec<PinTimingConstraint>,
    pub clock_domains: Vec<ClockDomainConstraint>,
    pub crossing_constraints: Vec<CrossingConstraint>,
    #[serde(default)]
    pub use_parasitic_extraction: bool,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            clock_period_ps: 120.0,
            input_arrival_ps: 0.0,
            sfq_phase_count: 1,
            sfq_pulse_window_ps: 4.0,
            node_constraints: Vec::new(),
            pin_constraints: Vec::new(),
            clock_domains: Vec::new(),
            crossing_constraints: Vec::new(),
            use_parasitic_extraction: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
/// Per-arc deterministic timing result.
pub struct TimingArcReport {
    pub from: PinRef,
    pub to: PinRef,
    pub is_false_path: bool,
    pub driver_kind: SfCellKind,
    pub route_mode: RouteMode,
    pub route_length_um: f64,
    pub cell_delay_ps: f64,
    pub wire_delay_ps: f64,
    pub launch_phase: usize,
    pub capture_phase: usize,
    pub launch_window_start_ps: f64,
    pub launch_window_end_ps: f64,
    pub capture_window_start_ps: f64,
    pub capture_window_end_ps: f64,
    pub arrival_phase_offset_ps: f64,
    pub capture_window_slack_ps: f64,
    pub capture_window_violation: bool,
    pub arrival_ps: f64,
    pub required_ps: f64,
    pub setup_slack_ps: f64,
    pub hold_slack_ps: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Complete deterministic STA result for a netlist.
pub struct TimingReport {
    pub arcs: Vec<TimingArcReport>,
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub total_negative_setup_slack_ps: f64,
    pub total_negative_hold_slack_ps: f64,
    pub critical_path_delay_ps: f64,
    pub setup_violations: usize,
    pub hold_violations: usize,
    pub capture_window_violations: usize,
    pub analyzed_arcs: usize,
    pub false_path_arcs: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_report: Option<rflux_extract::ExtractionReport>,
}

/// Recommendation for fixing a single hold violation arc.
///
/// In SFQ circuits, hold violations are fixed by inserting JTL segments of
/// precise length to add delay on the data path.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldFixRecommendation {
    /// The source pin of the violating arc.
    pub from: PinRef,
    /// The sink pin of the violating arc.
    pub to: PinRef,
    /// The current hold slack (negative means violation).
    pub hold_slack_ps: f64,
    /// The additional delay needed to fix the violation (positive value).
    pub required_delay_ps: f64,
    /// The recommended JTL length in micrometers to achieve the required delay.
    /// Computed as: required_delay_ps / jtl_delay_per_um.
    pub recommended_jtl_length_um: f64,
}

impl TimingReport {
    /// Compute hold fix recommendations for all violating arcs.
    ///
    /// For each arc with negative hold slack, recommends the JTL segment length
    /// needed to eliminate the violation. The `jtl_delay_per_um` parameter
    /// specifies the JTL delay in ps/um from the PDK interconnect model.
    #[must_use]
    pub fn hold_fix_recommendations(&self, jtl_delay_per_um: f64) -> Vec<HoldFixRecommendation> {
        if jtl_delay_per_um <= 0.0 {
            return Vec::new();
        }
        self.arcs
            .iter()
            .filter(|arc| arc.hold_slack_ps < 0.0 && !arc.is_false_path)
            .map(|arc| {
                let required_delay_ps = -arc.hold_slack_ps;
                let recommended_jtl_length_um = required_delay_ps / jtl_delay_per_um;
                HoldFixRecommendation {
                    from: arc.from,
                    to: arc.to,
                    hold_slack_ps: arc.hold_slack_ps,
                    required_delay_ps,
                    recommended_jtl_length_um,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Configuration for statistical static timing analysis (SSTA).
pub struct StatisticalTimingConfig {
    pub cell_delay_sigma_ratio: f64,
    pub wire_delay_sigma_ratio: f64,
    pub global_cell_delay_sigma_ratio: f64,
    pub global_wire_delay_sigma_ratio: f64,
    pub clock_uncertainty_sigma_ps: f64,
    pub cross_domain_uncertainty_sigma_ps: f64,
    pub max_delay_cross_domain_uncertainty_sigma_ps: f64,
    pub multicycle_cross_domain_uncertainty_sigma_ps: f64,
    pub sigma_multiplier: f64,
}

impl Default for StatisticalTimingConfig {
    fn default() -> Self {
        Self {
            cell_delay_sigma_ratio: 0.05,
            wire_delay_sigma_ratio: 0.05,
            global_cell_delay_sigma_ratio: 0.0,
            global_wire_delay_sigma_ratio: 0.0,
            clock_uncertainty_sigma_ps: 0.0,
            cross_domain_uncertainty_sigma_ps: 0.0,
            max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
            multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
            sigma_multiplier: 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Per-arc statistical timing result with mean and sigma.
pub struct StatisticalTimingArcReport {
    pub from: PinRef,
    pub to: PinRef,
    pub is_false_path: bool,
    pub route_mode: RouteMode,
    pub route_length_um: f64,
    pub launch_phase: usize,
    pub capture_phase: usize,
    pub launch_window_start_ps: f64,
    pub launch_window_end_ps: f64,
    pub capture_window_start_ps: f64,
    pub capture_window_end_ps: f64,
    pub arrival_phase_offset_ps: f64,
    pub capture_window_slack_ps: f64,
    pub capture_window_violation: bool,
    pub mean_arrival_ps: f64,
    pub mean_required_ps: f64,
    pub setup_slack_ps: f64,
    pub hold_slack_ps: f64,
    pub setup_sigma_ps: f64,
    pub hold_sigma_ps: f64,
    pub pessimistic_setup_slack_ps: f64,
    pub pessimistic_hold_slack_ps: f64,
}

#[derive(Debug, Clone, PartialEq)]
/// Complete SSTA result for a netlist.
pub struct StatisticalTimingReport {
    pub arcs: Vec<StatisticalTimingArcReport>,
    pub worst_pessimistic_setup_slack_ps: f64,
    pub worst_pessimistic_hold_slack_ps: f64,
    pub setup_risk_violations: usize,
    pub hold_risk_violations: usize,
    pub analyzed_arcs: usize,
    pub false_path_arcs: usize,
}

#[derive(Debug, Error)]
/// Errors from the timing analysis engine.
pub enum TimingError {
    #[error("timing analysis requires an acyclic netlist")]
    CyclicNetlist,
    #[error("missing routed edge for {0:?} -> {1:?}")]
    MissingRoute(PinRef, PinRef),
    #[error("missing cell timing model for node kind {0:?}")]
    MissingCellTiming(NodeKind),
    #[error("missing interconnect timing model for route mode {0:?}")]
    MissingInterconnectTiming(RouteMode),
}

impl TimingError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            TimingError::CyclicNetlist => "RFLOW-FLOW-004",
            TimingError::MissingRoute(..) => "RFLOW-FLOW-004",
            TimingError::MissingCellTiming(..) => "RFLOW-PDK-003",
            TimingError::MissingInterconnectTiming(..) => "RFLOW-PDK-003",
        }
    }

    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            TimingError::CyclicNetlist => {
                "Timing analysis requires a directed acyclic netlist."
            }
            TimingError::MissingRoute(_from, _to) => {
                "Run placement and routing before timing analysis."
            }
            TimingError::MissingCellTiming(_kind) => {
                "Provide a PDK with cell timing models for the required cell kinds."
            }
            TimingError::MissingInterconnectTiming(_mode) => {
                "Provide interconnect timing data for the required route mode in the PDK."
            }
        }
    }
}

#[derive(Debug, Default)]
/// The main STA engine.
///
/// Builds a timing graph from a netlist and PDK, then computes
/// arrival/required times and slack for all endpoints.
pub struct StaticTimingAnalyzer;

impl StaticTimingAnalyzer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(
        &self,
        netlist: &Netlist,
        routing: &RoutingReport,
        pdk: &Pdk,
        config: &TimingConfig,
        coupling_map: Option<&CouplingMap>,
    ) -> Result<TimingReport, TimingError> {
        let node_count = netlist.node_count();
        let mut adjacency = vec![Vec::<usize>::new(); node_count];
        let mut incoming = vec![Vec::<usize>::new(); node_count];
        let mut indegree = vec![0usize; node_count];
        let mut outdegree = vec![0usize; node_count];
        let edges = netlist.edge_pairs();

        let extracted_wire_delays: Option<std::collections::HashMap<(PinRef, PinRef), f64>> =
            if config.use_parasitic_extraction {
                let extractor = rflux_extract::ParasiticExtractor::from_pdk(pdk);
                let mut map = std::collections::HashMap::new();
                for route in &routing.routes {
                    let net_p = extractor.extract_net(route);
                    map.insert((route.from, route.to), net_p.parasitics.total_delay_ps);
                }
                Some(map)
            } else {
                None
            };

        for (edge_index, (from, to)) in edges.iter().enumerate() {
            adjacency[from.node.0].push(edge_index);
            incoming[to.node.0].push(edge_index);
            indegree[to.node.0] += 1;
            outdegree[from.node.0] += 1;
        }

        let mut queue = VecDeque::new();
        for (node_index, degree) in indegree.iter().enumerate() {
            if *degree == 0 {
                queue.push_back(node_index);
            }
        }

        let mut topo = Vec::<usize>::with_capacity(node_count);
        while let Some(node_index) = queue.pop_front() {
            topo.push(node_index);
            for &edge_index in &adjacency[node_index] {
                let (_, to) = edges[edge_index];
                indegree[to.node.0] -= 1;
                if indegree[to.node.0] == 0 {
                    queue.push_back(to.node.0);
                }
            }
        }

        if topo.len() != node_count {
            return Err(TimingError::CyclicNetlist);
        }

        let mut arrival = vec![config.input_arrival_ps; node_count];
        for constraint in &config.node_constraints {
            if let Some(input_arrival_ps) = constraint.input_arrival_ps {
                arrival[constraint.node.0] = input_arrival_ps;
            }
        }
        for constraint in &config.pin_constraints {
            if let Some(input_arrival_ps) = constraint.input_arrival_ps {
                arrival[constraint.pin.node.0] = input_arrival_ps;
            }
        }
        for &node_index in &topo {
            for &edge_index in &adjacency[node_index] {
                let (from, to) = edges[edge_index];
                let arc_delay = arc_delay_with_extraction(netlist, routing, pdk, from, to, coupling_map, extracted_wire_delays.as_ref())?;
                let candidate = arrival[from.node.0] + arc_delay;
                if candidate > arrival[to.node.0] {
                    arrival[to.node.0] = candidate;
                }
            }
        }

        let mut required = vec![f64::INFINITY; node_count];
        for node_index in 0..node_count {
            if outdegree[node_index] == 0 {
                required[node_index] = endpoint_required_ps(config, None, NodeId(node_index));
            }
        }
        for constraint in &config.node_constraints {
            if let Some(required_ps) = constraint.required_ps {
                required[constraint.node.0] = required[constraint.node.0].min(required_ps);
            }
        }
        for &node_index in topo.iter().rev() {
            for &edge_index in &adjacency[node_index] {
                let (from, to) = edges[edge_index];
                if matches!(
                    crossing_constraint_for_arc(config, from, to).map(|constraint| constraint.kind),
                    Some(CrossingConstraintKind::FalsePath)
                ) {
                    continue;
                }
                let arc_delay = arc_delay_with_extraction(netlist, routing, pdk, from, to, coupling_map, extracted_wire_delays.as_ref())?;
                let candidate = required[to.node.0] - arc_delay;
                if candidate < required[from.node.0] {
                    required[from.node.0] = candidate;
                }
            }
            if !required[node_index].is_finite() {
                required[node_index] = config.clock_period_ps;
            }
        }

        let mut arcs = Vec::<TimingArcReport>::with_capacity(edges.len());
        let mut worst_setup_slack_ps = f64::INFINITY;
        let mut worst_hold_slack_ps = f64::INFINITY;
        let mut setup_violations = 0usize;
        let mut hold_violations = 0usize;
        let mut capture_window_violations = 0usize;
        let mut false_path_arcs = 0usize;

        for (from, to) in edges {
            let (cell_delay_ps, wire_delay_ps) =
                arc_components_with_extraction(netlist, routing, pdk, from, to, coupling_map, extracted_wire_delays.as_ref())?;
            let sink_arrival = arrival[from.node.0] + cell_delay_ps + wire_delay_ps;
            let setup_requirement = setup_time_ps(netlist, pdk, to.node.0)?;
            let base_required_ps =
                required[to.node.0].min(endpoint_required_ps(config, Some(to), to.node));
            let is_false_path = matches!(
                crossing_constraint_for_arc(config, from, to).map(|constraint| constraint.kind),
                Some(CrossingConstraintKind::FalsePath)
            );
            let (arc_required_ps, setup_slack_ps) = apply_crossing_constraint(
                config,
                from,
                to,
                arrival[from.node.0],
                base_required_ps,
                setup_requirement,
                cell_delay_ps + wire_delay_ps,
                sink_arrival,
            );
            let hold_slack_ps = wire_delay_ps - hold_time_ps(netlist, pdk, to.node.0)?;
            let launch_phase = sfq_phase_for_pin(config, from);
            let capture_phase = sfq_phase_for_pin(config, to);
            let (launch_window_start_ps, launch_window_end_ps) =
                sfq_phase_window_ps(config, from, launch_phase);
            let (capture_window_start_ps, capture_window_end_ps) =
                sfq_phase_window_ps(config, to, capture_phase);
            let arrival_phase_offset_ps = sfq_phase_offset_ps(config, to, sink_arrival);
            let capture_window_slack_ps = capture_window_end_ps - arrival_phase_offset_ps;
            let capture_window_violation = !is_false_path
                && (arrival_phase_offset_ps < capture_window_start_ps
                    || arrival_phase_offset_ps > capture_window_end_ps);
            if is_false_path {
                false_path_arcs += 1;
            }
            if setup_slack_ps < 0.0 {
                setup_violations += 1;
            }
            if hold_slack_ps < 0.0 {
                hold_violations += 1;
            }
            if capture_window_violation {
                capture_window_violations += 1;
            }
            worst_setup_slack_ps = worst_setup_slack_ps.min(setup_slack_ps);
            worst_hold_slack_ps = worst_hold_slack_ps.min(hold_slack_ps);
            arcs.push(TimingArcReport {
                from,
                to,
                is_false_path,
                driver_kind: sf_cell_kind(&netlist.nodes()[from.node.0].kind),
                route_mode: route_mode_for_arc(routing, from, to)?,
                route_length_um: route_length_um(routing, from, to)?,
                cell_delay_ps,
                wire_delay_ps,
                launch_phase,
                capture_phase,
                launch_window_start_ps,
                launch_window_end_ps,
                capture_window_start_ps,
                capture_window_end_ps,
                arrival_phase_offset_ps,
                capture_window_slack_ps,
                capture_window_violation,
                arrival_ps: sink_arrival,
                required_ps: arc_required_ps,
                setup_slack_ps,
                hold_slack_ps,
            });
        }

        // Compute total negative slack (TNS) ? sum of all negative slacks
        let total_negative_setup_slack_ps: f64 = arcs
            .iter()
            .filter(|arc| arc.setup_slack_ps < 0.0 && !arc.is_false_path)
            .map(|arc| arc.setup_slack_ps)
            .sum();
        let total_negative_hold_slack_ps: f64 = arcs
            .iter()
            .filter(|arc| arc.hold_slack_ps < 0.0 && !arc.is_false_path)
            .map(|arc| arc.hold_slack_ps)
            .sum();

        let extraction_report = extracted_wire_delays.as_ref().map(|delays| {
            let extractor = rflux_extract::ParasiticExtractor::from_pdk(pdk);
            let mut report = extractor.extract_report(routing);
            report.nets.retain(|n| delays.contains_key(&(n.from, n.to)));
            report
        });

        Ok(TimingReport {
            arcs,
            worst_setup_slack_ps: if worst_setup_slack_ps.is_finite() {
                worst_setup_slack_ps
            } else {
                config.clock_period_ps
            },
            worst_hold_slack_ps: if worst_hold_slack_ps.is_finite() {
                worst_hold_slack_ps
            } else {
                0.0
            },
            total_negative_setup_slack_ps,
            total_negative_hold_slack_ps,
            critical_path_delay_ps: arrival.into_iter().fold(0.0, f64::max),
            setup_violations,
            hold_violations,
            capture_window_violations,
            analyzed_arcs: netlist.edge_count(),
            false_path_arcs,
            extraction_report,
        })
    }

    pub fn analyze_statistical(
        &self,
        netlist: &Netlist,
        routing: &RoutingReport,
        pdk: &Pdk,
        config: &TimingConfig,
        statistical_config: &StatisticalTimingConfig,
        coupling_map: Option<&CouplingMap>,
    ) -> Result<StatisticalTimingReport, TimingError> {
        let report = self.analyze(netlist, routing, pdk, config, coupling_map)?;
        let node_count = netlist.node_count();
        let edges = netlist.edge_pairs();
        let mut adjacency = vec![Vec::<usize>::new(); node_count];
        let mut indegree = vec![0usize; node_count];

        for (edge_index, (from, to)) in edges.iter().enumerate() {
            adjacency[from.node.0].push(edge_index);
            indegree[to.node.0] += 1;
        }

        let mut queue = VecDeque::new();
        for (node_index, degree) in indegree.iter().enumerate() {
            if *degree == 0 {
                queue.push_back(node_index);
            }
        }

        let mut topo = Vec::<usize>::with_capacity(node_count);
        while let Some(node_index) = queue.pop_front() {
            topo.push(node_index);
            for &edge_index in &adjacency[node_index] {
                let (_, to) = edges[edge_index];
                indegree[to.node.0] -= 1;
                if indegree[to.node.0] == 0 {
                    queue.push_back(to.node.0);
                }
            }
        }

        if topo.len() != node_count {
            return Err(TimingError::CyclicNetlist);
        }

        let mut arrival = vec![config.input_arrival_ps; node_count];
        for constraint in &config.node_constraints {
            if let Some(input_arrival_ps) = constraint.input_arrival_ps {
                arrival[constraint.node.0] = input_arrival_ps;
            }
        }
        for constraint in &config.pin_constraints {
            if let Some(input_arrival_ps) = constraint.input_arrival_ps {
                arrival[constraint.pin.node.0] = input_arrival_ps;
            }
        }

        let mut path_local_setup_sigma = vec![0.0_f64; node_count];
        let mut path_global_setup_sigma = vec![0.0_f64; node_count];
        let mut arc_setup_sigma = vec![0.0_f64; edges.len()];
        let mut arcs = Vec::with_capacity(report.arcs.len());
        let mut worst_pessimistic_setup_slack_ps = f64::INFINITY;
        let mut worst_pessimistic_hold_slack_ps = f64::INFINITY;
        let mut setup_risk_violations = 0usize;
        let mut hold_risk_violations = 0usize;

        for &node_index in &topo {
            for &edge_index in &adjacency[node_index] {
                let (from, to) = edges[edge_index];
                let arc = &report.arcs[edge_index];
                let sigma = statistical_arc_sigma_ps(netlist, pdk, arc, statistical_config);
                let candidate_arrival_ps = arc.arrival_ps;
                let candidate_local_setup_sigma_ps = (path_local_setup_sigma[from.node.0].powi(2)
                    + sigma.local_setup_sigma_ps.powi(2))
                .sqrt();
                let candidate_global_setup_sigma_ps =
                    path_global_setup_sigma[from.node.0] + sigma.global_setup_sigma_ps;
                let candidate_setup_sigma_ps = (candidate_local_setup_sigma_ps.powi(2)
                    + candidate_global_setup_sigma_ps.powi(2))
                .sqrt();
                arc_setup_sigma[edge_index] = candidate_setup_sigma_ps;

                if candidate_arrival_ps > arrival[to.node.0] + 1e-9 {
                    arrival[to.node.0] = candidate_arrival_ps;
                    path_local_setup_sigma[to.node.0] = candidate_local_setup_sigma_ps;
                    path_global_setup_sigma[to.node.0] = candidate_global_setup_sigma_ps;
                } else if (candidate_arrival_ps - arrival[to.node.0]).abs() <= 1e-9
                    && candidate_setup_sigma_ps
                        > (path_local_setup_sigma[to.node.0].powi(2)
                            + path_global_setup_sigma[to.node.0].powi(2))
                        .sqrt()
                {
                    path_local_setup_sigma[to.node.0] = candidate_local_setup_sigma_ps;
                    path_global_setup_sigma[to.node.0] = candidate_global_setup_sigma_ps;
                }
            }
        }

        for (edge_index, arc) in report.arcs.iter().enumerate() {
            let sigma = statistical_arc_sigma_ps(netlist, pdk, arc, statistical_config);
            let local_total_setup_sigma_ps =
                (sigma.local_setup_sigma_ps.powi(2) + sigma.global_setup_sigma_ps.powi(2)).sqrt();
            let cross_domain_sigma_ps =
                crossing_uncertainty_sigma_ps(config, statistical_config, arc.from, arc.to);
            let setup_sigma_ps = ((arc_setup_sigma[edge_index].max(local_total_setup_sigma_ps))
                .powi(2)
                + statistical_config.clock_uncertainty_sigma_ps.powi(2)
                + cross_domain_sigma_ps.powi(2))
            .sqrt();
            let hold_sigma_ps = ((sigma.local_hold_sigma_ps.powi(2)
                + sigma.global_hold_sigma_ps.powi(2))
                + statistical_config.clock_uncertainty_sigma_ps.powi(2)
                + cross_domain_sigma_ps.powi(2))
            .sqrt();
            let pessimistic_setup_slack_ps = if arc.is_false_path {
                f64::INFINITY
            } else {
                arc.setup_slack_ps - statistical_config.sigma_multiplier * setup_sigma_ps
            };
            let pessimistic_hold_slack_ps =
                arc.hold_slack_ps - statistical_config.sigma_multiplier * hold_sigma_ps;

            if pessimistic_setup_slack_ps < 0.0 {
                setup_risk_violations += 1;
            }
            if pessimistic_hold_slack_ps < 0.0 {
                hold_risk_violations += 1;
            }

            worst_pessimistic_setup_slack_ps =
                worst_pessimistic_setup_slack_ps.min(pessimistic_setup_slack_ps);
            worst_pessimistic_hold_slack_ps =
                worst_pessimistic_hold_slack_ps.min(pessimistic_hold_slack_ps);
            arcs.push(StatisticalTimingArcReport {
                from: arc.from,
                to: arc.to,
                is_false_path: arc.is_false_path,
                route_mode: arc.route_mode,
                route_length_um: arc.route_length_um,
                launch_phase: arc.launch_phase,
                capture_phase: arc.capture_phase,
                launch_window_start_ps: arc.launch_window_start_ps,
                launch_window_end_ps: arc.launch_window_end_ps,
                capture_window_start_ps: arc.capture_window_start_ps,
                capture_window_end_ps: arc.capture_window_end_ps,
                arrival_phase_offset_ps: arc.arrival_phase_offset_ps,
                capture_window_slack_ps: arc.capture_window_slack_ps,
                capture_window_violation: arc.capture_window_violation,
                mean_arrival_ps: arc.arrival_ps,
                mean_required_ps: arc.required_ps,
                setup_slack_ps: arc.setup_slack_ps,
                hold_slack_ps: arc.hold_slack_ps,
                setup_sigma_ps,
                hold_sigma_ps,
                pessimistic_setup_slack_ps,
                pessimistic_hold_slack_ps,
            });
        }

        Ok(StatisticalTimingReport {
            arcs,
            worst_pessimistic_setup_slack_ps: if worst_pessimistic_setup_slack_ps.is_finite() {
                worst_pessimistic_setup_slack_ps
            } else {
                config.clock_period_ps
            },
            worst_pessimistic_hold_slack_ps: if worst_pessimistic_hold_slack_ps.is_finite() {
                worst_pessimistic_hold_slack_ps
            } else {
                0.0
            },
            setup_risk_violations,
            hold_risk_violations,
            analyzed_arcs: report.analyzed_arcs,
            false_path_arcs: report.false_path_arcs,
        })
    }
}

#[allow(dead_code)]
fn arc_delay_ps(
    netlist: &Netlist,
    routing: &RoutingReport,
    pdk: &Pdk,
    from: PinRef,
    to: PinRef,
    coupling_map: Option<&CouplingMap>,
) -> Result<f64, TimingError> {
    let (cell_delay_ps, wire_delay_ps) =
        arc_components_ps(netlist, routing, pdk, from, to, coupling_map)?;
    Ok(cell_delay_ps + wire_delay_ps)
}

fn arc_delay_with_extraction(
    netlist: &Netlist,
    routing: &RoutingReport,
    pdk: &Pdk,
    from: PinRef,
    to: PinRef,
    coupling_map: Option<&CouplingMap>,
    extracted_wire_delays: Option<&std::collections::HashMap<(PinRef, PinRef), f64>>,
) -> Result<f64, TimingError> {
    let (cell_delay_ps, wire_delay_ps) =
        arc_components_with_extraction(netlist, routing, pdk, from, to, coupling_map, extracted_wire_delays)?;
    Ok(cell_delay_ps + wire_delay_ps)
}

fn arc_components_with_extraction(
    netlist: &Netlist,
    routing: &RoutingReport,
    pdk: &Pdk,
    from: PinRef,
    to: PinRef,
    coupling_map: Option<&CouplingMap>,
    extracted_wire_delays: Option<&std::collections::HashMap<(PinRef, PinRef), f64>>,
) -> Result<(f64, f64), TimingError> {
    let (cell_delay_ps, default_wire_delay_ps) =
        arc_components_ps(netlist, routing, pdk, from, to, coupling_map)?;
    let wire_delay_ps = extracted_wire_delays
        .and_then(|delays| delays.get(&(from, to)).copied())
        .unwrap_or(default_wire_delay_ps);
    Ok((cell_delay_ps, wire_delay_ps))
}

struct StatisticalArcSigma {
    local_setup_sigma_ps: f64,
    global_setup_sigma_ps: f64,
    local_hold_sigma_ps: f64,
    global_hold_sigma_ps: f64,
}

fn statistical_arc_sigma_ps(
    netlist: &Netlist,
    pdk: &Pdk,
    arc: &TimingArcReport,
    statistical_config: &StatisticalTimingConfig,
) -> StatisticalArcSigma {
    let driver = &netlist.nodes()[arc.from.node.0];
    let driver_kind = sf_cell_kind(&driver.kind);
    let driver_cell = pdk.cell_for_node(&driver.name, driver_kind);
    let metadata = pdk.characterization_metadata_for_cell(&driver.name);
    statistical_arc_sigma_ps_with_context(arc, statistical_config, driver_cell, metadata)
}

fn statistical_arc_sigma_ps_with_context(
    arc: &TimingArcReport,
    statistical_config: &StatisticalTimingConfig,
    driver_cell: Option<&SfCell>,
    metadata: Option<&CharacterizationArtifactMetadata>,
) -> StatisticalArcSigma {
    let mut cell_sensitivity = statistical_cell_sensitivity(arc.driver_kind, driver_cell);
    if let Some(metadata) = metadata {
        let detail_spread = metadata.delay_detail_spread_sigma_ps();
        let calibration_sigma = metadata.delay_calibration_sigma_ps + detail_spread;
        if calibration_sigma > 0.0 {
            let calibration_ratio =
                (calibration_sigma / arc.cell_delay_ps.max(1.0)).clamp(0.0, 0.5);
            cell_sensitivity *= 1.0 + calibration_ratio;
        }
    }
    let route_sensitivity = statistical_route_sensitivity(arc.route_mode, arc.route_length_um);
    let cell_sigma_ratio = statistical_config.cell_delay_sigma_ratio * cell_sensitivity;
    let global_cell_sigma_ratio =
        statistical_config.global_cell_delay_sigma_ratio * cell_sensitivity;
    let wire_sigma_ratio = statistical_config.wire_delay_sigma_ratio * route_sensitivity;
    let global_wire_sigma_ratio =
        statistical_config.global_wire_delay_sigma_ratio * route_sensitivity;
    let local_setup_sigma_ps = ((arc.cell_delay_ps * cell_sigma_ratio).powi(2)
        + (arc.wire_delay_ps * wire_sigma_ratio).powi(2))
    .sqrt();
    let global_setup_sigma_ps = (arc.cell_delay_ps * global_cell_sigma_ratio).abs()
        + (arc.wire_delay_ps * global_wire_sigma_ratio).abs();
    let local_hold_sigma_ps = (arc.wire_delay_ps * wire_sigma_ratio).abs();
    let global_hold_sigma_ps = (arc.wire_delay_ps * global_wire_sigma_ratio).abs();
    StatisticalArcSigma {
        local_setup_sigma_ps,
        global_setup_sigma_ps,
        local_hold_sigma_ps,
        global_hold_sigma_ps,
    }
}

fn statistical_cell_sensitivity(driver_kind: SfCellKind, driver_cell: Option<&SfCell>) -> f64 {
    let base = match driver_kind {
        SfCellKind::Port => 0.85,
        SfCellKind::GenericGate => 1.0,
        SfCellKind::Macro => 0.95,
        SfCellKind::Splitter => 0.90,
        SfCellKind::Dff => 1.20,
        SfCellKind::Jtl => 1.05,
        SfCellKind::Ptl => 1.10,
    };

    let Some(driver_cell) = driver_cell else {
        return base;
    };

    let default_area_um2 = default_area_for_kind(driver_kind);
    let default_pipeline_stages = default_pipeline_stages_for_kind(driver_kind);
    let area_ratio = if default_area_um2 <= f64::EPSILON {
        1.0
    } else {
        (driver_cell.area_um2 / default_area_um2).max(0.25)
    };
    let area_factor = 1.0 + (area_ratio - 1.0) * 0.12;
    let pipeline_delta =
        f64::from(driver_cell.pipeline_stages) - f64::from(default_pipeline_stages);
    let pipeline_factor = 1.0 + pipeline_delta * 0.06;

    (base * area_factor * pipeline_factor).clamp(base * 0.75, base * 1.50)
}

fn default_area_for_kind(kind: SfCellKind) -> f64 {
    match kind {
        SfCellKind::GenericGate => 12.0,
        SfCellKind::Macro => 48.0,
        SfCellKind::Splitter => 10.0,
        SfCellKind::Dff => 18.0,
        SfCellKind::Jtl => 6.0,
        SfCellKind::Ptl => 4.0,
        SfCellKind::Port => 0.0,
    }
}

fn default_pipeline_stages_for_kind(kind: SfCellKind) -> u8 {
    match kind {
        SfCellKind::GenericGate => 1,
        SfCellKind::Macro => 2,
        SfCellKind::Splitter => 0,
        SfCellKind::Dff => 1,
        SfCellKind::Jtl => 0,
        SfCellKind::Ptl => 0,
        SfCellKind::Port => 0,
    }
}

fn statistical_route_sensitivity(route_mode: RouteMode, route_length_um: f64) -> f64 {
    let length_factor = match route_mode {
        RouteMode::Jtl => 1.0 + 0.0025 * route_length_um.max(0.0),
        RouteMode::Ptl => 1.0 + 0.0060 * route_length_um.max(0.0),
    };
    let mode_factor = match route_mode {
        RouteMode::Jtl => 1.0,
        RouteMode::Ptl => 1.15,
    };
    (mode_factor * length_factor).max(1.0)
}

fn arc_components_ps(
    netlist: &Netlist,
    routing: &RoutingReport,
    pdk: &Pdk,
    from: PinRef,
    to: PinRef,
    coupling_map: Option<&CouplingMap>,
) -> Result<(f64, f64), TimingError> {
    let from_node = &netlist.nodes()[from.node.0];
    let to_node = &netlist.nodes()[to.node.0];
    let from_kind = &from_node.kind;
    let cell_timing = pdk
        .cell_timing_for_cell(&from_node.name, sf_cell_kind(from_kind))
        .ok_or_else(|| TimingError::MissingCellTiming(from_kind.clone()))?;
    let cell_delay_ps = pdk
        .characterized_arc_delay_ps(&from_node.name, from.port, &to_node.name, to.port)
        .unwrap_or(cell_timing.intrinsic_delay_ps);
    let (route_index, route) = routing
        .routes
        .iter()
        .enumerate()
        .find(|(_, route)| route.from == from && route.to == to)
        .ok_or(TimingError::MissingRoute(from, to))?;
    let wire_delay_ps = pdk
        .interconnect_delay_ps(interconnect_kind(route.mode), route.length_um)
        .ok_or(TimingError::MissingInterconnectTiming(route.mode))?;
    let coupling_extra = coupling_map
        .map(|cm| cm.coupling_delay_ps(route_index, wire_delay_ps))
        .unwrap_or(0.0);
    Ok((cell_delay_ps, wire_delay_ps + coupling_extra))
}

fn route_length_um(routing: &RoutingReport, from: PinRef, to: PinRef) -> Result<f64, TimingError> {
    routing
        .routes
        .iter()
        .find(|route| route.from == from && route.to == to)
        .map(|route| route.length_um)
        .ok_or(TimingError::MissingRoute(from, to))
}

fn route_mode_for_arc(
    routing: &RoutingReport,
    from: PinRef,
    to: PinRef,
) -> Result<RouteMode, TimingError> {
    routing
        .routes
        .iter()
        .find(|route| route.from == from && route.to == to)
        .map(|route| route.mode)
        .ok_or(TimingError::MissingRoute(from, to))
}

fn setup_time_ps(netlist: &Netlist, pdk: &Pdk, node_index: usize) -> Result<f64, TimingError> {
    let node = &netlist.nodes()[node_index];
    let kind = &node.kind;
    Ok(pdk
        .cell_timing_for_cell(&node.name, sf_cell_kind(kind))
        .ok_or_else(|| TimingError::MissingCellTiming(kind.clone()))?
        .setup_ps)
}

fn hold_time_ps(netlist: &Netlist, pdk: &Pdk, node_index: usize) -> Result<f64, TimingError> {
    let node = &netlist.nodes()[node_index];
    let kind = &node.kind;
    Ok(pdk
        .cell_timing_for_cell(&node.name, sf_cell_kind(kind))
        .ok_or_else(|| TimingError::MissingCellTiming(kind.clone()))?
        .hold_ps)
}

fn endpoint_required_ps(config: &TimingConfig, pin: Option<PinRef>, node: NodeId) -> f64 {
    let pin_required = pin.and_then(|pin_ref| {
        config
            .pin_constraints
            .iter()
            .find(|constraint| constraint.pin == pin_ref)
            .and_then(|constraint| constraint.required_ps)
    });
    let pin_domain_period = pin.and_then(|pin_ref| {
        config
            .pin_constraints
            .iter()
            .find(|constraint| constraint.pin == pin_ref)
            .and_then(|constraint| constraint.clock_domain)
            .and_then(|domain_id| {
                config
                    .clock_domains
                    .iter()
                    .find(|domain| domain.id == domain_id)
                    .map(|domain| domain.period_ps)
            })
    });
    let node_required = config
        .node_constraints
        .iter()
        .find(|constraint| constraint.node == node)
        .and_then(|constraint| constraint.required_ps);
    let node_domain_period = config
        .node_constraints
        .iter()
        .find(|constraint| constraint.node == node)
        .and_then(|constraint| constraint.clock_domain)
        .and_then(|domain_id| {
            config
                .clock_domains
                .iter()
                .find(|domain| domain.id == domain_id)
                .map(|domain| domain.period_ps)
        });

    pin_required
        .or(pin_domain_period)
        .or(node_required)
        .or(node_domain_period)
        .unwrap_or(config.clock_period_ps)
}

fn domain_of_pin(config: &TimingConfig, pin: PinRef) -> Option<usize> {
    config
        .pin_constraints
        .iter()
        .find(|constraint| constraint.pin == pin)
        .and_then(|constraint| constraint.clock_domain)
        .or_else(|| {
            config
                .node_constraints
                .iter()
                .find(|constraint| constraint.node == pin.node)
                .and_then(|constraint| constraint.clock_domain)
        })
}

fn crossing_constraint_for_arc(
    config: &TimingConfig,
    from: PinRef,
    to: PinRef,
) -> Option<CrossingConstraint> {
    let from_domain = domain_of_pin(config, from)?;
    let to_domain = domain_of_pin(config, to)?;
    if from_domain == to_domain {
        return None;
    }

    config
        .crossing_constraints
        .iter()
        .find(|constraint| {
            constraint.from_domain == from_domain && constraint.to_domain == to_domain
        })
        .copied()
}

fn is_cross_domain_arc(config: &TimingConfig, from: PinRef, to: PinRef) -> bool {
    let Some(from_domain) = domain_of_pin(config, from) else {
        return false;
    };
    let Some(to_domain) = domain_of_pin(config, to) else {
        return false;
    };
    from_domain != to_domain
}

fn crossing_uncertainty_sigma_ps(
    config: &TimingConfig,
    statistical_config: &StatisticalTimingConfig,
    from: PinRef,
    to: PinRef,
) -> f64 {
    if !is_cross_domain_arc(config, from, to) {
        return 0.0;
    }

    let kind_sigma_ps =
        match crossing_constraint_for_arc(config, from, to).map(|constraint| constraint.kind) {
            Some(CrossingConstraintKind::MaxDelay) => {
                statistical_config.max_delay_cross_domain_uncertainty_sigma_ps
            }
            Some(CrossingConstraintKind::Multicycle) => {
                statistical_config.multicycle_cross_domain_uncertainty_sigma_ps
            }
            _ => 0.0,
        };

    (statistical_config.cross_domain_uncertainty_sigma_ps.powi(2) + kind_sigma_ps.powi(2)).sqrt()
}

fn domain_period_ps(config: &TimingConfig, domain_id: usize) -> Option<f64> {
    config
        .clock_domains
        .iter()
        .find(|domain| domain.id == domain_id)
        .map(|domain| domain.period_ps)
}

fn sfq_phase_for_pin(config: &TimingConfig, pin: PinRef) -> usize {
    let phase_count = config.sfq_phase_count.max(1);
    let Some(domain_id) = domain_of_pin(config, pin) else {
        return 0;
    };

    config
        .clock_domains
        .iter()
        .position(|domain| domain.id == domain_id)
        .map_or(0, |index| index % phase_count)
}

fn sfq_phase_window_ps(config: &TimingConfig, pin: PinRef, phase: usize) -> (f64, f64) {
    let phase_count = config.sfq_phase_count.max(1);
    let period_ps = domain_of_pin(config, pin)
        .and_then(|domain_id| domain_period_ps(config, domain_id))
        .unwrap_or(config.clock_period_ps);
    let phase_spacing_ps = period_ps / phase_count as f64;
    let start_ps = (phase % phase_count) as f64 * phase_spacing_ps;
    let window_ps = config.sfq_pulse_window_ps.clamp(0.0, phase_spacing_ps);
    (start_ps, start_ps + window_ps)
}

fn sfq_phase_offset_ps(config: &TimingConfig, pin: PinRef, arrival_ps: f64) -> f64 {
    let period_ps = domain_of_pin(config, pin)
        .and_then(|domain_id| domain_period_ps(config, domain_id))
        .unwrap_or(config.clock_period_ps);
    if period_ps <= 0.0 {
        return arrival_ps;
    }
    arrival_ps.rem_euclid(period_ps)
}

#[allow(clippy::too_many_arguments)]
fn apply_crossing_constraint(
    config: &TimingConfig,
    from: PinRef,
    to: PinRef,
    source_arrival_ps: f64,
    base_required_ps: f64,
    setup_requirement_ps: f64,
    arc_delay_ps: f64,
    sink_arrival_ps: f64,
) -> (f64, f64) {
    let Some(constraint) = crossing_constraint_for_arc(config, from, to) else {
        return (
            base_required_ps,
            base_required_ps - sink_arrival_ps - setup_requirement_ps,
        );
    };

    match constraint.kind {
        CrossingConstraintKind::FalsePath => (f64::INFINITY, f64::INFINITY),
        CrossingConstraintKind::MaxDelay => {
            let max_delay_ps = constraint.value_ps.unwrap_or(config.clock_period_ps);
            let arc_required_ps =
                base_required_ps.min(source_arrival_ps + max_delay_ps + setup_requirement_ps);
            (arc_required_ps, max_delay_ps - arc_delay_ps)
        }
        CrossingConstraintKind::Multicycle => {
            let cycles = constraint.cycles.unwrap_or(1).max(1) as f64;
            let period_ps = domain_of_pin(config, to)
                .and_then(|domain_id| domain_period_ps(config, domain_id))
                .unwrap_or(config.clock_period_ps);
            let multicycle_required_ps =
                source_arrival_ps + cycles * period_ps + setup_requirement_ps;
            let arc_required_ps = base_required_ps.max(multicycle_required_ps);
            (
                arc_required_ps,
                arc_required_ps - sink_arrival_ps - setup_requirement_ps,
            )
        }
    }
}

fn sf_cell_kind(kind: &NodeKind) -> SfCellKind {
    match kind {
        NodeKind::CellInstance => SfCellKind::GenericGate,
        NodeKind::MacroCell => SfCellKind::Macro,
        NodeKind::Splitter => SfCellKind::Splitter,
        NodeKind::Dff => SfCellKind::Dff,
        NodeKind::Jtl => SfCellKind::Jtl,
        NodeKind::Ptl => SfCellKind::Ptl,
        NodeKind::Port => SfCellKind::Port,
    }
}

fn interconnect_kind(mode: RouteMode) -> InterconnectKind {
    match mode {
        RouteMode::Jtl => InterconnectKind::Jtl,
        RouteMode::Ptl => InterconnectKind::Ptl,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::PinRef;
    use rflux_route::{NetRoute, RoutingReport};
    use rflux_tech::{CellTimingModel, InterconnectTimingModel, PdkTimingCorner, TimingPoint};

    #[test]
    fn computes_setup_and_hold_slack_from_routed_paths() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: gate,
                    port: 0,
                },
            )
            .expect("a to gate");
        netlist
            .connect(
                PinRef {
                    node: gate,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("gate to sink");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef { node: a, port: 0 },
                    to: PinRef {
                        node: gate,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
                NetRoute {
                    from: PinRef {
                        node: gate,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink,
                        port: 0,
                    },
                    mode: RouteMode::Ptl,
                    segments: Vec::new(),
                    direct_length_um: 80.0,
                    length_um: 80.0,
                },
            ],
            total_length_um: 120.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 1,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.analyzed_arcs, 2);
        assert_eq!(report.hold_violations, 0);
        assert!(report.critical_path_delay_ps > 0.0);
        assert!(report.worst_setup_slack_ps < 120.0);
    }

    #[test]
    fn flags_hold_violation_on_short_jtl_arc() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::CellInstance, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 0.0,
                length_um: 0.0,
            }],
            total_length_um: 0.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 0);
        assert_eq!(report.hold_violations, 1);
        assert!(report.worst_hold_slack_ps < 0.0);
    }

    #[test]
    fn prefers_name_specific_characterized_cell_timing_over_kind_default() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let macro_buf = netlist.add_node(NodeKind::MacroCell, "macro_buf");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
            )
            .expect("source to macro_buf");
        netlist
            .connect(
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("macro_buf to sink");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef {
                        node: source,
                        port: 0,
                    },
                    to: PinRef {
                        node: macro_buf,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
                NetRoute {
                    from: PinRef {
                        node: macro_buf,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
            ],
            total_length_um: 80.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 2,
            ptl_routes: 0,
        };

        let base_pdk = Pdk::minimal("test");
        let characterized_pdk = base_pdk
            .with_characterized_library_json(
                r#"{
  "cell": {
    "name": "macro_buf",
    "kind": "Macro",
    "area_um2": 60.0,
    "pipeline_stages": 2
  },
  "timing": {
    "kind": "Macro",
    "intrinsic_delay_ps": 33.0,
    "setup_ps": 11.0,
    "hold_ps": 6.0
  }
}"#,
            )
            .expect("characterized artifact json should parse");

        let baseline = StaticTimingAnalyzer::new()
            .analyze(&netlist, &routing, &base_pdk, &TimingConfig::default(), None)
            .expect("baseline timing should succeed");
        let characterized = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &characterized_pdk,
                &TimingConfig::default(),
                None,
            )
            .expect("characterized timing should succeed");
        let baseline_macro_arc = baseline
            .arcs
            .iter()
            .find(|arc| arc.from.node == macro_buf)
            .expect("baseline macro arc should exist");
        let characterized_macro_arc = characterized
            .arcs
            .iter()
            .find(|arc| arc.from.node == macro_buf)
            .expect("characterized macro arc should exist");

        assert!(characterized.critical_path_delay_ps > baseline.critical_path_delay_ps);
        assert!(characterized.worst_setup_slack_ps < baseline.worst_setup_slack_ps);
        assert_eq!(baseline_macro_arc.cell_delay_ps, 14.0);
        assert_eq!(characterized_macro_arc.cell_delay_ps, 33.0);
    }

    #[test]
    fn sta_uses_active_pdk_timing_corner_for_cell_and_wire_delay() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: gate,
                    port: 0,
                },
            )
            .expect("source to gate");
        netlist
            .connect(
                PinRef {
                    node: gate,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("gate to sink");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef {
                        node: source,
                        port: 0,
                    },
                    to: PinRef {
                        node: gate,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
                NetRoute {
                    from: PinRef {
                        node: gate,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
            ],
            total_length_um: 80.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 2,
            ptl_routes: 0,
        };

        let base_pdk = Pdk::minimal("test");
        let mut slow_pdk = base_pdk.with_active_timing_corner("slow");
        slow_pdk.timing_corners.push(PdkTimingCorner {
            name: "slow".to_string(),
            process: Some("ss".to_string()),
            voltage_v: Some(2.4),
            temperature_k: Some(4.2),
            cell_timing: vec![CellTimingModel {
                kind: SfCellKind::GenericGate,
                intrinsic_delay_ps: 28.0,
                setup_ps: 8.0,
                hold_ps: 4.0,
            }],
            named_cell_timing: Vec::new(),
            interconnect_timing: vec![InterconnectTimingModel {
                kind: InterconnectKind::Jtl,
                points: vec![
                    TimingPoint {
                        length_um: 0.0,
                        delay_ps: 8.0,
                    },
                    TimingPoint {
                        length_um: 40.0,
                        delay_ps: 24.0,
                    },
                ],
            }],
        });

        let baseline = StaticTimingAnalyzer::new()
            .analyze(&netlist, &routing, &base_pdk, &TimingConfig::default(), None)
            .expect("baseline timing should succeed");
        let slow = StaticTimingAnalyzer::new()
            .analyze(&netlist, &routing, &slow_pdk, &TimingConfig::default(), None)
            .expect("slow-corner timing should succeed");
        let baseline_gate_arc = baseline
            .arcs
            .iter()
            .find(|arc| arc.from.node == gate)
            .expect("baseline gate arc should exist");
        let slow_gate_arc = slow
            .arcs
            .iter()
            .find(|arc| arc.from.node == gate)
            .expect("slow gate arc should exist");

        assert!(slow.critical_path_delay_ps > baseline.critical_path_delay_ps);
        assert!(slow.worst_setup_slack_ps < baseline.worst_setup_slack_ps);
        assert_eq!(baseline_gate_arc.cell_delay_ps, 8.0);
        assert_eq!(slow_gate_arc.cell_delay_ps, 28.0);
        assert_eq!(baseline_gate_arc.wire_delay_ps, 18.0);
        assert_eq!(slow_gate_arc.wire_delay_ps, 24.0);
    }

    #[test]
    fn sta_prefers_characterized_arc_delay_table_over_cell_intrinsic() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let macro_buf = netlist.add_node(NodeKind::MacroCell, "macro_buf");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
            )
            .expect("source to macro_buf");
        netlist
            .connect(
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("macro_buf to sink");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef {
                        node: source,
                        port: 0,
                    },
                    to: PinRef {
                        node: macro_buf,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
                NetRoute {
                    from: PinRef {
                        node: macro_buf,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
            ],
            total_length_um: 80.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 2,
            ptl_routes: 0,
        };

        let characterized_pdk = Pdk::minimal("test")
            .with_characterized_library_json(
                r#"{
  "cell": {
    "name": "macro_buf",
    "kind": "Macro",
    "area_um2": 60.0,
    "pipeline_stages": 2
  },
  "timing": {
    "kind": "Macro",
    "intrinsic_delay_ps": 14.0,
    "setup_ps": 8.0,
    "hold_ps": 5.0
  },
  "metadata": {
    "delay_calibration_sigma_ps": 0.0,
    "arc_delays": [{
      "name": "macro_to_sink",
      "driver_cell_name": "macro_buf",
      "from_port": 0,
      "sink_cell_name": "sink",
      "to_port": 0,
      "delay_ps": 41.0
    }]
  }
}"#,
            )
            .expect("characterized artifact json should parse");

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &characterized_pdk,
                &TimingConfig::default(),
                None,
            )
            .expect("timing should succeed");
        let macro_arc = report
            .arcs
            .iter()
            .find(|arc| arc.from.node == macro_buf)
            .expect("macro arc should exist");
        assert_eq!(macro_arc.cell_delay_ps, 41.0);
    }

    #[test]
    fn honors_node_specific_required_time() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig {
                    clock_period_ps: 120.0,
                    input_arrival_ps: 0.0,
                    sfq_phase_count: 1,
                    sfq_pulse_window_ps: 4.0,
                    node_constraints: vec![NodeTimingConstraint {
                        node: sink,
                        input_arrival_ps: None,
                        required_ps: Some(20.0),
                        clock_domain: None,
                    }],
                    pin_constraints: Vec::new(),
                    clock_domains: Vec::new(),
                    crossing_constraints: Vec::new(),
                    use_parasitic_extraction: false,
                },
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 1);
        assert!(report.worst_setup_slack_ps < 0.0);
    }

    #[test]
    fn honors_clock_domain_period_override() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig {
                    clock_period_ps: 120.0,
                    input_arrival_ps: 0.0,
                    sfq_phase_count: 1,
                    sfq_pulse_window_ps: 4.0,
                    node_constraints: vec![NodeTimingConstraint {
                        node: sink,
                        input_arrival_ps: None,
                        required_ps: None,
                        clock_domain: Some(1),
                    }],
                    pin_constraints: Vec::new(),
                    clock_domains: vec![ClockDomainConstraint {
                        id: 1,
                        period_ps: 24.0,
                    }],
                    crossing_constraints: Vec::new(),
                    use_parasitic_extraction: false,
                },
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 1);
        assert!(report.worst_setup_slack_ps < 0.0);
    }

    #[test]
    fn reports_sfq_phase_windows_for_clock_domains() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig {
                    clock_period_ps: 12.0,
                    input_arrival_ps: 0.0,
                    sfq_phase_count: 3,
                    sfq_pulse_window_ps: 1.5,
                    node_constraints: vec![
                        NodeTimingConstraint {
                            node: source,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(10),
                        },
                        NodeTimingConstraint {
                            node: sink,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(11),
                        },
                    ],
                    pin_constraints: Vec::new(),
                    clock_domains: vec![
                        ClockDomainConstraint {
                            id: 10,
                            period_ps: 12.0,
                        },
                        ClockDomainConstraint {
                            id: 11,
                            period_ps: 12.0,
                        },
                    ],
                    crossing_constraints: Vec::new(),
                    use_parasitic_extraction: false,
                },
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.arcs[0].launch_phase, 0);
        assert_eq!(report.arcs[0].capture_phase, 1);
        assert_eq!(report.arcs[0].launch_window_start_ps, 0.0);
        assert_eq!(report.arcs[0].launch_window_end_ps, 1.5);
        assert_eq!(report.arcs[0].capture_window_start_ps, 4.0);
        assert_eq!(report.arcs[0].capture_window_end_ps, 5.5);
        assert_eq!(report.arcs[0].arrival_phase_offset_ps, 6.0);
        assert_eq!(report.arcs[0].capture_window_slack_ps, -0.5);
        assert!(report.arcs[0].capture_window_violation);
    }

    #[test]
    fn pin_specific_required_time_overrides_node_constraint() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig {
                    clock_period_ps: 120.0,
                    input_arrival_ps: 0.0,
                    sfq_phase_count: 1,
                    sfq_pulse_window_ps: 4.0,
                    node_constraints: vec![NodeTimingConstraint {
                        node: sink,
                        input_arrival_ps: None,
                        required_ps: Some(120.0),
                        clock_domain: None,
                    }],
                    pin_constraints: vec![PinTimingConstraint {
                        pin: PinRef {
                            node: sink,
                            port: 0,
                        },
                        input_arrival_ps: None,
                        required_ps: Some(20.0),
                        clock_domain: None,
                    }],
                    clock_domains: Vec::new(),
                    crossing_constraints: Vec::new(),
                    use_parasitic_extraction: false,
                },
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 1);
        assert!(report.arcs[0].required_ps <= 20.0);
    }

    #[test]
    fn false_path_crossing_skips_setup_violation() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig {
                    clock_period_ps: 10.0,
                    input_arrival_ps: 0.0,
                    sfq_phase_count: 1,
                    sfq_pulse_window_ps: 4.0,
                    node_constraints: vec![
                        NodeTimingConstraint {
                            node: source,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(1),
                        },
                        NodeTimingConstraint {
                            node: sink,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(2),
                        },
                    ],
                    pin_constraints: Vec::new(),
                    clock_domains: vec![
                        ClockDomainConstraint {
                            id: 1,
                            period_ps: 10.0,
                        },
                        ClockDomainConstraint {
                            id: 2,
                            period_ps: 10.0,
                        },
                    ],
                    crossing_constraints: vec![CrossingConstraint {
                        from_domain: 1,
                        to_domain: 2,
                        kind: CrossingConstraintKind::FalsePath,
                        value_ps: None,
                        cycles: None,
                    }],
                    use_parasitic_extraction: false,
                },
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.arcs[0].from.node, source);
        assert_eq!(report.arcs[0].to.node, sink);
        assert!(report.arcs[0].is_false_path);
        assert!(report.arcs[0].setup_slack_ps.is_infinite());
    }

    #[test]
    fn multicycle_crossing_relaxes_setup_check() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig {
                    clock_period_ps: 10.0,
                    input_arrival_ps: 0.0,
                    sfq_phase_count: 1,
                    sfq_pulse_window_ps: 4.0,
                    node_constraints: vec![
                        NodeTimingConstraint {
                            node: source,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(1),
                        },
                        NodeTimingConstraint {
                            node: sink,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(2),
                        },
                    ],
                    pin_constraints: Vec::new(),
                    clock_domains: vec![
                        ClockDomainConstraint {
                            id: 1,
                            period_ps: 10.0,
                        },
                        ClockDomainConstraint {
                            id: 2,
                            period_ps: 10.0,
                        },
                    ],
                    crossing_constraints: vec![CrossingConstraint {
                        from_domain: 1,
                        to_domain: 2,
                        kind: CrossingConstraintKind::Multicycle,
                        value_ps: None,
                        cycles: Some(3),
                    }],
                    use_parasitic_extraction: false,
                },
                None,
            )
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 0);
        assert!(report.arcs[0].setup_slack_ps > 0.0);
    }

    #[test]
    fn statistical_timing_penalizes_setup_slack_by_sigma() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.10,
                    wire_delay_sigma_ratio: 0.10,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 0.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("statistical timing should succeed");

        assert_eq!(report.analyzed_arcs, 1);
        assert!(report.arcs[0].setup_sigma_ps > 0.0);
        assert!(report.arcs[0].pessimistic_setup_slack_ps < report.arcs[0].setup_slack_ps);
    }

    #[test]
    fn statistical_timing_preserves_false_path_exception() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig {
                    clock_period_ps: 10.0,
                    input_arrival_ps: 0.0,
                    sfq_phase_count: 1,
                    sfq_pulse_window_ps: 4.0,
                    node_constraints: vec![
                        NodeTimingConstraint {
                            node: source,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(1),
                        },
                        NodeTimingConstraint {
                            node: sink,
                            input_arrival_ps: None,
                            required_ps: None,
                            clock_domain: Some(2),
                        },
                    ],
                    pin_constraints: Vec::new(),
                    clock_domains: vec![
                        ClockDomainConstraint {
                            id: 1,
                            period_ps: 10.0,
                        },
                        ClockDomainConstraint {
                            id: 2,
                            period_ps: 10.0,
                        },
                    ],
                    crossing_constraints: vec![CrossingConstraint {
                        from_domain: 1,
                        to_domain: 2,
                        kind: CrossingConstraintKind::FalsePath,
                        value_ps: None,
                        cycles: None,
                    }],
                    use_parasitic_extraction: false,
                },
                &StatisticalTimingConfig::default(),
                None,
            )
            .expect("statistical timing should succeed");

        assert_eq!(report.false_path_arcs, 1);
        assert!(report.arcs[0].is_false_path);
        assert!(report.arcs[0].pessimistic_setup_slack_ps.is_infinite());
    }

    #[test]
    fn statistical_timing_accumulates_setup_sigma_along_paths() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let stage = netlist.add_node(NodeKind::CellInstance, "stage");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: stage,
                    port: 0,
                },
            )
            .expect("source to stage");
        netlist
            .connect(
                PinRef {
                    node: stage,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("stage to sink");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef {
                        node: source,
                        port: 0,
                    },
                    to: PinRef {
                        node: stage,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 30.0,
                    length_um: 30.0,
                },
                NetRoute {
                    from: PinRef {
                        node: stage,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink,
                        port: 0,
                    },
                    mode: RouteMode::Ptl,
                    segments: Vec::new(),
                    direct_length_um: 60.0,
                    length_um: 60.0,
                },
            ],
            total_length_um: 90.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 1,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.10,
                    wire_delay_sigma_ratio: 0.10,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 0.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("statistical timing should succeed");

        let stage_arc = report
            .arcs
            .iter()
            .find(|arc| arc.to.node == stage)
            .expect("stage arc should exist");
        let sink_arc = report
            .arcs
            .iter()
            .find(|arc| arc.to.node == sink)
            .expect("sink arc should exist");

        assert_eq!(report.arcs.len(), 2);
        assert!(stage_arc.setup_sigma_ps > 0.0);
        assert!(sink_arc.setup_sigma_ps > stage_arc.setup_sigma_ps);
        assert!(sink_arc.pessimistic_setup_slack_ps < sink_arc.setup_slack_ps);
    }

    #[test]
    fn statistical_timing_global_sigma_increases_path_risk() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let stage = netlist.add_node(NodeKind::CellInstance, "stage");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: stage,
                    port: 0,
                },
            )
            .expect("source to stage");
        netlist
            .connect(
                PinRef {
                    node: stage,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("stage to sink");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef {
                        node: source,
                        port: 0,
                    },
                    to: PinRef {
                        node: stage,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 30.0,
                    length_um: 30.0,
                },
                NetRoute {
                    from: PinRef {
                        node: stage,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink,
                        port: 0,
                    },
                    mode: RouteMode::Ptl,
                    segments: Vec::new(),
                    direct_length_um: 60.0,
                    length_um: 60.0,
                },
            ],
            total_length_um: 90.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 1,
        };

        let baseline = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.10,
                    wire_delay_sigma_ratio: 0.10,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 0.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("baseline statistical timing should succeed");

        let correlated = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.10,
                    wire_delay_sigma_ratio: 0.10,
                    global_cell_delay_sigma_ratio: 0.05,
                    global_wire_delay_sigma_ratio: 0.05,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 0.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("correlated statistical timing should succeed");

        let baseline_sink_arc = baseline
            .arcs
            .iter()
            .find(|arc| arc.to.node == sink)
            .expect("baseline sink arc should exist");
        let correlated_sink_arc = correlated
            .arcs
            .iter()
            .find(|arc| arc.to.node == sink)
            .expect("correlated sink arc should exist");

        assert!(correlated_sink_arc.setup_sigma_ps > baseline_sink_arc.setup_sigma_ps);
        assert!(
            correlated_sink_arc.pessimistic_setup_slack_ps
                < baseline_sink_arc.pessimistic_setup_slack_ps
        );
    }

    #[test]
    fn statistical_timing_clock_uncertainty_penalizes_slack() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let baseline = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.0,
                    wire_delay_sigma_ratio: 0.0,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 0.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("baseline statistical timing should succeed");

        let uncertain = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.0,
                    wire_delay_sigma_ratio: 0.0,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 2.5,
                    cross_domain_uncertainty_sigma_ps: 0.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("uncertain statistical timing should succeed");

        assert!((uncertain.arcs[0].setup_sigma_ps - 2.5).abs() < 1e-9);
        assert!((uncertain.arcs[0].hold_sigma_ps - 2.5).abs() < 1e-9);
        assert!(
            uncertain.arcs[0].pessimistic_setup_slack_ps
                < baseline.arcs[0].pessimistic_setup_slack_ps
        );
        assert!(
            uncertain.arcs[0].pessimistic_hold_slack_ps
                < baseline.arcs[0].pessimistic_hold_slack_ps
        );
    }

    #[test]
    fn statistical_timing_cross_domain_uncertainty_penalizes_crossings() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let timing_config = TimingConfig {
            clock_period_ps: 10.0,
            input_arrival_ps: 0.0,
            sfq_phase_count: 1,
            sfq_pulse_window_ps: 4.0,
            node_constraints: vec![
                NodeTimingConstraint {
                    node: source,
                    input_arrival_ps: None,
                    required_ps: None,
                    clock_domain: Some(1),
                },
                NodeTimingConstraint {
                    node: sink,
                    input_arrival_ps: None,
                    required_ps: None,
                    clock_domain: Some(2),
                },
            ],
            pin_constraints: Vec::new(),
            clock_domains: vec![
                ClockDomainConstraint {
                    id: 1,
                    period_ps: 10.0,
                },
                ClockDomainConstraint {
                    id: 2,
                    period_ps: 10.0,
                },
            ],
            crossing_constraints: Vec::new(),
            use_parasitic_extraction: false,
        };

        let baseline = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &timing_config,
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.0,
                    wire_delay_sigma_ratio: 0.0,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 0.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("baseline statistical timing should succeed");

        let uncertain = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &timing_config,
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.0,
                    wire_delay_sigma_ratio: 0.0,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 1.5,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("cross-domain statistical timing should succeed");

        assert!((uncertain.arcs[0].setup_sigma_ps - 1.5).abs() < 1e-9);
        assert!((uncertain.arcs[0].hold_sigma_ps - 1.5).abs() < 1e-9);
        assert!(
            uncertain.arcs[0].pessimistic_setup_slack_ps
                < baseline.arcs[0].pessimistic_setup_slack_ps
        );
        assert!(
            uncertain.arcs[0].pessimistic_hold_slack_ps
                < baseline.arcs[0].pessimistic_hold_slack_ps
        );
    }

    #[test]
    fn statistical_timing_multicycle_uncertainty_is_categorized_by_crossing_kind() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef {
                    node: source,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("source to sink");

        let routing = RoutingReport {
            routes: vec![NetRoute {
                from: PinRef {
                    node: source,
                    port: 0,
                },
                to: PinRef {
                    node: sink,
                    port: 0,
                },
                mode: RouteMode::Jtl,
                segments: Vec::new(),
                direct_length_um: 40.0,
                length_um: 40.0,
            }],
            total_length_um: 40.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 0,
        };

        let timing_config = TimingConfig {
            clock_period_ps: 10.0,
            input_arrival_ps: 0.0,
            sfq_phase_count: 1,
            sfq_pulse_window_ps: 4.0,
            node_constraints: vec![
                NodeTimingConstraint {
                    node: source,
                    input_arrival_ps: None,
                    required_ps: None,
                    clock_domain: Some(1),
                },
                NodeTimingConstraint {
                    node: sink,
                    input_arrival_ps: None,
                    required_ps: None,
                    clock_domain: Some(2),
                },
            ],
            pin_constraints: Vec::new(),
            clock_domains: vec![
                ClockDomainConstraint {
                    id: 1,
                    period_ps: 10.0,
                },
                ClockDomainConstraint {
                    id: 2,
                    period_ps: 10.0,
                },
            ],
            crossing_constraints: vec![CrossingConstraint {
                from_domain: 1,
                to_domain: 2,
                kind: CrossingConstraintKind::Multicycle,
                value_ps: None,
                cycles: Some(2),
            }],
            use_parasitic_extraction: false,
        };

        let report = StaticTimingAnalyzer::new()
            .analyze_statistical(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &timing_config,
                &StatisticalTimingConfig {
                    cell_delay_sigma_ratio: 0.0,
                    wire_delay_sigma_ratio: 0.0,
                    global_cell_delay_sigma_ratio: 0.0,
                    global_wire_delay_sigma_ratio: 0.0,
                    clock_uncertainty_sigma_ps: 0.0,
                    cross_domain_uncertainty_sigma_ps: 1.0,
                    max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
                    multicycle_cross_domain_uncertainty_sigma_ps: 2.0,
                    sigma_multiplier: 3.0,
                },
                None,
            )
            .expect("categorized statistical timing should succeed");

        assert!((report.arcs[0].setup_sigma_ps - (5.0_f64).sqrt()).abs() < 1e-9);
        assert!((report.arcs[0].hold_sigma_ps - (5.0_f64).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn statistical_timing_increases_wire_sigma_for_long_ptl_routes() {
        let jtl_arc = TimingArcReport {
            from: PinRef {
                node: NodeId(0),
                port: 0,
            },
            to: PinRef {
                node: NodeId(1),
                port: 0,
            },
            is_false_path: false,
            driver_kind: SfCellKind::GenericGate,
            route_mode: RouteMode::Jtl,
            route_length_um: 40.0,
            cell_delay_ps: 8.0,
            wire_delay_ps: 10.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: 0.0,
            launch_window_end_ps: 4.0,
            capture_window_start_ps: 0.0,
            capture_window_end_ps: 4.0,
            arrival_phase_offset_ps: 18.0,
            capture_window_slack_ps: -14.0,
            capture_window_violation: true,
            arrival_ps: 18.0,
            required_ps: 120.0,
            setup_slack_ps: 100.0,
            hold_slack_ps: 8.0,
        };
        let ptl_arc = TimingArcReport {
            route_mode: RouteMode::Ptl,
            route_length_um: 80.0,
            ..jtl_arc
        };
        let config = StatisticalTimingConfig {
            cell_delay_sigma_ratio: 0.0,
            wire_delay_sigma_ratio: 0.10,
            global_cell_delay_sigma_ratio: 0.0,
            global_wire_delay_sigma_ratio: 0.0,
            clock_uncertainty_sigma_ps: 0.0,
            cross_domain_uncertainty_sigma_ps: 0.0,
            max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
            multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
            sigma_multiplier: 3.0,
        };

        let jtl_sigma = statistical_arc_sigma_ps_with_context(&jtl_arc, &config, None, None);
        let ptl_sigma = statistical_arc_sigma_ps_with_context(&ptl_arc, &config, None, None);

        assert!(ptl_sigma.local_setup_sigma_ps > jtl_sigma.local_setup_sigma_ps);
        assert!(ptl_sigma.local_hold_sigma_ps > jtl_sigma.local_hold_sigma_ps);
    }

    #[test]
    fn statistical_timing_increases_cell_sigma_for_more_sensitive_devices() {
        let gate_arc = TimingArcReport {
            from: PinRef {
                node: NodeId(0),
                port: 0,
            },
            to: PinRef {
                node: NodeId(1),
                port: 0,
            },
            is_false_path: false,
            driver_kind: SfCellKind::GenericGate,
            route_mode: RouteMode::Jtl,
            route_length_um: 20.0,
            cell_delay_ps: 10.0,
            wire_delay_ps: 0.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: 0.0,
            launch_window_end_ps: 4.0,
            capture_window_start_ps: 0.0,
            capture_window_end_ps: 4.0,
            arrival_phase_offset_ps: 10.0,
            capture_window_slack_ps: -6.0,
            capture_window_violation: true,
            arrival_ps: 10.0,
            required_ps: 120.0,
            setup_slack_ps: 100.0,
            hold_slack_ps: 8.0,
        };
        let dff_arc = TimingArcReport {
            driver_kind: SfCellKind::Dff,
            ..gate_arc
        };
        let config = StatisticalTimingConfig {
            cell_delay_sigma_ratio: 0.10,
            wire_delay_sigma_ratio: 0.0,
            global_cell_delay_sigma_ratio: 0.0,
            global_wire_delay_sigma_ratio: 0.0,
            clock_uncertainty_sigma_ps: 0.0,
            cross_domain_uncertainty_sigma_ps: 0.0,
            max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
            multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
            sigma_multiplier: 3.0,
        };

        let gate_sigma = statistical_arc_sigma_ps_with_context(&gate_arc, &config, None, None);
        let dff_sigma = statistical_arc_sigma_ps_with_context(&dff_arc, &config, None, None);

        assert!(dff_sigma.local_setup_sigma_ps > gate_sigma.local_setup_sigma_ps);
    }

    #[test]
    fn statistical_timing_uses_characterized_cell_metadata_for_sigma_sensitivity() {
        let macro_arc = TimingArcReport {
            from: PinRef {
                node: NodeId(0),
                port: 0,
            },
            to: PinRef {
                node: NodeId(1),
                port: 0,
            },
            is_false_path: false,
            driver_kind: SfCellKind::Macro,
            route_mode: RouteMode::Jtl,
            route_length_um: 40.0,
            cell_delay_ps: 14.0,
            wire_delay_ps: 6.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: 0.0,
            launch_window_end_ps: 4.0,
            capture_window_start_ps: 0.0,
            capture_window_end_ps: 4.0,
            arrival_phase_offset_ps: 20.0,
            capture_window_slack_ps: -16.0,
            capture_window_violation: true,
            arrival_ps: 20.0,
            required_ps: 120.0,
            setup_slack_ps: 100.0,
            hold_slack_ps: 6.0,
        };
        let characterized_macro = SfCell {
            name: "macro_buf".to_string(),
            kind: SfCellKind::Macro,
            area_um2: 96.0,
            pipeline_stages: 4,
        };
        let config = StatisticalTimingConfig {
            cell_delay_sigma_ratio: 0.10,
            wire_delay_sigma_ratio: 0.0,
            global_cell_delay_sigma_ratio: 0.0,
            global_wire_delay_sigma_ratio: 0.0,
            clock_uncertainty_sigma_ps: 0.0,
            cross_domain_uncertainty_sigma_ps: 0.0,
            max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
            multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
            sigma_multiplier: 3.0,
        };

        let baseline_sigma = statistical_arc_sigma_ps_with_context(&macro_arc, &config, None, None);
        let characterized_sigma = statistical_arc_sigma_ps_with_context(
            &macro_arc,
            &config,
            Some(&characterized_macro),
            None,
        );

        assert!(characterized_sigma.local_setup_sigma_ps > baseline_sigma.local_setup_sigma_ps);
    }

    #[test]
    fn statistical_timing_uses_waveform_calibration_metadata_for_sigma() {
        let macro_arc = TimingArcReport {
            from: PinRef {
                node: NodeId(0),
                port: 0,
            },
            to: PinRef {
                node: NodeId(1),
                port: 0,
            },
            is_false_path: false,
            driver_kind: SfCellKind::Macro,
            route_mode: RouteMode::Jtl,
            route_length_um: 40.0,
            cell_delay_ps: 20.0,
            wire_delay_ps: 6.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: 0.0,
            launch_window_end_ps: 4.0,
            capture_window_start_ps: 0.0,
            capture_window_end_ps: 4.0,
            arrival_phase_offset_ps: 26.0,
            capture_window_slack_ps: -22.0,
            capture_window_violation: true,
            arrival_ps: 26.0,
            required_ps: 120.0,
            setup_slack_ps: 94.0,
            hold_slack_ps: 6.0,
        };
        let config = StatisticalTimingConfig {
            cell_delay_sigma_ratio: 0.10,
            wire_delay_sigma_ratio: 0.0,
            global_cell_delay_sigma_ratio: 0.0,
            global_wire_delay_sigma_ratio: 0.0,
            clock_uncertainty_sigma_ps: 0.0,
            cross_domain_uncertainty_sigma_ps: 0.0,
            max_delay_cross_domain_uncertainty_sigma_ps: 0.0,
            multicycle_cross_domain_uncertainty_sigma_ps: 0.0,
            sigma_multiplier: 3.0,
        };
        let metadata = CharacterizationArtifactMetadata {
            waveform_path: Some("wave.raw".to_string()),
            simulated_delay_ps: Some(28.0),
            sta_derived_delay_ps: Some(20.0),
            delay_calibration_sigma_ps: 4.0,
            delay_details: vec![
                rflux_tech::CharacterizationDelayDetail {
                    name: "stage_a".to_string(),
                    delay_ps: 18.0,
                },
                rflux_tech::CharacterizationDelayDetail {
                    name: "stage_b".to_string(),
                    delay_ps: 28.0,
                },
            ],
            arc_delays: Vec::new(),
        };

        let baseline_sigma = statistical_arc_sigma_ps_with_context(&macro_arc, &config, None, None);
        let calibrated_sigma =
            statistical_arc_sigma_ps_with_context(&macro_arc, &config, None, Some(&metadata));

        assert!(calibrated_sigma.local_setup_sigma_ps > baseline_sigma.local_setup_sigma_ps);
    }

    #[test]
    fn rejects_cycles() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .expect("a to b");
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: a, port: 0 })
            .expect("b to a");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef { node: a, port: 0 },
                    to: PinRef { node: b, port: 0 },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
                NetRoute {
                    from: PinRef { node: b, port: 0 },
                    to: PinRef { node: a, port: 0 },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
            ],
            total_length_um: 80.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 2,
            ptl_routes: 0,
        };

        let err = StaticTimingAnalyzer::new()
            .analyze(
                &netlist,
                &routing,
                &Pdk::minimal("test"),
                &TimingConfig::default(),
                None,
            )
            .expect_err("cycles must fail");

        assert!(matches!(err, TimingError::CyclicNetlist));
    }

    #[test]
    fn timing_error_codes_are_stable() {
        assert_eq!(TimingError::CyclicNetlist.code(), "RFLOW-FLOW-004");
        assert_eq!(
            TimingError::MissingRoute(
                PinRef {
                    node: rflux_ir::NodeId(0),
                    port: 0
                },
                PinRef {
                    node: rflux_ir::NodeId(1),
                    port: 0
                }
            )
            .code(),
            "RFLOW-FLOW-004"
        );
        assert_eq!(
            TimingError::MissingCellTiming(rflux_ir::NodeKind::CellInstance).code(),
            "RFLOW-PDK-003"
        );
        assert_eq!(
            TimingError::MissingInterconnectTiming(rflux_route::RouteMode::Jtl).code(),
            "RFLOW-PDK-003"
        );
        assert!(!TimingError::CyclicNetlist.suggestion().is_empty());
    }

    #[test]
    fn hold_fix_recommendations_empty_when_no_violations() {
        let report = TimingReport {
            arcs: Vec::new(),
            worst_setup_slack_ps: 10.0,
            worst_hold_slack_ps: 2.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: 0.0,
            critical_path_delay_ps: 12.0,
            setup_violations: 0,
            hold_violations: 0,
            capture_window_violations: 0,
            analyzed_arcs: 0,
            false_path_arcs: 0,
            extraction_report: None,
        };
        let recs = report.hold_fix_recommendations(0.5);
        assert!(recs.is_empty());
    }

    #[test]
    fn hold_fix_recommendations_for_violating_arc() {
        let report = TimingReport {
            arcs: vec![TimingArcReport {
                from: PinRef {
                    node: rflux_ir::NodeId(0),
                    port: 0,
                },
                to: PinRef {
                    node: rflux_ir::NodeId(1),
                    port: 0,
                },
                is_false_path: false,
                driver_kind: rflux_tech::SfCellKind::GenericGate,
                route_mode: rflux_route::RouteMode::Jtl,
                route_length_um: 20.0,
                cell_delay_ps: 8.0,
                wire_delay_ps: 2.0,
                launch_phase: 0,
                capture_phase: 0,
                launch_window_start_ps: 0.0,
                launch_window_end_ps: 4.0,
                capture_window_start_ps: 0.0,
                capture_window_end_ps: 4.0,
                arrival_phase_offset_ps: 1.0,
                capture_window_slack_ps: 3.0,
                capture_window_violation: false,
                arrival_ps: 10.0,
                required_ps: 12.0,
                setup_slack_ps: 2.0,
                hold_slack_ps: -3.0,
            }],
            worst_setup_slack_ps: 2.0,
            worst_hold_slack_ps: -3.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: -3.0,
            critical_path_delay_ps: 10.0,
            setup_violations: 0,
            hold_violations: 1,
            capture_window_violations: 0,
            analyzed_arcs: 1,
            false_path_arcs: 0,
            extraction_report: None,
        };
        let recs = report.hold_fix_recommendations(0.5);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].required_delay_ps, 3.0);
        assert_eq!(recs[0].recommended_jtl_length_um, 6.0);
    }

    #[test]
    fn hold_fix_recommendations_skips_false_paths() {
        let report = TimingReport {
            arcs: vec![TimingArcReport {
                from: PinRef {
                    node: rflux_ir::NodeId(0),
                    port: 0,
                },
                to: PinRef {
                    node: rflux_ir::NodeId(1),
                    port: 0,
                },
                is_false_path: true,
                driver_kind: rflux_tech::SfCellKind::GenericGate,
                route_mode: rflux_route::RouteMode::Jtl,
                route_length_um: 20.0,
                cell_delay_ps: 8.0,
                wire_delay_ps: 2.0,
                launch_phase: 0,
                capture_phase: 0,
                launch_window_start_ps: 0.0,
                launch_window_end_ps: 4.0,
                capture_window_start_ps: 0.0,
                capture_window_end_ps: 4.0,
                arrival_phase_offset_ps: 1.0,
                capture_window_slack_ps: 3.0,
                capture_window_violation: false,
                arrival_ps: 10.0,
                required_ps: 12.0,
                setup_slack_ps: 2.0,
                hold_slack_ps: -5.0,
            }],
            worst_setup_slack_ps: 2.0,
            worst_hold_slack_ps: -5.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: -5.0,
            critical_path_delay_ps: 10.0,
            setup_violations: 0,
            hold_violations: 1,
            capture_window_violations: 0,
            analyzed_arcs: 1,
            false_path_arcs: 1,
            extraction_report: None,
        };
        let recs = report.hold_fix_recommendations(0.5);
        assert!(recs.is_empty());
    }

    #[test]
    fn hold_fix_recommendations_empty_for_zero_jtl_delay() {
        let report = TimingReport {
            arcs: vec![TimingArcReport {
                from: PinRef {
                    node: rflux_ir::NodeId(0),
                    port: 0,
                },
                to: PinRef {
                    node: rflux_ir::NodeId(1),
                    port: 0,
                },
                is_false_path: false,
                driver_kind: rflux_tech::SfCellKind::GenericGate,
                route_mode: rflux_route::RouteMode::Jtl,
                route_length_um: 20.0,
                cell_delay_ps: 8.0,
                wire_delay_ps: 2.0,
                launch_phase: 0,
                capture_phase: 0,
                launch_window_start_ps: 0.0,
                launch_window_end_ps: 4.0,
                capture_window_start_ps: 0.0,
                capture_window_end_ps: 4.0,
                arrival_phase_offset_ps: 1.0,
                capture_window_slack_ps: 3.0,
                capture_window_violation: false,
                arrival_ps: 10.0,
                required_ps: 12.0,
                setup_slack_ps: 2.0,
                hold_slack_ps: -3.0,
            }],
            worst_setup_slack_ps: 2.0,
            worst_hold_slack_ps: -3.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: -3.0,
            critical_path_delay_ps: 10.0,
            setup_violations: 0,
            hold_violations: 1,
            capture_window_violations: 0,
            analyzed_arcs: 1,
            false_path_arcs: 0,
            extraction_report: None,
        };
        let recs = report.hold_fix_recommendations(0.0);
        assert!(recs.is_empty());
    }
}
