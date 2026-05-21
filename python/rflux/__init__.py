from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
import json
import sys

from ._version import __version__


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
        simulate_file as _core_simulate_file,
        simulate_text as _core_simulate_text,
        verify_layout as _core_verify_layout,
        version as core_version,
    )
except ImportError:
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
            simulate_file as _core_simulate_file,
            simulate_text as _core_simulate_text,
            verify_layout as _core_verify_layout,
            version as core_version,
        )
    except ImportError:
        # The extension may be unavailable before maturin develop is run.
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
        _core_simulate_file = None
        _core_simulate_text = None
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
class TimingAnalysisReport:
    worst_setup_slack_ps: float
    worst_hold_slack_ps: float
    critical_path_delay_ps: float
    analyzed_timing_arcs: int
    false_path_arcs: int
    setup_violations: int
    hold_violations: int
    detour_feedback_applied: bool
    hold_fix_applied: bool
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


class Pdk:
    def __init__(self, core) -> None:
        self._core = core

    @classmethod
    def minimal(cls, name: str = "py-minimal-pdk") -> "Pdk":
        if _CorePdk is None:
            raise RuntimeError("rflux extension is unavailable; run `uv run maturin develop`")
        return cls(_CorePdk.minimal(name))

    @classmethod
    def from_json(cls, payload: str) -> "Pdk":
        if _CorePdk is None:
            raise RuntimeError("rflux extension is unavailable; run `uv run maturin develop`")
        return cls(_CorePdk.from_json(payload))

    @property
    def name(self) -> str:
        return self._core.name

    def to_json(self) -> str:
        return self._core.to_json()

    def merge_characterized_library_json(self, serialized_entry: str) -> "Pdk":
        return Pdk(self._core.merge_characterized_library_json(serialized_entry))

    def merge_characterized_library_entries(self, serialized_entries: list[str]) -> "Pdk":
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
    violation_details: list["SimulationViolationDetail"]
    external_status_code: int | None
    external_result: str | None


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
    violation_details: list[SimulationViolationDetail]
    external_status_code: int | None
    external_result: str | None


def compile(circuit: Circuit) -> Circuit:
    # Phase-1 placeholder API. The backend wiring will be bound to Rust later.
    return circuit


def simulate_text(
    deck_text: str,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> SimulationReport:
    if simulation_mode not in {"auto", "event_only", "external_josim", "internal_transient"}:
        raise ValueError(f"unknown simulation mode: {simulation_mode}")
    if _core_simulate_text is None:
        raise RuntimeError("rflux extension is unavailable; run `uv run maturin develop`")

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
    file_path: str,
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> SimulationReport:
    if simulation_mode not in {"auto", "event_only", "external_josim", "internal_transient"}:
        raise ValueError(f"unknown simulation mode: {simulation_mode}")
    if _core_simulate_file is None:
        raise RuntimeError("rflux extension is unavailable; run `uv run maturin develop`")

    report = _core_simulate_file(file_path, simulation_mode, external_command)
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


def _effective_required_ps(
    timing_constraints: list[NodeTimingConstraint] | None,
    pin_timing_constraints: list[PinTimingConstraint] | None,
    clock_domains: list[ClockDomainConstraint] | None,
    crossing_constraints: list[CrossingConstraint] | None = None,
    default_period_ps: float = 120.0,
) -> float:
    domain_periods = {domain.id: domain.period_ps for domain in clock_domains or []}
    candidates = [default_period_ps]
    for constraint in crossing_constraints or []:
        if constraint.kind == "max_delay" and constraint.value_ps is not None:
            candidates.append(constraint.value_ps)
        elif constraint.kind == "multicycle" and constraint.cycles is not None:
            period_ps = domain_periods.get(constraint.to_domain, default_period_ps)
            candidates.append(period_ps * max(constraint.cycles, 1))
    for constraint in pin_timing_constraints or []:
        if constraint.required_ps is not None:
            candidates.append(constraint.required_ps)
        elif constraint.clock_domain is not None and constraint.clock_domain in domain_periods:
            candidates.append(domain_periods[constraint.clock_domain])
    for constraint in timing_constraints or []:
        if constraint.required_ps is not None:
            candidates.append(constraint.required_ps)
        elif constraint.clock_domain is not None and constraint.clock_domain in domain_periods:
            candidates.append(domain_periods[constraint.clock_domain])
    return min(candidates)


def _fallback_false_path_arcs(
    circuit: Circuit,
    timing_constraints: list[NodeTimingConstraint] | None,
    pin_timing_constraints: list[PinTimingConstraint] | None,
    crossing_constraints: list[CrossingConstraint] | None,
) -> int:
    false_path_pairs = {
        (constraint.from_domain, constraint.to_domain)
        for constraint in crossing_constraints or []
        if constraint.kind == "false_path"
    }
    if not false_path_pairs:
        return 0

    pin_domains = {
        (constraint.pin.node, constraint.pin.port): constraint.clock_domain
        for constraint in pin_timing_constraints or []
        if constraint.clock_domain is not None
    }
    node_domains = {
        constraint.node: constraint.clock_domain
        for constraint in timing_constraints or []
        if constraint.clock_domain is not None
    }

    count = 0
    for (from_node, from_port), (to_node, to_port) in circuit.edges():
        from_domain = pin_domains.get((from_node, from_port), node_domains.get(from_node))
        to_domain = pin_domains.get((to_node, to_port), node_domains.get(to_node))
        if from_domain is None or to_domain is None:
            continue
        if (from_domain, to_domain) in false_path_pairs and from_domain != to_domain:
            count += 1
    return count


def _fallback_timing_arcs(
    circuit: Circuit,
    timing_constraints: list[NodeTimingConstraint] | None,
    pin_timing_constraints: list[PinTimingConstraint] | None,
    clock_domains: list[ClockDomainConstraint] | None,
    crossing_constraints: list[CrossingConstraint] | None,
) -> list[TimingArcReport]:
    domain_periods = {domain.id: domain.period_ps for domain in clock_domains or []}
    pin_domains = {
        (constraint.pin.node, constraint.pin.port): constraint.clock_domain
        for constraint in pin_timing_constraints or []
        if constraint.clock_domain is not None
    }
    node_domains = {
        constraint.node: constraint.clock_domain
        for constraint in timing_constraints or []
        if constraint.clock_domain is not None
    }
    false_path_pairs = {
        (constraint.from_domain, constraint.to_domain)
        for constraint in crossing_constraints or []
        if constraint.kind == "false_path"
    }

    arcs: list[TimingArcReport] = []
    for index, ((from_node, from_port), (to_node, to_port)) in enumerate(circuit.edges(), start=1):
        from_domain = pin_domains.get((from_node, from_port), node_domains.get(from_node))
        to_domain = pin_domains.get((to_node, to_port), node_domains.get(to_node))
        is_false_path = (
            from_domain is not None
            and to_domain is not None
            and from_domain != to_domain
            and (from_domain, to_domain) in false_path_pairs
        )
        arrival_ps = float(index * 18)
        required_ps = float("inf") if is_false_path else _effective_required_ps(
            timing_constraints,
            pin_timing_constraints,
            clock_domains,
            crossing_constraints,
        )
        setup_slack_ps = float("inf") if is_false_path else required_ps - arrival_ps
        hold_slack_ps = 0.0
        arcs.append(
            TimingArcReport(
                from_pin=PinRef(node=from_node, port=from_port),
                to_pin=PinRef(node=to_node, port=to_port),
                is_false_path=is_false_path,
                route_mode="jtl",
                route_length_um=40.0,
                from_domain=from_domain,
                to_domain=to_domain,
                arrival_ps=arrival_ps,
                required_ps=required_ps,
                setup_slack_ps=setup_slack_ps,
                hold_slack_ps=hold_slack_ps,
            )
        )
    return arcs


def _fallback_compile_report(plan: CompilePlan) -> CompileReport:
    splitters_inserted = 0
    seen_sources: set[PinRef] = set()
    sink_inputs: dict[int, list[tuple[PinRef, int]]] = {}

    for connection in plan.connections:
        if connection.from_pin in seen_sources:
            splitters_inserted += 1
        else:
            seen_sources.add(connection.from_pin)

        sink_inputs.setdefault(connection.to_pin.node, []).append(
            (connection.from_pin, _source_level(plan, connection.from_pin))
        )

    balancing_dffs_inserted = 0
    if plan.balance_strategy is BalanceStrategy.EXPLICIT:
        balancing_dffs_inserted = len(plan.balancing_sources)
    elif plan.balance_strategy is BalanceStrategy.ALL_CONNECTED_SOURCES:
        balancing_dffs_inserted = len({connection.from_pin for connection in plan.connections})
    elif plan.balance_strategy is BalanceStrategy.BY_SINK_LEVEL:
        for incoming in sink_inputs.values():
            if len(incoming) < 2:
                continue
            max_level = max(level for _, level in incoming)
            balancing_dffs_inserted += sum(max_level - level for _, level in incoming)

    return CompileReport(
        connections_applied=len(plan.connections),
        splitters_inserted=splitters_inserted,
        balancing_dffs_inserted=balancing_dffs_inserted,
    )


def _source_level(plan: CompilePlan, source: PinRef) -> int:
    level_by_node: dict[int, int] = {}
    for connection in plan.connections:
        from_level = level_by_node.get(connection.from_pin.node, 0)
        candidate = from_level + 1
        current = level_by_node.get(connection.to_pin.node, 0)
        if candidate > current:
            level_by_node[connection.to_pin.node] = candidate
    return level_by_node.get(source.node, 0)


def compile_plan_report(circuit: Circuit, plan: CompilePlan) -> CompileReport:
    if _core_compile_plan is not None:
        core_plan = _to_core_compile_plan(plan)
        report = _core_compile_plan(circuit, core_plan)
        return CompileReport(
            connections_applied=report.connections_applied,
            splitters_inserted=report.splitters_inserted,
            balancing_dffs_inserted=report.balancing_dffs_inserted,
        )

    report = _fallback_compile_report(plan)
    for connection in plan.connections:
        circuit._node_count = max(  # type: ignore[attr-defined]
            circuit._node_count,  # type: ignore[attr-defined]
            connection.from_pin.node + 1,
            connection.to_pin.node + 1,
        )
        circuit._edges.append(  # type: ignore[attr-defined]
            (
                (connection.from_pin.node, connection.from_pin.port),
                (connection.to_pin.node, connection.to_pin.port),
            )
        )
        if not any(node_id == connection.from_pin.node for node_id, _, _ in circuit._nodes):  # type: ignore[attr-defined]
            circuit._nodes.append((connection.from_pin.node, "cell_instance", f"py_node_{connection.from_pin.node}"))  # type: ignore[attr-defined]
        if not any(node_id == connection.to_pin.node for node_id, _, _ in circuit._nodes):  # type: ignore[attr-defined]
            circuit._nodes.append((connection.to_pin.node, "cell_instance", f"py_node_{connection.to_pin.node}"))  # type: ignore[attr-defined]

    base_node_id = circuit._node_count  # type: ignore[attr-defined]
    for offset in range(report.splitters_inserted):
        circuit._nodes.append((base_node_id + offset, "splitter", f"auto_splitter_{offset}"))  # type: ignore[attr-defined]
    base_node_id += report.splitters_inserted
    for offset in range(report.balancing_dffs_inserted):
        circuit._nodes.append((base_node_id + offset, "dff", f"balance_dff_{offset}"))  # type: ignore[attr-defined]

    circuit._edge_count += (
        report.connections_applied + report.splitters_inserted + report.balancing_dffs_inserted
    )  # type: ignore[attr-defined]
    circuit._node_count += report.splitters_inserted + report.balancing_dffs_inserted  # type: ignore[attr-defined]
    return report


def compile_plan(circuit: Circuit, plan: CompilePlan) -> Circuit:
    _ = compile_plan_report(circuit, plan)
    return circuit


def compile_netlist(circuit: Circuit, plan: CompilePlan | None = None) -> SynthesisReport:
    if _core_compile_netlist is not None:
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

    compile_report = compile_plan_report(circuit, plan or CompilePlan())
    return SynthesisReport(
        connections_applied=compile_report.connections_applied,
        splitters_inserted=compile_report.splitters_inserted,
        balancing_dffs_inserted=compile_report.balancing_dffs_inserted,
        bool_gate_count_before=circuit.edge_count(),
        bool_gate_count_after=circuit.edge_count(),
        mapped_nodes=circuit.node_count(),
        total_area_um2=float(circuit.node_count()),
        path_balance_insertions=compile_report.balancing_dffs_inserted,
        bool_opt_compatible=True,
        node_count=circuit.node_count(),
        edge_count=circuit.edge_count(),
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
) -> LayoutReport:
    if _core_compile_layout is not None:
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

    synth_report = compile_netlist(circuit, plan)
    max_fixed_x_um = max((item.x_um for item in fixed_nodes or []), default=0.0)
    max_fixed_y_um = max((item.y_um for item in fixed_nodes or []), default=0.0)
    detour_penalty_um = 24.0 * len(blocked_regions or []) if circuit.edge_count() > 0 else 0.0
    critical_path_delay_ps = float(circuit.edge_count() * 18)
    worst_setup_slack_ps = _effective_required_ps(
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
    ) - critical_path_delay_ps
    false_path_arcs = _fallback_false_path_arcs(
        circuit,
        timing_constraints,
        pin_timing_constraints,
        crossing_constraints,
    )
    return LayoutReport(
        connections_applied=synth_report.connections_applied,
        splitters_inserted=synth_report.splitters_inserted,
        balancing_dffs_inserted=synth_report.balancing_dffs_inserted,
        mapped_nodes=synth_report.mapped_nodes,
        total_area_um2=synth_report.total_area_um2,
        bool_opt_compatible=synth_report.bool_opt_compatible,
        placed_nodes=circuit.node_count(),
        placement_width_um=max(float(max(circuit.node_count() - 1, 0) * 40), max_fixed_x_um + 40.0 if circuit.node_count() > 0 else max_fixed_x_um),
        placement_height_um=max(24.0 if circuit.node_count() > 0 else 0.0, max_fixed_y_um + 24.0 if circuit.node_count() > 0 else max_fixed_y_um),
        clock_sinks=0,
        clock_buffers=0,
        clock_phase_count=2,
        initial_hold_violations=0,
        final_hold_violations=0,
        hold_fix_applied=False,
        worst_setup_slack_ps=worst_setup_slack_ps,
        worst_hold_slack_ps=0.0,
        critical_path_delay_ps=critical_path_delay_ps,
        analyzed_timing_arcs=circuit.edge_count(),
        false_path_arcs=false_path_arcs,
        setup_violations=1 if worst_setup_slack_ps < 0.0 else 0,
        routed_nets=circuit.edge_count(),
        total_route_length_um=float(circuit.edge_count() * 40) + detour_penalty_um,
        initial_total_detour_overhead_um=detour_penalty_um,
        total_detour_overhead_um=detour_penalty_um,
        detoured_routes=len(blocked_regions or []) if circuit.edge_count() > 0 and blocked_regions else 0,
        detour_feedback_applied=False,
        jtl_routes=circuit.edge_count(),
        ptl_routes=0,
        node_count=synth_report.node_count,
        edge_count=synth_report.edge_count,
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
    if _core_analyze_timing is not None:
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
            detour_feedback_applied=report.detour_feedback_applied,
            hold_fix_applied=report.hold_fix_applied,
            timing_arcs=[
                TimingArcReport(
                    from_pin=PinRef(node=arc.from_pin.node, port=arc.from_pin.port),
                    to_pin=PinRef(node=arc.to_pin.node, port=arc.to_pin.port),
                    is_false_path=arc.is_false_path,
                    route_mode=arc.route_mode,
                    route_length_um=arc.route_length_um,
                    from_domain=arc.from_domain,
                    to_domain=arc.to_domain,
                    arrival_ps=arc.arrival_ps,
                    required_ps=arc.required_ps,
                    setup_slack_ps=arc.setup_slack_ps,
                    hold_slack_ps=arc.hold_slack_ps,
                )
                for arc in report.timing_arcs
            ],
        )

    layout = compile_layout(
        circuit,
        plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    timing_arcs = _fallback_timing_arcs(
        circuit,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
    )
    return TimingAnalysisReport(
        worst_setup_slack_ps=layout.worst_setup_slack_ps,
        worst_hold_slack_ps=layout.worst_hold_slack_ps,
        critical_path_delay_ps=layout.critical_path_delay_ps,
        analyzed_timing_arcs=layout.analyzed_timing_arcs,
        false_path_arcs=layout.false_path_arcs,
        setup_violations=layout.setup_violations,
        hold_violations=layout.final_hold_violations,
        detour_feedback_applied=layout.detour_feedback_applied,
        hold_fix_applied=layout.hold_fix_applied,
        timing_arcs=timing_arcs,
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
    if _core_analyze_timing_statistical is not None:
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

    deterministic = analyze_timing(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    crossing_kind_by_domain_pair = {
        (constraint.from_domain, constraint.to_domain): constraint.kind
        for constraint in (crossing_constraints or [])
    }

    def crossing_sigma_ps(arc: TimingArcReport) -> float:
        if arc.from_domain is None or arc.to_domain is None or arc.from_domain == arc.to_domain:
            return 0.0
        categorized_sigma_ps = 0.0
        match crossing_kind_by_domain_pair.get((arc.from_domain, arc.to_domain)):
            case "max_delay":
                categorized_sigma_ps = max_delay_cross_domain_uncertainty_sigma_ps
            case "multicycle":
                categorized_sigma_ps = multicycle_cross_domain_uncertainty_sigma_ps
            case _:
                categorized_sigma_ps = 0.0
        return (cross_domain_uncertainty_sigma_ps**2 + categorized_sigma_ps**2) ** 0.5

    timing_arcs = [
        StatisticalTimingArcReport(
            from_pin=arc.from_pin,
            to_pin=arc.to_pin,
            is_false_path=arc.is_false_path,
            route_mode=arc.route_mode,
            route_length_um=arc.route_length_um,
            from_domain=arc.from_domain,
            to_domain=arc.to_domain,
            mean_arrival_ps=arc.arrival_ps,
            mean_required_ps=arc.required_ps,
            setup_slack_ps=arc.setup_slack_ps,
            hold_slack_ps=arc.hold_slack_ps,
            setup_sigma_ps=(
                ((cell_delay_sigma_ratio * 8.0) ** 2 + (wire_delay_sigma_ratio * 10.0) ** 2)
                + ((global_cell_delay_sigma_ratio * 8.0) + (global_wire_delay_sigma_ratio * 10.0)) ** 2
                + (clock_uncertainty_sigma_ps**2)
                + (crossing_sigma_ps(arc) ** 2)
            ) ** 0.5,
            hold_sigma_ps=(
                (wire_delay_sigma_ratio * 10.0) ** 2
                + (global_wire_delay_sigma_ratio * 10.0) ** 2
                + (clock_uncertainty_sigma_ps**2)
                + (crossing_sigma_ps(arc) ** 2)
            ) ** 0.5,
            pessimistic_setup_slack_ps=(
                float("inf")
                if arc.is_false_path
                else arc.setup_slack_ps
                - sigma_multiplier
                * (
                    ((cell_delay_sigma_ratio * 8.0) ** 2 + (wire_delay_sigma_ratio * 10.0) ** 2)
                    + ((global_cell_delay_sigma_ratio * 8.0) + (global_wire_delay_sigma_ratio * 10.0)) ** 2
                    + (clock_uncertainty_sigma_ps**2)
                    + (crossing_sigma_ps(arc) ** 2)
                )
                ** 0.5
            ),
            pessimistic_hold_slack_ps=arc.hold_slack_ps
            - sigma_multiplier
            * (
                (
                    (wire_delay_sigma_ratio * 10.0) ** 2
                    + (global_wire_delay_sigma_ratio * 10.0) ** 2
                    + (clock_uncertainty_sigma_ps**2)
                    + (crossing_sigma_ps(arc) ** 2)
                )
                ** 0.5
            ),
        )
        for arc in deterministic.timing_arcs
    ]
    return StatisticalTimingAnalysisReport(
        worst_pessimistic_setup_slack_ps=min(
            (arc.pessimistic_setup_slack_ps for arc in timing_arcs),
            default=120.0,
        ),
        worst_pessimistic_hold_slack_ps=min(
            (arc.pessimistic_hold_slack_ps for arc in timing_arcs),
            default=0.0,
        ),
        analyzed_timing_arcs=deterministic.analyzed_timing_arcs,
        false_path_arcs=deterministic.false_path_arcs,
        setup_risk_violations=sum(arc.pessimistic_setup_slack_ps < 0.0 for arc in timing_arcs),
        hold_risk_violations=sum(arc.pessimistic_hold_slack_ps < 0.0 for arc in timing_arcs),
        sigma_multiplier=sigma_multiplier,
        timing_arcs=timing_arcs,
    )


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
    if _core_analyze_ac_bias is not None:
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

    layout = compile_layout(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    routed_nets = layout.routed_nets
    jtl_carrier_candidates = layout.jtl_routes
    ptl_coupling_risk_routes = layout.ptl_routes
    clock_sink_count = layout.clock_sinks
    estimated_static_power_savings_uw = jtl_carrier_candidates * 0.35
    estimated_area_overhead_ratio = 1.0 if routed_nets == 0 else 1.0 + 0.23 * (jtl_carrier_candidates / routed_nets)
    estimated_frequency_derate_ratio = 1.0 if clock_sink_count == 0 else max(0.25, 1.0 - 0.15 * (clock_sink_count / max(clock_sink_count + jtl_carrier_candidates, 1)))
    worst_setup_slack_ps = layout.worst_setup_slack_ps
    worst_hold_slack_ps = layout.worst_hold_slack_ps
    carrier_ratio = 0.0 if routed_nets == 0 else jtl_carrier_candidates / routed_nets
    coupling_penalty = 0.0 if routed_nets == 0 else ptl_coupling_risk_routes / routed_nets
    feasibility_score = min(1.0, max(0.0, carrier_ratio * 0.7 + estimated_frequency_derate_ratio * 0.3 - coupling_penalty * 0.4))
    normalized_power_savings = 0.0 if routed_nets == 0 else min(1.0, estimated_static_power_savings_uw / (routed_nets * 0.35))
    normalized_area_efficiency = min(1.0, 1.0 / estimated_area_overhead_ratio)
    normalized_coupling_margin = min(1.0, max(0.0, 1.0 - coupling_penalty))
    normalized_setup_guardband = 0.0 if worst_setup_slack_ps <= 0.0 else min(1.0, worst_setup_slack_ps / (worst_setup_slack_ps + 20.0))
    normalized_hold_guardband = 0.0 if worst_hold_slack_ps <= 0.0 else min(1.0, worst_hold_slack_ps / (worst_hold_slack_ps + 5.0))
    timing_guardband_score = min(1.0, max(0.0, normalized_setup_guardband * 0.7 + normalized_hold_guardband * 0.3))
    optimization_score = min(
        1.0,
        max(
            0.0,
            feasibility_score * 0.35
            + timing_guardband_score * 0.25
            + estimated_frequency_derate_ratio * 0.15
            + normalized_area_efficiency * 0.10
            + normalized_power_savings * 0.08
            + normalized_coupling_margin * 0.07,
        ),
    )
    return AcBiasReport(
        routed_nets=layout.routed_nets,
        jtl_carrier_candidates=layout.jtl_routes,
        ptl_coupling_risk_routes=layout.ptl_routes,
        clock_sink_count=layout.clock_sinks,
        estimated_static_power_savings_uw=estimated_static_power_savings_uw,
        estimated_area_overhead_ratio=estimated_area_overhead_ratio,
        estimated_frequency_derate_ratio=estimated_frequency_derate_ratio,
        worst_setup_slack_ps=worst_setup_slack_ps,
        worst_hold_slack_ps=worst_hold_slack_ps,
        timing_guardband_score=timing_guardband_score,
        feasibility_score=feasibility_score,
        optimization_score=optimization_score,
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
    if _core_optimize_ac_bias is not None:
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

    baseline = analyze_ac_bias(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    baseline_threshold = 60.0
    baseline_detour_margin = 12.0
    threshold_candidates = [baseline_threshold]
    detour_margin_candidates = [baseline_detour_margin]

    return AcBiasOptimizationReport(
        baseline=baseline,
        optimized=baseline,
        baseline_prefer_ptl_from_length_um=baseline_threshold,
        optimized_prefer_ptl_from_length_um=baseline_threshold,
        threshold_candidates_evaluated=len(threshold_candidates),
        baseline_detour_margin_um=baseline_detour_margin,
        optimized_detour_margin_um=baseline_detour_margin,
        detour_margin_candidates_evaluated=len(detour_margin_candidates),
        optimization_applied=False,
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
    if simulation_mode not in {"auto", "event_only", "external_josim", "internal_transient"}:
        raise ValueError(f"unknown simulation mode: {simulation_mode}")

    if _core_verify_layout is not None:
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

    if simulation_mode == "internal_transient":
        return VerificationReport(
            checked_routes=0,
            checked_ptl_routes=0,
            structural_violations=0,
            ptl_macro_boundary_violations=0,
            ptl_forbidden_length_violations=0,
            simulation_backend="internal_transient_unavailable",
            simulated_events=0,
            generated_deck_lines=0,
            generated_deck_path=None,
            waveform_path=None,
            reported_violations=0,
            reported_worst_delay_ps=None,
            delay_details=[],
            violation_details=[],
            external_status_code=None,
            external_result="internal_transient_not_implemented",
        )

    if simulation_mode == "external_josim":
        raise RuntimeError("external_josim simulation mode requires the compiled rflux._core extension")

    layout = compile_layout(
        circuit,
        plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    return VerificationReport(
        checked_routes=layout.routed_nets,
        checked_ptl_routes=layout.ptl_routes,
        structural_violations=0,
        ptl_macro_boundary_violations=0,
        ptl_forbidden_length_violations=0,
        simulation_backend="event_only",
        simulated_events=layout.analyzed_timing_arcs + layout.node_count,
        generated_deck_lines=layout.node_count + layout.edge_count + 2,
        generated_deck_path=None,
        waveform_path=None,
        reported_violations=0,
        reported_worst_delay_ps=None,
        delay_details=[],
        violation_details=[],
        external_status_code=None,
        external_result=None,
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
    if simulation_mode not in {"auto", "event_only", "external_josim", "internal_transient"}:
        raise ValueError(f"unknown simulation mode: {simulation_mode}")

    if _core_characterize_compound_cell is not None:
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

    layout = compile_layout(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    verification = verify_layout(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
        simulation_mode=simulation_mode,
        external_command=external_command,
    )
    generated_cell_kind = "macro" if layout.mapped_nodes > 1 or layout.node_count > 1 else "generic_gate"
    generated_pipeline_stages = max(layout.clock_phase_count, 1)
    generated_library_json = json.dumps(
        {
            "cell": {
                "name": cell_name,
                "kind": "Macro" if generated_cell_kind == "macro" else "GenericGate",
                "area_um2": layout.total_area_um2,
                "pipeline_stages": generated_pipeline_stages,
            },
            "timing": {
                "kind": "Macro" if generated_cell_kind == "macro" else "GenericGate",
                "intrinsic_delay_ps": (
                    verification.reported_worst_delay_ps
                    if verification.reported_worst_delay_ps is not None
                    else layout.critical_path_delay_ps
                ),
                "setup_ps": max(layout.critical_path_delay_ps * 0.12, float(layout.setup_violations)),
                "hold_ps": max(layout.worst_hold_slack_ps, 0.0),
            },
        },
        indent=2,
    )
    return CompoundCellCharacterizationReport(
        cell_name=cell_name,
        node_count=layout.node_count,
        edge_count=layout.edge_count,
        mapped_nodes=layout.mapped_nodes,
        total_area_um2=layout.total_area_um2,
        derived_intrinsic_delay_ps=(
            verification.reported_worst_delay_ps
            if verification.reported_worst_delay_ps is not None
            else layout.critical_path_delay_ps
        ),
        derived_setup_ps=max(layout.critical_path_delay_ps * 0.12, float(layout.setup_violations)),
        derived_hold_ps=max(layout.worst_hold_slack_ps, 0.0),
        generated_cell_kind=generated_cell_kind,
        generated_pipeline_stages=generated_pipeline_stages,
        generated_library_json=generated_library_json,
        simulated_delay_ps=verification.reported_worst_delay_ps,
        simulation_backend=verification.simulation_backend,
        generated_deck_lines=verification.generated_deck_lines,
        generated_deck_path=verification.generated_deck_path,
        waveform_path=verification.waveform_path,
        reported_violations=verification.reported_violations,
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
    if _core_analyze_advanced_constraints is not None:
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

    layout = compile_layout(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    total_length_um = max(layout.total_route_length_um, 1.0)
    routed_nets = max(layout.routed_nets, 1)
    estimated_thermal_load_uw = layout.jtl_routes * 0.22 + layout.ptl_routes * 0.08 + layout.clock_sinks * 0.05
    detour_overhead_ratio = min(1.0, max(0.0, layout.total_detour_overhead_um / total_length_um))
    ptl_coupling_ratio = min(1.0, max(0.0, layout.ptl_routes / routed_nets))
    jtl_density_per_100um = layout.jtl_routes / max(total_length_um / 100.0, 1.0)
    estimated_mechanical_stress_score = min(
        1.0,
        max(0.0, detour_overhead_ratio * 0.6 + ptl_coupling_ratio * 0.25 + (jtl_density_per_100um / 10.0) * 0.15),
    )
    violations: list[AdvancedConstraintViolation] = []
    if estimated_thermal_load_uw > max_estimated_thermal_load_uw:
        violations.append(AdvancedConstraintViolation("thermal", "estimated thermal load exceeds configured budget", estimated_thermal_load_uw, max_estimated_thermal_load_uw))
    if estimated_mechanical_stress_score > max_estimated_mechanical_stress_score:
        violations.append(AdvancedConstraintViolation("mechanical", "estimated mechanical stress score exceeds configured limit", estimated_mechanical_stress_score, max_estimated_mechanical_stress_score))
    if jtl_density_per_100um > max_jtl_density_per_100um:
        violations.append(AdvancedConstraintViolation("manufacturing", "JTL density exceeds configured manufacturing limit", jtl_density_per_100um, max_jtl_density_per_100um))
    if detour_overhead_ratio > max_detour_overhead_ratio:
        violations.append(AdvancedConstraintViolation("manufacturing", "detour overhead ratio exceeds configured manufacturability limit", detour_overhead_ratio, max_detour_overhead_ratio))
    if ptl_coupling_ratio > max_ptl_coupling_ratio:
        violations.append(AdvancedConstraintViolation("electrical", "PTL coupling ratio exceeds configured electrical limit", ptl_coupling_ratio, max_ptl_coupling_ratio))
    return AdvancedConstraintReport(
        estimated_thermal_load_uw=estimated_thermal_load_uw,
        estimated_mechanical_stress_score=estimated_mechanical_stress_score,
        jtl_density_per_100um=jtl_density_per_100um,
        detour_overhead_ratio=detour_overhead_ratio,
        ptl_coupling_ratio=ptl_coupling_ratio,
        manufacturing_hotspots=layout.detoured_routes + int(detour_overhead_ratio > 0.20) + int(ptl_coupling_ratio > 0.50),
        violation_count=len(violations),
        violations=violations,
    )


def merge_characterized_library(
    serialized_entries: list[str],
    base_name: str = "py-minimal-pdk",
) -> str:
    if _core_merge_characterized_library is not None:
        return _core_merge_characterized_library(serialized_entries, base_name)
    if _CorePdk is not None:
        pdk = Pdk.minimal(base_name)
        for entry in serialized_entries:
            pdk = pdk.merge_characterized_library_json(entry)
        return pdk.to_json()
    if len(serialized_entries) == 1:
        return serialized_entries[0]
    return json.dumps(
        {"entries": [json.loads(entry) for entry in serialized_entries]},
        indent=2,
    )


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
    if _core_optimize_ac_bias_with_characterized_library is not None:
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
            ac_bias=AcBiasOptimizationReport(
                baseline=AcBiasReport(
                    routed_nets=report.ac_bias.baseline.routed_nets,
                    jtl_carrier_candidates=report.ac_bias.baseline.jtl_carrier_candidates,
                    ptl_coupling_risk_routes=report.ac_bias.baseline.ptl_coupling_risk_routes,
                    clock_sink_count=report.ac_bias.baseline.clock_sink_count,
                    estimated_static_power_savings_uw=report.ac_bias.baseline.estimated_static_power_savings_uw,
                    estimated_area_overhead_ratio=report.ac_bias.baseline.estimated_area_overhead_ratio,
                    estimated_frequency_derate_ratio=report.ac_bias.baseline.estimated_frequency_derate_ratio,
                    worst_setup_slack_ps=report.ac_bias.baseline.worst_setup_slack_ps,
                    worst_hold_slack_ps=report.ac_bias.baseline.worst_hold_slack_ps,
                    timing_guardband_score=report.ac_bias.baseline.timing_guardband_score,
                    feasibility_score=report.ac_bias.baseline.feasibility_score,
                    optimization_score=report.ac_bias.baseline.optimization_score,
                ),
                optimized=AcBiasReport(
                    routed_nets=report.ac_bias.optimized.routed_nets,
                    jtl_carrier_candidates=report.ac_bias.optimized.jtl_carrier_candidates,
                    ptl_coupling_risk_routes=report.ac_bias.optimized.ptl_coupling_risk_routes,
                    clock_sink_count=report.ac_bias.optimized.clock_sink_count,
                    estimated_static_power_savings_uw=report.ac_bias.optimized.estimated_static_power_savings_uw,
                    estimated_area_overhead_ratio=report.ac_bias.optimized.estimated_area_overhead_ratio,
                    estimated_frequency_derate_ratio=report.ac_bias.optimized.estimated_frequency_derate_ratio,
                    worst_setup_slack_ps=report.ac_bias.optimized.worst_setup_slack_ps,
                    worst_hold_slack_ps=report.ac_bias.optimized.worst_hold_slack_ps,
                    timing_guardband_score=report.ac_bias.optimized.timing_guardband_score,
                    feasibility_score=report.ac_bias.optimized.feasibility_score,
                    optimization_score=report.ac_bias.optimized.optimization_score,
                ),
                baseline_prefer_ptl_from_length_um=report.ac_bias.baseline_prefer_ptl_from_length_um,
                optimized_prefer_ptl_from_length_um=report.ac_bias.optimized_prefer_ptl_from_length_um,
                baseline_detour_margin_um=report.ac_bias.baseline_detour_margin_um,
                optimized_detour_margin_um=report.ac_bias.optimized_detour_margin_um,
                threshold_candidates_evaluated=report.ac_bias.threshold_candidates_evaluated,
                detour_margin_candidates_evaluated=report.ac_bias.detour_margin_candidates_evaluated,
                optimization_applied=report.ac_bias.optimization_applied,
            ),
            baseline_constraints=AdvancedConstraintReport(
                estimated_thermal_load_uw=report.baseline_constraints.estimated_thermal_load_uw,
                estimated_mechanical_stress_score=report.baseline_constraints.estimated_mechanical_stress_score,
                jtl_density_per_100um=report.baseline_constraints.jtl_density_per_100um,
                detour_overhead_ratio=report.baseline_constraints.detour_overhead_ratio,
                ptl_coupling_ratio=report.baseline_constraints.ptl_coupling_ratio,
                manufacturing_hotspots=report.baseline_constraints.manufacturing_hotspots,
                violation_count=report.baseline_constraints.violation_count,
                violations=[
                    AdvancedConstraintViolation(
                        category=item.category,
                        detail=item.detail,
                        measured_value=item.measured_value,
                        limit_value=item.limit_value,
                    )
                    for item in report.baseline_constraints.violations
                ],
            ),
            optimized_constraints=AdvancedConstraintReport(
                estimated_thermal_load_uw=report.optimized_constraints.estimated_thermal_load_uw,
                estimated_mechanical_stress_score=report.optimized_constraints.estimated_mechanical_stress_score,
                jtl_density_per_100um=report.optimized_constraints.jtl_density_per_100um,
                detour_overhead_ratio=report.optimized_constraints.detour_overhead_ratio,
                ptl_coupling_ratio=report.optimized_constraints.ptl_coupling_ratio,
                manufacturing_hotspots=report.optimized_constraints.manufacturing_hotspots,
                violation_count=report.optimized_constraints.violation_count,
                violations=[
                    AdvancedConstraintViolation(
                        category=item.category,
                        detail=item.detail,
                        measured_value=item.measured_value,
                        limit_value=item.limit_value,
                    )
                    for item in report.optimized_constraints.violations
                ],
            ),
            characterized_cells_merged=report.characterized_cells_merged,
            library_optimization_score=report.library_optimization_score,
        )

    ac_bias = optimize_ac_bias(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
    )
    baseline_constraints = analyze_advanced_constraints(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
        max_estimated_thermal_load_uw=max_estimated_thermal_load_uw,
        max_estimated_mechanical_stress_score=max_estimated_mechanical_stress_score,
        max_jtl_density_per_100um=max_jtl_density_per_100um,
        max_detour_overhead_ratio=max_detour_overhead_ratio,
        max_ptl_coupling_ratio=max_ptl_coupling_ratio,
    )
    return LibraryAwareAcBiasOptimizationReport(
        ac_bias=ac_bias,
        baseline_constraints=baseline_constraints,
        optimized_constraints=baseline_constraints,
        characterized_cells_merged=len(characterized_library_entries),
        library_optimization_score=ac_bias.optimized.optimization_score,
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
    if _core_optimize_design_with_characterized_library is not None:
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

    ac_report = optimize_ac_bias_with_characterized_library(
        circuit,
        characterized_library_entries,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
        max_estimated_thermal_load_uw=max_estimated_thermal_load_uw,
        max_estimated_mechanical_stress_score=max_estimated_mechanical_stress_score,
        max_jtl_density_per_100um=max_jtl_density_per_100um,
        max_detour_overhead_ratio=max_detour_overhead_ratio,
        max_ptl_coupling_ratio=max_ptl_coupling_ratio,
    )
    baseline_statistical = analyze_timing_statistical(
        circuit,
        plan=plan,
        fixed_nodes=fixed_nodes,
        blocked_regions=blocked_regions,
        timing_constraints=timing_constraints,
        pin_timing_constraints=pin_timing_constraints,
        clock_domains=clock_domains,
        crossing_constraints=crossing_constraints,
        characterized_library_entries=characterized_library_entries,
        cell_delay_sigma_ratio=cell_delay_sigma_ratio,
        wire_delay_sigma_ratio=wire_delay_sigma_ratio,
        global_cell_delay_sigma_ratio=global_cell_delay_sigma_ratio,
        global_wire_delay_sigma_ratio=global_wire_delay_sigma_ratio,
        clock_uncertainty_sigma_ps=clock_uncertainty_sigma_ps,
        cross_domain_uncertainty_sigma_ps=cross_domain_uncertainty_sigma_ps,
        max_delay_cross_domain_uncertainty_sigma_ps=max_delay_cross_domain_uncertainty_sigma_ps,
        multicycle_cross_domain_uncertainty_sigma_ps=multicycle_cross_domain_uncertainty_sigma_ps,
        sigma_multiplier=sigma_multiplier,
    )
    return LibraryAwareDesignOptimizationReport(
        ac_bias=ac_report.ac_bias,
        baseline_statistical=baseline_statistical,
        optimized_statistical=baseline_statistical,
        baseline_constraints=ac_report.baseline_constraints,
        optimized_constraints=ac_report.optimized_constraints,
        characterized_cells_merged=ac_report.characterized_cells_merged,
        design_optimization_score=ac_report.library_optimization_score,
    )


__all__ = [
    "__version__",
    "BalanceStrategy",
    "BlockedRegion",
    "ClockDomainConstraint",
    "CrossingConstraint",
    "Circuit",
    "CompilePlan",
    "CompileReport",
    "ConnectionSpec",
    "FixedNodePlacement",
    "LayoutReport",
    "NodeTimingConstraint",
    "PinTimingConstraint",
    "TimingAnalysisReport",
    "TimingArcReport",
    "StatisticalTimingAnalysisReport",
    "StatisticalTimingArcReport",
    "AcBiasReport",
    "AcBiasOptimizationReport",
    "LibraryAwareAcBiasOptimizationReport",
    "LibraryAwareDesignOptimizationReport",
    "Pdk",
    "AdvancedConstraintReport",
    "AdvancedConstraintViolation",
    "VerificationReport",
    "CompoundCellCharacterizationReport",
    "PinRef",
    "SynthesisReport",
    "SimulationDelayDetail",
    "SimulationEndpointRef",
    "SimulationViolationDetail",
    "analyze_timing",
    "analyze_timing_statistical",
    "analyze_ac_bias",
    "analyze_advanced_constraints",
    "characterize_compound_cell",
    "optimize_ac_bias",
    "optimize_ac_bias_with_characterized_library",
    "optimize_design_with_characterized_library",
    "merge_characterized_library",
    "compile",
    "compile_layout",
    "compile_netlist",
    "compile_plan",
    "compile_plan_report",
    "core_version",
    "verify_layout",
]
