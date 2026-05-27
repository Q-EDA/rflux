from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum


class BalanceStrategy(str, Enum):
    NONE = "none"
    EXPLICIT = "explicit"
    ALL_CONNECTED_SOURCES = "all_connected_sources"
    BY_SINK_LEVEL = "by_sink_level"


@dataclass(frozen=True)
class PinRef:
    node: int
    port: int


@dataclass(frozen=True)
class ConnectionSpec:
    from_pin: PinRef
    to_pin: PinRef


@dataclass
class CompilePlan:
    connections: list[ConnectionSpec] = field(default_factory=list)
    balance_strategy: BalanceStrategy = BalanceStrategy.NONE
    balancing_sources: list[PinRef] = field(default_factory=list)


@dataclass(frozen=True)
class FixedNodePlacement:
    node: int
    x_um: float
    y_um: float


@dataclass(frozen=True)
class BlockedRegion:
    min_x_um: float
    max_x_um: float
    min_y_um: float
    max_y_um: float


@dataclass(frozen=True)
class NodeTimingConstraint:
    node: int
    input_arrival_ps: float | None = None
    required_ps: float | None = None
    clock_domain: int | None = None


@dataclass(frozen=True)
class ClockDomainConstraint:
    id: int
    period_ps: float


@dataclass(frozen=True)
class PinTimingConstraint:
    pin: PinRef
    input_arrival_ps: float | None = None
    required_ps: float | None = None
    clock_domain: int | None = None


@dataclass(frozen=True)
class CrossingConstraint:
    from_domain: int
    to_domain: int
    kind: str
    value_ps: float | None = None
    cycles: int | None = None


@dataclass(frozen=True)
class CompileReport:
    connections_applied: int
    splitters_inserted: int
    balancing_dffs_inserted: int


@dataclass(frozen=True)
class SynthesisReport:
    connections_applied: int
    splitters_inserted: int
    balancing_dffs_inserted: int
    bool_gate_count_before: int
    bool_gate_count_after: int
    mapped_nodes: int
    total_area_um2: float
    path_balance_insertions: int
    bool_opt_compatible: bool
    node_count: int
    edge_count: int


@dataclass(frozen=True)
class LayoutReport:
    connections_applied: int
    splitters_inserted: int
    balancing_dffs_inserted: int
    mapped_nodes: int
    total_area_um2: float
    bool_opt_compatible: bool
    placed_nodes: int
    placement_width_um: float
    placement_height_um: float
    clock_sinks: int
    clock_buffers: int
    clock_phase_count: int
    initial_hold_violations: int
    final_hold_violations: int
    hold_fix_applied: bool
    worst_setup_slack_ps: float
    worst_hold_slack_ps: float
    critical_path_delay_ps: float
    analyzed_timing_arcs: int
    false_path_arcs: int
    setup_violations: int
    capture_window_violations: int
    timing_closure: TimingClosureSummary
    timing_closure_loop: TimingClosureLoopReport
    routed_nets: int
    total_route_length_um: float
    initial_total_detour_overhead_um: float
    total_detour_overhead_um: float
    detoured_routes: int
    detour_feedback_applied: bool
    effective_prefer_ptl_from_length_um: float
    effective_detour_margin_um: float
    flow_config_patch: dict[str, object]
    jtl_routes: int
    ptl_routes: int
    node_count: int
    edge_count: int


@dataclass(frozen=True)
class TimingClosureLoopReport:
    detour_feedback_attempted: bool
    detour_feedback_applied: bool
    initial_total_detour_overhead_um: float
    final_total_detour_overhead_um: float
    route_delay_optimization_attempted: bool
    route_delay_optimization_applied: bool
    reduce_route_delay_candidate_available: bool
    recommended_prefer_ptl_from_length_um: float | None
    recommended_detour_margin_um: float | None
    recommended_route_mode: str | None
    estimated_route_length_um: float | None
    estimated_slack_deficit_ps: float | None
    reduce_route_delay_candidate_attempted: bool
    reduce_route_delay_candidate_improved: bool
    candidate_worst_setup_slack_ps: float | None
    candidate_setup_violations: int | None
    candidate_hold_violations: int | None
    candidate_route_mode: str | None
    candidate_route_length_um: float | None
    hold_fix_attempted: bool
    hold_fix_applied: bool
    initial_hold_violations: int
    final_hold_violations: int
    status: str
    next_step: str


@dataclass(frozen=True)
class TimingClosureSummary:
    closed: bool
    status: str
    setup_closed: bool
    hold_closed: bool
    capture_window_closed: bool
    setup_violations: int
    hold_violations: int
    capture_window_violations: int
    failing_checks: list[str]
    action_count: int
    primary_action: TimingClosureAction | None
    reduce_route_delay_actions: int
    relax_constraint_or_improve_library_timing_actions: int
    add_hold_padding_actions: int
    adjust_sfq_phase_or_pulse_window_actions: int
    actions: list[TimingClosureAction]
    next_step: str


@dataclass(frozen=True)
class TimingClosureAction:
    check: str
    priority: int
    remediation_kind: str
    from_pin: PinRef
    to_pin: PinRef
    slack_ps: float
    route_mode: str
    route_length_um: float
    from_domain: int | None
    to_domain: int | None


@dataclass(frozen=True)
class TimingAnalysisReport:
    worst_setup_slack_ps: float
    worst_hold_slack_ps: float
    critical_path_delay_ps: float
    analyzed_timing_arcs: int
    false_path_arcs: int
    setup_violations: int
    hold_violations: int
    capture_window_violations: int
    detour_feedback_applied: bool
    hold_fix_applied: bool
    closure: TimingClosureSummary
    timing_arcs: list[TimingArcReport]


@dataclass(frozen=True)
class TimingCornerAnalysisReport:
    corner_name: str
    is_default_corner: bool
    is_active_corner: bool
    worst_setup_slack_ps: float
    worst_hold_slack_ps: float
    critical_path_delay_ps: float
    analyzed_timing_arcs: int
    setup_violations: int
    hold_violations: int
    capture_window_violations: int
    closure: TimingClosureSummary


@dataclass(frozen=True)
class MultiCornerTimingAnalysisReport:
    active_timing_corner: str | None
    corner_count: int
    worst_setup_corner: str
    worst_hold_corner: str
    worst_critical_path_corner: str
    worst_setup_slack_ps: float
    worst_hold_slack_ps: float
    worst_critical_path_delay_ps: float
    corners: list[TimingCornerAnalysisReport]


@dataclass(frozen=True)
class StatisticalTimingAnalysisReport:
    worst_pessimistic_setup_slack_ps: float
    worst_pessimistic_hold_slack_ps: float
    analyzed_timing_arcs: int
    false_path_arcs: int
    setup_risk_violations: int
    hold_risk_violations: int
    sigma_multiplier: float
    timing_arcs: list[StatisticalTimingArcReport]


@dataclass(frozen=True)
class TimingArcReport:
    from_pin: PinRef
    to_pin: PinRef
    is_false_path: bool
    route_mode: str
    route_length_um: float
    from_domain: int | None
    to_domain: int | None
    launch_phase: int
    capture_phase: int
    launch_window_start_ps: float
    launch_window_end_ps: float
    capture_window_start_ps: float
    capture_window_end_ps: float
    arrival_phase_offset_ps: float
    capture_window_slack_ps: float
    capture_window_violation: bool
    arrival_ps: float
    required_ps: float
    setup_slack_ps: float
    hold_slack_ps: float


@dataclass(frozen=True)
class StatisticalTimingArcReport:
    from_pin: PinRef
    to_pin: PinRef
    is_false_path: bool
    route_mode: str
    route_length_um: float
    from_domain: int | None
    to_domain: int | None
    launch_phase: int
    capture_phase: int
    launch_window_start_ps: float
    launch_window_end_ps: float
    capture_window_start_ps: float
    capture_window_end_ps: float
    arrival_phase_offset_ps: float
    capture_window_slack_ps: float
    capture_window_violation: bool
    mean_arrival_ps: float
    mean_required_ps: float
    setup_slack_ps: float
    hold_slack_ps: float
    setup_sigma_ps: float
    hold_sigma_ps: float
    pessimistic_setup_slack_ps: float
    pessimistic_hold_slack_ps: float


@dataclass(frozen=True)
class AcBiasReport:
    routed_nets: int
    jtl_carrier_candidates: int
    ptl_coupling_risk_routes: int
    clock_sink_count: int
    estimated_static_power_savings_uw: float
    estimated_area_overhead_ratio: float
    estimated_frequency_derate_ratio: float
    worst_setup_slack_ps: float
    worst_hold_slack_ps: float
    timing_guardband_score: float
    feasibility_score: float
    optimization_score: float


@dataclass(frozen=True)
class AcBiasOptimizationReport:
    baseline: AcBiasReport
    optimized: AcBiasReport
    baseline_prefer_ptl_from_length_um: float
    optimized_prefer_ptl_from_length_um: float
    threshold_candidates_evaluated: int
    baseline_detour_margin_um: float
    optimized_detour_margin_um: float
    detour_margin_candidates_evaluated: int
    optimization_applied: bool


@dataclass(frozen=True)
class LibraryAwareAcBiasOptimizationReport:
    ac_bias: AcBiasOptimizationReport
    baseline_constraints: AdvancedConstraintReport
    optimized_constraints: AdvancedConstraintReport
    characterized_cells_merged: int
    library_optimization_score: float


@dataclass(frozen=True)
class LibraryAwareDesignOptimizationReport:
    ac_bias: AcBiasOptimizationReport
    baseline_statistical: StatisticalTimingAnalysisReport
    optimized_statistical: StatisticalTimingAnalysisReport
    baseline_constraints: AdvancedConstraintReport
    optimized_constraints: AdvancedConstraintReport
    characterized_cells_merged: int
    design_optimization_score: float
    baseline_cell_delay_sigma_ratio: float = 0.05
    optimized_cell_delay_sigma_ratio: float = 0.05
    baseline_sigma_multiplier: float = 3.0
    optimized_sigma_multiplier: float = 3.0
    baseline_placement_halo_scale: float = 1.0
    optimized_placement_halo_scale: float = 1.0
    placement_candidates_evaluated: int = 1
    statistical_candidates_evaluated: int = 1


@dataclass(frozen=True)
class CellLibraryEntry:
    name: str
    kind: str
    area_um2: float
    pipeline_stages: int
    intrinsic_delay_ps: float
    setup_ps: float
    hold_ps: float
    timing_source: str
    has_characterization_metadata: bool


@dataclass(frozen=True)
class CellLibraryMetadata:
    name: str
    version: str | None
    source: str | None


@dataclass(frozen=True)
class CellLibrarySummary:
    cell_count: int
    kind_count: int
    kind_counts: dict[str, int]
    named_timing_count: int
    kind_timing_count: int
    missing_timing_count: int
    characterized_cell_count: int
    named_timing_cells: list[str]
    missing_timing_cells: list[str]
    characterized_cells: list[str]


@dataclass(frozen=True)
class VerificationReport:
    checked_routes: int
    checked_ptl_routes: int
    structural_violations: int
    ptl_macro_boundary_violations: int
    ptl_forbidden_length_violations: int
    simulation_backend: str
    simulated_events: int
    generated_deck_lines: int
    generated_deck_path: str | None
    waveform_path: str | None
    reported_violations: int
    reported_worst_delay_ps: float | None
    delay_details: list[SimulationDelayDetail]
    measurement_details: list[SimulationMeasurementDetail]
    measurement_warnings: list[SimulationMeasurementWarning]
    violation_details: list[SimulationViolationDetail]
    external_status_code: int | None
    external_result: str | None


@dataclass(frozen=True)
class OutputMismatch:
    lhs: bool
    rhs: bool


@dataclass(frozen=True)
class StateTransitionMismatch:
    lhs_next: bool
    rhs_next: bool
    lhs_clock: bool
    rhs_clock: bool


@dataclass(frozen=True)
class CombinationalEquivalenceReport:
    equivalent: bool
    checked_outputs: list[str]
    counterexample_inputs: dict[str, bool]
    counterexample_outputs: dict[str, OutputMismatch]
    sat_recursive_calls: int
    sat_decisions: int
    sat_backtracks: int
    sat_restarts: int
    sat_elapsed_ns: int


@dataclass(frozen=True)
class SingleStepSequentialEquivalenceReport:
    equivalent: bool
    checked_outputs: list[str]
    checked_states: list[str]
    counterexample_inputs: dict[str, bool]
    counterexample_present_states: dict[str, bool]
    counterexample_outputs: dict[str, OutputMismatch]
    counterexample_states: dict[str, StateTransitionMismatch]
    sat_recursive_calls: int
    sat_decisions: int
    sat_backtracks: int
    sat_restarts: int
    sat_elapsed_ns: int


@dataclass(frozen=True)
class BoundedSequentialEquivalenceStepReport:
    step: int
    report: SingleStepSequentialEquivalenceReport


@dataclass(frozen=True)
class BoundedSequentialEquivalenceReport:
    equivalent: bool
    depth: int
    checked_steps: int
    unroll_mode: str
    checked_outputs: list[str]
    checked_states: list[str]
    first_failing_step: int | None
    steps: list[BoundedSequentialEquivalenceStepReport]
    sat_recursive_calls: int
    sat_decisions: int
    sat_backtracks: int
    sat_restarts: int
    sat_elapsed_ns: int


@dataclass(frozen=True)
class CompoundCellCharacterizationReport:
    cell_name: str
    node_count: int
    edge_count: int
    mapped_nodes: int
    total_area_um2: float
    derived_intrinsic_delay_ps: float
    derived_setup_ps: float
    derived_hold_ps: float
    generated_cell_kind: str
    generated_pipeline_stages: int
    generated_library_json: str
    simulated_delay_ps: float | None
    simulation_backend: str
    generated_deck_lines: int
    generated_deck_path: str | None
    waveform_path: str | None
    reported_violations: int


@dataclass(frozen=True)
class AdvancedConstraintViolation:
    category: str
    detail: str
    measured_value: float
    limit_value: float


@dataclass(frozen=True)
class AdvancedConstraintReport:
    estimated_thermal_load_uw: float
    estimated_mechanical_stress_score: float
    jtl_density_per_100um: float
    detour_overhead_ratio: float
    ptl_coupling_ratio: float
    manufacturing_hotspots: int
    violation_count: int
    violations: list[AdvancedConstraintViolation]


@dataclass(frozen=True)
class SimulationEndpointRef:
    raw: str
    node: str
    port: int | None


@dataclass(frozen=True)
class SimulationDelayDetail:
    name: str
    delay_ps: float
    from_ref: SimulationEndpointRef | None = None
    to_ref: SimulationEndpointRef | None = None


@dataclass(frozen=True)
class SimulationMeasurementDetail:
    name: str
    kind: str
    measured_value: float
    at_ref: SimulationEndpointRef | None = None


@dataclass(frozen=True)
class SimulationMeasurementWarning:
    name: str
    kind: str
    reason: str
    at_ref: SimulationEndpointRef | None = None


@dataclass(frozen=True)
class SimulationViolationDetail:
    kind: str
    detail: str
    at_ref: SimulationEndpointRef | None = None


@dataclass(frozen=True)
class SimulationReport:
    backend: str
    simulated_events: int
    generated_deck_lines: int
    generated_deck_path: str | None
    waveform_path: str | None
    reported_violations: int
    reported_worst_delay_ps: float | None
    delay_details: list[SimulationDelayDetail]
    measurement_details: list[SimulationMeasurementDetail]
    measurement_warnings: list[SimulationMeasurementWarning]
    violation_details: list[SimulationViolationDetail]
    external_status_code: int | None
    external_result: str | None