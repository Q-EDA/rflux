from __future__ import annotations

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


def simulate_text(deck_text: str, simulation_mode: str = "auto", external_command: str | None = None) -> SimulationReport: ...
def simulate_file(file_path: str | Path, simulation_mode: str = "auto", external_command: str | None = None) -> SimulationReport: ...
def is_supported_external_command(command: str) -> bool: ...

__all__: list[str]