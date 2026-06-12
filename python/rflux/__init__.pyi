from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from . import flow as flow, pdk as pdk, sim as sim, timing as timing, verify as verify
from ._types import (
    AcBiasOptimizationReport,
    AcBiasReport,
    AdvancedConstraintReport,
    AdvancedConstraintViolation,
    BalanceStrategy,
    BlockedRegion,
    BoundedSequentialEquivalenceReport,
    BoundedSequentialEquivalenceStepReport,
    CellLibraryEntry,
    CellLibraryMetadata,
    CellLibrarySummary,
    ClockDomainConstraint,
    CombinationalEquivalenceReport,
    CompilePlan,
    CompileReport,
    CompoundCellCharacterizationReport,
    ConnectionSpec,
    CrossingConstraint,
    FixedNodePlacement,
    LayoutReport,
    LibraryAwareAcBiasOptimizationReport,
    LibraryAwareDesignOptimizationReport,
    MultiCornerTimingAnalysisReport,
    NodeTimingConstraint,
    OutputMismatch,
    PinRef,
    PinTimingConstraint,
    SimulationDelayDetail,
    SimulationEndpointRef,
    SimulationMeasurementDetail,
    SimulationMeasurementWarning,
    SimulationReport,
    SimulationViolationDetail,
    SingleStepSequentialEquivalenceReport,
    StateTransitionMismatch,
    StatisticalTimingAnalysisReport,
    StatisticalTimingArcReport,
    SynthesisReport,
    TimingAnalysisReport,
    TimingArcReport,
    TimingClosureAction,
    TimingClosureLoopReport,
    TimingClosureSummary,
    TimingCornerAnalysisReport,
    VerificationReport,
)
from .pdk import Pdk

__version__: str
FLOW_CONFIG_KIND: str
FLOW_CONFIG_SCHEMA_VERSION: int
SIMULATION_MODES: tuple[str, ...]
DEFAULT_CLOCK_PERIOD_PS: float
DEFAULT_INPUT_ARRIVAL_PS: float
DEFAULT_SFQ_PHASE_COUNT: int
DEFAULT_SFQ_PULSE_WINDOW_PS: float
DEFAULT_MIN_HOLD_JTL_LENGTH_UM: float


class RfluxError(Exception): ...
class RfluxCoreUnavailableError(RfluxError, RuntimeError): ...


@dataclass(frozen=True)
class CoreStatus:
    available: bool
    version: str
    extension_path: str | None
    import_error: str | None


class Circuit:
    def __init__(self, name: str = "") -> None: ...
    def add_node(self, kind: str, name: str, logic_op: str | None = None) -> int: ...
    def connect(self, from_node: int, from_port: int, to_node: int, to_port: int) -> None: ...
    def node_count(self) -> int: ...
    def edge_count(self) -> int: ...
    def nodes(self) -> list[tuple[int, str, str]]: ...
    def edges(self) -> list[tuple[tuple[int, int], tuple[int, int]]]: ...
    def to_json(self) -> str: ...
    @staticmethod
    def from_json(payload: str, name: str = "") -> Circuit: ...


def core_version() -> str: ...
def core_available() -> bool: ...
def core_status() -> CoreStatus: ...

def compile(circuit: Circuit, plan: CompilePlan | None = None) -> Circuit: ...
def compile_plan_report(circuit: Circuit, plan: CompilePlan) -> CompileReport: ...
def compile_plan(circuit: Circuit, plan: CompilePlan) -> Circuit: ...
def compile_netlist(circuit: Circuit, plan: CompilePlan | None = None) -> SynthesisReport: ...
def compile_layout(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, min_hold_jtl_length_um: float | None = None, prefer_ptl_from_length_um: float | None = None, detour_margin_um: float | None = None, characterized_library_json: str | None = None, characterized_library_entries: list[str] | None = None) -> LayoutReport: ...
def analyze_timing(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, characterized_library_json: str | None = None, characterized_library_entries: list[str] | None = None) -> TimingAnalysisReport: ...
def analyze_timing_corners(circuit: Circuit, pdk: Pdk, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None) -> MultiCornerTimingAnalysisReport: ...
def analyze_timing_statistical(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, characterized_library_json: str | None = None, characterized_library_entries: list[str] | None = None, cell_delay_sigma_ratio: float = 0.05, wire_delay_sigma_ratio: float = 0.05, global_cell_delay_sigma_ratio: float = 0.0, global_wire_delay_sigma_ratio: float = 0.0, clock_uncertainty_sigma_ps: float = 0.0, cross_domain_uncertainty_sigma_ps: float = 0.0, max_delay_cross_domain_uncertainty_sigma_ps: float = 0.0, multicycle_cross_domain_uncertainty_sigma_ps: float = 0.0, sigma_multiplier: float = 3.0) -> StatisticalTimingAnalysisReport: ...
def analyze_ac_bias(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None) -> AcBiasReport: ...
def analyze_advanced_constraints(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, max_estimated_thermal_load_uw: float = 8.0, max_estimated_mechanical_stress_score: float = 0.75, max_jtl_density_per_100um: float = 8.0, max_detour_overhead_ratio: float = 0.35, max_ptl_coupling_ratio: float = 0.65) -> AdvancedConstraintReport: ...
def optimize_ac_bias(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None) -> AcBiasOptimizationReport: ...
def characterize_compound_cell(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, cell_name: str = "compound_cell", simulation_mode: str = "auto", external_command: str | None = None) -> CompoundCellCharacterizationReport: ...
def merge_characterized_library(serialized_entries: list[str], base_name: str = "py-minimal-pdk") -> str: ...
def optimize_ac_bias_with_characterized_library(circuit: Circuit, characterized_library_entries: list[str], plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, max_estimated_thermal_load_uw: float = 8.0, max_estimated_mechanical_stress_score: float = 0.75, max_jtl_density_per_100um: float = 8.0, max_detour_overhead_ratio: float = 0.35, max_ptl_coupling_ratio: float = 0.65) -> LibraryAwareAcBiasOptimizationReport: ...
def optimize_design_with_characterized_library(circuit: Circuit, characterized_library_entries: list[str], plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, max_estimated_thermal_load_uw: float = 8.0, max_estimated_mechanical_stress_score: float = 0.75, max_jtl_density_per_100um: float = 8.0, max_detour_overhead_ratio: float = 0.35, max_ptl_coupling_ratio: float = 0.65, cell_delay_sigma_ratio: float = 0.05, wire_delay_sigma_ratio: float = 0.05, global_cell_delay_sigma_ratio: float = 0.0, global_wire_delay_sigma_ratio: float = 0.0, clock_uncertainty_sigma_ps: float = 0.0, cross_domain_uncertainty_sigma_ps: float = 0.0, max_delay_cross_domain_uncertainty_sigma_ps: float = 0.0, multicycle_cross_domain_uncertainty_sigma_ps: float = 0.0, sigma_multiplier: float = 3.0) -> LibraryAwareDesignOptimizationReport: ...
def verify_layout(circuit: Circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list[BlockedRegion] | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, simulation_mode: str = "auto", external_command: str | None = None) -> VerificationReport: ...
def simulate_text(deck_text: str, simulation_mode: str = "auto", external_command: str | None = None) -> SimulationReport: ...
def simulate_file(file_path: str | Path, simulation_mode: str = "auto", external_command: str | None = None) -> SimulationReport: ...
def is_supported_external_command(command: str) -> bool: ...
def read_bench_file(file_path: str | Path, name: str | None = None) -> Circuit: ...
def read_bench_text(text: str, name: str | None = None) -> Circuit: ...
def check_equivalence(lhs: Circuit, rhs: Circuit) -> CombinationalEquivalenceReport: ...
def check_single_step_sequential_equivalence(lhs: Circuit, rhs: Circuit) -> SingleStepSequentialEquivalenceReport: ...
def check_bounded_sequential_equivalence(lhs: Circuit, rhs: Circuit, depth: int = 2) -> BoundedSequentialEquivalenceReport: ...

__all__ = [
    "__version__",
    "BalanceStrategy",
    "BlockedRegion",
    "ClockDomainConstraint",
    "CrossingConstraint",
    "Circuit",
    "CoreStatus",
    "RfluxError",
    "RfluxCoreUnavailableError",
    "FLOW_CONFIG_KIND",
    "FLOW_CONFIG_SCHEMA_VERSION",
    "SIMULATION_MODES",
    "CellLibraryEntry",
    "CellLibraryMetadata",
    "CellLibrarySummary",
    "CompilePlan",
    "CompileReport",
    "ConnectionSpec",
    "FixedNodePlacement",
    "LayoutReport",
    "NodeTimingConstraint",
    "PinTimingConstraint",
    "TimingAnalysisReport",
    "TimingArcReport",
    "TimingCornerAnalysisReport",
    "MultiCornerTimingAnalysisReport",
    "TimingClosureAction",
    "TimingClosureLoopReport",
    "TimingClosureSummary",
    "StatisticalTimingAnalysisReport",
    "StatisticalTimingArcReport",
    "AcBiasReport",
    "AcBiasOptimizationReport",
    "LibraryAwareAcBiasOptimizationReport",
    "LibraryAwareDesignOptimizationReport",
    "Pdk",
    "AdvancedConstraintReport",
    "AdvancedConstraintViolation",
    "BoundedSequentialEquivalenceReport",
    "BoundedSequentialEquivalenceStepReport",
    "CombinationalEquivalenceReport",
    "VerificationReport",
    "CompoundCellCharacterizationReport",
    "OutputMismatch",
    "PinRef",
    "SynthesisReport",
    "SimulationDelayDetail",
    "SimulationEndpointRef",
    "SimulationMeasurementDetail",
    "SimulationMeasurementWarning",
    "SimulationReport",
    "SimulationViolationDetail",
    "SingleStepSequentialEquivalenceReport",
    "StateTransitionMismatch",
    "analyze_timing",
    "analyze_timing_corners",
    "analyze_timing_statistical",
    "analyze_ac_bias",
    "analyze_advanced_constraints",
    "characterize_compound_cell",
    "optimize_ac_bias",
    "optimize_ac_bias_with_characterized_library",
    "optimize_design_with_characterized_library",
    "merge_characterized_library",
    "read_bench_file",
    "read_bench_text",
    "simulate_file",
    "is_supported_external_command",
    "simulate_text",
    "compile",
    "compile_layout",
    "compile_netlist",
    "compile_plan",
    "compile_plan_report",
    "check_equivalence",
    "check_bounded_sequential_equivalence",
    "check_single_step_sequential_equivalence",
    "core_available",
    "core_status",
    "core_version",
    "flow",
    "pdk",
    "sim",
    "timing",
    "verify_layout",
    "verify",
]