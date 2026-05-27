from __future__ import annotations

from ._types import (
    AcBiasOptimizationReport,
    AcBiasReport,
    AdvancedConstraintReport,
    AdvancedConstraintViolation,
    CellLibraryEntry,
    CellLibraryMetadata,
    CellLibrarySummary,
    OutputMismatch,
    PinRef,
    SingleStepSequentialEquivalenceReport,
    StateTransitionMismatch,
    StatisticalTimingAnalysisReport,
    StatisticalTimingArcReport,
    TimingClosureAction,
    TimingClosureLoopReport,
    TimingClosureSummary,
    TimingCornerAnalysisReport,
)


def _cell_library_entry_from_core(entry) -> CellLibraryEntry:
    return CellLibraryEntry(
        name=entry.name,
        kind=entry.kind,
        area_um2=entry.area_um2,
        pipeline_stages=entry.pipeline_stages,
        intrinsic_delay_ps=entry.intrinsic_delay_ps,
        setup_ps=entry.setup_ps,
        hold_ps=entry.hold_ps,
        timing_source=entry.timing_source,
        has_characterization_metadata=entry.has_characterization_metadata,
    )


def _cell_library_metadata_from_core(metadata) -> CellLibraryMetadata:
    return CellLibraryMetadata(
        name=metadata.name,
        version=metadata.version,
        source=metadata.source,
    )


def _cell_library_summary_from_core(summary) -> CellLibrarySummary:
    return CellLibrarySummary(
        cell_count=summary.cell_count,
        kind_count=summary.kind_count,
        kind_counts=dict(summary.kind_counts),
        named_timing_count=summary.named_timing_count,
        kind_timing_count=summary.kind_timing_count,
        missing_timing_count=summary.missing_timing_count,
        characterized_cell_count=summary.characterized_cell_count,
        named_timing_cells=list(summary.named_timing_cells),
        missing_timing_cells=list(summary.missing_timing_cells),
        characterized_cells=list(summary.characterized_cells),
    )


def _single_step_sequential_report_from_core(report) -> SingleStepSequentialEquivalenceReport:
    return SingleStepSequentialEquivalenceReport(
        equivalent=report.equivalent,
        checked_outputs=list(report.checked_outputs),
        checked_states=list(report.checked_states),
        counterexample_inputs={entry.name: entry.value for entry in report.counterexample_inputs},
        counterexample_present_states={
            entry.name: entry.value for entry in report.counterexample_present_states
        },
        counterexample_outputs={
            entry.name: OutputMismatch(lhs=entry.lhs, rhs=entry.rhs)
            for entry in report.counterexample_outputs
        },
        counterexample_states={
            entry.name: StateTransitionMismatch(
                lhs_next=entry.lhs_next,
                rhs_next=entry.rhs_next,
                lhs_clock=entry.lhs_clock,
                rhs_clock=entry.rhs_clock,
            )
            for entry in report.counterexample_states
        },
        sat_recursive_calls=report.sat_recursive_calls,
        sat_decisions=report.sat_decisions,
        sat_backtracks=report.sat_backtracks,
        sat_restarts=report.sat_restarts,
        sat_elapsed_ns=report.sat_elapsed_ns,
    )


def _ac_bias_optimization_report_from_core(report) -> AcBiasOptimizationReport:
    return AcBiasOptimizationReport(
        baseline=AcBiasReport(
            routed_nets=report.baseline.routed_nets,
            jtl_carrier_candidates=report.baseline.jtl_carrier_candidates,
            ptl_coupling_risk_routes=report.baseline.ptl_coupling_risk_routes,
            clock_sink_count=report.baseline.clock_sink_count,
            estimated_static_power_savings_uw=report.baseline.estimated_static_power_savings_uw,
            estimated_area_overhead_ratio=report.baseline.estimated_area_overhead_ratio,
            estimated_frequency_derate_ratio=report.baseline.estimated_frequency_derate_ratio,
            worst_setup_slack_ps=report.baseline.worst_setup_slack_ps,
            worst_hold_slack_ps=report.baseline.worst_hold_slack_ps,
            timing_guardband_score=report.baseline.timing_guardband_score,
            feasibility_score=report.baseline.feasibility_score,
            optimization_score=report.baseline.optimization_score,
        ),
        optimized=AcBiasReport(
            routed_nets=report.optimized.routed_nets,
            jtl_carrier_candidates=report.optimized.jtl_carrier_candidates,
            ptl_coupling_risk_routes=report.optimized.ptl_coupling_risk_routes,
            clock_sink_count=report.optimized.clock_sink_count,
            estimated_static_power_savings_uw=report.optimized.estimated_static_power_savings_uw,
            estimated_area_overhead_ratio=report.optimized.estimated_area_overhead_ratio,
            estimated_frequency_derate_ratio=report.optimized.estimated_frequency_derate_ratio,
            worst_setup_slack_ps=report.optimized.worst_setup_slack_ps,
            worst_hold_slack_ps=report.optimized.worst_hold_slack_ps,
            timing_guardband_score=report.optimized.timing_guardband_score,
            feasibility_score=report.optimized.feasibility_score,
            optimization_score=report.optimized.optimization_score,
        ),
        baseline_prefer_ptl_from_length_um=report.baseline_prefer_ptl_from_length_um,
        optimized_prefer_ptl_from_length_um=report.optimized_prefer_ptl_from_length_um,
        baseline_detour_margin_um=report.baseline_detour_margin_um,
        optimized_detour_margin_um=report.optimized_detour_margin_um,
        threshold_candidates_evaluated=report.threshold_candidates_evaluated,
        detour_margin_candidates_evaluated=report.detour_margin_candidates_evaluated,
        optimization_applied=report.optimization_applied,
    )


def _advanced_constraint_report_from_core(report) -> AdvancedConstraintReport:
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
                category=item.category,
                detail=item.detail,
                measured_value=item.measured_value,
                limit_value=item.limit_value,
            )
            for item in report.violations
        ],
    )


def _statistical_timing_report_from_core(report) -> StatisticalTimingAnalysisReport:
    return StatisticalTimingAnalysisReport(
        worst_pessimistic_setup_slack_ps=report.worst_pessimistic_setup_slack_ps,
        worst_pessimistic_hold_slack_ps=report.worst_pessimistic_hold_slack_ps,
        analyzed_timing_arcs=report.analyzed_timing_arcs,
        false_path_arcs=report.false_path_arcs,
        setup_risk_violations=report.setup_risk_violations,
        hold_risk_violations=report.hold_risk_violations,
        sigma_multiplier=report.sigma_multiplier,
        timing_arcs=[
            StatisticalTimingArcReport(
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
                mean_arrival_ps=arc.mean_arrival_ps,
                mean_required_ps=arc.mean_required_ps,
                setup_slack_ps=arc.setup_slack_ps,
                hold_slack_ps=arc.hold_slack_ps,
                setup_sigma_ps=arc.setup_sigma_ps,
                hold_sigma_ps=arc.hold_sigma_ps,
                pessimistic_setup_slack_ps=arc.pessimistic_setup_slack_ps,
                pessimistic_hold_slack_ps=arc.pessimistic_hold_slack_ps,
            )
            for arc in report.timing_arcs
        ],
    )


def _timing_closure_loop_from_core(loop_report) -> TimingClosureLoopReport:
    return TimingClosureLoopReport(
        detour_feedback_attempted=loop_report.detour_feedback_attempted,
        detour_feedback_applied=loop_report.detour_feedback_applied,
        initial_total_detour_overhead_um=loop_report.initial_total_detour_overhead_um,
        final_total_detour_overhead_um=loop_report.final_total_detour_overhead_um,
        route_delay_optimization_attempted=loop_report.route_delay_optimization_attempted,
        route_delay_optimization_applied=loop_report.route_delay_optimization_applied,
        reduce_route_delay_candidate_available=loop_report.reduce_route_delay_candidate_available,
        recommended_prefer_ptl_from_length_um=loop_report.recommended_prefer_ptl_from_length_um,
        recommended_detour_margin_um=loop_report.recommended_detour_margin_um,
        recommended_route_mode=loop_report.recommended_route_mode,
        estimated_route_length_um=loop_report.estimated_route_length_um,
        estimated_slack_deficit_ps=loop_report.estimated_slack_deficit_ps,
        reduce_route_delay_candidate_attempted=loop_report.reduce_route_delay_candidate_attempted,
        reduce_route_delay_candidate_improved=loop_report.reduce_route_delay_candidate_improved,
        candidate_worst_setup_slack_ps=loop_report.candidate_worst_setup_slack_ps,
        candidate_setup_violations=loop_report.candidate_setup_violations,
        candidate_hold_violations=loop_report.candidate_hold_violations,
        candidate_route_mode=loop_report.candidate_route_mode,
        candidate_route_length_um=loop_report.candidate_route_length_um,
        hold_fix_attempted=loop_report.hold_fix_attempted,
        hold_fix_applied=loop_report.hold_fix_applied,
        initial_hold_violations=loop_report.initial_hold_violations,
        final_hold_violations=loop_report.final_hold_violations,
        status=loop_report.status,
        next_step=loop_report.next_step,
    )


def _timing_closure_from_core(closure) -> TimingClosureSummary:
    return TimingClosureSummary(
        closed=closure.closed,
        status=closure.status,
        setup_closed=closure.setup_closed,
        hold_closed=closure.hold_closed,
        capture_window_closed=closure.capture_window_closed,
        setup_violations=closure.setup_violations,
        hold_violations=closure.hold_violations,
        capture_window_violations=closure.capture_window_violations,
        failing_checks=list(closure.failing_checks),
        action_count=closure.action_count,
        primary_action=(
            None
            if closure.primary_action is None
            else _timing_closure_action_from_core(closure.primary_action)
        ),
        reduce_route_delay_actions=closure.reduce_route_delay_actions,
        relax_constraint_or_improve_library_timing_actions=(
            closure.relax_constraint_or_improve_library_timing_actions
        ),
        add_hold_padding_actions=closure.add_hold_padding_actions,
        adjust_sfq_phase_or_pulse_window_actions=(
            closure.adjust_sfq_phase_or_pulse_window_actions
        ),
        actions=[_timing_closure_action_from_core(action) for action in closure.actions],
        next_step=closure.next_step,
    )


def _timing_closure_action_from_core(action) -> TimingClosureAction:
    return TimingClosureAction(
        check=action.check,
        priority=action.priority,
        remediation_kind=action.remediation_kind,
        from_pin=PinRef(node=action.from_pin.node, port=action.from_pin.port),
        to_pin=PinRef(node=action.to_pin.node, port=action.to_pin.port),
        slack_ps=action.slack_ps,
        route_mode=action.route_mode,
        route_length_um=action.route_length_um,
        from_domain=action.from_domain,
        to_domain=action.to_domain,
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
        closure=_timing_closure_from_core(report.closure),
    )