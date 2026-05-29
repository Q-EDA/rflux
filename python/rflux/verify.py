import sys

from ._types import (
    BoundedSequentialEquivalenceReport,
    BoundedSequentialEquivalenceStepReport,
    CombinationalEquivalenceReport,
    OutputMismatch,
    SimulationDelayDetail,
    SimulationEndpointRef,
    SimulationMeasurementDetail,
    SimulationMeasurementWarning,
    SimulationViolationDetail,
    SingleStepSequentialEquivalenceReport,
    StateTransitionMismatch,
    VerificationReport,
)


_api = sys.modules[__package__]


def verify_layout(
    circuit,
    plan=None,
    fixed_nodes=None,
    blocked_regions=None,
    timing_constraints=None,
    pin_timing_constraints=None,
    clock_domains=None,
    crossing_constraints=None,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> VerificationReport:
    """Run structural, routing, and simulation-backed layout checks."""
    _api._validate_simulation_mode(simulation_mode)
    _api._require_core_extension("verify_layout(...)", _api._core_verify_layout)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_verify_layout(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        simulation_mode,
        external_command,
    )
    return VerificationReport(
        checked_routes=report.checked_routes,
        checked_ptl_routes=report.checked_ptl_routes,
        structural_violations=report.structural_violations,
        ptl_macro_boundary_violations=report.ptl_macro_boundary_violations,
        ptl_forbidden_length_violations=report.ptl_forbidden_length_violations,
        simulation_backend=report.simulation_backend,
        josim_alignment_level=report.josim_alignment_level,
        josim_alignment_available=report.josim_alignment_available,
        josim_next_step=report.josim_next_step,
        josim_quality_passed=report.josim_quality_passed,
        josim_quality_status=report.josim_quality_status,
        simulated_events=report.simulated_events,
        generated_deck_lines=report.generated_deck_lines,
        generated_deck_path=report.generated_deck_path,
        waveform_path=report.waveform_path,
        waveform_format=getattr(report, "waveform_format", None),
        external_summary_contract=getattr(report, "external_summary_contract", None),
        reported_violations=report.reported_violations,
        reported_worst_delay_ps=report.reported_worst_delay_ps,
        delay_details=[
            SimulationDelayDetail(
                name=detail.name,
                delay_ps=detail.delay_ps,
                from_ref=(
                    None
                    if detail.from_ref is None
                    else SimulationEndpointRef(
                        raw=detail.from_ref.raw,
                        node=detail.from_ref.node,
                        port=detail.from_ref.port,
                    )
                ),
                to_ref=(
                    None
                    if detail.to_ref is None
                    else SimulationEndpointRef(
                        raw=detail.to_ref.raw,
                        node=detail.to_ref.node,
                        port=detail.to_ref.port,
                    )
                ),
            )
            for detail in report.delay_details
        ],
        measurement_details=[
            SimulationMeasurementDetail(
                name=detail.name,
                kind=detail.kind,
                measured_value=detail.measured_value,
                at_ref=(
                    None
                    if detail.at_ref is None
                    else SimulationEndpointRef(
                        raw=detail.at_ref.raw,
                        node=detail.at_ref.node,
                        port=detail.at_ref.port,
                    )
                ),
            )
            for detail in getattr(report, "measurement_details", [])
        ],
        measurement_warnings=[
            SimulationMeasurementWarning(
                name=warning.name,
                kind=warning.kind,
                reason=warning.reason,
                at_ref=(
                    None
                    if warning.at_ref is None
                    else SimulationEndpointRef(
                        raw=warning.at_ref.raw,
                        node=warning.at_ref.node,
                        port=warning.at_ref.port,
                    )
                ),
            )
            for warning in getattr(report, "measurement_warnings", [])
        ],
        violation_details=[
            SimulationViolationDetail(
                kind=detail.kind,
                detail=detail.detail,
                at_ref=(
                    None
                    if detail.at_ref is None
                    else SimulationEndpointRef(
                        raw=detail.at_ref.raw,
                        node=detail.at_ref.node,
                        port=detail.at_ref.port,
                    )
                ),
            )
            for detail in report.violation_details
        ],
        external_status_code=report.external_status_code,
        external_result=report.external_result,
    )


def check_equivalence(lhs, rhs) -> CombinationalEquivalenceReport:
    """Check combinational equivalence between two circuits."""
    _api._require_core_extension("check_equivalence(...)", _api._core_check_equivalence)

    report = _api._core_check_equivalence(lhs, rhs)
    return CombinationalEquivalenceReport(
        equivalent=report.equivalent,
        checked_outputs=list(report.checked_outputs),
        counterexample_inputs={entry.name: entry.value for entry in report.counterexample_inputs},
        counterexample_outputs={
            entry.name: OutputMismatch(lhs=entry.lhs, rhs=entry.rhs)
            for entry in report.counterexample_outputs
        },
        sat_recursive_calls=report.sat_recursive_calls,
        sat_decisions=report.sat_decisions,
        sat_backtracks=report.sat_backtracks,
        sat_restarts=report.sat_restarts,
        sat_elapsed_ns=report.sat_elapsed_ns,
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


def check_single_step_sequential_equivalence(
    lhs,
    rhs,
) -> SingleStepSequentialEquivalenceReport:
    """Check one-step sequential equivalence between two circuits."""
    _api._require_core_extension(
        "check_single_step_sequential_equivalence(...)",
        _api._core_check_single_step_sequential_equivalence,
    )
    return _single_step_sequential_report_from_core(
        _api._core_check_single_step_sequential_equivalence(lhs, rhs)
    )


def check_bounded_sequential_equivalence(
    lhs,
    rhs,
    depth: int = 2,
) -> BoundedSequentialEquivalenceReport:
    """Check state-unrolled bounded sequential equivalence up to ``depth`` steps."""
    _api._require_core_extension(
        "check_bounded_sequential_equivalence(...)",
        _api._core_check_bounded_sequential_equivalence,
    )

    report = _api._core_check_bounded_sequential_equivalence(lhs, rhs, depth)
    return BoundedSequentialEquivalenceReport(
        equivalent=report.equivalent,
        depth=report.depth,
        checked_steps=report.checked_steps,
        unroll_mode=report.unroll_mode,
        checked_outputs=list(report.checked_outputs),
        checked_states=list(report.checked_states),
        first_failing_step=report.first_failing_step,
        steps=[
            BoundedSequentialEquivalenceStepReport(
                step=step.step,
                report=_single_step_sequential_report_from_core(step.report),
            )
            for step in report.steps
        ],
        sat_recursive_calls=report.sat_recursive_calls,
        sat_decisions=report.sat_decisions,
        sat_backtracks=report.sat_backtracks,
        sat_restarts=report.sat_restarts,
        sat_elapsed_ns=report.sat_elapsed_ns,
    )

__all__ = [
    "BoundedSequentialEquivalenceReport",
    "BoundedSequentialEquivalenceStepReport",
    "CombinationalEquivalenceReport",
    "OutputMismatch",
    "SingleStepSequentialEquivalenceReport",
    "StateTransitionMismatch",
    "VerificationReport",
    "check_bounded_sequential_equivalence",
    "check_equivalence",
    "check_single_step_sequential_equivalence",
    "verify_layout",
]