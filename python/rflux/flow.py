import sys

from . import (
    DEFAULT_CLOCK_PERIOD_PS,
    DEFAULT_INPUT_ARRIVAL_PS,
    DEFAULT_MIN_HOLD_JTL_LENGTH_UM,
    DEFAULT_SFQ_PHASE_COUNT,
    DEFAULT_SFQ_PULSE_WINDOW_PS,
)
from ._types import (
    AcBiasOptimizationReport,
    AcBiasReport,
    BalanceStrategy,
    BlockedRegion,
    CellLibraryEntry,
    CellLibraryMetadata,
    CellLibrarySummary,
    CompilePlan,
    CompileReport,
    CompoundCellCharacterizationReport,
    ConnectionSpec,
    FixedNodePlacement,
    LayoutReport,
    LibraryAwareAcBiasOptimizationReport,
    LibraryAwareDesignOptimizationReport,
    NodeTimingConstraint,
    PinRef,
    PinTimingConstraint,
    SynthesisReport,
)


_api = sys.modules[__package__]


def compile(circuit, plan: CompilePlan | None = None):
    """Apply a synthesis compile plan to ``circuit`` and return the same circuit."""
    return compile_plan(circuit, plan or CompilePlan())


def compile_plan_report(circuit, plan: CompilePlan) -> CompileReport:
    """Apply a compile plan and return the synthesis edit counts."""
    _api._require_core_extension("compile_plan_report(...)", _api._core_compile_plan)
    core_plan = _api._to_core_compile_plan(plan)
    report = _api._core_compile_plan(circuit, core_plan)
    return CompileReport(
        connections_applied=report.connections_applied,
        splitters_inserted=report.splitters_inserted,
        balancing_dffs_inserted=report.balancing_dffs_inserted,
    )


def compile_plan(circuit, plan: CompilePlan):
    """Apply a compile plan in place and return ``circuit``."""
    _ = compile_plan_report(circuit, plan)
    return circuit


def compile_netlist(circuit, plan: CompilePlan | None = None) -> SynthesisReport:
    """Run synthesis-level compilation and return a netlist summary."""
    _api._require_core_extension("compile_netlist(...)", _api._core_compile_netlist)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_compile_netlist(circuit, core_plan)
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
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
    min_hold_jtl_length_um: float | None = None,
    prefer_ptl_from_length_um: float | None = None,
    detour_margin_um: float | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
) -> LayoutReport:
    """Run synthesis, placement, routing, timing, and layout verification."""
    _api._require_core_extension("compile_layout(...)", _api._core_compile_layout)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_compile_layout(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        min_hold_jtl_length_um,
        prefer_ptl_from_length_um,
        detour_margin_um,
        characterized_library_json,
        characterized_library_entries,
    )
    flow_config_patch = _layout_flow_config_patch(
        report,
        min_hold_jtl_length_um=min_hold_jtl_length_um,
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
        timing_closure=_api._timing_closure_from_core(report.timing_closure),
        timing_closure_loop=_api._timing_closure_loop_from_core(report.timing_closure_loop),
        routed_nets=report.routed_nets,
        total_route_length_um=report.total_route_length_um,
        initial_total_detour_overhead_um=report.initial_total_detour_overhead_um,
        total_detour_overhead_um=report.total_detour_overhead_um,
        detoured_routes=report.detoured_routes,
        detour_feedback_applied=report.detour_feedback_applied,
        effective_prefer_ptl_from_length_um=report.effective_prefer_ptl_from_length_um,
        effective_detour_margin_um=report.effective_detour_margin_um,
        flow_config_patch=flow_config_patch,
        jtl_routes=report.jtl_routes,
        ptl_routes=report.ptl_routes,
        node_count=report.node_count,
        edge_count=report.edge_count,
    )


def _layout_flow_config_patch(report, *, min_hold_jtl_length_um: float | None) -> dict[str, object]:
    return {
        "schema_version": _api.FLOW_CONFIG_SCHEMA_VERSION,
        "kind": _api.FLOW_CONFIG_KIND,
        "metadata": {
            "source_command": "compile_layout",
            "source_report_kind": "compile_layout",
            "timing_closure_status": report.timing_closure.status,
            "route_delay_optimization_applied": (
                report.timing_closure_loop.route_delay_optimization_applied
            ),
            "hold_fix_applied": report.hold_fix_applied,
        },
        "payload": {
            "timing": {
                "clock_period_ps": DEFAULT_CLOCK_PERIOD_PS,
                "input_arrival_ps": DEFAULT_INPUT_ARRIVAL_PS,
                "sfq_phase_count": DEFAULT_SFQ_PHASE_COUNT,
                "sfq_pulse_window_ps": DEFAULT_SFQ_PULSE_WINDOW_PS,
            },
            "routing": {
                "prefer_ptl_from_length_um": report.effective_prefer_ptl_from_length_um,
                "detour_margin_um": report.effective_detour_margin_um,
                "min_hold_jtl_length_um": (
                    min_hold_jtl_length_um
                    if min_hold_jtl_length_um is not None
                    else DEFAULT_MIN_HOLD_JTL_LENGTH_UM
                ),
            },
        },
    }


def analyze_ac_bias(
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
) -> AcBiasReport:
    """Analyze AC-bias routing feasibility and timing guardband metrics."""
    _api._require_core_extension("analyze_ac_bias(...)", _api._core_analyze_ac_bias)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_analyze_ac_bias(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
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
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
) -> AcBiasOptimizationReport:
    """Search AC-bias routing knobs and return before/after metrics."""
    _api._require_core_extension("optimize_ac_bias(...)", _api._core_optimize_ac_bias)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_optimize_ac_bias(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
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


def characterize_compound_cell(
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
    cell_name: str = "compound_cell",
    simulation_mode: str = "auto",
    external_command: str | None = None,
) -> CompoundCellCharacterizationReport:
    """Characterize a circuit as a reusable compound cell library entry."""
    _api._validate_simulation_mode(simulation_mode)
    _api._require_core_extension(
        "characterize_compound_cell(...)",
        _api._core_characterize_compound_cell,
    )
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_characterize_compound_cell(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
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


def merge_characterized_library(
    serialized_entries: list[str],
    base_name: str = "py-minimal-pdk",
) -> str:
    """Merge characterized library JSON entries into a PDK JSON document."""
    _api._require_core_extension(
        "merge_characterized_library(...)",
        _api._core_merge_characterized_library,
    )
    return _api._core_merge_characterized_library(serialized_entries, base_name)


def optimize_ac_bias_with_characterized_library(
    circuit,
    characterized_library_entries: list[str],
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
    max_estimated_thermal_load_uw: float = 8.0,
    max_estimated_mechanical_stress_score: float = 0.75,
    max_jtl_density_per_100um: float = 8.0,
    max_detour_overhead_ratio: float = 0.35,
    max_ptl_coupling_ratio: float = 0.65,
) -> LibraryAwareAcBiasOptimizationReport:
    """Optimize AC-bias settings using characterized cell library feedback."""
    _api._require_core_extension(
        "optimize_ac_bias_with_characterized_library(...)",
        _api._core_optimize_ac_bias_with_characterized_library,
    )
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_optimize_ac_bias_with_characterized_library(
        circuit,
        characterized_library_entries,
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
    return LibraryAwareAcBiasOptimizationReport(
        ac_bias=_api._ac_bias_optimization_report_from_core(report.ac_bias),
        baseline_constraints=_api._advanced_constraint_report_from_core(report.baseline_constraints),
        optimized_constraints=_api._advanced_constraint_report_from_core(report.optimized_constraints),
        characterized_cells_merged=report.characterized_cells_merged,
        library_optimization_score=report.library_optimization_score,
    )


def optimize_design_with_characterized_library(
    circuit,
    characterized_library_entries: list[str],
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
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
    _api._require_core_extension(
        "optimize_design_with_characterized_library(...)",
        _api._core_optimize_design_with_characterized_library,
    )
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    report = _api._core_optimize_design_with_characterized_library(
        circuit,
        characterized_library_entries,
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
        ac_bias=_api._ac_bias_optimization_report_from_core(report.ac_bias),
        baseline_statistical=_api._statistical_timing_report_from_core(report.baseline_statistical),
        optimized_statistical=_api._statistical_timing_report_from_core(report.optimized_statistical),
        baseline_constraints=_api._advanced_constraint_report_from_core(report.baseline_constraints),
        optimized_constraints=_api._advanced_constraint_report_from_core(report.optimized_constraints),
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



def build_clock_tree(circuit):
    """Build an H-tree clock distribution network for a circuit.

    Returns a dict with clock tree metrics: sink_count, buffer_count,
    levels, total_wire_length_um, estimated_skew_ps, phase_count.
    """
    return _api._core_build_clock_tree(circuit)


def build_bias_grid(circuit):
    """Build a bias distribution grid estimate for a circuit.

    Returns a dict with bias grid metrics: grid_cells,
    total_wire_length_um, connected_nodes, estimated_total_bias_current_ma.
    """
    return _api._core_build_bias_grid(circuit)


def export_gds(
    circuit,
    gds_path: str,
    library_name: str | None = None,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
    min_hold_jtl_length_um: float | None = None,
    prefer_ptl_from_length_um: float | None = None,
    detour_margin_um: float | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
) -> None:
    """Run the full layout flow and export the result as a GDS-II file."""
    _api._require_core_extension("export_gds(...)", _api._core_export_gds)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    _api._core_export_gds(
        circuit,
        gds_path,
        library_name,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        min_hold_jtl_length_um,
        prefer_ptl_from_length_um,
        detour_margin_um,
        characterized_library_json,
        characterized_library_entries,
    )


def export_svg(
    circuit,
    svg_path: str,
    title: str | None = None,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
    min_hold_jtl_length_um: float | None = None,
    prefer_ptl_from_length_um: float | None = None,
    detour_margin_um: float | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
) -> None:
    """Run the full layout flow and export the result as an SVG visualization."""
    _api._require_core_extension("export_svg(...)", _api._core_export_svg)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    _api._core_export_svg(
        circuit,
        svg_path,
        title,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        min_hold_jtl_length_um,
        prefer_ptl_from_length_um,
        detour_margin_um,
        characterized_library_json,
        characterized_library_entries,
    )


def run_dse(
    circuit,
    plan: CompilePlan | None = None,
    fixed_nodes: list[FixedNodePlacement] | None = None,
    blocked_regions: list[BlockedRegion] | None = None,
    timing_constraints: list[NodeTimingConstraint] | None = None,
    pin_timing_constraints: list[PinTimingConstraint] | None = None,
    clock_domains: list | None = None,
    crossing_constraints: list | None = None,
    clock_period_ps_values: list[float] | None = None,
    prefer_ptl_from_length_um_values: list[float] | None = None,
    detour_margin_um_values: list[float] | None = None,
    min_hold_jtl_length_um_values: list[float] | None = None,
    sfq_phase_count_values: list[int] | None = None,
    characterized_library_json: str | None = None,
    characterized_library_entries: list[str] | None = None,
) -> dict:
    """Run Design Space Exploration and return all evaluated points, Pareto front, and recommended config."""
    _api._require_core_extension("run_dse(...)", _api._core_run_dse)
    core_plan = _api._to_core_compile_plan(plan) if plan is not None else None
    return _api._core_run_dse(
        circuit,
        core_plan,
        _api._to_core_fixed_nodes(fixed_nodes),
        _api._to_core_blocked_regions(blocked_regions),
        _api._to_core_timing_constraints(timing_constraints),
        _api._to_core_pin_timing_constraints(pin_timing_constraints),
        _api._to_core_clock_domains(clock_domains),
        _api._to_core_crossing_constraints(crossing_constraints),
        clock_period_ps_values,
        prefer_ptl_from_length_um_values,
        detour_margin_um_values,
        min_hold_jtl_length_um_values,
        sfq_phase_count_values,
        characterized_library_json,
        characterized_library_entries,
    )


__all__ = [
    "DEFAULT_CLOCK_PERIOD_PS",
    "DEFAULT_INPUT_ARRIVAL_PS",
    "DEFAULT_MIN_HOLD_JTL_LENGTH_UM",
    "DEFAULT_SFQ_PHASE_COUNT",
    "DEFAULT_SFQ_PULSE_WINDOW_PS",
    "AcBiasOptimizationReport",
    "AcBiasReport",
    "BalanceStrategy",
    "BlockedRegion",
    "CellLibraryEntry",
    "CellLibraryMetadata",
    "CellLibrarySummary",
    "CompilePlan",
    "CompileReport",
    "CompoundCellCharacterizationReport",
    "ConnectionSpec",
    "FixedNodePlacement",
    "LayoutReport",
    "LibraryAwareAcBiasOptimizationReport",
    "LibraryAwareDesignOptimizationReport",
    "PinRef",
    "SynthesisReport",
    "analyze_ac_bias",
    "characterize_compound_cell",
    "compile",
    "compile_layout",
    "compile_netlist",
    "compile_plan",
    "compile_plan_report",
    "merge_characterized_library",
    "optimize_ac_bias",
    "optimize_ac_bias_with_characterized_library",
    "optimize_design_with_characterized_library",
    "build_clock_tree",
    "build_bias_grid",
    "export_gds",
    "export_svg",
    "run_dse",
]
