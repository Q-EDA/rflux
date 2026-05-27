import sys

from .pdk import Pdk
from ._types import (
    AdvancedConstraintReport,
    AdvancedConstraintViolation,
    ClockDomainConstraint,
    CompilePlan,
    CrossingConstraint,
    FixedNodePlacement,
    MultiCornerTimingAnalysisReport,
    NodeTimingConstraint,
    PinRef,
    PinTimingConstraint,
    StatisticalTimingAnalysisReport,
    StatisticalTimingArcReport,
    TimingAnalysisReport,
    TimingArcReport,
    TimingClosureAction,
    TimingClosureLoopReport,
    TimingClosureSummary,
    TimingCornerAnalysisReport,
)


_api = sys.modules[__package__]


def analyze_advanced_constraints(
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    max_estimated_thermal_load_uw: float = 8.0,
    max_estimated_mechanical_stress_score: float = 0.75,
    max_jtl_density_per_100um: float = 8.0,
    max_detour_overhead_ratio: float = 0.35,
    max_ptl_coupling_ratio: float = 0.65,
) -> AdvancedConstraintReport:
    """Evaluate thermal, mechanical, manufacturing, and coupling budgets."""
    _api._require_core_extension(
        "analyze_advanced_constraints(...)",
        _api._core_analyze_advanced_constraints,
    )
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_analyze_advanced_constraints(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        max_estimated_thermal_load_uw,
        max_estimated_mechanical_stress_score,
        max_jtl_density_per_100um,
        max_detour_overhead_ratio,
        max_ptl_coupling_ratio,
    )
    return AdvancedConstraintReport(
        estimated_thermal_load_uw=report.estimated_thermal_load_uw,
        estimated_mechanical_stress_score=report.estimated_mechanical_stress_score,
        jtl_density_per_100um=report.jtl_density_per_100um,
        detour_overhead_ratio=report.detour_overhead_ratio,
        ptl_coupling_ratio=report.ptl_coupling_ratio,
        manufacturing_hotspots=report.manufacturing_hotspots,
        violation_count=report.violation_count,
        violations=[
            AdvancedConstraintViolation(
                category=violation.category,
                detail=violation.detail,
                measured_value=violation.measured_value,
                limit_value=violation.limit_value,
            )
            for violation in report.violations
        ],
    )


def analyze_timing(
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
) -> TimingAnalysisReport:
    """Run static timing analysis and return per-arc closure diagnostics."""
    _api._require_core_extension("analyze_timing(...)", _api._core_analyze_timing)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_analyze_timing(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        characterized_library_json,
        characterized_library_entries,
    )
    return TimingAnalysisReport(
        worst_setup_slack_ps=report.worst_setup_slack_ps,
        worst_hold_slack_ps=report.worst_hold_slack_ps,
        critical_path_delay_ps=report.critical_path_delay_ps,
        analyzed_timing_arcs=report.analyzed_timing_arcs,
        false_path_arcs=report.false_path_arcs,
        setup_violations=report.setup_violations,
        hold_violations=report.hold_violations,
        capture_window_violations=report.capture_window_violations,
        detour_feedback_applied=report.detour_feedback_applied,
        hold_fix_applied=report.hold_fix_applied,
        closure=_api._timing_closure_from_core(report.closure),
        timing_arcs=[
            TimingArcReport(
                from_pin=PinRef(node=arc.from_pin.node, port=arc.from_pin.port),
                to_pin=PinRef(node=arc.to_pin.node, port=arc.to_pin.port),
                is_false_path=arc.is_false_path,
                route_mode=arc.route_mode,
                route_length_um=arc.route_length_um,
                from_domain=arc.from_domain,
                to_domain=arc.to_domain,
                launch_phase=arc.launch_phase,
                capture_phase=arc.capture_phase,
                launch_window_start_ps=arc.launch_window_start_ps,
                launch_window_end_ps=arc.launch_window_end_ps,
                capture_window_start_ps=arc.capture_window_start_ps,
                capture_window_end_ps=arc.capture_window_end_ps,
                arrival_phase_offset_ps=arc.arrival_phase_offset_ps,
                capture_window_slack_ps=arc.capture_window_slack_ps,
                capture_window_violation=arc.capture_window_violation,
                arrival_ps=arc.arrival_ps,
                required_ps=arc.required_ps,
                setup_slack_ps=arc.setup_slack_ps,
                hold_slack_ps=arc.hold_slack_ps,
            )
            for arc in report.timing_arcs
        ],
    )


def _timing_corner_analysis_from_core(report) -> TimingCornerAnalysisReport:
    return TimingCornerAnalysisReport(
        corner_name=report.corner_name,
        is_default_corner=report.is_default_corner,
        is_active_corner=report.is_active_corner,
        worst_setup_slack_ps=report.worst_setup_slack_ps,
        worst_hold_slack_ps=report.worst_hold_slack_ps,
        critical_path_delay_ps=report.critical_path_delay_ps,
        analyzed_timing_arcs=report.analyzed_timing_arcs,
        setup_violations=report.setup_violations,
        hold_violations=report.hold_violations,
        capture_window_violations=report.capture_window_violations,
        closure=_api._timing_closure_from_core(report.closure),
    )


def analyze_timing_corners(
    circuit,
    pdk: Pdk,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
) -> MultiCornerTimingAnalysisReport:
    """Run timing analysis for the default PDK corner and each named timing corner."""
    _api._require_core_extension(
        "analyze_timing_corners(...)",
        _api._core_analyze_timing_corners,
    )
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_analyze_timing_corners(
        circuit,
        pdk._core,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
    )
    return MultiCornerTimingAnalysisReport(
        active_timing_corner=report.active_timing_corner,
        corner_count=report.corner_count,
        worst_setup_corner=report.worst_setup_corner,
        worst_hold_corner=report.worst_hold_corner,
        worst_critical_path_corner=report.worst_critical_path_corner,
        worst_setup_slack_ps=report.worst_setup_slack_ps,
        worst_hold_slack_ps=report.worst_hold_slack_ps,
        worst_critical_path_delay_ps=report.worst_critical_path_delay_ps,
        corners=[_timing_corner_analysis_from_core(corner) for corner in report.corners],
    )


def analyze_timing_statistical(
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
    cell_delay_sigma_ratio: float = 0.05,
    wire_delay_sigma_ratio: float = 0.05,
    global_cell_delay_sigma_ratio: float = 0.0,
    global_wire_delay_sigma_ratio: float = 0.0,
    clock_uncertainty_sigma_ps: float = 0.0,
    cross_domain_uncertainty_sigma_ps: float = 0.0,
    max_delay_cross_domain_uncertainty_sigma_ps: float = 0.0,
    multicycle_cross_domain_uncertainty_sigma_ps: float = 0.0,
    sigma_multiplier: float = 3.0,
) -> StatisticalTimingAnalysisReport:
    """Run statistical timing analysis and return pessimistic slack estimates."""
    _api._require_core_extension(
        "analyze_timing_statistical(...)",
        _api._core_analyze_timing_statistical,
    )
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_analyze_timing_statistical(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        characterized_library_json,
        characterized_library_entries,
        cell_delay_sigma_ratio,
        wire_delay_sigma_ratio,
        global_cell_delay_sigma_ratio,
        global_wire_delay_sigma_ratio,
        clock_uncertainty_sigma_ps,
        cross_domain_uncertainty_sigma_ps,
        max_delay_cross_domain_uncertainty_sigma_ps,
        multicycle_cross_domain_uncertainty_sigma_ps,
        sigma_multiplier,
    )
    return _api._statistical_timing_report_from_core(report)

__all__ = [
    "AdvancedConstraintReport",
    "AdvancedConstraintViolation",
    "ClockDomainConstraint",
    "CrossingConstraint",
    "MultiCornerTimingAnalysisReport",
    "NodeTimingConstraint",
    "PinRef",
    "PinTimingConstraint",
    "StatisticalTimingAnalysisReport",
    "StatisticalTimingArcReport",
    "TimingAnalysisReport",
    "TimingArcReport",
    "TimingClosureAction",
    "TimingClosureLoopReport",
    "TimingClosureSummary",
    "TimingCornerAnalysisReport",
    "analyze_advanced_constraints",
    "analyze_timing",
    "analyze_timing_corners",
    "analyze_timing_statistical",
]