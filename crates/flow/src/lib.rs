use std::collections::BTreeSet;

use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_place::{
    BlockedRegion as PlacementBlockedRegion, LevelizedPlacer, PlaceError, Placement,
    PlacementConfig,
};
use rflux_route::{
    BlockedRegion as RoutingBlockedRegion, RouteError, RouteMode, RoutingConfig, RoutingReport,
    SimpleRouter,
};
use rflux_sim::run_generated_deck;
use rflux_synth::{Compiler, SynthError, SynthesisConfig, SynthesisReport};
use rflux_tech::{CellTimingModel, CharacterizedCellLibraryEntry, Pdk, SfCell, SfCellKind};
use rflux_timing::{
    StaticTimingAnalyzer, StatisticalTimingConfig, TimingConfig, TimingError, TimingReport,
};
use thiserror::Error;

pub mod bias_grid;
pub mod clock_tree;

pub use rflux_sim::{
    parse_simulator_output, SimulationBackend, SimulationConfig, SimulationDelayDetail,
    SimulationEndpointRef, SimulationMode, SimulationReport, SimulationViolationDetail,
};

const TIMING_CLOSURE_MAX_ACTIONS_PER_CHECK: usize = 3;

#[derive(Debug, Clone)]
pub struct FlowConfig {
    pub synthesis: SynthesisConfig,
    pub placement: PlacementConfig,
    pub routing: RoutingConfig,
    pub timing: TimingConfig,
    pub clock_phase_count: usize,
    pub min_hold_jtl_length_um: f64,
    /// Multiplier applied to library-aware macro halo after characterization feedback.
    pub placement_halo_scale: f64,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            synthesis: SynthesisConfig::default(),
            placement: PlacementConfig::default(),
            routing: RoutingConfig::default(),
            timing: TimingConfig::default(),
            clock_phase_count: 2,
            min_hold_jtl_length_um: 0.0,
            placement_halo_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacementReport {
    pub placed_nodes: usize,
    pub width_um: f64,
    pub height_um: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutingSummary {
    pub routed_nets: usize,
    pub total_length_um: f64,
    pub total_detour_overhead_um: f64,
    pub detoured_routes: usize,
    pub jtl_routes: usize,
    pub ptl_routes: usize,
    pub effective_prefer_ptl_from_length_um: f64,
    pub effective_detour_margin_um: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSummary {
    pub clock_sinks: usize,
    pub clock_buffers: usize,
    pub phase_count: usize,
    pub assigned_phases: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingSummary {
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub critical_path_delay_ps: f64,
    pub analyzed_arcs: usize,
    pub false_path_arcs: usize,
    pub setup_violations: usize,
    pub capture_window_violations: usize,
    pub initial_hold_violations: usize,
    pub final_hold_violations: usize,
    pub hold_fix_applied: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimingClosureSummary {
    pub closed: bool,
    pub status: String,
    pub setup_closed: bool,
    pub hold_closed: bool,
    pub capture_window_closed: bool,
    pub setup_violations: usize,
    pub hold_violations: usize,
    pub capture_window_violations: usize,
    pub failing_checks: Vec<String>,
    pub action_count: usize,
    pub primary_action: Option<TimingClosureAction>,
    pub reduce_route_delay_actions: usize,
    pub relax_constraint_or_improve_library_timing_actions: usize,
    pub add_hold_padding_actions: usize,
    pub adjust_sfq_phase_or_pulse_window_actions: usize,
    pub actions: Vec<TimingClosureAction>,
    pub next_step: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingClosureAction {
    pub check: TimingClosureCheck,
    pub priority: usize,
    pub remediation_kind: TimingClosureRemediationKind,
    pub from: PinRef,
    pub to: PinRef,
    pub slack_ps: f64,
    pub route_mode: RouteMode,
    pub route_length_um: f64,
    pub from_domain: Option<usize>,
    pub to_domain: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimingClosureLoopReport {
    pub detour_feedback_attempted: bool,
    pub detour_feedback_applied: bool,
    pub initial_total_detour_overhead_um: f64,
    pub final_total_detour_overhead_um: f64,
    pub route_delay_optimization_attempted: bool,
    pub route_delay_optimization_applied: bool,
    pub reduce_route_delay_candidate_available: bool,
    pub recommended_prefer_ptl_from_length_um: Option<f64>,
    pub recommended_detour_margin_um: Option<f64>,
    pub recommended_route_mode: Option<RouteMode>,
    pub estimated_route_length_um: Option<f64>,
    pub estimated_slack_deficit_ps: Option<f64>,
    pub reduce_route_delay_candidate_attempted: bool,
    pub reduce_route_delay_candidate_improved: bool,
    pub candidate_worst_setup_slack_ps: Option<f64>,
    pub candidate_setup_violations: Option<usize>,
    pub candidate_hold_violations: Option<usize>,
    pub candidate_route_mode: Option<RouteMode>,
    pub candidate_route_length_um: Option<f64>,
    pub hold_fix_attempted: bool,
    pub hold_fix_applied: bool,
    pub initial_hold_violations: usize,
    pub final_hold_violations: usize,
    pub status: String,
    pub next_step: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingClosureCheck {
    Setup,
    Hold,
    CaptureWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingClosureRemediationKind {
    ReduceRouteDelay,
    RelaxConstraintOrImproveLibraryTiming,
    AddHoldPadding,
    AdjustSfqPhaseOrPulseWindow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingArcSummary {
    pub from: PinRef,
    pub to: PinRef,
    pub is_false_path: bool,
    pub route_mode: RouteMode,
    pub route_length_um: f64,
    pub from_domain: Option<usize>,
    pub to_domain: Option<usize>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct TimingAnalysisReport {
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub critical_path_delay_ps: f64,
    pub analyzed_arcs: usize,
    pub false_path_arcs: usize,
    pub setup_violations: usize,
    pub hold_violations: usize,
    pub capture_window_violations: usize,
    pub detour_feedback_applied: bool,
    pub hold_fix_applied: bool,
    pub closure: TimingClosureSummary,
    pub timing_arcs: Vec<TimingArcSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimingCornerAnalysisReport {
    pub corner_name: String,
    pub is_default_corner: bool,
    pub is_active_corner: bool,
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub critical_path_delay_ps: f64,
    pub analyzed_arcs: usize,
    pub setup_violations: usize,
    pub hold_violations: usize,
    pub capture_window_violations: usize,
    pub closure: TimingClosureSummary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiCornerTimingAnalysisReport {
    pub active_timing_corner: Option<String>,
    pub corner_count: usize,
    pub worst_setup_corner: String,
    pub worst_hold_corner: String,
    pub worst_critical_path_corner: String,
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub worst_critical_path_delay_ps: f64,
    pub corners: Vec<TimingCornerAnalysisReport>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatisticalTimingArcSummary {
    pub from: PinRef,
    pub to: PinRef,
    pub is_false_path: bool,
    pub route_mode: RouteMode,
    pub route_length_um: f64,
    pub from_domain: Option<usize>,
    pub to_domain: Option<usize>,
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
pub struct StatisticalTimingAnalysisReport {
    pub worst_pessimistic_setup_slack_ps: f64,
    pub worst_pessimistic_hold_slack_ps: f64,
    pub analyzed_arcs: usize,
    pub false_path_arcs: usize,
    pub setup_risk_violations: usize,
    pub hold_risk_violations: usize,
    pub sigma_multiplier: f64,
    pub timing_arcs: Vec<StatisticalTimingArcSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcBiasReport {
    pub routed_nets: usize,
    pub jtl_carrier_candidates: usize,
    pub ptl_coupling_risk_routes: usize,
    pub clock_sink_count: usize,
    pub estimated_static_power_savings_uw: f64,
    pub estimated_area_overhead_ratio: f64,
    pub estimated_frequency_derate_ratio: f64,
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub timing_guardband_score: f64,
    pub feasibility_score: f64,
    pub optimization_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcBiasOptimizationReport {
    pub baseline: AcBiasReport,
    pub optimized: AcBiasReport,
    pub baseline_prefer_ptl_from_length_um: f64,
    pub optimized_prefer_ptl_from_length_um: f64,
    pub baseline_detour_margin_um: f64,
    pub optimized_detour_margin_um: f64,
    pub threshold_candidates_evaluated: usize,
    pub detour_margin_candidates_evaluated: usize,
    pub optimization_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundCellCharacterizationConfig {
    pub cell_name: String,
}

impl Default for CompoundCellCharacterizationConfig {
    fn default() -> Self {
        Self {
            cell_name: "compound_cell".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompoundCellCharacterizationReport {
    pub cell_name: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub mapped_nodes: usize,
    pub total_area_um2: f64,
    pub derived_intrinsic_delay_ps: f64,
    pub derived_setup_ps: f64,
    pub derived_hold_ps: f64,
    pub generated_cell_kind: String,
    pub generated_pipeline_stages: u8,
    pub generated_library_json: String,
    pub simulated_delay_ps: Option<f64>,
    pub simulation_backend: SimulationBackend,
    pub generated_deck_lines: usize,
    pub generated_deck_path: Option<String>,
    pub waveform_path: Option<String>,
    pub reported_violations: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedConstraintConfig {
    pub max_estimated_thermal_load_uw: f64,
    pub max_estimated_mechanical_stress_score: f64,
    pub max_jtl_density_per_100um: f64,
    pub max_detour_overhead_ratio: f64,
    pub max_ptl_coupling_ratio: f64,
}

impl Default for AdvancedConstraintConfig {
    fn default() -> Self {
        Self {
            max_estimated_thermal_load_uw: 8.0,
            max_estimated_mechanical_stress_score: 0.75,
            max_jtl_density_per_100um: 8.0,
            max_detour_overhead_ratio: 0.35,
            max_ptl_coupling_ratio: 0.65,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedConstraintViolation {
    pub category: String,
    pub detail: String,
    pub measured_value: f64,
    pub limit_value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedConstraintReport {
    pub estimated_thermal_load_uw: f64,
    pub estimated_mechanical_stress_score: f64,
    pub jtl_density_per_100um: f64,
    pub detour_overhead_ratio: f64,
    pub ptl_coupling_ratio: f64,
    pub manufacturing_hotspots: usize,
    pub violation_count: usize,
    pub violations: Vec<AdvancedConstraintViolation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationReport {
    pub checked_routes: usize,
    pub checked_ptl_routes: usize,
    pub structural_violations: usize,
    pub ptl_macro_boundary_violations: usize,
    pub ptl_forbidden_length_violations: usize,
    pub simulation: SimulationReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutReport {
    pub synthesis: SynthesisReport,
    pub placement: PlacementReport,
    pub routing: RoutingSummary,
    pub clock: ClockSummary,
    pub timing: TimingSummary,
    pub timing_closure: TimingClosureSummary,
    pub timing_closure_loop: TimingClosureLoopReport,
    pub initial_total_detour_overhead_um: f64,
    pub detour_feedback_applied: bool,
}

#[derive(Debug, Clone)]
struct CompiledArtifacts {
    synthesis: SynthesisReport,
    placement: Placement,
    routing: RoutingReport,
    effective_routing_config: RoutingConfig,
    clock: ClockSummary,
    timing: TimingReport,
    initial_total_detour_overhead_um: f64,
    initial_hold_violations: usize,
    hold_fix_attempted: bool,
    detour_feedback_applied: bool,
    route_delay_optimization_attempted: bool,
    route_delay_optimization_applied: bool,
    hold_fix_applied: bool,
}

#[derive(Debug, Error)]
pub enum FlowError {
    #[error("synthesis failed: {0}")]
    Synthesis(#[from] SynthError),
    #[error("placement failed: {0}")]
    Placement(#[from] PlaceError),
    #[error("routing failed: {0}")]
    Routing(#[from] RouteError),
    #[error("timing failed: {0}")]
    Timing(#[from] TimingError),
    #[error("characterized library json invalid: {0}")]
    CharacterizedLibraryJson(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryAwareAcBiasOptimizationReport {
    pub ac_bias: AcBiasOptimizationReport,
    pub baseline_constraints: AdvancedConstraintReport,
    pub optimized_constraints: AdvancedConstraintReport,
    pub characterized_cells_merged: usize,
    pub library_optimization_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryAwareDesignOptimizationReport {
    pub ac_bias: AcBiasOptimizationReport,
    pub baseline_statistical: StatisticalTimingAnalysisReport,
    pub optimized_statistical: StatisticalTimingAnalysisReport,
    pub baseline_constraints: AdvancedConstraintReport,
    pub optimized_constraints: AdvancedConstraintReport,
    pub characterized_cells_merged: usize,
    pub design_optimization_score: f64,
    pub baseline_statistical_config: StatisticalTimingConfig,
    pub optimized_statistical_config: StatisticalTimingConfig,
    pub baseline_placement_halo_scale: f64,
    pub optimized_placement_halo_scale: f64,
    pub placement_candidates_evaluated: usize,
    pub statistical_candidates_evaluated: usize,
}

#[derive(Debug, Default)]
pub struct FlowRunner {
    compiler: Compiler,
    placer: LevelizedPlacer,
    router: SimpleRouter,
    timing: StaticTimingAnalyzer,
}

impl FlowRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile_layout(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> Result<LayoutReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, config)?;
        let timing_closure = timing_closure_summary(
            artifacts.timing.setup_violations,
            artifacts.timing.hold_violations,
            &artifacts.timing,
            &artifacts.routing,
            &config.timing,
        );
        let timing_closure_loop =
            self.timing_closure_loop_report(netlist, pdk, config, &artifacts, &timing_closure)?;

        Ok(LayoutReport {
            synthesis: artifacts.synthesis,
            placement: PlacementReport {
                placed_nodes: artifacts.placement.nodes.len(),
                width_um: artifacts.placement.width_um,
                height_um: artifacts.placement.height_um,
            },
            routing: RoutingSummary {
                routed_nets: artifacts.routing.routes.len(),
                total_length_um: artifacts.routing.total_length_um,
                total_detour_overhead_um: artifacts.routing.total_detour_overhead_um,
                detoured_routes: artifacts.routing.detoured_routes,
                jtl_routes: artifacts.routing.jtl_routes,
                ptl_routes: artifacts.routing.ptl_routes,
                effective_prefer_ptl_from_length_um: artifacts
                    .effective_routing_config
                    .prefer_ptl_from_length_um,
                effective_detour_margin_um: artifacts.effective_routing_config.detour_margin_um,
            },
            clock: artifacts.clock,
            timing: TimingSummary {
                worst_setup_slack_ps: artifacts.timing.worst_setup_slack_ps,
                worst_hold_slack_ps: artifacts.timing.worst_hold_slack_ps,
                critical_path_delay_ps: artifacts.timing.critical_path_delay_ps,
                analyzed_arcs: artifacts.timing.analyzed_arcs,
                false_path_arcs: artifacts.timing.false_path_arcs,
                setup_violations: artifacts.timing.setup_violations,
                capture_window_violations: artifacts.timing.capture_window_violations,
                initial_hold_violations: artifacts.initial_hold_violations,
                final_hold_violations: artifacts.timing.hold_violations,
                hold_fix_applied: artifacts.hold_fix_applied,
            },
            timing_closure,
            timing_closure_loop,
            initial_total_detour_overhead_um: artifacts.initial_total_detour_overhead_um,
            detour_feedback_applied: artifacts.detour_feedback_applied,
        })
    }

    pub fn analyze_timing(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> Result<TimingAnalysisReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, config)?;
        let closure = timing_closure_summary(
            artifacts.timing.setup_violations,
            artifacts.timing.hold_violations,
            &artifacts.timing,
            &artifacts.routing,
            &config.timing,
        );
        Ok(TimingAnalysisReport {
            worst_setup_slack_ps: artifacts.timing.worst_setup_slack_ps,
            worst_hold_slack_ps: artifacts.timing.worst_hold_slack_ps,
            critical_path_delay_ps: artifacts.timing.critical_path_delay_ps,
            analyzed_arcs: artifacts.timing.analyzed_arcs,
            false_path_arcs: artifacts.timing.false_path_arcs,
            setup_violations: artifacts.timing.setup_violations,
            hold_violations: artifacts.timing.hold_violations,
            capture_window_violations: artifacts.timing.capture_window_violations,
            detour_feedback_applied: artifacts.detour_feedback_applied,
            hold_fix_applied: artifacts.hold_fix_applied,
            closure,
            timing_arcs: artifacts
                .timing
                .arcs
                .iter()
                .map(|arc| TimingArcSummary {
                    from: arc.from,
                    to: arc.to,
                    is_false_path: arc.is_false_path,
                    route_mode: route_mode_for_arc(&artifacts.routing, arc.from, arc.to)
                        .unwrap_or(RouteMode::Jtl),
                    route_length_um: arc.route_length_um,
                    from_domain: domain_of_pin_from_config(&config.timing, arc.from),
                    to_domain: domain_of_pin_from_config(&config.timing, arc.to),
                    launch_phase: arc.launch_phase,
                    capture_phase: arc.capture_phase,
                    launch_window_start_ps: arc.launch_window_start_ps,
                    launch_window_end_ps: arc.launch_window_end_ps,
                    capture_window_start_ps: arc.capture_window_start_ps,
                    capture_window_end_ps: arc.capture_window_end_ps,
                    arrival_phase_offset_ps: arc.arrival_phase_offset_ps,
                    capture_window_slack_ps: arc.capture_window_slack_ps,
                    capture_window_violation: arc.capture_window_violation,
                    arrival_ps: arc.arrival_ps,
                    required_ps: arc.required_ps,
                    setup_slack_ps: arc.setup_slack_ps,
                    hold_slack_ps: arc.hold_slack_ps,
                })
                .collect(),
        })
    }

    pub fn analyze_timing_corners(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> Result<MultiCornerTimingAnalysisReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, config)?;
        let mut corner_reports = Vec::with_capacity(pdk.timing_corners.len() + 1);
        let mut default_pdk = pdk.clone();
        default_pdk.active_timing_corner = None;
        let default_timing =
            self.timing
                .analyze(netlist, &artifacts.routing, &default_pdk, &config.timing)?;
        corner_reports.push(timing_corner_analysis_report(
            "default",
            true,
            pdk.active_timing_corner.is_none(),
            &default_timing,
            &artifacts.routing,
            &config.timing,
        ));

        for corner_name in pdk.timing_corner_names() {
            let corner_pdk = pdk.with_active_timing_corner(corner_name);
            let timing =
                self.timing
                    .analyze(netlist, &artifacts.routing, &corner_pdk, &config.timing)?;
            corner_reports.push(timing_corner_analysis_report(
                corner_name,
                false,
                pdk.active_timing_corner.as_deref() == Some(corner_name),
                &timing,
                &artifacts.routing,
                &config.timing,
            ));
        }

        Ok(multi_corner_timing_report(
            pdk.active_timing_corner.clone(),
            corner_reports,
        ))
    }

    pub fn analyze_timing_statistical(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
        statistical_config: &StatisticalTimingConfig,
    ) -> Result<StatisticalTimingAnalysisReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, config)?;
        let statistical_report = self.timing.analyze_statistical(
            netlist,
            &artifacts.routing,
            pdk,
            &config.timing,
            statistical_config,
        )?;

        Ok(StatisticalTimingAnalysisReport {
            worst_pessimistic_setup_slack_ps: statistical_report.worst_pessimistic_setup_slack_ps,
            worst_pessimistic_hold_slack_ps: statistical_report.worst_pessimistic_hold_slack_ps,
            analyzed_arcs: statistical_report.analyzed_arcs,
            false_path_arcs: statistical_report.false_path_arcs,
            setup_risk_violations: statistical_report.setup_risk_violations,
            hold_risk_violations: statistical_report.hold_risk_violations,
            sigma_multiplier: statistical_config.sigma_multiplier,
            timing_arcs: statistical_report
                .arcs
                .iter()
                .map(|arc| StatisticalTimingArcSummary {
                    from: arc.from,
                    to: arc.to,
                    is_false_path: arc.is_false_path,
                    route_mode: route_mode_for_arc(&artifacts.routing, arc.from, arc.to)
                        .unwrap_or(RouteMode::Jtl),
                    route_length_um: arc.route_length_um,
                    from_domain: domain_of_pin_from_config(&config.timing, arc.from),
                    to_domain: domain_of_pin_from_config(&config.timing, arc.to),
                    launch_phase: arc.launch_phase,
                    capture_phase: arc.capture_phase,
                    launch_window_start_ps: arc.launch_window_start_ps,
                    launch_window_end_ps: arc.launch_window_end_ps,
                    capture_window_start_ps: arc.capture_window_start_ps,
                    capture_window_end_ps: arc.capture_window_end_ps,
                    arrival_phase_offset_ps: arc.arrival_phase_offset_ps,
                    capture_window_slack_ps: arc.capture_window_slack_ps,
                    capture_window_violation: arc.capture_window_violation,
                    mean_arrival_ps: arc.mean_arrival_ps,
                    mean_required_ps: arc.mean_required_ps,
                    setup_slack_ps: arc.setup_slack_ps,
                    hold_slack_ps: arc.hold_slack_ps,
                    setup_sigma_ps: arc.setup_sigma_ps,
                    hold_sigma_ps: arc.hold_sigma_ps,
                    pessimistic_setup_slack_ps: arc.pessimistic_setup_slack_ps,
                    pessimistic_hold_slack_ps: arc.pessimistic_hold_slack_ps,
                })
                .collect(),
        })
    }

    pub fn analyze_ac_bias(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> Result<AcBiasReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, config)?;
        Ok(ac_bias_report_from_artifacts(&artifacts))
    }

    pub fn optimize_ac_bias(
        &mut self,
        netlist: &Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> Result<AcBiasOptimizationReport, FlowError> {
        let mut baseline_netlist = netlist.clone();
        let baseline_artifacts = self.compile_artifacts(&mut baseline_netlist, pdk, config)?;
        let baseline = ac_bias_report_from_artifacts(&baseline_artifacts);
        let baseline_threshold = config.routing.prefer_ptl_from_length_um;
        let baseline_detour_margin = config.routing.detour_margin_um;
        let longest_ptl_length_um = baseline_artifacts
            .routing
            .routes
            .iter()
            .filter(|route| matches!(route.mode, RouteMode::Ptl))
            .map(|route| route.length_um)
            .fold(0.0_f64, f64::max);

        let mut threshold_candidates = vec![baseline_threshold];
        if longest_ptl_length_um > 0.0 {
            let midpoint_threshold = ((baseline_threshold + longest_ptl_length_um + 1.0) / 2.0)
                .max(baseline_threshold + 10.0);
            threshold_candidates.push(midpoint_threshold);
            threshold_candidates.push(longest_ptl_length_um + 1.0);
        }
        threshold_candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        threshold_candidates.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);

        let mut detour_margin_candidates = vec![baseline_detour_margin];
        if !config.routing.blocked_regions.is_empty() {
            detour_margin_candidates.push((baseline_detour_margin * 0.5).max(0.0));
            detour_margin_candidates.push(baseline_detour_margin + 6.0);
        }
        detour_margin_candidates
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        detour_margin_candidates.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);

        let mut best_threshold = baseline_threshold;
        let mut best_detour_margin = baseline_detour_margin;
        let mut best_report = baseline;

        for &threshold in &threshold_candidates {
            for &detour_margin in &detour_margin_candidates {
                if (threshold - baseline_threshold).abs() <= 1e-9
                    && (detour_margin - baseline_detour_margin).abs() <= 1e-9
                {
                    continue;
                }

                let mut candidate_config = config.clone();
                candidate_config.routing.prefer_ptl_from_length_um = threshold;
                candidate_config.routing.detour_margin_um = detour_margin;
                let mut candidate_netlist = netlist.clone();
                let candidate_artifacts =
                    self.compile_artifacts(&mut candidate_netlist, pdk, &candidate_config)?;
                let candidate_report = ac_bias_report_from_artifacts(&candidate_artifacts);
                if ac_bias_report_better_than(&candidate_report, &best_report) {
                    best_threshold = threshold;
                    best_detour_margin = detour_margin;
                    best_report = candidate_report;
                }
            }
        }

        Ok(AcBiasOptimizationReport {
            baseline,
            optimized: best_report,
            baseline_prefer_ptl_from_length_um: baseline_threshold,
            optimized_prefer_ptl_from_length_um: best_threshold,
            baseline_detour_margin_um: baseline_detour_margin,
            optimized_detour_margin_um: best_detour_margin,
            threshold_candidates_evaluated: threshold_candidates.len(),
            detour_margin_candidates_evaluated: detour_margin_candidates.len(),
            optimization_applied: best_threshold > baseline_threshold + 1e-9
                || best_detour_margin > baseline_detour_margin + 1e-9
                || best_detour_margin < baseline_detour_margin - 1e-9,
        })
    }

    pub fn optimize_ac_bias_with_characterized_library(
        &mut self,
        netlist: &Netlist,
        base_pdk: &Pdk,
        config: &FlowConfig,
        constraint_config: &AdvancedConstraintConfig,
        characterized_library_entries: &[impl AsRef<str>],
    ) -> Result<LibraryAwareAcBiasOptimizationReport, FlowError> {
        let merged_pdk = base_pdk.merge_characterized_library_json_strings(
            &characterized_library_entries
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
        )?;
        let characterized_cells_merged = characterized_library_entries.len();

        let mut baseline_netlist = netlist.clone();
        let baseline_artifacts =
            self.compile_artifacts(&mut baseline_netlist, &merged_pdk, config)?;
        let baseline = ac_bias_report_from_artifacts(&baseline_artifacts);
        let baseline_constraints =
            advanced_constraint_report_from_artifacts(&baseline_artifacts, constraint_config);
        let baseline_threshold = config.routing.prefer_ptl_from_length_um;
        let baseline_detour_margin = config.routing.detour_margin_um;
        let longest_ptl_length_um = baseline_artifacts
            .routing
            .routes
            .iter()
            .filter(|route| matches!(route.mode, RouteMode::Ptl))
            .map(|route| route.length_um)
            .fold(0.0_f64, f64::max);

        let mut threshold_candidates = vec![baseline_threshold];
        if longest_ptl_length_um > 0.0 {
            let midpoint_threshold = ((baseline_threshold + longest_ptl_length_um + 1.0) / 2.0)
                .max(baseline_threshold + 10.0);
            threshold_candidates.push(midpoint_threshold);
            threshold_candidates.push(longest_ptl_length_um + 1.0);
        }
        threshold_candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        threshold_candidates.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);

        let mut detour_margin_candidates = vec![baseline_detour_margin];
        if !config.routing.blocked_regions.is_empty() {
            detour_margin_candidates.push((baseline_detour_margin * 0.5).max(0.0));
            detour_margin_candidates.push(baseline_detour_margin + 6.0);
        }
        if baseline_constraints.violation_count > 0 {
            detour_margin_candidates.push(baseline_detour_margin + 12.0);
        }
        detour_margin_candidates
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        detour_margin_candidates.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);

        let mut best_threshold = baseline_threshold;
        let mut best_detour_margin = baseline_detour_margin;
        let mut best_report = baseline;
        let mut best_constraints = baseline_constraints.clone();
        let mut best_score = library_aware_optimization_score(&baseline, &baseline_constraints);

        for &threshold in &threshold_candidates {
            for &detour_margin in &detour_margin_candidates {
                if (threshold - baseline_threshold).abs() <= 1e-9
                    && (detour_margin - baseline_detour_margin).abs() <= 1e-9
                {
                    continue;
                }

                let mut candidate_config = config.clone();
                candidate_config.routing.prefer_ptl_from_length_um = threshold;
                candidate_config.routing.detour_margin_um = detour_margin;
                let mut candidate_netlist = netlist.clone();
                let candidate_artifacts =
                    self.compile_artifacts(&mut candidate_netlist, &merged_pdk, &candidate_config)?;
                let candidate_report = ac_bias_report_from_artifacts(&candidate_artifacts);
                let candidate_constraints = advanced_constraint_report_from_artifacts(
                    &candidate_artifacts,
                    constraint_config,
                );
                let candidate_score =
                    library_aware_optimization_score(&candidate_report, &candidate_constraints);
                if candidate_score > best_score + 1e-9
                    || (candidate_score - best_score).abs() <= 1e-9
                        && ac_bias_report_better_than(&candidate_report, &best_report)
                {
                    best_threshold = threshold;
                    best_detour_margin = detour_margin;
                    best_report = candidate_report;
                    best_constraints = candidate_constraints;
                    best_score = candidate_score;
                }
            }
        }

        let optimization_applied = best_threshold > baseline_threshold + 1e-9
            || best_detour_margin > baseline_detour_margin + 1e-9
            || best_detour_margin < baseline_detour_margin - 1e-9
            || best_score
                > library_aware_optimization_score(&baseline, &baseline_constraints) + 1e-9;

        Ok(LibraryAwareAcBiasOptimizationReport {
            ac_bias: AcBiasOptimizationReport {
                baseline,
                optimized: best_report,
                baseline_prefer_ptl_from_length_um: baseline_threshold,
                optimized_prefer_ptl_from_length_um: best_threshold,
                baseline_detour_margin_um: baseline_detour_margin,
                optimized_detour_margin_um: best_detour_margin,
                threshold_candidates_evaluated: threshold_candidates.len(),
                detour_margin_candidates_evaluated: detour_margin_candidates.len(),
                optimization_applied,
            },
            baseline_constraints,
            optimized_constraints: best_constraints,
            characterized_cells_merged,
            library_optimization_score: best_score,
        })
    }

    pub fn optimize_design_with_characterized_library(
        &mut self,
        netlist: &Netlist,
        base_pdk: &Pdk,
        config: &FlowConfig,
        constraint_config: &AdvancedConstraintConfig,
        statistical_config: &StatisticalTimingConfig,
        characterized_library_entries: &[impl AsRef<str>],
    ) -> Result<LibraryAwareDesignOptimizationReport, FlowError> {
        let merged_pdk = base_pdk.merge_characterized_library_json_strings(
            &characterized_library_entries
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
        )?;
        let characterized_cells_merged = characterized_library_entries.len();

        let mut baseline_netlist = netlist.clone();
        let baseline_artifacts =
            self.compile_artifacts(&mut baseline_netlist, &merged_pdk, config)?;
        let baseline_ac = ac_bias_report_from_artifacts(&baseline_artifacts);
        let baseline_constraints =
            advanced_constraint_report_from_artifacts(&baseline_artifacts, constraint_config);
        let baseline_statistical = self.analyze_timing_statistical(
            &mut baseline_netlist.clone(),
            &merged_pdk,
            config,
            statistical_config,
        )?;
        let baseline_threshold = config.routing.prefer_ptl_from_length_um;
        let baseline_detour_margin = config.routing.detour_margin_um;
        let longest_ptl_length_um = baseline_artifacts
            .routing
            .routes
            .iter()
            .filter(|route| matches!(route.mode, RouteMode::Ptl))
            .map(|route| route.length_um)
            .fold(0.0_f64, f64::max);

        let mut threshold_candidates = vec![baseline_threshold];
        if longest_ptl_length_um > 0.0 {
            threshold_candidates.push(
                ((baseline_threshold + longest_ptl_length_um + 1.0) / 2.0)
                    .max(baseline_threshold + 10.0),
            );
            threshold_candidates.push(longest_ptl_length_um + 1.0);
        }
        threshold_candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        threshold_candidates.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);

        let mut detour_margin_candidates = vec![baseline_detour_margin];
        if !config.routing.blocked_regions.is_empty() {
            detour_margin_candidates.push((baseline_detour_margin * 0.5).max(0.0));
            detour_margin_candidates.push(baseline_detour_margin + 6.0);
        }
        if baseline_constraints.violation_count > 0
            || baseline_statistical.setup_risk_violations > 0
        {
            detour_margin_candidates.push(baseline_detour_margin + 12.0);
        }
        detour_margin_candidates
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        detour_margin_candidates.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);

        let placement_halo_candidates = placement_halo_scale_candidates(netlist, &merged_pdk);
        let statistical_candidates =
            statistical_config_candidates(statistical_config, netlist, &merged_pdk);

        let baseline_halo_scale = config.placement_halo_scale;
        let mut best_threshold = baseline_threshold;
        let mut best_detour_margin = baseline_detour_margin;
        let mut best_halo_scale = baseline_halo_scale;
        let mut best_statistical_config = *statistical_config;
        let mut best_ac = baseline_ac;
        let mut best_statistical = baseline_statistical.clone();
        let mut best_constraints = baseline_constraints.clone();
        let mut best_score = design_optimization_score(
            &baseline_ac,
            &baseline_statistical,
            &baseline_constraints,
            config.timing.clock_period_ps,
        );

        for &halo_scale in &placement_halo_candidates {
            for &candidate_statistical_config in &statistical_candidates {
                for &threshold in &threshold_candidates {
                    for &detour_margin in &detour_margin_candidates {
                        if (threshold - baseline_threshold).abs() <= 1e-9
                            && (detour_margin - baseline_detour_margin).abs() <= 1e-9
                            && (halo_scale - baseline_halo_scale).abs() <= 1e-9
                            && candidate_statistical_config == *statistical_config
                        {
                            continue;
                        }

                        let mut candidate_config = config.clone();
                        candidate_config.routing.prefer_ptl_from_length_um = threshold;
                        candidate_config.routing.detour_margin_um = detour_margin;
                        candidate_config.placement_halo_scale = halo_scale;
                        let mut candidate_netlist = netlist.clone();
                        let candidate_artifacts = self.compile_artifacts(
                            &mut candidate_netlist,
                            &merged_pdk,
                            &candidate_config,
                        )?;
                        let candidate_ac = ac_bias_report_from_artifacts(&candidate_artifacts);
                        let candidate_constraints = advanced_constraint_report_from_artifacts(
                            &candidate_artifacts,
                            constraint_config,
                        );
                        let candidate_statistical = self.analyze_timing_statistical(
                            &mut candidate_netlist.clone(),
                            &merged_pdk,
                            &candidate_config,
                            &candidate_statistical_config,
                        )?;
                        let candidate_score = design_optimization_score(
                            &candidate_ac,
                            &candidate_statistical,
                            &candidate_constraints,
                            candidate_config.timing.clock_period_ps,
                        );
                        if candidate_score > best_score + 1e-9
                            || ((candidate_score - best_score).abs() <= 1e-9
                                && ac_bias_report_better_than(&candidate_ac, &best_ac))
                        {
                            best_threshold = threshold;
                            best_detour_margin = detour_margin;
                            best_halo_scale = halo_scale;
                            best_statistical_config = candidate_statistical_config;
                            best_ac = candidate_ac;
                            best_statistical = candidate_statistical;
                            best_constraints = candidate_constraints;
                            best_score = candidate_score;
                        }
                    }
                }
            }
        }

        let baseline_score = design_optimization_score(
            &baseline_ac,
            &baseline_statistical,
            &baseline_constraints,
            config.timing.clock_period_ps,
        );
        let optimization_applied = best_threshold > baseline_threshold + 1e-9
            || best_detour_margin > baseline_detour_margin + 1e-9
            || best_detour_margin < baseline_detour_margin - 1e-9
            || (best_halo_scale - baseline_halo_scale).abs() > 1e-9
            || best_statistical_config != *statistical_config
            || best_score > baseline_score + 1e-9;

        Ok(LibraryAwareDesignOptimizationReport {
            ac_bias: AcBiasOptimizationReport {
                baseline: baseline_ac,
                optimized: best_ac,
                baseline_prefer_ptl_from_length_um: baseline_threshold,
                optimized_prefer_ptl_from_length_um: best_threshold,
                baseline_detour_margin_um: baseline_detour_margin,
                optimized_detour_margin_um: best_detour_margin,
                threshold_candidates_evaluated: threshold_candidates.len(),
                detour_margin_candidates_evaluated: detour_margin_candidates.len(),
                optimization_applied,
            },
            baseline_statistical,
            optimized_statistical: best_statistical,
            baseline_constraints,
            optimized_constraints: best_constraints,
            characterized_cells_merged,
            design_optimization_score: best_score,
            baseline_statistical_config: *statistical_config,
            optimized_statistical_config: best_statistical_config,
            baseline_placement_halo_scale: baseline_halo_scale,
            optimized_placement_halo_scale: best_halo_scale,
            placement_candidates_evaluated: placement_halo_candidates.len(),
            statistical_candidates_evaluated: statistical_candidates.len(),
        })
    }

    pub fn characterize_compound_cell(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        flow_config: &FlowConfig,
        simulation_config: &SimulationConfig,
        characterization_config: &CompoundCellCharacterizationConfig,
    ) -> Result<CompoundCellCharacterizationReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, flow_config)?;
        let simulation = simulate_hook(netlist, &artifacts, simulation_config);
        Ok(compound_cell_characterization_from_artifacts(
            netlist,
            &artifacts,
            simulation,
            characterization_config,
        ))
    }

    pub fn analyze_advanced_constraints(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        flow_config: &FlowConfig,
        constraint_config: &AdvancedConstraintConfig,
    ) -> Result<AdvancedConstraintReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, flow_config)?;
        Ok(advanced_constraint_report_from_artifacts(
            &artifacts,
            constraint_config,
        ))
    }

    pub fn verify_layout(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        flow_config: &FlowConfig,
        simulation_config: &SimulationConfig,
    ) -> Result<VerificationReport, FlowError> {
        let artifacts = self.compile_artifacts(netlist, pdk, flow_config)?;
        let checked_ptl_routes = artifacts
            .routing
            .routes
            .iter()
            .filter(|route| matches!(route.mode, RouteMode::Ptl))
            .count();

        let ptl_macro_boundary_violations = artifacts
            .routing
            .routes
            .iter()
            .filter(|route| {
                matches!(route.mode, RouteMode::Ptl)
                    && (matches!(
                        netlist.nodes()[route.from.node.0].kind,
                        rflux_ir::NodeKind::MacroCell
                    ) || matches!(
                        netlist.nodes()[route.to.node.0].kind,
                        rflux_ir::NodeKind::MacroCell
                    ))
            })
            .count();

        let ptl_forbidden_length_violations = artifacts
            .routing
            .routes
            .iter()
            .filter(|route| {
                matches!(route.mode, RouteMode::Ptl) && !pdk.is_ptl_length_allowed(route.length_um)
            })
            .count();

        let structural_violations = artifacts
            .routing
            .routes
            .iter()
            .filter(|route| route.segments.is_empty() && route.length_um > 0.0)
            .count();

        Ok(VerificationReport {
            checked_routes: artifacts.routing.routes.len(),
            checked_ptl_routes,
            structural_violations,
            ptl_macro_boundary_violations,
            ptl_forbidden_length_violations,
            simulation: simulate_hook(netlist, &artifacts, simulation_config),
        })
    }

    fn compile_artifacts(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> Result<CompiledArtifacts, FlowError> {
        let synthesis = self
            .compiler
            .compile_netlist(netlist, pdk, &config.synthesis)?;
        let placement_config = placement_config_with_library_feedback(
            netlist,
            pdk,
            &config.placement,
            config.placement_halo_scale,
        );
        let placement = self.placer.place(netlist, &placement_config)?;
        let routing_config = routing_config_with_library_feedback(netlist, pdk, &config.routing);
        let initial_routing = self
            .router
            .route(netlist, &placement, pdk, &routing_config)?;
        let initial_total_detour_overhead_um = initial_routing.total_detour_overhead_um;

        let (placement, routing, detour_feedback_applied) = self.apply_detour_feedback_if_helpful(
            netlist,
            pdk,
            config,
            placement,
            initial_routing,
            &routing_config,
        )?;
        let initial_timing = self
            .timing
            .analyze(netlist, &routing, pdk, &config.timing)?;
        let initial_closure = timing_closure_summary(
            initial_timing.setup_violations,
            initial_timing.hold_violations,
            &initial_timing,
            &routing,
            &config.timing,
        );
        let (
            routing,
            mut effective_routing_config,
            initial_timing,
            route_delay_optimization_attempted,
            route_delay_optimization_applied,
        ) = self.apply_route_delay_optimization_if_helpful(
            netlist,
            &placement,
            pdk,
            config,
            routing,
            initial_timing,
            &initial_closure,
        )?;
        let initial_hold_violations = initial_timing.hold_violations;
        let hold_fix_attempted = config.min_hold_jtl_length_um > 0.0 && initial_hold_violations > 0;
        let (routing, timing, hold_fix_routing_config, hold_fix_applied) = self
            .apply_hold_fix_if_helpful(netlist, &placement, pdk, config, routing, initial_timing)?;
        if hold_fix_applied {
            effective_routing_config = hold_fix_routing_config;
        }
        let clock = build_clock_summary(netlist, &placement, config.clock_phase_count);

        Ok(CompiledArtifacts {
            synthesis,
            placement,
            routing,
            effective_routing_config,
            clock,
            timing,
            initial_total_detour_overhead_um,
            initial_hold_violations,
            hold_fix_attempted,
            detour_feedback_applied,
            route_delay_optimization_attempted,
            route_delay_optimization_applied,
            hold_fix_applied,
        })
    }

    fn apply_detour_feedback_if_helpful(
        &mut self,
        netlist: &Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
        initial_placement: Placement,
        initial_routing: RoutingReport,
        base_routing_config: &RoutingConfig,
    ) -> Result<(Placement, RoutingReport, bool), FlowError> {
        if initial_routing.total_detour_overhead_um <= 0.0 || initial_routing.detoured_routes == 0 {
            return Ok((initial_placement, initial_routing, false));
        }

        let mut feedback_config = config.placement.clone();
        feedback_config
            .blocked_regions
            .extend(detour_feedback_regions(
                &initial_placement,
                &initial_routing,
            ));
        let feedback_placement_config = placement_config_with_library_feedback(
            netlist,
            pdk,
            &feedback_config,
            config.placement_halo_scale,
        );

        let feedback_placement = self.placer.place(netlist, &feedback_placement_config)?;
        let feedback_routing =
            self.router
                .route(netlist, &feedback_placement, pdk, base_routing_config)?;

        if feedback_routing.total_detour_overhead_um < initial_routing.total_detour_overhead_um {
            Ok((feedback_placement, feedback_routing, true))
        } else {
            Ok((initial_placement, initial_routing, false))
        }
    }

    fn apply_hold_fix_if_helpful(
        &mut self,
        netlist: &Netlist,
        placement: &Placement,
        pdk: &Pdk,
        config: &FlowConfig,
        initial_routing: RoutingReport,
        initial_timing: TimingReport,
    ) -> Result<(RoutingReport, TimingReport, RoutingConfig, bool), FlowError> {
        let initial_violations = initial_timing.hold_violations;
        let mut routing_config =
            routing_config_with_library_feedback(netlist, pdk, &config.routing);
        if config.min_hold_jtl_length_um <= 0.0 || initial_violations == 0 {
            return Ok((initial_routing, initial_timing, routing_config, false));
        }

        routing_config.detour_margin_um = routing_config
            .detour_margin_um
            .max(config.min_hold_jtl_length_um / 2.0);
        routing_config.blocked_regions.extend(hold_fix_regions(
            &initial_routing,
            routing_config.detour_margin_um,
        ));

        let rerouted = self
            .router
            .route(netlist, placement, pdk, &routing_config)?;
        let rerouted_timing = self
            .timing
            .analyze(netlist, &rerouted, pdk, &config.timing)?;
        if rerouted_timing.hold_violations < initial_violations {
            Ok((rerouted, rerouted_timing, routing_config, true))
        } else {
            Ok((initial_routing, initial_timing, routing_config, false))
        }
    }

    fn apply_route_delay_optimization_if_helpful(
        &mut self,
        netlist: &Netlist,
        placement: &Placement,
        pdk: &Pdk,
        config: &FlowConfig,
        initial_routing: RoutingReport,
        initial_timing: TimingReport,
        initial_closure: &TimingClosureSummary,
    ) -> Result<(RoutingReport, RoutingConfig, TimingReport, bool, bool), FlowError> {
        let base_routing_config =
            routing_config_with_library_feedback(netlist, pdk, &config.routing);
        let reduce_route_delay_actions = reduce_route_delay_actions(initial_closure);
        let Some(threshold_um) = recommended_prefer_ptl_from_length_um(&reduce_route_delay_actions)
        else {
            return Ok((
                initial_routing,
                base_routing_config,
                initial_timing,
                false,
                false,
            ));
        };

        let mut candidate_routing_config = base_routing_config.clone();
        candidate_routing_config.prefer_ptl_from_length_um = threshold_um;
        if let Some(detour_margin_um) = recommended_detour_margin_um(
            &initial_routing,
            reduce_route_delay_actions.first().copied(),
        ) {
            candidate_routing_config.detour_margin_um = detour_margin_um;
        }

        let candidate_routing =
            self.router
                .route(netlist, placement, pdk, &candidate_routing_config)?;
        let candidate_timing =
            self.timing
                .analyze(netlist, &candidate_routing, pdk, &config.timing)?;
        if route_delay_candidate_is_better(&candidate_timing, &initial_timing) {
            Ok((
                candidate_routing,
                candidate_routing_config,
                candidate_timing,
                true,
                true,
            ))
        } else {
            Ok((
                initial_routing,
                base_routing_config,
                initial_timing,
                true,
                false,
            ))
        }
    }

    fn timing_closure_loop_report(
        &self,
        netlist: &Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
        artifacts: &CompiledArtifacts,
        closure: &TimingClosureSummary,
    ) -> Result<TimingClosureLoopReport, FlowError> {
        let mut report = timing_closure_loop_report(artifacts, closure);
        let Some(action) = representative_reduce_route_delay_action(closure) else {
            return Ok(report);
        };
        let Some(threshold_um) = report.recommended_prefer_ptl_from_length_um else {
            return Ok(report);
        };

        let mut candidate_config =
            routing_config_with_library_feedback(netlist, pdk, &config.routing);
        candidate_config.prefer_ptl_from_length_um = threshold_um;
        if let Some(detour_margin_um) = report.recommended_detour_margin_um {
            candidate_config.detour_margin_um = detour_margin_um;
        }
        let candidate_routing =
            self.router
                .route(netlist, &artifacts.placement, pdk, &candidate_config)?;
        let candidate_timing =
            self.timing
                .analyze(netlist, &candidate_routing, pdk, &config.timing)?;
        let candidate_route = candidate_routing
            .routes
            .iter()
            .find(|route| route.from == action.from && route.to == action.to);

        report.reduce_route_delay_candidate_attempted = true;
        report.candidate_worst_setup_slack_ps = Some(candidate_timing.worst_setup_slack_ps);
        report.candidate_setup_violations = Some(candidate_timing.setup_violations);
        report.candidate_hold_violations = Some(candidate_timing.hold_violations);
        report.candidate_route_mode = candidate_route.map(|route| route.mode);
        report.candidate_route_length_um = candidate_route.map(|route| route.length_um);
        report.reduce_route_delay_candidate_improved =
            route_delay_candidate_is_better(&candidate_timing, &artifacts.timing);

        Ok(report)
    }

    /// Generate an H-tree clock distribution network.
    pub fn build_clock_tree(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> clock_tree::ClockTreeReport {
        let placement = match self.placer.place(netlist, &config.placement) {
            Ok(p) => p,
            Err(_) => {
                return clock_tree::ClockTreeReport {
                    sink_count: 0,
                    buffer_count: 0,
                    levels: 0,
                    total_wire_length_um: 0.0,
                    estimated_skew_ps: 0.0,
                    phase_count: config.clock_phase_count,
                    phases: Vec::new(),
                }
            }
        };
        let sinks = clock_tree::find_clock_sinks(netlist, &placement);
        clock_tree::build_h_tree(
            netlist,
            &sinks,
            &placement,
            &clock_tree::ClockTreeConfig {
                phase_count: config.clock_phase_count,
                ..clock_tree::ClockTreeConfig::default()
            },
        )
    }

    /// Generate a bias distribution grid estimate.
    pub fn build_bias_grid(
        &self,
        netlist: &Netlist,
        pdk: &Pdk,
        config: &FlowConfig,
    ) -> bias_grid::BiasGridReport {
        let placement = match self.placer.place(netlist, &config.placement) {
            Ok(p) => p,
            Err(_) => {
                return bias_grid::BiasGridReport {
                    grid_cells: 0,
                    total_wire_length_um: 0.0,
                    connected_nodes: 0,
                    estimated_total_bias_current_ma: 0.0,
                }
            }
        };
        bias_grid::build_bias_grid(netlist, &placement, &bias_grid::BiasGridConfig::default())
    }
}

fn simulate_hook(
    netlist: &Netlist,
    artifacts: &CompiledArtifacts,
    simulation_config: &SimulationConfig,
) -> SimulationReport {
    let simulated_events = artifacts.timing.analyzed_arcs + artifacts.synthesis.node_count;
    let deck = generate_simulation_deck(netlist, artifacts);
    run_generated_deck(&deck, simulated_events, simulation_config)
}

fn domain_of_pin_from_config(config: &TimingConfig, pin: PinRef) -> Option<usize> {
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

fn route_mode_for_arc(routing: &RoutingReport, from: PinRef, to: PinRef) -> Option<RouteMode> {
    routing
        .routes
        .iter()
        .find(|route| route.from == from && route.to == to)
        .map(|route| route.mode)
}

fn generate_simulation_deck(netlist: &Netlist, artifacts: &CompiledArtifacts) -> String {
    let mut deck = String::from("* rflux autogenerated simulation deck\n");
    deck.push_str(".title rflux verification\n");
    deck.push_str(".param tstep=0.5p tstop=20p vdd=1m\n");

    let mut pin_nets = BTreeSet::new();

    for node in netlist.nodes() {
        deck.push_str(&format!(
            "* node {} {} {}\n",
            node.id.0,
            node.name,
            node_kind_name(&node.kind)
        ));
        if matches!(node.kind, NodeKind::Port) {
            let drive_net = flow_pin_net_name(PinRef {
                node: node.id,
                port: 0,
            });
            pin_nets.insert(drive_net.clone());
            deck.push_str(&format!(
                "VDRV_{} {} 0 PULSE(0,vdd,0,0.5p,0.5p,5p,10p)\n",
                node.id.0, drive_net
            ));
        }
    }
    for route in &artifacts.routing.routes {
        let from_net = flow_pin_net_name(route.from);
        let to_net = flow_pin_net_name(route.to);
        pin_nets.insert(from_net.clone());
        pin_nets.insert(to_net.clone());
        deck.push_str(&format!(
            "R{}_{}_{} {} {} {}\n",
            route.from.node.0,
            route.from.port,
            route.to.node.0,
            from_net,
            to_net,
            flow_route_resistance_ohm(route)
        ));
    }
    for pin_net in pin_nets {
        deck.push_str(&format!(
            "CLOAD_{} {} 0 1f\n",
            pin_net.replace('-', "_"),
            pin_net
        ));
    }
    deck.push_str(&format!(
        ".measure events param={}\n",
        artifacts.timing.analyzed_arcs
    ));
    deck.push_str(".tran {tstep} {tstop}\n");
    deck.push_str(".print tran v(all)\n");
    deck.push_str(".end\n");
    deck
}

fn flow_pin_net_name(pin: PinRef) -> String {
    format!("n{}_{}", pin.node.0, pin.port)
}

fn flow_route_resistance_ohm(route: &rflux_route::NetRoute) -> f64 {
    match route.mode {
        RouteMode::Jtl => 20.0 + route.length_um.max(0.0) * 0.4,
        RouteMode::Ptl => 35.0 + route.length_um.max(0.0) * 0.6,
    }
}

fn ac_bias_report_from_artifacts(artifacts: &CompiledArtifacts) -> AcBiasReport {
    let routed_nets = artifacts.routing.routes.len();
    let jtl_carrier_candidates = artifacts.routing.jtl_routes;
    let ptl_coupling_risk_routes = artifacts.routing.ptl_routes;
    let clock_sink_count = artifacts.clock.clock_sinks;
    let estimated_static_power_savings_uw = jtl_carrier_candidates as f64 * 0.35;
    let estimated_area_overhead_ratio = if routed_nets == 0 {
        1.0
    } else {
        1.0 + 0.23 * (jtl_carrier_candidates as f64 / routed_nets as f64)
    };
    let estimated_frequency_derate_ratio = if clock_sink_count == 0 {
        1.0
    } else {
        (1.0 - 0.15
            * (clock_sink_count as f64 / (clock_sink_count + jtl_carrier_candidates).max(1) as f64))
            .max(0.25)
    };
    let worst_setup_slack_ps = artifacts.timing.worst_setup_slack_ps;
    let worst_hold_slack_ps = artifacts.timing.worst_hold_slack_ps;
    let carrier_ratio = if routed_nets == 0 {
        0.0
    } else {
        jtl_carrier_candidates as f64 / routed_nets as f64
    };
    let coupling_penalty = if routed_nets == 0 {
        0.0
    } else {
        ptl_coupling_risk_routes as f64 / routed_nets as f64
    };
    let feasibility_score = (carrier_ratio * 0.7 + estimated_frequency_derate_ratio * 0.3
        - coupling_penalty * 0.4)
        .clamp(0.0, 1.0);
    let normalized_power_savings = if routed_nets == 0 {
        0.0
    } else {
        (estimated_static_power_savings_uw / (routed_nets as f64 * 0.35)).clamp(0.0, 1.0)
    };
    let normalized_area_efficiency = (1.0 / estimated_area_overhead_ratio).clamp(0.0, 1.0);
    let normalized_coupling_margin = (1.0 - coupling_penalty).clamp(0.0, 1.0);
    let timing_guardband_score = timing_guardband_score(worst_setup_slack_ps, worst_hold_slack_ps);
    let optimization_score = (feasibility_score * 0.35
        + timing_guardband_score * 0.25
        + estimated_frequency_derate_ratio * 0.15
        + normalized_area_efficiency * 0.10
        + normalized_power_savings * 0.08
        + normalized_coupling_margin * 0.07)
        .clamp(0.0, 1.0);

    AcBiasReport {
        routed_nets,
        jtl_carrier_candidates,
        ptl_coupling_risk_routes,
        clock_sink_count,
        estimated_static_power_savings_uw,
        estimated_area_overhead_ratio,
        estimated_frequency_derate_ratio,
        worst_setup_slack_ps,
        worst_hold_slack_ps,
        timing_guardband_score,
        feasibility_score,
        optimization_score,
    }
}

fn timing_guardband_score(worst_setup_slack_ps: f64, worst_hold_slack_ps: f64) -> f64 {
    fn normalized_slack(slack_ps: f64, scale_ps: f64) -> f64 {
        if slack_ps <= 0.0 {
            0.0
        } else {
            (slack_ps / (slack_ps + scale_ps)).clamp(0.0, 1.0)
        }
    }

    (normalized_slack(worst_setup_slack_ps, 20.0) * 0.7
        + normalized_slack(worst_hold_slack_ps, 5.0) * 0.3)
        .clamp(0.0, 1.0)
}

fn timing_closure_summary(
    setup_violations: usize,
    hold_violations: usize,
    timing: &TimingReport,
    routing: &RoutingReport,
    config: &TimingConfig,
) -> TimingClosureSummary {
    let setup_closed = setup_violations == 0;
    let hold_closed = hold_violations == 0;
    let capture_window_closure_enabled =
        !config.clock_domains.is_empty() || config.sfq_phase_count > 1;
    let closure_capture_window_violations = if capture_window_closure_enabled {
        timing.capture_window_violations
    } else {
        0
    };
    let capture_window_closed = closure_capture_window_violations == 0;
    let mut failing_checks = Vec::new();
    if !setup_closed {
        failing_checks.push("setup".to_string());
    }
    if !hold_closed {
        failing_checks.push("hold".to_string());
    }
    if !capture_window_closed {
        failing_checks.push("capture_window".to_string());
    }
    let mut actions = Vec::new();
    if !setup_closed {
        let mut setup_arcs = timing
            .arcs
            .iter()
            .filter(|arc| !arc.is_false_path && arc.setup_slack_ps < 0.0)
            .collect::<Vec<_>>();
        setup_arcs.sort_by(|left, right| left.setup_slack_ps.total_cmp(&right.setup_slack_ps));
        for arc in setup_arcs
            .into_iter()
            .take(TIMING_CLOSURE_MAX_ACTIONS_PER_CHECK)
        {
            actions.push(timing_closure_action(
                TimingClosureCheck::Setup,
                1,
                arc.from,
                arc.to,
                arc.setup_slack_ps,
                arc.route_length_um,
                routing,
                config,
            ));
        }
    }
    if !hold_closed {
        let mut hold_arcs = timing
            .arcs
            .iter()
            .filter(|arc| !arc.is_false_path && arc.hold_slack_ps < 0.0)
            .collect::<Vec<_>>();
        hold_arcs.sort_by(|left, right| left.hold_slack_ps.total_cmp(&right.hold_slack_ps));
        for arc in hold_arcs
            .into_iter()
            .take(TIMING_CLOSURE_MAX_ACTIONS_PER_CHECK)
        {
            actions.push(timing_closure_action(
                TimingClosureCheck::Hold,
                2,
                arc.from,
                arc.to,
                arc.hold_slack_ps,
                arc.route_length_um,
                routing,
                config,
            ));
        }
    }
    if capture_window_closure_enabled && !capture_window_closed {
        let mut capture_window_arcs = timing
            .arcs
            .iter()
            .filter(|arc| !arc.is_false_path && arc.capture_window_violation)
            .collect::<Vec<_>>();
        capture_window_arcs.sort_by(|left, right| {
            left.capture_window_slack_ps
                .total_cmp(&right.capture_window_slack_ps)
        });
        for arc in capture_window_arcs
            .into_iter()
            .take(TIMING_CLOSURE_MAX_ACTIONS_PER_CHECK)
        {
            actions.push(timing_closure_action(
                TimingClosureCheck::CaptureWindow,
                3,
                arc.from,
                arc.to,
                arc.capture_window_slack_ps,
                arc.route_length_um,
                routing,
                config,
            ));
        }
    }
    let closed = setup_closed && hold_closed && capture_window_closed;
    let action_count = actions.len();
    let primary_action = actions.first().copied();
    let reduce_route_delay_actions = actions
        .iter()
        .filter(|action| action.remediation_kind == TimingClosureRemediationKind::ReduceRouteDelay)
        .count();
    let relax_constraint_or_improve_library_timing_actions = actions
        .iter()
        .filter(|action| {
            action.remediation_kind
                == TimingClosureRemediationKind::RelaxConstraintOrImproveLibraryTiming
        })
        .count();
    let add_hold_padding_actions = actions
        .iter()
        .filter(|action| action.remediation_kind == TimingClosureRemediationKind::AddHoldPadding)
        .count();
    let adjust_sfq_phase_or_pulse_window_actions = actions
        .iter()
        .filter(|action| {
            action.remediation_kind == TimingClosureRemediationKind::AdjustSfqPhaseOrPulseWindow
        })
        .count();
    TimingClosureSummary {
        closed,
        status: if closed { "closed" } else { "open" }.to_string(),
        setup_closed,
        hold_closed,
        capture_window_closed,
        setup_violations,
        hold_violations,
        capture_window_violations: closure_capture_window_violations,
        failing_checks,
        action_count,
        primary_action,
        reduce_route_delay_actions,
        relax_constraint_or_improve_library_timing_actions,
        add_hold_padding_actions,
        adjust_sfq_phase_or_pulse_window_actions,
        actions,
        next_step: if closed {
            "Timing closure reached for setup, hold, and SFQ capture-window checks.".to_string()
        } else {
            "Inspect timing_arcs with setup, hold, or capture-window violations, adjust constraints, SFQ phases, pulse windows, or physical routing, then rerun timing analysis.".to_string()
        },
    }
}

fn timing_corner_analysis_report(
    corner_name: &str,
    is_default_corner: bool,
    is_active_corner: bool,
    timing: &TimingReport,
    routing: &RoutingReport,
    config: &TimingConfig,
) -> TimingCornerAnalysisReport {
    TimingCornerAnalysisReport {
        corner_name: corner_name.to_string(),
        is_default_corner,
        is_active_corner,
        worst_setup_slack_ps: timing.worst_setup_slack_ps,
        worst_hold_slack_ps: timing.worst_hold_slack_ps,
        critical_path_delay_ps: timing.critical_path_delay_ps,
        analyzed_arcs: timing.analyzed_arcs,
        setup_violations: timing.setup_violations,
        hold_violations: timing.hold_violations,
        capture_window_violations: timing.capture_window_violations,
        closure: timing_closure_summary(
            timing.setup_violations,
            timing.hold_violations,
            timing,
            routing,
            config,
        ),
    }
}

fn multi_corner_timing_report(
    active_timing_corner: Option<String>,
    corners: Vec<TimingCornerAnalysisReport>,
) -> MultiCornerTimingAnalysisReport {
    let worst_setup = corners
        .iter()
        .min_by(|a, b| {
            a.worst_setup_slack_ps
                .partial_cmp(&b.worst_setup_slack_ps)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("multi-corner timing report requires at least one corner");
    let worst_hold = corners
        .iter()
        .min_by(|a, b| {
            a.worst_hold_slack_ps
                .partial_cmp(&b.worst_hold_slack_ps)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("multi-corner timing report requires at least one corner");
    let worst_critical_path = corners
        .iter()
        .max_by(|a, b| {
            a.critical_path_delay_ps
                .partial_cmp(&b.critical_path_delay_ps)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("multi-corner timing report requires at least one corner");

    MultiCornerTimingAnalysisReport {
        active_timing_corner,
        corner_count: corners.len(),
        worst_setup_corner: worst_setup.corner_name.clone(),
        worst_hold_corner: worst_hold.corner_name.clone(),
        worst_critical_path_corner: worst_critical_path.corner_name.clone(),
        worst_setup_slack_ps: worst_setup.worst_setup_slack_ps,
        worst_hold_slack_ps: worst_hold.worst_hold_slack_ps,
        worst_critical_path_delay_ps: worst_critical_path.critical_path_delay_ps,
        corners,
    }
}

fn timing_closure_action(
    check: TimingClosureCheck,
    priority: usize,
    from: PinRef,
    to: PinRef,
    slack_ps: f64,
    route_length_um: f64,
    routing: &RoutingReport,
    config: &TimingConfig,
) -> TimingClosureAction {
    let route_mode = route_mode_for_arc(routing, from, to).unwrap_or(RouteMode::Jtl);
    TimingClosureAction {
        check,
        priority,
        remediation_kind: timing_closure_remediation_kind(check, route_mode, route_length_um),
        from,
        to,
        slack_ps,
        route_mode,
        route_length_um,
        from_domain: domain_of_pin_from_config(config, from),
        to_domain: domain_of_pin_from_config(config, to),
    }
}

fn timing_closure_remediation_kind(
    check: TimingClosureCheck,
    route_mode: RouteMode,
    route_length_um: f64,
) -> TimingClosureRemediationKind {
    match check {
        TimingClosureCheck::Hold => TimingClosureRemediationKind::AddHoldPadding,
        TimingClosureCheck::CaptureWindow => {
            TimingClosureRemediationKind::AdjustSfqPhaseOrPulseWindow
        }
        TimingClosureCheck::Setup
            if matches!(route_mode, RouteMode::Ptl) || route_length_um > 80.0 =>
        {
            TimingClosureRemediationKind::ReduceRouteDelay
        }
        TimingClosureCheck::Setup => {
            TimingClosureRemediationKind::RelaxConstraintOrImproveLibraryTiming
        }
    }
}

fn timing_closure_loop_report(
    artifacts: &CompiledArtifacts,
    closure: &TimingClosureSummary,
) -> TimingClosureLoopReport {
    let detour_feedback_attempted = artifacts.initial_total_detour_overhead_um > 0.0;
    let final_total_detour_overhead_um = artifacts.routing.total_detour_overhead_um;
    let reduce_route_delay_actions = reduce_route_delay_actions(closure);
    let reduce_route_delay_action = reduce_route_delay_actions.first().copied();
    let reduce_route_delay_candidate_available = !reduce_route_delay_actions.is_empty();
    let recommended_prefer_ptl_from_length_um =
        recommended_prefer_ptl_from_length_um(&reduce_route_delay_actions);
    let recommended_detour_margin_um =
        recommended_detour_margin_um(&artifacts.routing, reduce_route_delay_action);
    let recommended_route_mode = reduce_route_delay_action.map(|action| {
        if matches!(action.route_mode, RouteMode::Ptl) {
            RouteMode::Jtl
        } else {
            action.route_mode
        }
    });
    let estimated_route_length_um = reduce_route_delay_action.map(|action| action.route_length_um);
    let estimated_slack_deficit_ps =
        reduce_route_delay_action.map(|action| (-action.slack_ps).max(0.0));
    let status = if closure.closed {
        "closed".to_string()
    } else if artifacts.detour_feedback_applied
        || artifacts.route_delay_optimization_applied
        || artifacts.hold_fix_applied
    {
        "improved_open".to_string()
    } else if detour_feedback_attempted
        || artifacts.route_delay_optimization_attempted
        || artifacts.hold_fix_attempted
    {
        "attempted_open".to_string()
    } else {
        "not_attempted_open".to_string()
    };
    let next_step = if closure.closed {
        "Timing closure loop converged; preserve the applied physical implementation settings."
            .to_string()
    } else if closure.add_hold_padding_actions > 0 && !artifacts.hold_fix_attempted {
        "Enable hold padding reroute with a positive min_hold_jtl_length_um and rerun compile_layout.".to_string()
    } else if closure.reduce_route_delay_actions > 0 && !artifacts.route_delay_optimization_applied
    {
        "Review the route-delay candidate and update placement constraints or routing limits before rerunning compile_layout.".to_string()
    } else {
        "Review closure.primary_action and rerun compile_layout after applying the selected remediation.".to_string()
    };

    TimingClosureLoopReport {
        detour_feedback_attempted,
        detour_feedback_applied: artifacts.detour_feedback_applied,
        initial_total_detour_overhead_um: artifacts.initial_total_detour_overhead_um,
        final_total_detour_overhead_um,
        route_delay_optimization_attempted: artifacts.route_delay_optimization_attempted,
        route_delay_optimization_applied: artifacts.route_delay_optimization_applied,
        reduce_route_delay_candidate_available,
        recommended_prefer_ptl_from_length_um,
        recommended_detour_margin_um,
        recommended_route_mode,
        estimated_route_length_um,
        estimated_slack_deficit_ps,
        reduce_route_delay_candidate_attempted: false,
        reduce_route_delay_candidate_improved: false,
        candidate_worst_setup_slack_ps: None,
        candidate_setup_violations: None,
        candidate_hold_violations: None,
        candidate_route_mode: None,
        candidate_route_length_um: None,
        hold_fix_attempted: artifacts.hold_fix_attempted,
        hold_fix_applied: artifacts.hold_fix_applied,
        initial_hold_violations: artifacts.initial_hold_violations,
        final_hold_violations: artifacts.timing.hold_violations,
        status,
        next_step,
    }
}

fn reduce_route_delay_actions(closure: &TimingClosureSummary) -> Vec<&TimingClosureAction> {
    closure
        .actions
        .iter()
        .filter(|action| action.remediation_kind == TimingClosureRemediationKind::ReduceRouteDelay)
        .collect()
}

fn representative_reduce_route_delay_action(
    closure: &TimingClosureSummary,
) -> Option<&TimingClosureAction> {
    reduce_route_delay_actions(closure).into_iter().next()
}

fn recommended_prefer_ptl_from_length_um(actions: &[&TimingClosureAction]) -> Option<f64> {
    let representative = actions.first()?;
    if matches!(representative.route_mode, RouteMode::Ptl) {
        actions
            .iter()
            .filter(|action| matches!(action.route_mode, RouteMode::Ptl))
            .map(|action| action.route_length_um)
            .reduce(f64::max)
            .map(|length_um| length_um + 1.0)
    } else {
        actions
            .iter()
            .filter(|action| !matches!(action.route_mode, RouteMode::Ptl))
            .map(|action| action.route_length_um)
            .reduce(f64::min)
            .map(|length_um| length_um.max(20.0))
    }
}

fn recommended_detour_margin_um(
    routing: &RoutingReport,
    action: Option<&TimingClosureAction>,
) -> Option<f64> {
    action.map(|_action| {
        if routing.detoured_routes > 0 {
            (routing.total_detour_overhead_um / routing.detoured_routes as f64).max(6.0)
        } else {
            0.0
        }
    })
}

fn route_delay_candidate_is_better(candidate: &TimingReport, current: &TimingReport) -> bool {
    if candidate.hold_violations > current.hold_violations
        || candidate.capture_window_violations > current.capture_window_violations
    {
        return false;
    }

    candidate.setup_violations < current.setup_violations
        || candidate.worst_setup_slack_ps > current.worst_setup_slack_ps + 1e-9
}

fn default_macro_area_um2() -> f64 {
    48.0
}

fn default_macro_pipeline_stages() -> u8 {
    2
}

fn characterized_cell_cost_scale(cell: &SfCell) -> f64 {
    let area_scale = (cell.area_um2 / default_macro_area_um2())
        .sqrt()
        .clamp(0.75, 2.5);
    let pipeline_scale =
        1.0 + (cell.pipeline_stages as f64 - default_macro_pipeline_stages() as f64) * 0.15;
    (area_scale * pipeline_scale).clamp(0.75, 3.0)
}

fn placement_config_with_library_feedback(
    netlist: &Netlist,
    pdk: &Pdk,
    base: &PlacementConfig,
    halo_scale: f64,
) -> PlacementConfig {
    let mut config = base.clone();
    let mut macro_scale = 1.0_f64;

    for node in netlist.nodes() {
        if !matches!(node.kind, NodeKind::MacroCell) {
            continue;
        }
        let Some(cell) = pdk.cell_for_node(&node.name, SfCellKind::Macro) else {
            continue;
        };
        macro_scale = macro_scale.max(characterized_cell_cost_scale(cell));
    }

    if macro_scale > 1.0 + f64::EPSILON {
        config.macro_halo_x_um = base.macro_halo_x_um * macro_scale;
        config.macro_halo_y_um = base.macro_halo_y_um * macro_scale;
    }

    let halo_scale = halo_scale.clamp(0.5, 2.0);
    if (halo_scale - 1.0).abs() > f64::EPSILON {
        config.macro_halo_x_um *= halo_scale;
        config.macro_halo_y_um *= halo_scale;
    }

    config
}

fn routing_config_with_library_feedback(
    netlist: &Netlist,
    pdk: &Pdk,
    base: &RoutingConfig,
) -> RoutingConfig {
    let mut config = base.clone();
    let mut macro_scale = 1.0_f64;

    for node in netlist.nodes() {
        if !matches!(node.kind, NodeKind::MacroCell) {
            continue;
        }
        let Some(cell) = pdk.cell_for_node(&node.name, SfCellKind::Macro) else {
            continue;
        };
        macro_scale = macro_scale.max(characterized_cell_cost_scale(cell));
    }

    if macro_scale > 1.0 + f64::EPSILON {
        config.detour_margin_um = base.detour_margin_um * macro_scale;
        config.prefer_ptl_from_length_um =
            (base.prefer_ptl_from_length_um / macro_scale.sqrt()).max(20.0);
    }

    config
}

fn characterization_delay_details_from_simulation(
    simulation: &SimulationReport,
) -> Vec<rflux_tech::CharacterizationDelayDetail> {
    simulation
        .delay_details
        .iter()
        .map(|detail| rflux_tech::CharacterizationDelayDetail {
            name: detail.name.clone(),
            delay_ps: detail.delay_ps,
        })
        .collect()
}

fn intrinsic_delay_from_simulation(simulation: &SimulationReport, sta_fallback_ps: f64) -> f64 {
    if !simulation.delay_details.is_empty() {
        simulation
            .delay_details
            .iter()
            .map(|detail| detail.delay_ps)
            .fold(0.0_f64, f64::max)
    } else {
        simulation
            .reported_worst_delay_ps
            .unwrap_or(sta_fallback_ps)
    }
    .max(0.0)
}

fn delay_calibration_sigma_from_simulation(
    simulation: &SimulationReport,
    sta_derived_delay_ps: f64,
) -> f64 {
    let delay_details = characterization_delay_details_from_simulation(simulation);
    let detail_spread = if delay_details.len() >= 2 {
        let max_delay = delay_details
            .iter()
            .map(|detail| detail.delay_ps)
            .fold(0.0_f64, f64::max);
        let min_delay = delay_details
            .iter()
            .map(|detail| detail.delay_ps)
            .fold(f64::INFINITY, f64::min);
        ((max_delay - min_delay) * 0.5).max(0.0)
    } else {
        0.0
    };
    let worst_delta = simulation
        .reported_worst_delay_ps
        .or_else(|| {
            delay_details
                .iter()
                .map(|detail| detail.delay_ps)
                .reduce(f64::max)
        })
        .filter(|delay| *delay > 0.0)
        .map(|simulated| (simulated - sta_derived_delay_ps).abs() * 0.35)
        .unwrap_or(0.0);
    (detail_spread * 0.40 + worst_delta).max(0.0)
}

fn library_aware_optimization_score(
    ac_bias: &AcBiasReport,
    constraints: &AdvancedConstraintReport,
) -> f64 {
    let constraint_score = (1.0 / (1.0 + constraints.violation_count as f64)).clamp(0.0, 1.0);
    (ac_bias.optimization_score * 0.55
        + ac_bias.timing_guardband_score * 0.20
        + ac_bias.feasibility_score * 0.10
        + constraint_score * 0.15)
        .clamp(0.0, 1.0)
}

fn statistical_timing_score(report: &StatisticalTimingAnalysisReport, clock_period_ps: f64) -> f64 {
    if !report.worst_pessimistic_setup_slack_ps.is_finite() {
        return 0.0;
    }
    if report.worst_pessimistic_setup_slack_ps <= 0.0 {
        return 0.0;
    }
    (report.worst_pessimistic_setup_slack_ps
        / (report.worst_pessimistic_setup_slack_ps + clock_period_ps.max(1.0)))
    .clamp(0.0, 1.0)
}

fn placement_halo_scale_candidates(netlist: &Netlist, pdk: &Pdk) -> Vec<f64> {
    let has_characterized_macro = netlist.nodes().iter().any(|node| {
        matches!(node.kind, NodeKind::MacroCell)
            && pdk.characterization_metadata_for_cell(&node.name).is_some()
    });
    if !has_characterized_macro {
        return vec![1.0_f64];
    }

    let mut candidates: Vec<f64> = vec![1.0, 0.92, 1.08];
    candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    candidates.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    candidates
}

fn statistical_config_candidates(
    base: &StatisticalTimingConfig,
    netlist: &Netlist,
    pdk: &Pdk,
) -> Vec<StatisticalTimingConfig> {
    let mut calibration_sigma = 0.0_f64;
    for node in netlist.nodes() {
        let Some(metadata) = pdk.characterization_metadata_for_cell(&node.name) else {
            continue;
        };
        calibration_sigma = calibration_sigma.max(metadata.delay_calibration_sigma_ps);
    }

    let mut candidates = vec![*base];
    let conservative = StatisticalTimingConfig {
        cell_delay_sigma_ratio: (base.cell_delay_sigma_ratio * 1.20).clamp(0.02, 0.25),
        wire_delay_sigma_ratio: (base.wire_delay_sigma_ratio * 1.15).clamp(0.02, 0.25),
        sigma_multiplier: (base.sigma_multiplier * 1.05).clamp(2.5, 4.5),
        ..*base
    };
    let optimistic = StatisticalTimingConfig {
        cell_delay_sigma_ratio: (base.cell_delay_sigma_ratio * 0.85).clamp(0.02, 0.20),
        wire_delay_sigma_ratio: (base.wire_delay_sigma_ratio * 0.85).clamp(0.02, 0.20),
        sigma_multiplier: (base.sigma_multiplier * 0.95).clamp(2.5, 4.0),
        ..*base
    };
    candidates.push(conservative);
    candidates.push(optimistic);

    if calibration_sigma > 0.0 {
        let calibration_ratio = (calibration_sigma / 18.0).clamp(0.0, 0.35);
        let calibration_aware = StatisticalTimingConfig {
            cell_delay_sigma_ratio: (base.cell_delay_sigma_ratio * (1.0 + calibration_ratio))
                .clamp(0.02, 0.30),
            wire_delay_sigma_ratio: (base.wire_delay_sigma_ratio * (1.0 + calibration_ratio * 0.5))
                .clamp(0.02, 0.25),
            global_cell_delay_sigma_ratio: (base.global_cell_delay_sigma_ratio
                + calibration_ratio * 0.02)
                .clamp(0.0, 0.15),
            ..*base
        };
        candidates.push(calibration_aware);
    }

    candidates.sort_by(|a, b| {
        a.cell_delay_sigma_ratio
            .partial_cmp(&b.cell_delay_sigma_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.dedup_by(|a, b| {
        (a.cell_delay_sigma_ratio - b.cell_delay_sigma_ratio).abs() <= 1e-9
            && (a.sigma_multiplier - b.sigma_multiplier).abs() <= 1e-9
    });
    candidates
}

fn design_optimization_score(
    ac_bias: &AcBiasReport,
    statistical: &StatisticalTimingAnalysisReport,
    constraints: &AdvancedConstraintReport,
    clock_period_ps: f64,
) -> f64 {
    let constraint_score = (1.0 / (1.0 + constraints.violation_count as f64)).clamp(0.0, 1.0);
    let ssta_score = statistical_timing_score(statistical, clock_period_ps);
    let risk_penalty =
        (statistical.setup_risk_violations as f64 + statistical.hold_risk_violations as f64) * 0.04;
    (library_aware_optimization_score(ac_bias, constraints) * 0.50
        + ssta_score * 0.35
        + constraint_score * 0.15
        - risk_penalty)
        .clamp(0.0, 1.0)
}

fn delay_detail_name_matches_edge(
    driver_name: &str,
    sink_name: &str,
    detail: &SimulationDelayDetail,
) -> bool {
    let driver = driver_name.to_ascii_lowercase();
    let sink = sink_name.to_ascii_lowercase();
    let joined = detail.name.to_ascii_lowercase();
    if joined.contains(&driver) && joined.contains(&sink) {
        return true;
    }

    let normalized_name = detail.name.to_ascii_lowercase();
    let tokens = normalized_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let driver_hit = tokens
        .iter()
        .any(|token| *token == driver || driver.contains(token) || token.contains(driver.as_str()));
    let sink_hit = tokens
        .iter()
        .any(|token| *token == sink || sink.contains(token) || token.contains(sink.as_str()));
    driver_hit && sink_hit
}

fn match_delay_detail_to_edge(
    netlist: &Netlist,
    detail: &SimulationDelayDetail,
    edges: &[(PinRef, PinRef)],
) -> Option<(PinRef, PinRef)> {
    let mut best: Option<(PinRef, PinRef, usize)> = None;
    for (from, to) in edges {
        let driver = &netlist.nodes()[from.node.0].name;
        let sink = &netlist.nodes()[to.node.0].name;
        let mut score = 0usize;
        if delay_detail_name_matches_edge(driver, sink, detail) {
            score += 4;
        }
        if detail
            .name
            .to_ascii_lowercase()
            .contains(&driver.to_ascii_lowercase())
        {
            score += 2;
        }
        if detail
            .name
            .to_ascii_lowercase()
            .contains(&sink.to_ascii_lowercase())
        {
            score += 2;
        }
        if score == 0 {
            continue;
        }
        if best
            .as_ref()
            .map(|(_, _, current)| score > *current)
            .unwrap_or(true)
        {
            best = Some((*from, *to, score));
        }
    }
    best.map(|(from, to, _)| (from, to))
}

fn characterization_arc_delays_from_simulation(
    netlist: &Netlist,
    simulation: &SimulationReport,
    characterized_cell_name: &str,
) -> Vec<rflux_tech::CharacterizationArcDelay> {
    let mut arc_delays = simulation
        .delay_details
        .iter()
        .filter_map(|detail| {
            let from = detail.from_ref.as_ref()?;
            let to = detail.to_ref.as_ref()?;
            Some(rflux_tech::CharacterizationArcDelay {
                name: detail.name.clone(),
                driver_cell_name: from.node.clone(),
                from_port: from.port.unwrap_or(0),
                sink_cell_name: to.node.clone(),
                to_port: to.port.unwrap_or(0),
                delay_ps: detail.delay_ps,
            })
        })
        .collect::<Vec<_>>();

    append_canonical_characterized_output_arcs(netlist, characterized_cell_name, &mut arc_delays);

    if !arc_delays.is_empty() {
        return arc_delays;
    }

    let edges = netlist.edge_pairs();
    if edges.is_empty() || simulation.delay_details.is_empty() {
        return arc_delays;
    }

    let mut used_edges = vec![false; edges.len()];
    let mut unmatched_details = Vec::new();

    for detail in &simulation.delay_details {
        if let Some((from, to)) = match_delay_detail_to_edge(netlist, detail, &edges) {
            if let Some(index) = edges.iter().position(|edge| edge.0 == from && edge.1 == to) {
                if !used_edges[index] {
                    let driver = &netlist.nodes()[from.node.0];
                    let sink = &netlist.nodes()[to.node.0];
                    arc_delays.push(rflux_tech::CharacterizationArcDelay {
                        name: detail.name.clone(),
                        driver_cell_name: driver.name.clone(),
                        from_port: from.port,
                        sink_cell_name: sink.name.clone(),
                        to_port: to.port,
                        delay_ps: detail.delay_ps,
                    });
                    used_edges[index] = true;
                    continue;
                }
            }
        }
        unmatched_details.push(detail);
    }

    let mut unused_edge_indices = (0..edges.len())
        .filter(|index| !used_edges[*index])
        .collect::<Vec<_>>();
    for (detail, edge_index) in unmatched_details.iter().zip(unused_edge_indices.drain(..)) {
        let (from, to) = edges[edge_index];
        let driver = &netlist.nodes()[from.node.0];
        let sink = &netlist.nodes()[to.node.0];
        arc_delays.push(rflux_tech::CharacterizationArcDelay {
            name: detail.name.clone(),
            driver_cell_name: driver.name.clone(),
            from_port: from.port,
            sink_cell_name: sink.name.clone(),
            to_port: to.port,
            delay_ps: detail.delay_ps,
        });
    }

    append_canonical_characterized_output_arcs(netlist, characterized_cell_name, &mut arc_delays);

    arc_delays
}

fn append_canonical_characterized_output_arcs(
    netlist: &Netlist,
    characterized_cell_name: &str,
    arc_delays: &mut Vec<rflux_tech::CharacterizationArcDelay>,
) {
    let mut existing_keys = arc_delays
        .iter()
        .map(|arc| {
            (
                arc.driver_cell_name.clone(),
                arc.from_port,
                arc.sink_cell_name.clone(),
                arc.to_port,
            )
        })
        .collect::<std::collections::BTreeSet<_>>();

    let port_names = netlist
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Port))
        .map(|node| node.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    let mut canonical_arcs = Vec::new();
    for arc in arc_delays.iter() {
        if !port_names.contains(arc.sink_cell_name.as_str()) {
            continue;
        }
        let key = (
            characterized_cell_name.to_string(),
            arc.from_port,
            "*".to_string(),
            arc.to_port,
        );
        if !existing_keys.insert(key) {
            continue;
        }
        canonical_arcs.push(rflux_tech::CharacterizationArcDelay {
            name: format!("{}_output_port_{}", characterized_cell_name, arc.from_port),
            driver_cell_name: characterized_cell_name.to_string(),
            from_port: arc.from_port,
            sink_cell_name: "*".to_string(),
            to_port: arc.to_port,
            delay_ps: arc.delay_ps,
        });
    }

    arc_delays.extend(canonical_arcs);
}

fn compound_cell_characterization_from_artifacts(
    netlist: &Netlist,
    artifacts: &CompiledArtifacts,
    simulation: SimulationReport,
    characterization_config: &CompoundCellCharacterizationConfig,
) -> CompoundCellCharacterizationReport {
    let sta_fallback_ps = artifacts.timing.critical_path_delay_ps.max(0.0);
    let derived_intrinsic_delay_ps = intrinsic_delay_from_simulation(&simulation, sta_fallback_ps);
    let derived_setup_ps = (artifacts.timing.critical_path_delay_ps * 0.12)
        .max(artifacts.timing.setup_violations as f64);
    let derived_hold_ps = artifacts.timing.worst_hold_slack_ps.max(0.0);
    let generated_kind =
        if artifacts.synthesis.tech_map.mapped_nodes > 1 || artifacts.synthesis.node_count > 1 {
            SfCellKind::Macro
        } else {
            SfCellKind::GenericGate
        };
    let generated_pipeline_stages = artifacts.clock.phase_count.max(1).min(u8::MAX as usize) as u8;
    let delay_details = characterization_delay_details_from_simulation(&simulation);
    let arc_delays = characterization_arc_delays_from_simulation(
        netlist,
        &simulation,
        &characterization_config.cell_name,
    );
    let delay_calibration_sigma_ps =
        delay_calibration_sigma_from_simulation(&simulation, derived_intrinsic_delay_ps);
    let generated_entry = CharacterizedCellLibraryEntry {
        cell: SfCell {
            name: characterization_config.cell_name.clone(),
            kind: generated_kind,
            area_um2: artifacts.synthesis.tech_map.total_area_um2,
            pipeline_stages: generated_pipeline_stages,
        },
        timing: CellTimingModel {
            kind: generated_kind,
            intrinsic_delay_ps: derived_intrinsic_delay_ps,
            setup_ps: derived_setup_ps,
            hold_ps: derived_hold_ps,
        },
        metadata: Some(rflux_tech::CharacterizationArtifactMetadata {
            waveform_path: simulation.waveform_path.clone(),
            simulated_delay_ps: simulation.reported_worst_delay_ps,
            sta_derived_delay_ps: Some(derived_intrinsic_delay_ps),
            delay_calibration_sigma_ps,
            delay_details,
            arc_delays,
        }),
    };
    let generated_library_json = serde_json::to_string_pretty(&generated_entry)
        .unwrap_or_else(|_| "{\"error\":\"characterization_serialize_failed\"}".to_string());

    CompoundCellCharacterizationReport {
        cell_name: characterization_config.cell_name.clone(),
        node_count: artifacts.synthesis.node_count,
        edge_count: artifacts.synthesis.edge_count,
        mapped_nodes: artifacts.synthesis.tech_map.mapped_nodes,
        total_area_um2: artifacts.synthesis.tech_map.total_area_um2,
        derived_intrinsic_delay_ps,
        derived_setup_ps,
        derived_hold_ps,
        generated_cell_kind: match generated_kind {
            SfCellKind::GenericGate => "generic_gate".to_string(),
            SfCellKind::Macro => "macro".to_string(),
            SfCellKind::Splitter => "splitter".to_string(),
            SfCellKind::Dff => "dff".to_string(),
            SfCellKind::Jtl => "jtl".to_string(),
            SfCellKind::Ptl => "ptl".to_string(),
            SfCellKind::Port => "port".to_string(),
        },
        generated_pipeline_stages,
        generated_library_json,
        simulated_delay_ps: simulation.reported_worst_delay_ps,
        simulation_backend: simulation.backend,
        generated_deck_lines: simulation.generated_deck_lines,
        generated_deck_path: simulation.generated_deck_path,
        waveform_path: simulation.waveform_path,
        reported_violations: simulation.reported_violations,
    }
}

fn advanced_constraint_report_from_artifacts(
    artifacts: &CompiledArtifacts,
    constraint_config: &AdvancedConstraintConfig,
) -> AdvancedConstraintReport {
    let total_length_um = artifacts.routing.total_length_um.max(1.0);
    let routed_nets = artifacts.routing.routes.len().max(1) as f64;
    let normalized_mapped_area = (artifacts.synthesis.tech_map.total_area_um2 / 100.0).max(0.0);
    let estimated_thermal_load_uw = artifacts.routing.jtl_routes as f64 * 0.22
        + artifacts.routing.ptl_routes as f64 * 0.08
        + artifacts.clock.clock_sinks as f64 * 0.05
        + normalized_mapped_area * 0.04;
    let detour_overhead_ratio =
        (artifacts.routing.total_detour_overhead_um / total_length_um).clamp(0.0, 1.0);
    let ptl_coupling_ratio = (artifacts.routing.ptl_routes as f64 / routed_nets).clamp(0.0, 1.0);
    let jtl_density_per_100um =
        artifacts.routing.jtl_routes as f64 / (total_length_um / 100.0).max(1.0);
    let estimated_mechanical_stress_score = (detour_overhead_ratio * 0.55
        + ptl_coupling_ratio * 0.20
        + jtl_density_per_100um / 10.0 * 0.15
        + (normalized_mapped_area / (normalized_mapped_area + 1.0)) * 0.10)
        .clamp(0.0, 1.0);
    let manufacturing_hotspots = artifacts.routing.detoured_routes
        + usize::from(detour_overhead_ratio > 0.20)
        + usize::from(ptl_coupling_ratio > 0.50);

    let mut violations = Vec::new();
    if estimated_thermal_load_uw > constraint_config.max_estimated_thermal_load_uw {
        violations.push(AdvancedConstraintViolation {
            category: "thermal".to_string(),
            detail: "estimated thermal load exceeds configured budget".to_string(),
            measured_value: estimated_thermal_load_uw,
            limit_value: constraint_config.max_estimated_thermal_load_uw,
        });
    }
    if estimated_mechanical_stress_score > constraint_config.max_estimated_mechanical_stress_score {
        violations.push(AdvancedConstraintViolation {
            category: "mechanical".to_string(),
            detail: "estimated mechanical stress score exceeds configured limit".to_string(),
            measured_value: estimated_mechanical_stress_score,
            limit_value: constraint_config.max_estimated_mechanical_stress_score,
        });
    }
    if jtl_density_per_100um > constraint_config.max_jtl_density_per_100um {
        violations.push(AdvancedConstraintViolation {
            category: "manufacturing".to_string(),
            detail: "JTL density exceeds configured manufacturing limit".to_string(),
            measured_value: jtl_density_per_100um,
            limit_value: constraint_config.max_jtl_density_per_100um,
        });
    }
    if detour_overhead_ratio > constraint_config.max_detour_overhead_ratio {
        violations.push(AdvancedConstraintViolation {
            category: "manufacturing".to_string(),
            detail: "detour overhead ratio exceeds configured manufacturability limit".to_string(),
            measured_value: detour_overhead_ratio,
            limit_value: constraint_config.max_detour_overhead_ratio,
        });
    }
    if ptl_coupling_ratio > constraint_config.max_ptl_coupling_ratio {
        violations.push(AdvancedConstraintViolation {
            category: "electrical".to_string(),
            detail: "PTL coupling ratio exceeds configured electrical limit".to_string(),
            measured_value: ptl_coupling_ratio,
            limit_value: constraint_config.max_ptl_coupling_ratio,
        });
    }

    AdvancedConstraintReport {
        estimated_thermal_load_uw,
        estimated_mechanical_stress_score,
        jtl_density_per_100um,
        detour_overhead_ratio,
        ptl_coupling_ratio,
        manufacturing_hotspots,
        violation_count: violations.len(),
        violations,
    }
}

fn ac_bias_report_better_than(candidate: &AcBiasReport, current: &AcBiasReport) -> bool {
    if candidate.optimization_score > current.optimization_score + 1e-9 {
        return true;
    }
    if (candidate.optimization_score - current.optimization_score).abs() > 1e-9 {
        return false;
    }

    if candidate.timing_guardband_score > current.timing_guardband_score + 1e-9 {
        return true;
    }
    if (candidate.timing_guardband_score - current.timing_guardband_score).abs() > 1e-9 {
        return false;
    }

    if candidate.feasibility_score > current.feasibility_score + 1e-9 {
        return true;
    }
    if (candidate.feasibility_score - current.feasibility_score).abs() > 1e-9 {
        return false;
    }

    if candidate.ptl_coupling_risk_routes != current.ptl_coupling_risk_routes {
        return candidate.ptl_coupling_risk_routes < current.ptl_coupling_risk_routes;
    }

    if candidate.estimated_frequency_derate_ratio > current.estimated_frequency_derate_ratio + 1e-9
    {
        return true;
    }
    if (candidate.estimated_frequency_derate_ratio - current.estimated_frequency_derate_ratio).abs()
        > 1e-9
    {
        return false;
    }

    candidate.jtl_carrier_candidates > current.jtl_carrier_candidates
}

fn node_kind_name(kind: &rflux_ir::NodeKind) -> &'static str {
    match kind {
        rflux_ir::NodeKind::CellInstance => "CELL",
        rflux_ir::NodeKind::MacroCell => "MACRO",
        rflux_ir::NodeKind::Splitter => "SPLITTER",
        rflux_ir::NodeKind::Dff => "DFF",
        rflux_ir::NodeKind::Jtl => "JTL",
        rflux_ir::NodeKind::Ptl => "PTL",
        rflux_ir::NodeKind::Port => "PORT",
    }
}

fn detour_feedback_regions(
    placement: &Placement,
    routing: &RoutingReport,
) -> Vec<PlacementBlockedRegion> {
    let mut rows = Vec::<f64>::new();
    for route in &routing.routes {
        if route.length_um <= route.direct_length_um {
            continue;
        }

        if let Some(source) = placement.point_of(route.from.node) {
            if !rows.iter().any(|row| (*row - source.y_um).abs() < 1e-9) {
                rows.push(source.y_um);
            }
        }
        if let Some(sink) = placement.point_of(route.to.node) {
            if !rows.iter().any(|row| (*row - sink.y_um).abs() < 1e-9) {
                rows.push(sink.y_um);
            }
        }
    }

    rows.into_iter()
        .map(|row_y_um| PlacementBlockedRegion {
            min_x_um: 0.0,
            max_x_um: placement.width_um,
            min_y_um: row_y_um,
            max_y_um: row_y_um,
        })
        .collect()
}

fn build_clock_summary(
    netlist: &Netlist,
    placement: &Placement,
    clock_phase_count: usize,
) -> ClockSummary {
    let phase_count = clock_phase_count.max(1);
    let mut clock_sinks = 0usize;
    let mut clock_buffers = 0usize;
    let mut assigned_phases = 0usize;

    for node in netlist.nodes() {
        if !matches!(
            node.kind,
            rflux_ir::NodeKind::Dff | rflux_ir::NodeKind::MacroCell
        ) {
            continue;
        }
        clock_sinks += 1;
        if let Some(point) = placement.point_of(node.id) {
            let level = (point.x_um / 40.0).round().max(0.0) as usize;
            clock_buffers += level;
            assigned_phases += level % phase_count;
        }
    }

    ClockSummary {
        clock_sinks,
        clock_buffers,
        phase_count,
        assigned_phases,
    }
}

fn hold_fix_regions(routing: &RoutingReport, detour_margin_um: f64) -> Vec<RoutingBlockedRegion> {
    routing
        .routes
        .iter()
        .filter(|route| matches!(route.mode, RouteMode::Jtl))
        .map(|route| {
            let min_x = route
                .segments
                .iter()
                .map(|segment| segment.start.x_um.min(segment.end.x_um))
                .fold(f64::INFINITY, f64::min);
            let max_x = route
                .segments
                .iter()
                .map(|segment| segment.start.x_um.max(segment.end.x_um))
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = route
                .segments
                .iter()
                .map(|segment| segment.start.y_um.min(segment.end.y_um))
                .fold(f64::INFINITY, f64::min);
            let max_y = route
                .segments
                .iter()
                .map(|segment| segment.start.y_um.max(segment.end.y_um))
                .fold(f64::NEG_INFINITY, f64::max);

            let x_shrink = ((max_x - min_x) / 4.0).min(detour_margin_um / 2.0);
            let y_shrink = ((max_y - min_y) / 4.0).min(detour_margin_um / 2.0);

            RoutingBlockedRegion {
                min_x_um: min_x + x_shrink,
                max_x_um: max_x - x_shrink,
                min_y_um: min_y + y_shrink - detour_margin_um,
                max_y_um: max_y - y_shrink + detour_margin_um,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{NodeKind, PinRef};
    use rflux_place::{FixedNodePlacement, Point};
    use rflux_route::{BlockedRegion, NetRoute};
    use rflux_synth::{BoolOptReport, CompilePlan, CompileReport, ConnectionSpec, TechMapReport};
    use rflux_tech::{InterconnectKind, InterconnectTimingModel, PdkTimingCorner, TimingPoint};

    #[test]
    fn compiles_netlist_into_layout_report() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");

        let mut config = FlowConfig::default();
        config.synthesis.plan = CompilePlan {
            connections: vec![
                ConnectionSpec {
                    from: PinRef { node: a, port: 0 },
                    to: PinRef {
                        node: gate,
                        port: 0,
                    },
                },
                ConnectionSpec {
                    from: PinRef { node: b, port: 0 },
                    to: PinRef {
                        node: gate,
                        port: 1,
                    },
                },
            ],
            ..CompilePlan::default()
        };

        let mut runner = FlowRunner::new();
        let report = runner
            .compile_layout(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("flow should succeed");

        assert_eq!(report.synthesis.compile.connections_applied, 2);
        assert_eq!(report.placement.placed_nodes, 3);
        assert_eq!(report.routing.routed_nets, 2);
        assert!(report.routing.total_length_um > 0.0);
        assert!(report.initial_total_detour_overhead_um >= 0.0);
        assert!(!report.detour_feedback_applied);
        assert_eq!(report.clock.phase_count, 2);
        assert_eq!(
            report.timing.initial_hold_violations,
            report.timing.final_hold_violations
        );
        assert_eq!(report.timing.analyzed_arcs, 2);
        assert!(report.timing_closure.closed);
        assert_eq!(report.timing_closure.status, "closed");
    }

    #[test]
    fn analyze_timing_exposes_arc_route_and_domain_context() {
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

        let mut config = FlowConfig::default();
        config.timing.node_constraints = vec![
            rflux_timing::NodeTimingConstraint {
                node: source,
                input_arrival_ps: None,
                required_ps: None,
                clock_domain: Some(1),
            },
            rflux_timing::NodeTimingConstraint {
                node: sink,
                input_arrival_ps: None,
                required_ps: None,
                clock_domain: Some(2),
            },
        ];
        config.timing.clock_domains = vec![
            rflux_timing::ClockDomainConstraint {
                id: 1,
                period_ps: 10.0,
            },
            rflux_timing::ClockDomainConstraint {
                id: 2,
                period_ps: 10.0,
            },
        ];
        config.timing.sfq_phase_count = 2;
        config.timing.sfq_pulse_window_ps = 2.5;
        config.timing.crossing_constraints = vec![rflux_timing::CrossingConstraint {
            from_domain: 1,
            to_domain: 2,
            kind: rflux_timing::CrossingConstraintKind::FalsePath,
            value_ps: None,
            cycles: None,
        }];

        let report = FlowRunner::new()
            .analyze_timing(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("timing should succeed");

        assert_eq!(report.timing_arcs.len(), 1);
        assert_eq!(report.timing_arcs[0].route_mode, RouteMode::Jtl);
        assert_eq!(report.timing_arcs[0].route_length_um, 40.0);
        assert_eq!(report.timing_arcs[0].from_domain, Some(1));
        assert_eq!(report.timing_arcs[0].to_domain, Some(2));
        assert_eq!(report.timing_arcs[0].launch_phase, 0);
        assert_eq!(report.timing_arcs[0].capture_phase, 1);
        assert_eq!(report.timing_arcs[0].launch_window_start_ps, 0.0);
        assert_eq!(report.timing_arcs[0].launch_window_end_ps, 2.5);
        assert_eq!(report.timing_arcs[0].capture_window_start_ps, 5.0);
        assert_eq!(report.timing_arcs[0].capture_window_end_ps, 7.5);
        assert_eq!(report.capture_window_violations, 0);
        assert_eq!(report.timing_arcs[0].arrival_phase_offset_ps, 8.0);
        assert_eq!(report.timing_arcs[0].capture_window_slack_ps, -0.5);
        assert!(!report.timing_arcs[0].capture_window_violation);
        assert!(report.timing_arcs[0].is_false_path);
        assert!(report.closure.closed);
        assert_eq!(report.closure.status, "closed");
    }

    #[test]
    fn analyze_timing_reports_open_closure_for_setup_violations() {
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

        let mut config = FlowConfig::default();
        config.timing.node_constraints = vec![rflux_timing::NodeTimingConstraint {
            node: sink,
            input_arrival_ps: None,
            required_ps: Some(20.0),
            clock_domain: None,
        }];

        let report = FlowRunner::new()
            .analyze_timing(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 1);
        assert!(!report.closure.closed);
        assert_eq!(report.closure.status, "open");
        assert_eq!(report.closure.failing_checks, vec!["setup"]);
        assert_eq!(report.closure.action_count, 1);
        assert_eq!(report.closure.actions.len(), 1);
        assert_eq!(report.closure.actions[0].check, TimingClosureCheck::Setup);
        assert_eq!(
            report.closure.primary_action,
            Some(report.closure.actions[0])
        );
        assert_eq!(report.closure.actions[0].priority, 1);
        assert_eq!(
            report.closure.actions[0].remediation_kind,
            TimingClosureRemediationKind::RelaxConstraintOrImproveLibraryTiming
        );
        assert_eq!(report.closure.reduce_route_delay_actions, 0);
        assert_eq!(
            report
                .closure
                .relax_constraint_or_improve_library_timing_actions,
            1
        );
        assert_eq!(report.closure.add_hold_padding_actions, 0);
        assert_eq!(
            report.closure.actions[0].from,
            PinRef {
                node: source,
                port: 0
            }
        );
        assert_eq!(
            report.closure.actions[0].to,
            PinRef {
                node: sink,
                port: 0
            }
        );
        assert!(report.closure.actions[0].slack_ps < 0.0);
        assert!(report
            .closure
            .next_step
            .contains("Inspect timing_arcs with setup, hold, or capture-window violations"));
    }

    #[test]
    fn analyze_timing_reports_top_closure_actions_for_setup_violations() {
        let mut netlist = Netlist::new();
        let sources = (0..4)
            .map(|index| netlist.add_node(NodeKind::Port, format!("source_{index}")))
            .collect::<Vec<_>>();
        let sinks = (0..4)
            .map(|index| netlist.add_node(NodeKind::Dff, format!("sink_{index}")))
            .collect::<Vec<_>>();
        for (index, (source, sink)) in sources.iter().zip(sinks.iter()).enumerate() {
            netlist
                .connect(
                    PinRef {
                        node: *source,
                        port: index as u16,
                    },
                    PinRef {
                        node: *sink,
                        port: 0,
                    },
                )
                .expect("source to sink");
        }

        let mut config = FlowConfig::default();
        config.timing.node_constraints = sinks
            .iter()
            .enumerate()
            .map(|(index, sink)| rflux_timing::NodeTimingConstraint {
                node: *sink,
                input_arrival_ps: None,
                required_ps: Some(18.0 + index as f64),
                clock_domain: None,
            })
            .collect();

        let report = FlowRunner::new()
            .analyze_timing(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 4);
        assert!(!report.closure.closed);
        assert_eq!(report.closure.failing_checks, vec!["setup"]);
        assert_eq!(report.closure.action_count, 3);
        assert_eq!(report.closure.actions.len(), 3);
        assert!(report
            .closure
            .actions
            .iter()
            .all(|action| action.check == TimingClosureCheck::Setup));
        assert_eq!(
            report.closure.primary_action,
            Some(report.closure.actions[0])
        );
        assert!(report
            .closure
            .actions
            .windows(2)
            .all(|pair| pair[0].slack_ps <= pair[1].slack_ps));
        let mut expected_top_arcs = report
            .timing_arcs
            .iter()
            .filter(|arc| arc.setup_slack_ps < 0.0)
            .collect::<Vec<_>>();
        expected_top_arcs
            .sort_by(|left, right| left.setup_slack_ps.total_cmp(&right.setup_slack_ps));
        assert_eq!(
            report
                .closure
                .actions
                .iter()
                .map(|action| action.to)
                .collect::<Vec<_>>(),
            expected_top_arcs
                .iter()
                .take(3)
                .map(|arc| arc.to)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn analyze_timing_reports_open_closure_for_capture_window_violations() {
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

        let mut config = FlowConfig::default();
        config.timing.node_constraints = vec![rflux_timing::NodeTimingConstraint {
            node: sink,
            input_arrival_ps: None,
            required_ps: Some(120.0),
            clock_domain: Some(1),
        }];
        config.timing.clock_domains = vec![rflux_timing::ClockDomainConstraint {
            id: 1,
            period_ps: 10.0,
        }];

        let report = FlowRunner::new()
            .analyze_timing(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("timing should succeed");

        assert_eq!(report.setup_violations, 0);
        assert_eq!(report.hold_violations, 0);
        assert_eq!(report.capture_window_violations, 1);
        assert!(!report.timing_arcs[0].is_false_path);
        assert!(report.timing_arcs[0].capture_window_violation);
        assert_eq!(report.timing_arcs[0].capture_window_start_ps, 0.0);
        assert_eq!(report.timing_arcs[0].capture_window_end_ps, 4.0);
        assert_eq!(report.timing_arcs[0].arrival_phase_offset_ps, 8.0);
        assert_eq!(report.timing_arcs[0].capture_window_slack_ps, -4.0);
        assert!(!report.closure.closed);
        assert_eq!(report.closure.status, "open");
        assert!(report.closure.setup_closed);
        assert!(report.closure.hold_closed);
        assert!(!report.closure.capture_window_closed);
        assert_eq!(report.closure.failing_checks, vec!["capture_window"]);
        assert_eq!(report.closure.action_count, 1);
        assert_eq!(
            report.closure.actions[0].check,
            TimingClosureCheck::CaptureWindow
        );
        assert_eq!(
            report.closure.actions[0].remediation_kind,
            TimingClosureRemediationKind::AdjustSfqPhaseOrPulseWindow
        );
        assert_eq!(report.closure.adjust_sfq_phase_or_pulse_window_actions, 1);
        assert_eq!(report.closure.add_hold_padding_actions, 0);
        assert_eq!(report.closure.reduce_route_delay_actions, 0);
        assert_eq!(
            report
                .closure
                .relax_constraint_or_improve_library_timing_actions,
            0
        );
    }

    #[test]
    fn analyze_timing_statistical_reports_sigma_penalty() {
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

        let report = FlowRunner::new()
            .analyze_timing_statistical(
                &mut netlist,
                &Pdk::minimal("test"),
                &FlowConfig::default(),
                &StatisticalTimingConfig::default(),
            )
            .expect("statistical timing should succeed");

        assert_eq!(report.analyzed_arcs, 1);
        assert_eq!(report.timing_arcs[0].route_mode, RouteMode::Jtl);
        assert!(report.timing_arcs[0].setup_sigma_ps > 0.0);
        assert!(
            report.timing_arcs[0].pessimistic_setup_slack_ps < report.timing_arcs[0].setup_slack_ps
        );
    }

    #[test]
    fn analyze_ac_bias_reports_jtl_carrier_capacity() {
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

        let report = FlowRunner::new()
            .analyze_ac_bias(&mut netlist, &Pdk::minimal("test"), &FlowConfig::default())
            .expect("ac bias analysis should succeed");

        assert_eq!(report.routed_nets, 1);
        assert_eq!(report.jtl_carrier_candidates, 1);
        assert!(report.estimated_static_power_savings_uw > 0.0);
        assert!(report.worst_setup_slack_ps.is_finite());
        assert!(report.worst_hold_slack_ps.is_finite());
        assert!(report.timing_guardband_score >= 0.0);
        assert!(report.feasibility_score > 0.0);
        assert!(report.optimization_score > 0.0);
    }

    #[test]
    fn optimize_ac_bias_reduces_ptl_coupling_risk_when_threshold_can_increase() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::CellInstance, "source");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");
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

        let mut config = FlowConfig::default();
        config.placement.fixed_nodes = vec![
            FixedNodePlacement {
                node: source,
                point: Point {
                    x_um: 0.0,
                    y_um: 0.0,
                },
            },
            FixedNodePlacement {
                node: sink,
                point: Point {
                    x_um: 120.0,
                    y_um: 0.0,
                },
            },
        ];
        config.routing.prefer_ptl_from_length_um = 60.0;

        let report = FlowRunner::new()
            .optimize_ac_bias(&netlist, &Pdk::minimal("test"), &config)
            .expect("ac bias optimization should succeed");

        assert!(report.threshold_candidates_evaluated >= 2);
        assert_eq!(report.baseline.ptl_coupling_risk_routes, 1);
        assert_eq!(report.optimized.ptl_coupling_risk_routes, 0);
        assert!(report.optimized.optimization_score >= report.baseline.optimization_score);
        assert!(report.optimized.timing_guardband_score >= 0.0);
        assert!(report.optimized.feasibility_score >= report.baseline.feasibility_score);
        assert!(report.optimization_applied);
        assert!(
            report.optimized_prefer_ptl_from_length_um > report.baseline_prefer_ptl_from_length_um
        );
    }

    #[test]
    fn optimize_ac_bias_can_reduce_detour_margin_when_blockage_allows_shorter_route() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::CellInstance, "source");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");
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

        let mut config = FlowConfig::default();
        config.placement.fixed_nodes = vec![
            FixedNodePlacement {
                node: source,
                point: Point {
                    x_um: 0.0,
                    y_um: 0.0,
                },
            },
            FixedNodePlacement {
                node: sink,
                point: Point {
                    x_um: 100.0,
                    y_um: 0.0,
                },
            },
        ];
        config.routing.prefer_ptl_from_length_um = 200.0;
        config.routing.detour_margin_um = 20.0;
        config.routing.blocked_regions = vec![RoutingBlockedRegion {
            min_x_um: 40.0,
            max_x_um: 60.0,
            min_y_um: -4.0,
            max_y_um: 4.0,
        }];

        let report = FlowRunner::new()
            .optimize_ac_bias(&netlist, &Pdk::minimal("test"), &config)
            .expect("ac bias optimization should succeed");

        assert!(report.detour_margin_candidates_evaluated >= 2);
        assert!(report.optimized_detour_margin_um < report.baseline_detour_margin_um);
        assert!(report.optimized.timing_guardband_score >= report.baseline.timing_guardband_score);
        assert!(report.optimization_applied);
    }

    #[test]
    fn statistical_timing_consumes_characterized_library_artifact_feedback() {
        let mut characterization_netlist = Netlist::new();
        let source = characterization_netlist.add_node(NodeKind::Port, "source");
        let gate = characterization_netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = characterization_netlist.add_node(NodeKind::Port, "sink");
        characterization_netlist
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
        characterization_netlist
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

        let mut runner = FlowRunner::new();
        let base_pdk = Pdk::minimal("test");
        let characterization = runner
            .characterize_compound_cell(
                &mut characterization_netlist,
                &base_pdk,
                &FlowConfig::default(),
                &SimulationConfig::default(),
                &CompoundCellCharacterizationConfig {
                    cell_name: "macro_buf".to_string(),
                },
            )
            .expect("characterization should succeed");

        let customized_artifact = characterization
            .generated_library_json
            .replace("\"area_um2\": 12.0", "\"area_um2\": 96.0")
            .replace("\"pipeline_stages\": 2", "\"pipeline_stages\": 4");
        let characterized_pdk = base_pdk
            .with_characterized_library_json(&customized_artifact)
            .expect("generated artifact should be consumable");

        let mut consumer_netlist = Netlist::new();
        let consumer_source = consumer_netlist.add_node(NodeKind::Port, "consumer_source");
        let macro_buf = consumer_netlist.add_node(NodeKind::MacroCell, "macro_buf");
        let consumer_sink = consumer_netlist.add_node(NodeKind::Dff, "consumer_sink");
        consumer_netlist
            .connect(
                PinRef {
                    node: consumer_source,
                    port: 0,
                },
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
            )
            .expect("consumer source to macro_buf");
        consumer_netlist
            .connect(
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
                PinRef {
                    node: consumer_sink,
                    port: 0,
                },
            )
            .expect("macro_buf to consumer sink");

        let stat_config = StatisticalTimingConfig {
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

        let baseline = FlowRunner::new()
            .analyze_timing_statistical(
                &mut consumer_netlist.clone(),
                &base_pdk,
                &FlowConfig::default(),
                &stat_config,
            )
            .expect("baseline statistical timing should succeed");
        let characterized = FlowRunner::new()
            .analyze_timing_statistical(
                &mut consumer_netlist,
                &characterized_pdk,
                &FlowConfig::default(),
                &stat_config,
            )
            .expect("characterized statistical timing should succeed");

        assert!(
            characterized.worst_pessimistic_setup_slack_ps
                < baseline.worst_pessimistic_setup_slack_ps
        );
    }

    #[test]
    fn characterize_compound_cell_reports_timing_library_ready_summary() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = netlist.add_node(NodeKind::Port, "sink");
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

        let report = FlowRunner::new()
            .characterize_compound_cell(
                &mut netlist,
                &Pdk::minimal("test"),
                &FlowConfig::default(),
                &SimulationConfig::default(),
                &CompoundCellCharacterizationConfig {
                    cell_name: "macro_buf".to_string(),
                },
            )
            .expect("characterization should succeed");

        assert_eq!(report.cell_name, "macro_buf");
        assert!(report.node_count >= 2);
        assert!(report.derived_intrinsic_delay_ps > 0.0);
        assert_eq!(report.generated_cell_kind, "macro");
        assert!(report.generated_pipeline_stages >= 1);
        assert!(report.generated_library_json.contains("macro_buf"));
        assert!(report.generated_deck_lines > 0);
        assert_eq!(report.simulation_backend, SimulationBackend::EventOnly);
    }

    #[test]
    fn characterized_library_artifact_feeds_timing_bias_and_constraint_reports() {
        let mut characterization_netlist = Netlist::new();
        let source = characterization_netlist.add_node(NodeKind::Port, "source");
        let gate = characterization_netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = characterization_netlist.add_node(NodeKind::Port, "sink");
        characterization_netlist
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
        characterization_netlist
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

        let mut runner = FlowRunner::new();
        let base_pdk = Pdk::minimal("test");
        let characterization = runner
            .characterize_compound_cell(
                &mut characterization_netlist,
                &base_pdk,
                &FlowConfig::default(),
                &SimulationConfig::default(),
                &CompoundCellCharacterizationConfig {
                    cell_name: "macro_buf".to_string(),
                },
            )
            .expect("characterization should succeed");
        let characterized_pdk = base_pdk
            .with_characterized_library_json(&characterization.generated_library_json)
            .expect("generated library artifact should be consumable");

        let mut consumer_netlist = Netlist::new();
        let consumer_source = consumer_netlist.add_node(NodeKind::Port, "consumer_source");
        let macro_buf = consumer_netlist.add_node(NodeKind::MacroCell, "macro_buf");
        let consumer_sink = consumer_netlist.add_node(NodeKind::Dff, "consumer_sink");
        consumer_netlist
            .connect(
                PinRef {
                    node: consumer_source,
                    port: 0,
                },
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
            )
            .expect("consumer source to macro_buf");
        consumer_netlist
            .connect(
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
                PinRef {
                    node: consumer_sink,
                    port: 0,
                },
            )
            .expect("macro_buf to consumer sink");

        let baseline_timing = FlowRunner::new()
            .analyze_timing(
                &mut consumer_netlist.clone(),
                &base_pdk,
                &FlowConfig::default(),
            )
            .expect("baseline timing should succeed");
        let characterized_timing = FlowRunner::new()
            .analyze_timing(
                &mut consumer_netlist.clone(),
                &characterized_pdk,
                &FlowConfig::default(),
            )
            .expect("characterized timing should succeed");
        let baseline_bias = FlowRunner::new()
            .analyze_ac_bias(
                &mut consumer_netlist.clone(),
                &base_pdk,
                &FlowConfig::default(),
            )
            .expect("baseline ac bias should succeed");
        let characterized_bias = FlowRunner::new()
            .analyze_ac_bias(
                &mut consumer_netlist.clone(),
                &characterized_pdk,
                &FlowConfig::default(),
            )
            .expect("characterized ac bias should succeed");
        let baseline_constraints = FlowRunner::new()
            .analyze_advanced_constraints(
                &mut consumer_netlist.clone(),
                &base_pdk,
                &FlowConfig::default(),
                &AdvancedConstraintConfig::default(),
            )
            .expect("baseline advanced constraints should succeed");
        let characterized_constraints = FlowRunner::new()
            .analyze_advanced_constraints(
                &mut consumer_netlist,
                &characterized_pdk,
                &FlowConfig::default(),
                &AdvancedConstraintConfig::default(),
            )
            .expect("characterized advanced constraints should succeed");

        assert_ne!(
            characterized_timing.critical_path_delay_ps,
            baseline_timing.critical_path_delay_ps
        );
        assert_ne!(
            characterized_bias.timing_guardband_score,
            baseline_bias.timing_guardband_score
        );
        assert_ne!(
            characterized_constraints.estimated_thermal_load_uw,
            baseline_constraints.estimated_thermal_load_uw
        );
        assert_ne!(
            characterized_constraints.estimated_mechanical_stress_score,
            baseline_constraints.estimated_mechanical_stress_score
        );
    }

    #[test]
    fn characterized_library_bundle_merges_multiple_entries() {
        let base = Pdk::minimal("test");
        let bundle = rflux_tech::CharacterizedCellLibraryBundle {
            entries: vec![
                rflux_tech::CharacterizedCellLibraryEntry {
                    cell: SfCell {
                        name: "macro_a".to_string(),
                        kind: SfCellKind::Macro,
                        area_um2: 40.0,
                        pipeline_stages: 2,
                    },
                    timing: CellTimingModel {
                        kind: SfCellKind::Macro,
                        intrinsic_delay_ps: 18.0,
                        setup_ps: 4.0,
                        hold_ps: 3.0,
                    },
                    metadata: None,
                },
                rflux_tech::CharacterizedCellLibraryEntry {
                    cell: SfCell {
                        name: "macro_b".to_string(),
                        kind: SfCellKind::Macro,
                        area_um2: 120.0,
                        pipeline_stages: 4,
                    },
                    timing: CellTimingModel {
                        kind: SfCellKind::Macro,
                        intrinsic_delay_ps: 24.0,
                        setup_ps: 5.0,
                        hold_ps: 4.0,
                    },
                    metadata: Some(rflux_tech::CharacterizationArtifactMetadata {
                        waveform_path: Some("macro_b.raw".to_string()),
                        simulated_delay_ps: Some(26.0),
                        sta_derived_delay_ps: Some(24.0),
                        delay_calibration_sigma_ps: 0.8,
                        delay_details: Vec::new(),
                        arc_delays: Vec::new(),
                    }),
                },
            ],
        };
        let serialized = serde_json::to_string(&bundle).expect("bundle should serialize");
        let merged = base
            .with_characterized_library_bundle_json(&serialized)
            .expect("bundle should merge");

        assert_eq!(
            merged
                .cell_timing_for_cell("macro_a", SfCellKind::Macro)
                .expect("macro_a timing")
                .intrinsic_delay_ps,
            18.0
        );
        assert_eq!(
            merged
                .characterization_metadata_for_cell("macro_b")
                .expect("macro_b metadata")
                .delay_calibration_sigma_ps,
            0.8
        );
    }

    #[test]
    fn characterized_library_feedback_scales_placement_and_routing_cost_models() {
        let base = PlacementConfig::default();
        let base_routing = RoutingConfig::default();
        let base_pdk = Pdk::minimal("test");
        let characterized_pdk =
            base_pdk.with_characterized_cell(rflux_tech::CharacterizedCellLibraryEntry {
                cell: SfCell {
                    name: "macro_buf".to_string(),
                    kind: SfCellKind::Macro,
                    area_um2: 120.0,
                    pipeline_stages: 4,
                },
                timing: CellTimingModel {
                    kind: SfCellKind::Macro,
                    intrinsic_delay_ps: 20.0,
                    setup_ps: 5.0,
                    hold_ps: 4.0,
                },
                metadata: None,
            });

        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let macro_buf = netlist.add_node(NodeKind::MacroCell, "macro_buf");
        let sink = netlist.add_node(NodeKind::Port, "sink");
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

        let baseline_placement =
            placement_config_with_library_feedback(&netlist, &base_pdk, &base, 1.0);
        let characterized_placement =
            placement_config_with_library_feedback(&netlist, &characterized_pdk, &base, 1.0);
        let baseline_routing =
            routing_config_with_library_feedback(&netlist, &base_pdk, &base_routing);
        let characterized_routing =
            routing_config_with_library_feedback(&netlist, &characterized_pdk, &base_routing);

        assert!(characterized_placement.macro_halo_x_um > baseline_placement.macro_halo_x_um);
        assert!(characterized_placement.macro_halo_y_um > baseline_placement.macro_halo_y_um);
        assert!(characterized_routing.detour_margin_um > baseline_routing.detour_margin_um);
        assert!(
            characterized_routing.prefer_ptl_from_length_um
                < baseline_routing.prefer_ptl_from_length_um
        );
    }

    #[test]
    fn characterization_uses_simulation_delay_detail_vector_for_intrinsic_delay() {
        let artifacts = CompiledArtifacts {
            synthesis: SynthesisReport {
                compile: CompileReport::default(),
                bool_opt: BoolOptReport {
                    gate_count_before: 0,
                    gate_count_after: 0,
                },
                tech_map: TechMapReport {
                    mapped_nodes: 2,
                    total_area_um2: 48.0,
                },
                path_balance: Default::default(),
                bool_opt_compatibility: Default::default(),
                node_count: 2,
                edge_count: 1,
            },
            placement: Placement {
                nodes: Vec::new(),
                width_um: 40.0,
                height_um: 24.0,
            },
            routing: RoutingReport {
                routes: Vec::new(),
                total_length_um: 40.0,
                total_detour_overhead_um: 0.0,
                detoured_routes: 0,
                jtl_routes: 1,
                ptl_routes: 0,
            },
            effective_routing_config: RoutingConfig::default(),
            clock: ClockSummary {
                clock_sinks: 1,
                clock_buffers: 0,
                phase_count: 2,
                assigned_phases: 2,
            },
            timing: TimingReport {
                arcs: Vec::new(),
                worst_setup_slack_ps: 10.0,
                worst_hold_slack_ps: 2.0,
                total_negative_setup_slack_ps: 0.0,
                total_negative_hold_slack_ps: 0.0,
                critical_path_delay_ps: 12.0,
                setup_violations: 0,
                hold_violations: 0,
                capture_window_violations: 0,
                analyzed_arcs: 1,
                false_path_arcs: 0,
            },
            initial_total_detour_overhead_um: 0.0,
            initial_hold_violations: 0,
            hold_fix_attempted: false,
            detour_feedback_applied: false,
            route_delay_optimization_attempted: false,
            route_delay_optimization_applied: false,
            hold_fix_applied: false,
        };
        let simulation = SimulationReport {
            backend: SimulationBackend::ExternalCompleted,
            requested_mode: "external_josim".to_string(),
            simulated_events: 2,
            generated_deck_lines: 4,
            generated_deck_path: None,
            waveform_path: None,
            waveform_format: None,
            external_summary_contract: None,
            diagnostic_code: None,
            reported_violations: 0,
            reported_worst_delay_ps: Some(20.0),
            delay_details: vec![
                SimulationDelayDetail {
                    name: "gate_to_sink".to_string(),
                    delay_ps: 9.0,
                    from_ref: None,
                    to_ref: None,
                },
                SimulationDelayDetail {
                    name: "source_to_gate".to_string(),
                    delay_ps: 11.0,
                    from_ref: None,
                    to_ref: None,
                },
            ],
            measurement_details: Vec::new(),
            measurement_warnings: Vec::new(),
            violation_details: Vec::new(),
            external_status_code: Some(0),
            external_result: Some("ok".to_string()),
        };

        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = netlist.add_node(NodeKind::Port, "sink");
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

        let report = compound_cell_characterization_from_artifacts(
            &netlist,
            &artifacts,
            simulation,
            &CompoundCellCharacterizationConfig {
                cell_name: "macro_buf".to_string(),
            },
        );
        let entry: rflux_tech::CharacterizedCellLibraryEntry =
            serde_json::from_str(&report.generated_library_json).expect("artifact should parse");
        let metadata = entry.metadata.expect("metadata should exist");
        assert_eq!(metadata.arc_delays.len(), 3);
        let source_arc = metadata
            .arc_delays
            .iter()
            .find(|arc| arc.driver_cell_name == "source")
            .expect("source arc");
        let gate_arc = metadata
            .arc_delays
            .iter()
            .find(|arc| arc.driver_cell_name == "gate")
            .expect("gate arc");
        let canonical_arc = metadata
            .arc_delays
            .iter()
            .find(|arc| arc.driver_cell_name == "macro_buf" && arc.sink_cell_name == "*")
            .expect("canonical arc");
        assert_eq!(source_arc.delay_ps, 11.0);
        assert_eq!(gate_arc.delay_ps, 9.0);
        assert_eq!(canonical_arc.delay_ps, 9.0);
    }

    #[test]
    fn generated_characterization_artifact_feeds_sta_via_canonical_output_arc() {
        let artifacts = CompiledArtifacts {
            synthesis: SynthesisReport {
                compile: CompileReport::default(),
                bool_opt: BoolOptReport {
                    gate_count_before: 0,
                    gate_count_after: 0,
                },
                tech_map: TechMapReport {
                    mapped_nodes: 2,
                    total_area_um2: 48.0,
                },
                path_balance: Default::default(),
                bool_opt_compatibility: Default::default(),
                node_count: 2,
                edge_count: 1,
            },
            placement: Placement {
                nodes: Vec::new(),
                width_um: 40.0,
                height_um: 24.0,
            },
            routing: RoutingReport {
                routes: Vec::new(),
                total_length_um: 40.0,
                total_detour_overhead_um: 0.0,
                detoured_routes: 0,
                jtl_routes: 1,
                ptl_routes: 0,
            },
            effective_routing_config: RoutingConfig::default(),
            clock: ClockSummary {
                clock_sinks: 1,
                clock_buffers: 0,
                phase_count: 2,
                assigned_phases: 2,
            },
            timing: TimingReport {
                arcs: Vec::new(),
                worst_setup_slack_ps: 10.0,
                worst_hold_slack_ps: 2.0,
                total_negative_setup_slack_ps: 0.0,
                total_negative_hold_slack_ps: 0.0,
                critical_path_delay_ps: 12.0,
                setup_violations: 0,
                hold_violations: 0,
                capture_window_violations: 0,
                analyzed_arcs: 1,
                false_path_arcs: 0,
            },
            initial_total_detour_overhead_um: 0.0,
            initial_hold_violations: 0,
            hold_fix_attempted: false,
            detour_feedback_applied: false,
            route_delay_optimization_attempted: false,
            route_delay_optimization_applied: false,
            hold_fix_applied: false,
        };
        let simulation = SimulationReport {
            backend: SimulationBackend::ExternalCompleted,
            requested_mode: "external_josim".to_string(),
            simulated_events: 2,
            generated_deck_lines: 4,
            generated_deck_path: None,
            waveform_path: None,
            waveform_format: None,
            external_summary_contract: None,
            diagnostic_code: None,
            reported_violations: 0,
            reported_worst_delay_ps: Some(20.0),
            delay_details: vec![
                SimulationDelayDetail {
                    name: "gate_to_sink".to_string(),
                    delay_ps: 9.0,
                    from_ref: None,
                    to_ref: None,
                },
                SimulationDelayDetail {
                    name: "source_to_gate".to_string(),
                    delay_ps: 11.0,
                    from_ref: None,
                    to_ref: None,
                },
            ],
            measurement_details: Vec::new(),
            measurement_warnings: Vec::new(),
            violation_details: Vec::new(),
            external_status_code: Some(0),
            external_result: Some("ok".to_string()),
        };

        let mut characterization_netlist = Netlist::new();
        let source = characterization_netlist.add_node(NodeKind::Port, "source");
        let gate = characterization_netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = characterization_netlist.add_node(NodeKind::Port, "sink");
        characterization_netlist
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
        characterization_netlist
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

        let characterization = compound_cell_characterization_from_artifacts(
            &characterization_netlist,
            &artifacts,
            simulation,
            &CompoundCellCharacterizationConfig {
                cell_name: "macro_buf".to_string(),
            },
        );
        let characterized_pdk = Pdk::minimal("test")
            .with_characterized_library_json(&characterization.generated_library_json)
            .expect("characterized artifact should merge");

        let mut consumer = Netlist::new();
        let consumer_source = consumer.add_node(NodeKind::Port, "consumer_source");
        let consumer_macro = consumer.add_node(NodeKind::MacroCell, "macro_buf");
        let consumer_sink = consumer.add_node(NodeKind::Dff, "consumer_sink");
        consumer
            .connect(
                PinRef {
                    node: consumer_source,
                    port: 0,
                },
                PinRef {
                    node: consumer_macro,
                    port: 0,
                },
            )
            .expect("consumer source to macro_buf");
        consumer
            .connect(
                PinRef {
                    node: consumer_macro,
                    port: 0,
                },
                PinRef {
                    node: consumer_sink,
                    port: 0,
                },
            )
            .expect("macro_buf to consumer sink");

        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef {
                        node: consumer_source,
                        port: 0,
                    },
                    to: PinRef {
                        node: consumer_macro,
                        port: 0,
                    },
                    mode: RouteMode::Jtl,
                    segments: Vec::new(),
                    direct_length_um: 40.0,
                    length_um: 40.0,
                },
                NetRoute {
                    from: PinRef {
                        node: consumer_macro,
                        port: 0,
                    },
                    to: PinRef {
                        node: consumer_sink,
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

        let timing = StaticTimingAnalyzer::new()
            .analyze(
                &consumer,
                &routing,
                &characterized_pdk,
                &TimingConfig::default(),
            )
            .expect("timing should succeed");
        let macro_arc = timing
            .arcs
            .iter()
            .find(|arc| arc.from.node == consumer_macro)
            .expect("macro arc should exist");
        assert_eq!(macro_arc.cell_delay_ps, 9.0);
    }

    #[test]
    fn characterized_arc_delay_table_feeds_sta_arc_assembly() {
        let base_pdk = Pdk::minimal("test");
        let characterized_pdk =
            base_pdk.with_characterized_cell(rflux_tech::CharacterizedCellLibraryEntry {
                cell: SfCell {
                    name: "macro_buf".to_string(),
                    kind: SfCellKind::Macro,
                    area_um2: 60.0,
                    pipeline_stages: 2,
                },
                timing: CellTimingModel {
                    kind: SfCellKind::Macro,
                    intrinsic_delay_ps: 14.0,
                    setup_ps: 8.0,
                    hold_ps: 5.0,
                },
                metadata: Some(rflux_tech::CharacterizationArtifactMetadata {
                    arc_delays: vec![rflux_tech::CharacterizationArcDelay {
                        name: "macro_to_sink".to_string(),
                        driver_cell_name: "macro_buf".to_string(),
                        from_port: 0,
                        sink_cell_name: "sink".to_string(),
                        to_port: 0,
                        delay_ps: 41.0,
                    }],
                    ..rflux_tech::CharacterizationArtifactMetadata::default()
                }),
            });

        let mut consumer = Netlist::new();
        let mut runner = FlowRunner::new();
        let consumer_source = consumer.add_node(NodeKind::Port, "consumer_source");
        let consumer_macro = consumer.add_node(NodeKind::MacroCell, "macro_buf");
        let consumer_sink = consumer.add_node(NodeKind::Dff, "sink");
        consumer
            .connect(
                PinRef {
                    node: consumer_source,
                    port: 0,
                },
                PinRef {
                    node: consumer_macro,
                    port: 0,
                },
            )
            .expect("consumer source to macro_buf");
        consumer
            .connect(
                PinRef {
                    node: consumer_macro,
                    port: 0,
                },
                PinRef {
                    node: consumer_sink,
                    port: 0,
                },
            )
            .expect("macro_buf to consumer sink");

        let artifacts = runner
            .compile_artifacts(&mut consumer, &characterized_pdk, &FlowConfig::default())
            .expect("artifacts should compile");
        let timing = StaticTimingAnalyzer::new()
            .analyze(
                &consumer,
                &artifacts.routing,
                &characterized_pdk,
                &FlowConfig::default().timing,
            )
            .expect("timing should succeed");
        let macro_arc = timing
            .arcs
            .iter()
            .find(|arc| arc.from.node == consumer_macro)
            .expect("macro arc should exist");
        assert_eq!(macro_arc.cell_delay_ps, 41.0);
    }

    #[test]
    fn optimize_design_with_characterized_library_includes_statistical_feedback() {
        let mut characterization_netlist = Netlist::new();
        let source = characterization_netlist.add_node(NodeKind::Port, "source");
        let gate = characterization_netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = characterization_netlist.add_node(NodeKind::Port, "sink");
        characterization_netlist
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
        characterization_netlist
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

        let mut runner = FlowRunner::new();
        let base_pdk = Pdk::minimal("test");
        let characterization = runner
            .characterize_compound_cell(
                &mut characterization_netlist,
                &base_pdk,
                &FlowConfig::default(),
                &SimulationConfig::default(),
                &CompoundCellCharacterizationConfig {
                    cell_name: "macro_buf".to_string(),
                },
            )
            .expect("characterization should succeed");

        let mut consumer = Netlist::new();
        let consumer_source = consumer.add_node(NodeKind::Port, "consumer_source");
        let macro_buf = consumer.add_node(NodeKind::MacroCell, "macro_buf");
        let consumer_sink = consumer.add_node(NodeKind::Dff, "consumer_sink");
        consumer
            .connect(
                PinRef {
                    node: consumer_source,
                    port: 0,
                },
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
            )
            .expect("consumer source to macro_buf");
        consumer
            .connect(
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
                PinRef {
                    node: consumer_sink,
                    port: 0,
                },
            )
            .expect("macro_buf to consumer sink");

        let report = runner
            .optimize_design_with_characterized_library(
                &consumer,
                &base_pdk,
                &FlowConfig::default(),
                &AdvancedConstraintConfig::default(),
                &StatisticalTimingConfig::default(),
                &[characterization.generated_library_json],
            )
            .expect("design optimization should succeed");

        assert_eq!(report.characterized_cells_merged, 1);
        assert!(report.design_optimization_score > 0.0);
        assert!(report.baseline_statistical.analyzed_arcs > 0);
        assert!(report.placement_candidates_evaluated >= 1);
        assert!(report.statistical_candidates_evaluated >= 2);
    }

    #[test]
    fn optimize_ac_bias_with_characterized_library_scores_constraints() {
        let mut characterization_netlist = Netlist::new();
        let source = characterization_netlist.add_node(NodeKind::Port, "source");
        let gate = characterization_netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = characterization_netlist.add_node(NodeKind::Port, "sink");
        characterization_netlist
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
        characterization_netlist
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

        let mut runner = FlowRunner::new();
        let base_pdk = Pdk::minimal("test");
        let characterization = runner
            .characterize_compound_cell(
                &mut characterization_netlist,
                &base_pdk,
                &FlowConfig::default(),
                &SimulationConfig::default(),
                &CompoundCellCharacterizationConfig {
                    cell_name: "macro_buf".to_string(),
                },
            )
            .expect("characterization should succeed");

        let mut consumer_netlist = Netlist::new();
        let consumer_source = consumer_netlist.add_node(NodeKind::Port, "consumer_source");
        let macro_buf = consumer_netlist.add_node(NodeKind::MacroCell, "macro_buf");
        let consumer_sink = consumer_netlist.add_node(NodeKind::Dff, "consumer_sink");
        consumer_netlist
            .connect(
                PinRef {
                    node: consumer_source,
                    port: 0,
                },
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
            )
            .expect("consumer source to macro_buf");
        consumer_netlist
            .connect(
                PinRef {
                    node: macro_buf,
                    port: 0,
                },
                PinRef {
                    node: consumer_sink,
                    port: 0,
                },
            )
            .expect("macro_buf to consumer sink");

        let report = runner
            .optimize_ac_bias_with_characterized_library(
                &consumer_netlist,
                &base_pdk,
                &FlowConfig::default(),
                &AdvancedConstraintConfig::default(),
                &[characterization.generated_library_json],
            )
            .expect("library-aware optimization should succeed");

        assert_eq!(report.characterized_cells_merged, 1);
        assert!(report.library_optimization_score > 0.0);
        assert!(report.ac_bias.optimization_applied || report.library_optimization_score > 0.0);
    }

    #[test]
    fn characterize_compound_cell_emits_waveform_aware_library_metadata() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");
        let sink = netlist.add_node(NodeKind::Port, "sink");
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

        let report = FlowRunner::new()
            .characterize_compound_cell(
                &mut netlist,
                &Pdk::minimal("test"),
                &FlowConfig::default(),
                &SimulationConfig::default(),
                &CompoundCellCharacterizationConfig {
                    cell_name: "macro_buf".to_string(),
                },
            )
            .expect("characterization should succeed");

        assert!(report.generated_library_json.contains("metadata"));
        assert!(report
            .generated_library_json
            .contains("sta_derived_delay_ps"));
        let entry: rflux_tech::CharacterizedCellLibraryEntry =
            serde_json::from_str(&report.generated_library_json).expect("artifact should parse");
        let metadata = entry.metadata.expect("metadata should be present");
        assert_eq!(
            metadata.sta_derived_delay_ps,
            Some(report.derived_intrinsic_delay_ps)
        );
    }

    #[test]
    fn analyze_advanced_constraints_flags_budget_violations() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::CellInstance, "source");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");
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

        let mut config = FlowConfig::default();
        config.placement.fixed_nodes = vec![
            FixedNodePlacement {
                node: source,
                point: Point {
                    x_um: 0.0,
                    y_um: 0.0,
                },
            },
            FixedNodePlacement {
                node: sink,
                point: Point {
                    x_um: 120.0,
                    y_um: 0.0,
                },
            },
        ];
        config.routing.prefer_ptl_from_length_um = 60.0;
        config.routing.blocked_regions = vec![RoutingBlockedRegion {
            min_x_um: 40.0,
            max_x_um: 60.0,
            min_y_um: -4.0,
            max_y_um: 4.0,
        }];

        let report = FlowRunner::new()
            .analyze_advanced_constraints(
                &mut netlist,
                &Pdk::minimal("test"),
                &config,
                &AdvancedConstraintConfig {
                    max_estimated_thermal_load_uw: 0.05,
                    max_estimated_mechanical_stress_score: 0.05,
                    max_jtl_density_per_100um: 0.05,
                    max_detour_overhead_ratio: 0.01,
                    max_ptl_coupling_ratio: 0.01,
                },
            )
            .expect("advanced constraint analysis should succeed");

        assert!(report.violation_count >= 3);
        assert!(report.manufacturing_hotspots > 0);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.category == "thermal"));
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.category == "manufacturing"));
    }

    #[test]
    fn ac_bias_report_prefers_timing_safer_candidate_when_other_metrics_match() {
        let current = AcBiasReport {
            routed_nets: 4,
            jtl_carrier_candidates: 2,
            ptl_coupling_risk_routes: 1,
            clock_sink_count: 2,
            estimated_static_power_savings_uw: 0.7,
            estimated_area_overhead_ratio: 1.1,
            estimated_frequency_derate_ratio: 0.95,
            worst_setup_slack_ps: 4.0,
            worst_hold_slack_ps: 1.0,
            timing_guardband_score: 0.2,
            feasibility_score: 0.6,
            optimization_score: 0.55,
        };
        let candidate = AcBiasReport {
            worst_setup_slack_ps: 18.0,
            worst_hold_slack_ps: 6.0,
            timing_guardband_score: 0.7,
            optimization_score: 0.7,
            ..current
        };

        assert!(ac_bias_report_better_than(&candidate, &current));
    }

    #[test]
    fn honors_fixed_nodes_through_flow_config() {
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "a");
        let gate = netlist.add_node(NodeKind::MacroCell, "gate");

        let mut config = FlowConfig::default();
        config.placement.fixed_nodes = vec![FixedNodePlacement {
            node: gate,
            point: Point {
                x_um: 120.0,
                y_um: 48.0,
            },
        }];

        let mut runner = FlowRunner::new();
        let report = runner
            .compile_layout(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("flow should succeed");

        assert_eq!(report.placement.width_um, 160.0);
        assert_eq!(report.placement.height_um, 72.0);
        assert_eq!(report.clock.clock_sinks, 1);
    }

    #[test]
    fn applies_detour_feedback_when_replacement_reduces_overhead() {
        let mut netlist = Netlist::new();
        let input = netlist.add_node(NodeKind::Port, "input");
        let output = netlist.add_node(NodeKind::Port, "output");
        netlist
            .connect(
                PinRef {
                    node: input,
                    port: 0,
                },
                PinRef {
                    node: output,
                    port: 0,
                },
            )
            .expect("input to output");

        let mut config = FlowConfig::default();
        config.routing.blocked_regions = vec![BlockedRegion {
            min_x_um: 10.0,
            max_x_um: 30.0,
            min_y_um: -4.0,
            max_y_um: 4.0,
        }];

        let mut runner = FlowRunner::new();
        let report = runner
            .compile_layout(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("flow should succeed");

        assert!(report.initial_total_detour_overhead_um > 0.0);
        assert!(report.detour_feedback_applied);
        assert_eq!(report.routing.total_detour_overhead_um, 0.0);
        assert_eq!(report.routing.detoured_routes, 0);
        assert!(report.timing.worst_setup_slack_ps.is_finite());
    }

    #[test]
    fn timing_closure_loop_recommends_route_delay_threshold_from_top_actions() {
        let mut netlist = Netlist::new();
        let sources = (0..4)
            .map(|index| netlist.add_node(NodeKind::CellInstance, format!("source_{index}")))
            .collect::<Vec<_>>();
        let sinks = (0..4)
            .map(|index| netlist.add_node(NodeKind::Dff, format!("sink_{index}")))
            .collect::<Vec<_>>();
        for (index, (source, sink)) in sources.iter().zip(sinks.iter()).enumerate() {
            netlist
                .connect(
                    PinRef {
                        node: *source,
                        port: index as u16,
                    },
                    PinRef {
                        node: *sink,
                        port: 0,
                    },
                )
                .expect("source to sink");
        }

        let mut config = FlowConfig::default();
        config.routing.prefer_ptl_from_length_um = 60.0;
        config.placement.fixed_nodes = vec![
            FixedNodePlacement {
                node: sources[0],
                point: Point {
                    x_um: 0.0,
                    y_um: 0.0,
                },
            },
            FixedNodePlacement {
                node: sinks[0],
                point: Point {
                    x_um: 90.0,
                    y_um: 0.0,
                },
            },
            FixedNodePlacement {
                node: sources[1],
                point: Point {
                    x_um: 0.0,
                    y_um: 24.0,
                },
            },
            FixedNodePlacement {
                node: sinks[1],
                point: Point {
                    x_um: 120.0,
                    y_um: 24.0,
                },
            },
            FixedNodePlacement {
                node: sources[2],
                point: Point {
                    x_um: 0.0,
                    y_um: 48.0,
                },
            },
            FixedNodePlacement {
                node: sinks[2],
                point: Point {
                    x_um: 110.0,
                    y_um: 48.0,
                },
            },
            FixedNodePlacement {
                node: sources[3],
                point: Point {
                    x_um: 0.0,
                    y_um: 72.0,
                },
            },
            FixedNodePlacement {
                node: sinks[3],
                point: Point {
                    x_um: 100.0,
                    y_um: 72.0,
                },
            },
        ];
        config.timing.node_constraints = vec![
            rflux_timing::NodeTimingConstraint {
                node: sinks[0],
                input_arrival_ps: None,
                required_ps: Some(10.0),
                clock_domain: None,
            },
            rflux_timing::NodeTimingConstraint {
                node: sinks[1],
                input_arrival_ps: None,
                required_ps: Some(24.0),
                clock_domain: None,
            },
            rflux_timing::NodeTimingConstraint {
                node: sinks[2],
                input_arrival_ps: None,
                required_ps: Some(22.0),
                clock_domain: None,
            },
            rflux_timing::NodeTimingConstraint {
                node: sinks[3],
                input_arrival_ps: None,
                required_ps: Some(20.0),
                clock_domain: None,
            },
        ];

        let report = FlowRunner::new()
            .compile_layout(&mut netlist, &Pdk::minimal("test"), &config)
            .expect("flow should succeed");

        assert_eq!(report.timing.setup_violations, 4);
        assert_eq!(report.timing_closure.reduce_route_delay_actions, 3);
        let reduce_route_delay_actions = report
            .timing_closure
            .actions
            .iter()
            .filter(|action| {
                action.remediation_kind == TimingClosureRemediationKind::ReduceRouteDelay
            })
            .collect::<Vec<_>>();
        let representative_action = reduce_route_delay_actions[0];
        let longest_top_action_length_um = reduce_route_delay_actions
            .iter()
            .map(|action| action.route_length_um)
            .reduce(f64::max)
            .expect("top reduce-route actions");
        assert_eq!(
            report.timing_closure_loop.estimated_route_length_um,
            Some(representative_action.route_length_um)
        );
        assert_eq!(
            report
                .timing_closure_loop
                .recommended_prefer_ptl_from_length_um,
            Some(longest_top_action_length_um + 1.0)
        );
        assert!(
            report
                .timing_closure_loop
                .reduce_route_delay_candidate_attempted
        );
        assert_eq!(
            report.timing_closure_loop.candidate_route_length_um,
            Some(representative_action.route_length_um)
        );
    }

    #[test]
    fn applies_route_delay_optimization_when_candidate_improves_setup() {
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

        let mut pdk = Pdk::minimal("route-delay-optimization");
        pdk.interconnect_timing = vec![
            InterconnectTimingModel {
                kind: InterconnectKind::Jtl,
                points: vec![
                    TimingPoint {
                        length_um: 0.0,
                        delay_ps: 4.0,
                    },
                    TimingPoint {
                        length_um: 120.0,
                        delay_ps: 10.0,
                    },
                ],
            },
            InterconnectTimingModel {
                kind: InterconnectKind::Ptl,
                points: vec![
                    TimingPoint {
                        length_um: 0.0,
                        delay_ps: 4.0,
                    },
                    TimingPoint {
                        length_um: 120.0,
                        delay_ps: 40.0,
                    },
                ],
            },
        ];

        let mut config = FlowConfig::default();
        config.routing.prefer_ptl_from_length_um = 60.0;
        config.placement.fixed_nodes = vec![
            FixedNodePlacement {
                node: source,
                point: Point {
                    x_um: 0.0,
                    y_um: 0.0,
                },
            },
            FixedNodePlacement {
                node: sink,
                point: Point {
                    x_um: 100.0,
                    y_um: 0.0,
                },
            },
        ];
        config.timing.node_constraints = vec![rflux_timing::NodeTimingConstraint {
            node: sink,
            input_arrival_ps: None,
            required_ps: Some(30.0),
            clock_domain: None,
        }];

        let report = FlowRunner::new()
            .compile_layout(&mut netlist, &pdk, &config)
            .expect("flow should succeed");

        assert!(
            report
                .timing_closure_loop
                .route_delay_optimization_attempted
        );
        assert!(report.timing_closure_loop.route_delay_optimization_applied);
        assert_eq!(report.routing.ptl_routes, 0);
        assert_eq!(report.routing.jtl_routes, 1);
        assert_eq!(report.routing.effective_prefer_ptl_from_length_um, 121.0);
        assert_eq!(report.routing.effective_detour_margin_um, 0.0);
        assert!(report.timing.setup_violations < 1);
        assert_eq!(report.timing_closure.status, "closed");
        assert_eq!(report.timing_closure_loop.status, "closed");
    }

    #[test]
    fn applies_hold_fix_reroute_when_jtl_path_is_too_short() {
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

        let mut config = FlowConfig::default();
        config.timing.clock_period_ps = 120.0;
        config.min_hold_jtl_length_um = 60.0;
        let mut pdk = Pdk::minimal("test");
        if let Some(model) = pdk
            .cell_timing
            .iter_mut()
            .find(|model| model.kind == rflux_tech::SfCellKind::Dff)
        {
            model.hold_ps = 20.0;
        }

        let mut runner = FlowRunner::new();
        let report = runner
            .compile_layout(&mut netlist, &pdk, &config)
            .expect("flow should succeed");

        assert!(report.timing.initial_hold_violations > 0);
        assert_eq!(report.timing.final_hold_violations, 0);
        assert!(report.timing.hold_fix_applied);
        assert!(report.timing_closure.closed);
        assert_eq!(report.timing_closure.status, "closed");
        assert!(report.timing_closure.hold_closed);
        assert_eq!(report.timing_closure.hold_violations, 0);
        assert_eq!(report.timing_closure.action_count, 0);
        assert_eq!(report.timing_closure.add_hold_padding_actions, 0);
        assert!(report.timing_closure_loop.hold_fix_attempted);
        assert!(report.timing_closure_loop.hold_fix_applied);
        assert_eq!(report.timing_closure_loop.initial_hold_violations, 1);
        assert_eq!(report.timing_closure_loop.final_hold_violations, 0);
        assert_eq!(report.timing_closure_loop.status, "closed");
    }

    #[test]
    fn analyzes_timing_without_full_layout_report() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("a to sink");

        let report = FlowRunner::new()
            .analyze_timing(&mut netlist, &Pdk::minimal("test"), &FlowConfig::default())
            .expect("timing analysis should succeed");

        assert_eq!(report.analyzed_arcs, 1);
        assert!(report.critical_path_delay_ps > 0.0);
    }

    #[test]
    fn analyzes_timing_across_pdk_corners() {
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

        let mut pdk = Pdk::minimal("corner-signoff").with_active_timing_corner("slow");
        pdk.timing_corners.push(PdkTimingCorner {
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

        let report = FlowRunner::new()
            .analyze_timing_corners(&mut netlist, &pdk, &FlowConfig::default())
            .expect("multi-corner timing analysis should succeed");

        assert_eq!(report.active_timing_corner.as_deref(), Some("slow"));
        assert_eq!(report.corner_count, 2);
        assert_eq!(report.corners[0].corner_name, "default");
        assert!(report.corners[0].is_default_corner);
        assert!(!report.corners[0].is_active_corner);
        assert_eq!(report.corners[1].corner_name, "slow");
        assert!(!report.corners[1].is_default_corner);
        assert!(report.corners[1].is_active_corner);
        assert_eq!(report.worst_setup_corner, "slow");
        assert_eq!(report.worst_critical_path_corner, "slow");
        assert!(
            report.corners[1].critical_path_delay_ps > report.corners[0].critical_path_delay_ps
        );
        assert!(report.corners[1].worst_setup_slack_ps < report.corners[0].worst_setup_slack_ps);
    }

    #[test]
    fn verifies_ptl_macro_boundary_and_event_simulation() {
        let mut netlist = Netlist::new();
        let macro_node = netlist.add_node(NodeKind::MacroCell, "macro");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");
        netlist
            .connect(
                PinRef {
                    node: macro_node,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("macro to sink");

        let mut config = FlowConfig::default();
        config.placement.fixed_nodes = vec![
            FixedNodePlacement {
                node: macro_node,
                point: Point {
                    x_um: 0.0,
                    y_um: 0.0,
                },
            },
            FixedNodePlacement {
                node: sink,
                point: Point {
                    x_um: 120.0,
                    y_um: 0.0,
                },
            },
        ];
        config.routing.prefer_ptl_from_length_um = 60.0;

        let report = FlowRunner::new()
            .verify_layout(
                &mut netlist,
                &Pdk::minimal("test"),
                &config,
                &SimulationConfig::default(),
            )
            .expect("verification should succeed");

        assert_eq!(report.checked_ptl_routes, 1);
        assert_eq!(report.ptl_macro_boundary_violations, 1);
        assert_eq!(report.simulation.backend, SimulationBackend::EventOnly);
        assert!(report.simulation.simulated_events > 0);
        assert!(report.simulation.generated_deck_lines > 0);
        assert_eq!(report.simulation.reported_violations, 0);
        assert_eq!(report.simulation.reported_worst_delay_ps, None);
        assert!(report.simulation.delay_details.is_empty());
        assert!(report.simulation.violation_details.is_empty());
    }

    #[test]
    fn generates_simulation_deck_for_routes() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");
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

        let artifacts = FlowRunner::new()
            .compile_artifacts(&mut netlist, &Pdk::minimal("test"), &FlowConfig::default())
            .expect("artifacts should build");
        let deck = generate_simulation_deck(&netlist, &artifacts);

        assert!(deck.contains(".title rflux verification"));
        assert!(deck.contains(".tran {tstep} {tstop}"));
        assert!(deck.contains(".measure events"));
        assert!(deck.contains("VDRV_0 n0_0 0 PULSE(0,vdd,0,0.5p,0.5p,5p,10p)"));
        assert!(deck.contains("CLOAD_n0_0 n0_0 0 1f"));
        assert!(deck.contains(".end"));
    }

    #[test]
    fn verify_layout_internal_transient_uses_linear_deck_subset() {
        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "a");
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

        let report = FlowRunner::new()
            .verify_layout(
                &mut netlist,
                &Pdk::minimal("test"),
                &FlowConfig::default(),
                &SimulationConfig {
                    mode: SimulationMode::InternalTransient,
                    external_command: None,
                },
            )
            .expect("verification should succeed");

        assert_eq!(
            report.simulation.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.simulation.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulation.simulated_events > 0);
        assert!(report.simulation.waveform_path.is_some());
        assert!(
            report
                .simulation
                .reported_worst_delay_ps
                .unwrap_or_default()
                > 0.0
        );
    }

    #[test]
    fn parses_external_simulator_output() {
        let (
            events,
            result,
            waveform_path,
            external_summary_contract,
            reported_violations,
            reported_worst_delay_ps,
            delay_details,
            measurement_details,
            violation_details,
        ) = parse_simulator_output(
            "RFLOW_EVENTS=42\nRFLOW_RESULT=PASS\nRFLOW_WAVEFORM=C:/tmp/out.raw\nRFLOW_VIOLATIONS=3\nRFLOW_WORST_DELAY_PS=18.5\nRFLOW_DELAY_DETAIL=critical_path,18.5\nRFLOW_MEASUREMENT_DETAIL=out_rms,rms,0.001\nRFLOW_VIOLATION_DETAIL=hold,sink_dff\n",
        );

        assert_eq!(events, Some(42));
        assert_eq!(result.as_deref(), Some("pass"));
        assert_eq!(waveform_path.as_deref(), Some("C:/tmp/out.raw"));
        assert_eq!(external_summary_contract.as_deref(), Some("legacy"));
        assert_eq!(reported_violations, Some(3));
        assert_eq!(reported_worst_delay_ps, Some(18.5));
        assert_eq!(delay_details.len(), 1);
        assert_eq!(delay_details[0].name, "critical_path");
        assert_eq!(delay_details[0].delay_ps, 18.5);
        assert_eq!(delay_details[0].from_ref, None);
        assert_eq!(delay_details[0].to_ref, None);
        assert_eq!(measurement_details.len(), 1);
        assert_eq!(measurement_details[0].name, "out_rms");
        assert_eq!(measurement_details[0].kind, "rms");
        assert_eq!(measurement_details[0].measured_value, 0.001);
        assert_eq!(violation_details.len(), 1);
        assert_eq!(violation_details[0].kind, "hold");
        assert_eq!(violation_details[0].detail, "sink_dff");
        assert_eq!(violation_details[0].at_ref, None);
    }

    #[test]
    fn parses_alias_simulator_summary_output() {
        let (
            events,
            result,
            waveform_path,
            external_summary_contract,
            reported_violations,
            reported_worst_delay_ps,
            delay_details,
            measurement_details,
            violation_details,
        ) = parse_simulator_output(
            "Status: PASS\nMeasured_Events: 17\nRaw_File: C:/tmp/josim.raw\nViolation_Count: 2\nMeasured_Delay_Ps: 11.25\nDelay_Detail: name=ptl_link,delay_ps=11.25,from=n0:0,to=n1:0\nViolation_Detail: kind=setup,detail=crossing_1_2,at=n1:0\n",
        );

        assert_eq!(events, Some(17));
        assert_eq!(result.as_deref(), Some("pass"));
        assert_eq!(waveform_path.as_deref(), Some("C:/tmp/josim.raw"));
        assert_eq!(external_summary_contract.as_deref(), Some("legacy"));
        assert_eq!(reported_violations, Some(2));
        assert_eq!(reported_worst_delay_ps, Some(11.25));
        assert_eq!(delay_details.len(), 1);
        assert_eq!(delay_details[0].name, "ptl_link");
        assert_eq!(delay_details[0].delay_ps, 11.25);
        assert_eq!(
            delay_details[0]
                .from_ref
                .as_ref()
                .map(|endpoint| endpoint.raw.as_str()),
            Some("n0:0")
        );
        assert_eq!(
            delay_details[0]
                .from_ref
                .as_ref()
                .map(|endpoint| endpoint.node.as_str()),
            Some("n0")
        );
        assert_eq!(
            delay_details[0]
                .from_ref
                .as_ref()
                .and_then(|endpoint| endpoint.port),
            Some(0)
        );
        assert_eq!(
            delay_details[0]
                .to_ref
                .as_ref()
                .map(|endpoint| endpoint.raw.as_str()),
            Some("n1:0")
        );
        assert_eq!(
            delay_details[0]
                .to_ref
                .as_ref()
                .map(|endpoint| endpoint.node.as_str()),
            Some("n1")
        );
        assert_eq!(
            delay_details[0]
                .to_ref
                .as_ref()
                .and_then(|endpoint| endpoint.port),
            Some(0)
        );
        assert!(measurement_details.is_empty());
        assert_eq!(violation_details.len(), 1);
        assert_eq!(violation_details[0].kind, "setup");
        assert_eq!(violation_details[0].detail, "crossing_1_2");
        assert_eq!(
            violation_details[0]
                .at_ref
                .as_ref()
                .map(|endpoint| endpoint.raw.as_str()),
            Some("n1:0")
        );
        assert_eq!(
            violation_details[0]
                .at_ref
                .as_ref()
                .map(|endpoint| endpoint.node.as_str()),
            Some("n1")
        );
        assert_eq!(
            violation_details[0]
                .at_ref
                .as_ref()
                .and_then(|endpoint| endpoint.port),
            Some(0)
        );
    }
}
