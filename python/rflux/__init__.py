from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
import importlib.util
import json
import sys

from ._version import __version__


SIMULATION_MODES = ("auto", "event_only", "external_josim", "internal_transient")
_CORE_IMPORT_ERROR: str | None = None


class RfluxError(Exception):
    """Base class for rflux Python API errors."""


class RfluxCoreUnavailableError(RfluxError, RuntimeError):
    """Raised when an API requires the compiled ``rflux._core`` extension."""


@dataclass(frozen=True)
class CoreStatus:
    available: bool
    version: str
    extension_path: str | None
    import_error: str | None


def _extend_package_path_for_extension() -> None:
    package_path = Path(__file__).resolve().parent
    package_parent = package_path.parent.resolve()

    for entry in sys.path:
        try:
            candidate_root = Path(entry).resolve()
        except OSError:
            continue

        if candidate_root == package_parent:
            continue

        candidate_package = candidate_root / "rflux"
        if candidate_package != package_path and candidate_package.is_dir():
            candidate_str = str(candidate_package)
            if candidate_str not in __path__:
                __path__.append(candidate_str)


_extend_package_path_for_extension()

try:
    from ._core import (  # type: ignore[attr-defined]
        Circuit,
        PyBlockedRegion as _CoreBlockedRegion,
        PyClockDomainConstraint as _CoreClockDomainConstraint,
        PyCrossingConstraint as _CoreCrossingConstraint,
        PyCompilePlan as _CoreCompilePlan,
        PyConnectionSpec as _CoreConnectionSpec,
        PyFixedNodePlacement as _CoreFixedNodePlacement,
        PyNodeTimingConstraint as _CoreNodeTimingConstraint,
        PyPinTimingConstraint as _CorePinTimingConstraint,
        PyPinRef as _CorePinRef,
        analyze_advanced_constraints as _core_analyze_advanced_constraints,
        analyze_timing as _core_analyze_timing,
        analyze_ac_bias as _core_analyze_ac_bias,
        optimize_ac_bias as _core_optimize_ac_bias,
        optimize_ac_bias_with_characterized_library as _core_optimize_ac_bias_with_characterized_library,
        optimize_design_with_characterized_library as _core_optimize_design_with_characterized_library,
        merge_characterized_library as _core_merge_characterized_library,
        PyPdk as _CorePdk,
        analyze_timing_statistical as _core_analyze_timing_statistical,
        characterize_compound_cell as _core_characterize_compound_cell,
        compile_layout as _core_compile_layout,
        compile_netlist as _core_compile_netlist,
        compile_plan as _core_compile_plan,
        check_equivalence as _core_check_equivalence,
        check_single_step_sequential_equivalence as _core_check_single_step_sequential_equivalence,
        check_bounded_sequential_equivalence as _core_check_bounded_sequential_equivalence,
        simulate_file as _core_simulate_file,
        simulate_text as _core_simulate_text,
        read_bench_file as _core_read_bench_file,
        read_bench_text as _core_read_bench_text,
        verify_layout as _core_verify_layout,
        version as core_version,
    )
except ImportError as relative_import_error:
    try:
        from _core import (  # type: ignore[attr-defined]
            Circuit,
            PyBlockedRegion as _CoreBlockedRegion,
            PyClockDomainConstraint as _CoreClockDomainConstraint,
            PyCrossingConstraint as _CoreCrossingConstraint,
            PyCompilePlan as _CoreCompilePlan,
            PyConnectionSpec as _CoreConnectionSpec,
            PyFixedNodePlacement as _CoreFixedNodePlacement,
            PyNodeTimingConstraint as _CoreNodeTimingConstraint,
            PyPinTimingConstraint as _CorePinTimingConstraint,
            PyPinRef as _CorePinRef,
            analyze_advanced_constraints as _core_analyze_advanced_constraints,
            analyze_timing as _core_analyze_timing,
            analyze_ac_bias as _core_analyze_ac_bias,
            optimize_ac_bias as _core_optimize_ac_bias,
            optimize_ac_bias_with_characterized_library as _core_optimize_ac_bias_with_characterized_library,
            optimize_design_with_characterized_library as _core_optimize_design_with_characterized_library,
            merge_characterized_library as _core_merge_characterized_library,
            PyPdk as _CorePdk,
            analyze_timing_statistical as _core_analyze_timing_statistical,
            characterize_compound_cell as _core_characterize_compound_cell,
            compile_layout as _core_compile_layout,
            compile_netlist as _core_compile_netlist,
            compile_plan as _core_compile_plan,
            check_equivalence as _core_check_equivalence,
            check_single_step_sequential_equivalence as _core_check_single_step_sequential_equivalence,
            check_bounded_sequential_equivalence as _core_check_bounded_sequential_equivalence,
            simulate_file as _core_simulate_file,
            simulate_text as _core_simulate_text,
            read_bench_file as _core_read_bench_file,
            read_bench_text as _core_read_bench_text,
            verify_layout as _core_verify_layout,
            version as core_version,
        )
    except ImportError as absolute_import_error:
        # The extension may be unavailable before maturin develop is run.
        _CORE_IMPORT_ERROR = (
            f"relative import failed: {relative_import_error}; "
            f"absolute import failed: {absolute_import_error}"
        )
        core_version = lambda: "unavailable"
        _CoreBlockedRegion = None
        _CoreClockDomainConstraint = None
        _CoreCrossingConstraint = None
        _CoreCompilePlan = None
        _CoreConnectionSpec = None
        _CoreFixedNodePlacement = None
        _CoreNodeTimingConstraint = None
        _CorePinTimingConstraint = None
        _CorePinRef = None
        _core_analyze_advanced_constraints = None
        _core_analyze_timing = None
        _core_analyze_ac_bias = None
        _core_optimize_ac_bias = None
        _core_optimize_ac_bias_with_characterized_library = None
        _core_optimize_design_with_characterized_library = None
        _core_merge_characterized_library = None
        _CorePdk = None
        _core_analyze_timing_statistical = None
        _core_characterize_compound_cell = None
        _core_compile_layout = None
        _core_compile_netlist = None
        _core_compile_plan = None
        _core_check_equivalence = None
        _core_check_single_step_sequential_equivalence = None
        _core_check_bounded_sequential_equivalence = None
        _core_simulate_file = None
        _core_simulate_text = None
        _core_read_bench_file = None
        _core_read_bench_text = None
        _core_verify_layout = None

        class Circuit:  # type: ignore[no-redef]
            def __init__(self, name: str = "") -> None:
                self.name = name
                self._node_count = 0
                self._edge_count = 0
                self._nodes: list[tuple[int, str, str]] = []
                self._edges: list[tuple[tuple[int, int], tuple[int, int]]] = []

            def add_node(self, kind: str, name: str, logic_op: str | None = None) -> int:
                node_id = self._node_count
                self._node_count += 1
                self._nodes.append((node_id, kind, name))
                return node_id

            def connect(self, from_node: int, from_port: int, to_node: int, to_port: int) -> None:
                self._edge_count += 1
                self._edges.append(((from_node, from_port), (to_node, to_port)))

            def node_count(self) -> int:
                return self._node_count

            def edge_count(self) -> int:
                return self._edge_count

            def nodes(self) -> list[tuple[int, str, str]]:
                return list(self._nodes)

            def edges(self) -> list[tuple[tuple[int, int], tuple[int, int]]]:
                return list(self._edges)

            def to_json(self) -> str:
                return json.dumps(
                    {
                        "nodes": [
                            {"id": node_id, "kind": kind, "name": name}
                            for node_id, kind, name in self._nodes
                        ],
                        "edges": [
                            [
                                {"node": from_node, "port": from_port},
                                {"node": to_node, "port": to_port},
                            ]
                            for (from_node, from_port), (to_node, to_port) in self._edges
                        ],
                    },
                    indent=2,
                )

            @staticmethod
            def from_json(payload: str, name: str = "") -> "Circuit":
                data = json.loads(payload)
                circuit = Circuit(name)
                for node in data.get("nodes", []):
                    node_id = int(node["id"])
                    while circuit._node_count <= node_id:
                        circuit._node_count += 1
                    circuit._nodes.append((node_id, str(node["kind"]), str(node["name"])))
                circuit._node_count = len(circuit._nodes)
                for from_pin, to_pin in data.get("edges", []):
                    circuit._edges.append(
                        (
                            (int(from_pin["node"]), int(from_pin["port"])),
                            (int(to_pin["node"]), int(to_pin["port"])),
                        )
                    )
                circuit._edge_count = len(circuit._edges)
                return circuit


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
    timing_closure: "TimingClosureSummary"
    timing_closure_loop: "TimingClosureLoopReport"
    routed_nets: int
    total_route_length_um: float
    initial_total_detour_overhead_um: float
    total_detour_overhead_um: float
    detoured_routes: int
    detour_feedback_applied: bool
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
    primary_action: "TimingClosureAction | None"
    reduce_route_delay_actions: int
    relax_constraint_or_improve_library_timing_actions: int
    add_hold_padding_actions: int
    adjust_sfq_phase_or_pulse_window_actions: int
    actions: list["TimingClosureAction"]
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
    timing_arcs: list["TimingArcReport"]


@dataclass(frozen=True)
class StatisticalTimingAnalysisReport:
    worst_pessimistic_setup_slack_ps: float
    worst_pessimistic_hold_slack_ps: float
    analyzed_timing_arcs: int
    false_path_arcs: int
    setup_risk_violations: int
    hold_risk_violations: int
    sigma_multiplier: float
    timing_arcs: list["StatisticalTimingArcReport"]


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
    baseline_constraints: "AdvancedConstraintReport"
    optimized_constraints: "AdvancedConstraintReport"
    characterized_cells_merged: int
    library_optimization_score: float


@dataclass(frozen=True)
class LibraryAwareDesignOptimizationReport:
    ac_bias: AcBiasOptimizationReport
    baseline_statistical: StatisticalTimingAnalysisReport
    optimized_statistical: StatisticalTimingAnalysisReport
    baseline_constraints: "AdvancedConstraintReport"
    optimized_constraints: "AdvancedConstraintReport"
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


class Pdk:
    def __init__(self, core) -> None:
        if core is None:
            raise ValueError("Pdk core object must not be None")
        self._core = core

    def __repr__(self) -> str:
        version = self.cell_library_version
        version_suffix = "" if version is None else f", version={version!r}"
        return f"Pdk(name={self.name!r}, cell_library={self.cell_library_name!r}{version_suffix})"

    @classmethod
    def minimal(cls, name: str = "py-minimal-pdk") -> "Pdk":
        """Create the built-in minimal PDK."""
        _require_core_extension("Pdk.minimal(...)", _CorePdk)
        return cls(_CorePdk.minimal(name))

    @classmethod
    def from_json(cls, payload: str) -> "Pdk":
        """Load a PDK from its JSON representation."""
        _require_core_extension("Pdk.from_json(...)", _CorePdk)
        return cls(_CorePdk.from_json(payload))

    @property
    def name(self) -> str:
        """PDK name."""
        return self._core.name

    def to_json(self) -> str:
        """Serialize this PDK to JSON."""
        return self._core.to_json()

    @property
    def cell_library_name(self) -> str:
        """Name of the active cell library."""
        return self._core.cell_library_name

    @property
    def cell_library_version(self) -> str | None:
        """Version of the active cell library, when available."""
        return self._core.cell_library_version

    @property
    def cell_library_source(self) -> str | None:
        """Source label or path for the active cell library, when available."""
        return self._core.cell_library_source

    def cell_library_metadata(self) -> CellLibraryMetadata:
        """Return metadata for the active cell library."""
        return _cell_library_metadata_from_core(self._core.cell_library_metadata())

    def cell_library_kinds(self) -> list[str]:
        """Return the cell kinds represented by the active library."""
        return list(self._core.cell_library_kinds())

    def cell_library_entries(self) -> list[CellLibraryEntry]:
        """Return all active cell library entries."""
        return [_cell_library_entry_from_core(entry) for entry in self._core.cell_library_entries()]

    def cell_library_summary(self) -> CellLibrarySummary:
        """Return a compact summary of active cell library coverage."""
        return _cell_library_summary_from_core(self._core.cell_library_summary())

    def cell_library_entries_by_kind(self, kind: str) -> list[CellLibraryEntry]:
        """Return active cell library entries matching ``kind``."""
        return [
            _cell_library_entry_from_core(entry)
            for entry in self._core.cell_library_entries_by_kind(kind)
        ]

    def cell_library_entry(self, cell_name: str) -> CellLibraryEntry | None:
        """Return a named cell library entry, if present."""
        entry = self._core.cell_library_entry(cell_name)
        return None if entry is None else _cell_library_entry_from_core(entry)

    def merge_characterized_library_json(self, serialized_entry: str) -> "Pdk":
        """Return a PDK with one characterized cell entry merged in."""
        return Pdk(self._core.merge_characterized_library_json(serialized_entry))

    def merge_characterized_library_entries(self, serialized_entries: list[str]) -> "Pdk":
        """Return a PDK with multiple characterized cell entries merged in."""
        return Pdk(self._core.merge_characterized_library_entries(serialized_entries))


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
    delay_details: list["SimulationDelayDetail"]
    measurement_details: list["SimulationMeasurementDetail"]
    measurement_warnings: list["SimulationMeasurementWarning"]
    violation_details: list["SimulationViolationDetail"]
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


def core_available() -> bool:
    """Return whether the compiled ``rflux._core`` extension is importable."""
    return core_version() != "unavailable"


def core_status() -> CoreStatus:
    """Return diagnostic information for the compiled ``rflux._core`` extension."""
    extension_path = None
    if core_available():
        spec = importlib.util.find_spec("rflux._core")
        extension_path = None if spec is None else spec.origin
    return CoreStatus(
        available=core_available(),
        version=core_version(),
        extension_path=extension_path,
        import_error=_CORE_IMPORT_ERROR,
    )


def _require_core_extension(api_name: str, binding) -> None:
    if binding is None:
        details = "" if _CORE_IMPORT_ERROR is None else f" Import error: {_CORE_IMPORT_ERROR}"
        raise RfluxCoreUnavailableError(
            f"{api_name} requires the compiled rflux._core extension; run `uv run maturin develop -m crates/py/Cargo.toml`."
            f"{details}"
        )


def _validate_simulation_mode(simulation_mode: str) -> None:
    if simulation_mode not in SIMULATION_MODES:
        allowed = ", ".join(SIMULATION_MODES)
        raise ValueError(f"unknown simulation mode: {simulation_mode}; expected one of: {allowed}")


def compile(circuit: Circuit, plan: CompilePlan | None = None) -> Circuit:
    """Apply a synthesis compile plan to ``circuit`` and return the same circuit.

    This is the lightweight in-memory facade. Use :func:`compile_netlist` when
    you need the synthesis summary, or :func:`compile_layout` when you need
    placement, routing, timing, and verification reports.
    """
    return compile_plan(circuit, plan or CompilePlan())


def read_bench_file(file_path: str | Path, name: str | None = None) -> Circuit:
    """Read a Berkeley BENCH file into a :class:`Circuit`."""
    _require_core_extension("read_bench_file(...)", _core_read_bench_file)
    return _core_read_bench_file(str(file_path), name)


def read_bench_text(text: str, name: str | None = None) -> Circuit:
    """Parse Berkeley BENCH text into a :class:`Circuit`."""
    _require_core_extension("read_bench_text(...)", _core_read_bench_text)
    return _core_read_bench_text(text, name)


def simulate_text(
    deck_text: str,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> SimulationReport:
    """Simulate a SPICE-like deck string and return a structured report."""
    _validate_simulation_mode(simulation_mode)
    _require_core_extension("simulate_text(...)", _core_simulate_text)

    report = _core_simulate_text(deck_text, simulation_mode, external_command)
    return SimulationReport(
        backend=report.backend,
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


def simulate_file(
    file_path: str | Path,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> SimulationReport:
    """Simulate a SPICE-like deck file and return a structured report."""
    _validate_simulation_mode(simulation_mode)
    _require_core_extension("simulate_file(...)", _core_simulate_file)

    report = _core_simulate_file(str(file_path), simulation_mode, external_command)
    return SimulationReport(
        backend=report.backend,
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


def check_equivalence(
    lhs: Circuit,
    rhs: Circuit,
) -> CombinationalEquivalenceReport:
    """Check combinational equivalence between two circuits."""
    _require_core_extension("check_equivalence(...)", _core_check_equivalence)

    report = _core_check_equivalence(lhs, rhs)
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


def check_single_step_sequential_equivalence(
    lhs: Circuit,
    rhs: Circuit,
) -> SingleStepSequentialEquivalenceReport:
    """Check one-step sequential equivalence between two circuits."""
    _require_core_extension(
        "check_single_step_sequential_equivalence(...)",
        _core_check_single_step_sequential_equivalence,
    )

    report = _core_check_single_step_sequential_equivalence(lhs, rhs)
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


def check_bounded_sequential_equivalence(
    lhs: Circuit,
    rhs: Circuit,
    depth: int = 2,
) -> BoundedSequentialEquivalenceReport:
    """Check state-unrolled bounded sequential equivalence up to ``depth`` steps."""
    _require_core_extension(
        "check_bounded_sequential_equivalence(...)",
        _core_check_bounded_sequential_equivalence,
    )

    report = _core_check_bounded_sequential_equivalence(lhs, rhs, depth)
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


def _to_core_compile_plan(plan: CompilePlan):
    if _CoreCompilePlan is None or _CoreConnectionSpec is None or _CorePinRef is None:
        return None

    return _CoreCompilePlan(
        connections=[
            _CoreConnectionSpec(
                _CorePinRef(item.from_pin.node, item.from_pin.port),
                _CorePinRef(item.to_pin.node, item.to_pin.port),
            )
            for item in plan.connections
        ],
        balance_strategy=plan.balance_strategy.value,
        balancing_sources=[
            _CorePinRef(pin.node, pin.port) for pin in plan.balancing_sources
        ],
    )


def _to_core_fixed_nodes(fixed_nodes: list[FixedNodePlacement] | None):
    if _CoreFixedNodePlacement is None or fixed_nodes is None:
        return None

    return [
        _CoreFixedNodePlacement(item.node, item.x_um, item.y_um)
        for item in fixed_nodes
    ]


def _to_core_blocked_regions(blocked_regions: list[BlockedRegion] | None):
    if _CoreBlockedRegion is None or blocked_regions is None:
        return None

    return [
        _CoreBlockedRegion(
            item.min_x_um,
            item.max_x_um,
            item.min_y_um,
            item.max_y_um,
        )
        for item in blocked_regions
    ]


def _to_core_timing_constraints(timing_constraints: list[NodeTimingConstraint] | None):
    if _CoreNodeTimingConstraint is None or timing_constraints is None:
        return None

    return [
        _CoreNodeTimingConstraint(
            item.node,
            item.input_arrival_ps,
            item.required_ps,
            item.clock_domain,
        )
        for item in timing_constraints
    ]


def _to_core_clock_domains(clock_domains: list[ClockDomainConstraint] | None):
    if _CoreClockDomainConstraint is None or clock_domains is None:
        return None

    return [
        _CoreClockDomainConstraint(item.id, item.period_ps)
        for item in clock_domains
    ]


def _to_core_pin_timing_constraints(pin_timing_constraints: list[PinTimingConstraint] | None):
    if _CorePinTimingConstraint is None or pin_timing_constraints is None or _CorePinRef is None:
        return None

    return [
        _CorePinTimingConstraint(
            _CorePinRef(item.pin.node, item.pin.port),
            item.input_arrival_ps,
            item.required_ps,
            item.clock_domain,
        )
        for item in pin_timing_constraints
    ]


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


def _to_core_crossing_constraints(crossing_constraints: list[CrossingConstraint] | None):
    if _CoreCrossingConstraint is None or crossing_constraints is None:
        return None

    return [
        _CoreCrossingConstraint(
            item.from_domain,
            item.to_domain,
            item.kind,
            item.value_ps,
            item.cycles,
        )
        for item in crossing_constraints
    ]


def compile_plan_report(circuit: Circuit, plan: CompilePlan) -> CompileReport:
    """Apply a compile plan and return the synthesis edit counts."""
    _require_core_extension("compile_plan_report(...)", _core_compile_plan)
    core_plan = _to_core_compile_plan(plan)
    report = _core_compile_plan(circuit, core_plan)
    return CompileReport(
        connections_applied=report.connections_applied,
        splitters_inserted=report.splitters_inserted,
        balancing_dffs_inserted=report.balancing_dffs_inserted,
    )


def compile_plan(circuit: Circuit, plan: CompilePlan) -> Circuit:
    """Apply a compile plan in place and return ``circuit``."""
    _ = compile_plan_report(circuit, plan)
    return circuit


def compile_netlist(circuit: Circuit, plan: CompilePlan | None = None) -> SynthesisReport:
    """Run synthesis-level compilation and return a netlist summary."""
    _require_core_extension("compile_netlist(...)", _core_compile_netlist)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_compile_netlist(circuit, core_plan)
    return SynthesisReport(
        connections_applied=report.connections_applied,
        splitters_inserted=report.splitters_inserted,
        balancing_dffs_inserted=report.balancing_dffs_inserted,
        bool_gate_count_before=report.bool_gate_count_before,
        bool_gate_count_after=report.bool_gate_count_after,
        mapped_nodes=report.mapped_nodes,
        total_area_um2=report.total_area_um2,
        path_balance_insertions=report.path_balance_insertions,
        bool_opt_compatible=report.bool_opt_compatible,
        node_count=report.node_count,
        edge_count=report.edge_count,
    )


def compile_layout(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    min_hold_jtl_length_um: float | None = None,
    prefer_ptl_from_length_um: float | None = None,
    detour_margin_um: float | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
) -> LayoutReport:
    """Run synthesis, placement, routing, timing, and layout verification."""
    _require_core_extension("compile_layout(...)", _core_compile_layout)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_compile_layout(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
        min_hold_jtl_length_um,
        prefer_ptl_from_length_um,
        detour_margin_um,
        characterized_library_json,
        characterized_library_entries,
    )
    return LayoutReport(
        connections_applied=report.connections_applied,
        splitters_inserted=report.splitters_inserted,
        balancing_dffs_inserted=report.balancing_dffs_inserted,
        mapped_nodes=report.mapped_nodes,
        total_area_um2=report.total_area_um2,
        bool_opt_compatible=report.bool_opt_compatible,
        placed_nodes=report.placed_nodes,
        placement_width_um=report.placement_width_um,
        placement_height_um=report.placement_height_um,
        clock_sinks=report.clock_sinks,
        clock_buffers=report.clock_buffers,
        clock_phase_count=report.clock_phase_count,
        initial_hold_violations=report.initial_hold_violations,
        final_hold_violations=report.final_hold_violations,
        hold_fix_applied=report.hold_fix_applied,
        worst_setup_slack_ps=report.worst_setup_slack_ps,
        worst_hold_slack_ps=report.worst_hold_slack_ps,
        critical_path_delay_ps=report.critical_path_delay_ps,
        analyzed_timing_arcs=report.analyzed_timing_arcs,
        false_path_arcs=report.false_path_arcs,
        setup_violations=report.setup_violations,
        capture_window_violations=report.capture_window_violations,
        timing_closure=_timing_closure_from_core(report.timing_closure),
        timing_closure_loop=_timing_closure_loop_from_core(report.timing_closure_loop),
        routed_nets=report.routed_nets,
        total_route_length_um=report.total_route_length_um,
        initial_total_detour_overhead_um=report.initial_total_detour_overhead_um,
        total_detour_overhead_um=report.total_detour_overhead_um,
        detoured_routes=report.detoured_routes,
        detour_feedback_applied=report.detour_feedback_applied,
        jtl_routes=report.jtl_routes,
        ptl_routes=report.ptl_routes,
        node_count=report.node_count,
        edge_count=report.edge_count,
    )


def _timing_closure_loop_from_core(loop_report) -> TimingClosureLoopReport:
    return TimingClosureLoopReport(
        detour_feedback_attempted=loop_report.detour_feedback_attempted,
        detour_feedback_applied=loop_report.detour_feedback_applied,
        initial_total_detour_overhead_um=loop_report.initial_total_detour_overhead_um,
        final_total_detour_overhead_um=loop_report.final_total_detour_overhead_um,
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
        actions=[
            _timing_closure_action_from_core(action)
            for action in closure.actions
        ],
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


def analyze_timing(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
) -> TimingAnalysisReport:
    """Run static timing analysis and return per-arc closure diagnostics."""
    _require_core_extension("analyze_timing(...)", _core_analyze_timing)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_analyze_timing(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
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
        closure=_timing_closure_from_core(report.closure),
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


def analyze_timing_statistical(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
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
    _require_core_extension("analyze_timing_statistical(...)", _core_analyze_timing_statistical)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_analyze_timing_statistical(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
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
    return _statistical_timing_report_from_core(report)


def analyze_ac_bias(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
) -> AcBiasReport:
    """Analyze AC-bias routing feasibility and timing guardband metrics."""
    _require_core_extension("analyze_ac_bias(...)", _core_analyze_ac_bias)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_analyze_ac_bias(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
    )
    return AcBiasReport(
        routed_nets=report.routed_nets,
        jtl_carrier_candidates=report.jtl_carrier_candidates,
        ptl_coupling_risk_routes=report.ptl_coupling_risk_routes,
        clock_sink_count=report.clock_sink_count,
        estimated_static_power_savings_uw=report.estimated_static_power_savings_uw,
        estimated_area_overhead_ratio=report.estimated_area_overhead_ratio,
        estimated_frequency_derate_ratio=report.estimated_frequency_derate_ratio,
        worst_setup_slack_ps=report.worst_setup_slack_ps,
        worst_hold_slack_ps=report.worst_hold_slack_ps,
        timing_guardband_score=report.timing_guardband_score,
        feasibility_score=report.feasibility_score,
        optimization_score=report.optimization_score,
    )


def optimize_ac_bias(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
) -> AcBiasOptimizationReport:
    """Search AC-bias routing knobs and return before/after metrics."""
    _require_core_extension("optimize_ac_bias(...)", _core_optimize_ac_bias)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_optimize_ac_bias(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
    )
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


def verify_layout(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> VerificationReport:
    """Run structural, routing, and simulation-backed layout checks."""
    _validate_simulation_mode(simulation_mode)
    _require_core_extension("verify_layout(...)", _core_verify_layout)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_verify_layout(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
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


def characterize_compound_cell(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    cell_name: str = "compound_cell",
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> CompoundCellCharacterizationReport:
    """Characterize a circuit as a reusable compound cell library entry."""
    _validate_simulation_mode(simulation_mode)

    _require_core_extension("characterize_compound_cell(...)", _core_characterize_compound_cell)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_characterize_compound_cell(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
        cell_name,
        simulation_mode,
        external_command,
    )
    return CompoundCellCharacterizationReport(
        cell_name=report.cell_name,
        node_count=report.node_count,
        edge_count=report.edge_count,
        mapped_nodes=report.mapped_nodes,
        total_area_um2=report.total_area_um2,
        derived_intrinsic_delay_ps=report.derived_intrinsic_delay_ps,
        derived_setup_ps=report.derived_setup_ps,
        derived_hold_ps=report.derived_hold_ps,
        generated_cell_kind=report.generated_cell_kind,
        generated_pipeline_stages=report.generated_pipeline_stages,
        generated_library_json=report.generated_library_json,
        simulated_delay_ps=report.simulated_delay_ps,
        simulation_backend=report.simulation_backend,
        generated_deck_lines=report.generated_deck_lines,
        generated_deck_path=report.generated_deck_path,
        waveform_path=report.waveform_path,
        reported_violations=report.reported_violations,
    )


def analyze_advanced_constraints(
    circuit: Circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
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
    _require_core_extension("analyze_advanced_constraints(...)", _core_analyze_advanced_constraints)
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_analyze_advanced_constraints(
        circuit,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
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


def merge_characterized_library(
    serialized_entries: list[str],
    base_name: str = "py-minimal-pdk",
) -> str:
    """Merge characterized library JSON entries into a PDK JSON document."""
    _require_core_extension("merge_characterized_library(...)", _core_merge_characterized_library)
    return _core_merge_characterized_library(serialized_entries, base_name)


def optimize_ac_bias_with_characterized_library(
    circuit: Circuit,
    characterized_library_entries: list[str],
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    max_estimated_thermal_load_uw: float = 8.0,
    max_estimated_mechanical_stress_score: float = 0.75,
    max_jtl_density_per_100um: float = 8.0,
    max_detour_overhead_ratio: float = 0.35,
    max_ptl_coupling_ratio: float = 0.65,
) -> LibraryAwareAcBiasOptimizationReport:
    """Optimize AC-bias settings using characterized cell library feedback."""
    _require_core_extension(
        "optimize_ac_bias_with_characterized_library(...)",
        _core_optimize_ac_bias_with_characterized_library,
    )
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_optimize_ac_bias_with_characterized_library(
        circuit,
        characterized_library_entries,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
        max_estimated_thermal_load_uw,
        max_estimated_mechanical_stress_score,
        max_jtl_density_per_100um,
        max_detour_overhead_ratio,
        max_ptl_coupling_ratio,
    )
    return LibraryAwareAcBiasOptimizationReport(
        ac_bias=_ac_bias_optimization_report_from_core(report.ac_bias),
        baseline_constraints=_advanced_constraint_report_from_core(report.baseline_constraints),
        optimized_constraints=_advanced_constraint_report_from_core(report.optimized_constraints),
        characterized_cells_merged=report.characterized_cells_merged,
        library_optimization_score=report.library_optimization_score,
    )


def optimize_design_with_characterized_library(
    circuit: Circuit,
    characterized_library_entries: list[str],
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list[ClockDomainConstraint] | None = None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    max_estimated_thermal_load_uw: float = 8.0,
    max_estimated_mechanical_stress_score: float = 0.75,
    max_jtl_density_per_100um: float = 8.0,
    max_detour_overhead_ratio: float = 0.35,
    max_ptl_coupling_ratio: float = 0.65,
    cell_delay_sigma_ratio: float = 0.05,
    wire_delay_sigma_ratio: float = 0.05,
    global_cell_delay_sigma_ratio: float = 0.0,
    global_wire_delay_sigma_ratio: float = 0.0,
    clock_uncertainty_sigma_ps: float = 0.0,
    cross_domain_uncertainty_sigma_ps: float = 0.0,
    max_delay_cross_domain_uncertainty_sigma_ps: float = 0.0,
    multicycle_cross_domain_uncertainty_sigma_ps: float = 0.0,
    sigma_multiplier: float = 3.0,
) -> LibraryAwareDesignOptimizationReport:
    """Optimize routing, constraints, and statistical timing using library feedback."""
    _require_core_extension(
        "optimize_design_with_characterized_library(...)",
        _core_optimize_design_with_characterized_library,
    )
    core_plan = _to_core_compile_plan(plan) if plan is not None else None
    report = _core_optimize_design_with_characterized_library(
        circuit,
        characterized_library_entries,
        core_plan,
        _to_core_fixed_nodes(fixed_nodes),
        _to_core_blocked_regions(blocked_regions),
        _to_core_timing_constraints(timing_constraints),
        _to_core_pin_timing_constraints(pin_timing_constraints),
        _to_core_clock_domains(clock_domains),
        _to_core_crossing_constraints(crossing_constraints),
        max_estimated_thermal_load_uw,
        max_estimated_mechanical_stress_score,
        max_jtl_density_per_100um,
        max_detour_overhead_ratio,
        max_ptl_coupling_ratio,
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
    return LibraryAwareDesignOptimizationReport(
        ac_bias=_ac_bias_optimization_report_from_core(report.ac_bias),
        baseline_statistical=_statistical_timing_report_from_core(report.baseline_statistical),
        optimized_statistical=_statistical_timing_report_from_core(report.optimized_statistical),
        baseline_constraints=_advanced_constraint_report_from_core(report.baseline_constraints),
        optimized_constraints=_advanced_constraint_report_from_core(report.optimized_constraints),
        characterized_cells_merged=report.characterized_cells_merged,
        design_optimization_score=report.design_optimization_score,
        baseline_cell_delay_sigma_ratio=report.baseline_cell_delay_sigma_ratio,
        optimized_cell_delay_sigma_ratio=report.optimized_cell_delay_sigma_ratio,
        baseline_sigma_multiplier=report.baseline_sigma_multiplier,
        optimized_sigma_multiplier=report.optimized_sigma_multiplier,
        baseline_placement_halo_scale=report.baseline_placement_halo_scale,
        optimized_placement_halo_scale=report.optimized_placement_halo_scale,
        placement_candidates_evaluated=report.placement_candidates_evaluated,
        statistical_candidates_evaluated=report.statistical_candidates_evaluated,
    )


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
    "verify_layout",
]
