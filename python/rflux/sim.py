import sys
from pathlib import Path

from . import SIMULATION_MODES
from ._types import (
    SimulationDelayDetail,
    SimulationEndpointRef,
    SimulationMeasurementDetail,
    SimulationMeasurementWarning,
    SimulationReport,
    SimulationViolationDetail,
)


_api = sys.modules[__package__]


def _simulation_report_from_core(report) -> SimulationReport:
    return SimulationReport(
        backend=report.backend,
        josim_alignment_level=report.josim_alignment_level,
        josim_alignment_available=report.josim_alignment_available,
        josim_next_step=report.josim_next_step,
        simulated_events=report.simulated_events,
        generated_deck_lines=report.generated_deck_lines,
        generated_deck_path=report.generated_deck_path,
        waveform_path=report.waveform_path,
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


def simulate_text(
    deck_text: str,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> SimulationReport:
    """Simulate a SPICE-like deck string and return a structured report."""
    _api._validate_simulation_mode(simulation_mode)
    _api._require_core_extension("simulate_text(...)", _api._core_simulate_text)
    return _simulation_report_from_core(
        _api._core_simulate_text(deck_text, simulation_mode, external_command)
    )


def simulate_file(
    file_path: str | Path,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> SimulationReport:
    """Simulate a SPICE-like deck file and return a structured report."""
    _api._validate_simulation_mode(simulation_mode)
    _api._require_core_extension("simulate_file(...)", _api._core_simulate_file)
    return _simulation_report_from_core(
        _api._core_simulate_file(str(file_path), simulation_mode, external_command)
    )

__all__ = [
    "SIMULATION_MODES",
    "SimulationDelayDetail",
    "SimulationEndpointRef",
    "SimulationMeasurementDetail",
    "SimulationMeasurementWarning",
    "SimulationReport",
    "SimulationViolationDetail",
    "simulate_file",
    "simulate_text",
]