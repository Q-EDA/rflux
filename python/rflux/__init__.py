from dataclasses import dataclass
from pathlib import Path
import importlib.util
import json
import sys

from ._version import __version__


FLOW_CONFIG_KIND = "rflux_flow_config"
FLOW_CONFIG_SCHEMA_VERSION = 1
DEFAULT_CLOCK_PERIOD_PS = 120.0
DEFAULT_INPUT_ARRIVAL_PS = 0.0
DEFAULT_SFQ_PHASE_COUNT = 1
DEFAULT_SFQ_PULSE_WINDOW_PS = 4.0
DEFAULT_MIN_HOLD_JTL_LENGTH_UM = 0.0

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
        analyze_timing_corners as _core_analyze_timing_corners,
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
        is_supported_external_command as _core_is_supported_external_command,
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
            analyze_timing_corners as _core_analyze_timing_corners,
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
            is_supported_external_command as _core_is_supported_external_command,
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
        _core_analyze_timing_corners = None
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
        _core_is_supported_external_command = None
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


from ._conversions import (
    _ac_bias_optimization_report_from_core,
    _advanced_constraint_report_from_core,
    _cell_library_entry_from_core,
    _cell_library_metadata_from_core,
    _cell_library_summary_from_core,
    _single_step_sequential_report_from_core,
    _statistical_timing_report_from_core,
    _timing_closure_action_from_core,
    _timing_closure_from_core,
    _timing_closure_loop_from_core,
    _timing_corner_analysis_from_core,
)
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


from . import flow, pdk, sim, timing, verify

Pdk = pdk.Pdk
compile = flow.compile
compile_plan_report = flow.compile_plan_report
compile_plan = flow.compile_plan
compile_netlist = flow.compile_netlist
simulate_text = sim.simulate_text
simulate_file = sim.simulate_file
is_supported_external_command = sim.is_supported_external_command
analyze_timing = timing.analyze_timing
analyze_timing_corners = timing.analyze_timing_corners
analyze_timing_statistical = timing.analyze_timing_statistical
analyze_advanced_constraints = timing.analyze_advanced_constraints
compile_layout = flow.compile_layout
analyze_ac_bias = flow.analyze_ac_bias
optimize_ac_bias = flow.optimize_ac_bias
characterize_compound_cell = flow.characterize_compound_cell
merge_characterized_library = flow.merge_characterized_library
optimize_ac_bias_with_characterized_library = flow.optimize_ac_bias_with_characterized_library
optimize_design_with_characterized_library = flow.optimize_design_with_characterized_library
check_equivalence = verify.check_equivalence
check_single_step_sequential_equivalence = verify.check_single_step_sequential_equivalence
check_bounded_sequential_equivalence = verify.check_bounded_sequential_equivalence
verify_layout = verify.verify_layout


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
