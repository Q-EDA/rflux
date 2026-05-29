use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use rflux_flow::{
    AcBiasOptimizationReport, AcBiasReport, AdvancedConstraintConfig, AdvancedConstraintReport,
    AdvancedConstraintViolation, CompoundCellCharacterizationConfig,
    CompoundCellCharacterizationReport, FlowConfig, FlowRunner, LayoutReport,
    LibraryAwareAcBiasOptimizationReport, LibraryAwareDesignOptimizationReport,
    MultiCornerTimingAnalysisReport, SimulationBackend, SimulationConfig, SimulationMode,
    SimulationReport, StatisticalTimingAnalysisReport, TimingAnalysisReport,
    TimingCornerAnalysisReport, VerificationReport,
};
use rflux_io::{read_bench_netlist, read_netlist_as, NetlistInputFormat};
use rflux_ir::{LogicOp, Netlist, NodeKind, PinRef};
use rflux_place::{FixedNodePlacement, Point};
use rflux_route::{BlockedRegion, RouteMode};
use rflux_sim::{
    is_supported_external_command as is_supported_external_command_core,
    simulate_file as simulate_file_core,
    simulate_text as simulate_text_core,
};
use rflux_synth::{
    BalanceStrategy, BoolOptConfig, CompilePlan, CompileReport, Compiler, ConnectionSpec,
    SynthesisConfig, SynthesisReport,
};
use rflux_tech::{Pdk, SfCellKind};
use rflux_timing::{
    ClockDomainConstraint, CrossingConstraint, CrossingConstraintKind, NodeTimingConstraint,
    PinTimingConstraint, StatisticalTimingConfig, TimingConfig,
};
use rflux_verify::Verifier;
use std::fs;

#[derive(Clone)]
#[pyclass]
struct PyEquivalenceInputAssignment {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    value: bool,
}

#[derive(Clone)]
#[pyclass]
struct PyOutputMismatchEntry {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    lhs: bool,
    #[pyo3(get)]
    rhs: bool,
}

#[derive(Clone)]
#[pyclass]
struct PyStateMismatchEntry {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    lhs_next: bool,
    #[pyo3(get)]
    rhs_next: bool,
    #[pyo3(get)]
    lhs_clock: bool,
    #[pyo3(get)]
    rhs_clock: bool,
}

#[derive(Clone)]
#[pyclass]
struct PyCombinationalEquivalenceReport {
    #[pyo3(get)]
    equivalent: bool,
    #[pyo3(get)]
    checked_outputs: Vec<String>,
    #[pyo3(get)]
    counterexample_inputs: Vec<PyEquivalenceInputAssignment>,
    #[pyo3(get)]
    counterexample_outputs: Vec<PyOutputMismatchEntry>,
    #[pyo3(get)]
    sat_recursive_calls: usize,
    #[pyo3(get)]
    sat_decisions: usize,
    #[pyo3(get)]
    sat_backtracks: usize,
    #[pyo3(get)]
    sat_restarts: usize,
    #[pyo3(get)]
    sat_elapsed_ns: u128,
}

#[derive(Clone)]
#[pyclass]
struct PySingleStepSequentialEquivalenceReport {
    #[pyo3(get)]
    equivalent: bool,
    #[pyo3(get)]
    checked_outputs: Vec<String>,
    #[pyo3(get)]
    checked_states: Vec<String>,
    #[pyo3(get)]
    counterexample_inputs: Vec<PyEquivalenceInputAssignment>,
    #[pyo3(get)]
    counterexample_present_states: Vec<PyEquivalenceInputAssignment>,
    #[pyo3(get)]
    counterexample_outputs: Vec<PyOutputMismatchEntry>,
    #[pyo3(get)]
    counterexample_states: Vec<PyStateMismatchEntry>,
    #[pyo3(get)]
    sat_recursive_calls: usize,
    #[pyo3(get)]
    sat_decisions: usize,
    #[pyo3(get)]
    sat_backtracks: usize,
    #[pyo3(get)]
    sat_restarts: usize,
    #[pyo3(get)]
    sat_elapsed_ns: u128,
}

#[derive(Clone)]
#[pyclass]
struct PyBoundedSequentialEquivalenceStepReport {
    #[pyo3(get)]
    step: usize,
    #[pyo3(get)]
    report: PySingleStepSequentialEquivalenceReport,
}

#[derive(Clone)]
#[pyclass]
struct PyBoundedSequentialEquivalenceReport {
    #[pyo3(get)]
    equivalent: bool,
    #[pyo3(get)]
    depth: usize,
    #[pyo3(get)]
    checked_steps: usize,
    #[pyo3(get)]
    unroll_mode: String,
    #[pyo3(get)]
    checked_outputs: Vec<String>,
    #[pyo3(get)]
    checked_states: Vec<String>,
    #[pyo3(get)]
    first_failing_step: Option<usize>,
    #[pyo3(get)]
    steps: Vec<PyBoundedSequentialEquivalenceStepReport>,
    #[pyo3(get)]
    sat_recursive_calls: usize,
    #[pyo3(get)]
    sat_decisions: usize,
    #[pyo3(get)]
    sat_backtracks: usize,
    #[pyo3(get)]
    sat_restarts: usize,
    #[pyo3(get)]
    sat_elapsed_ns: u128,
}

impl From<rflux_verify::CombinationalEquivalenceReport> for PyCombinationalEquivalenceReport {
    fn from(value: rflux_verify::CombinationalEquivalenceReport) -> Self {
        Self {
            equivalent: value.equivalent,
            checked_outputs: value.checked_outputs,
            counterexample_inputs: value
                .counterexample_inputs
                .unwrap_or_default()
                .into_iter()
                .map(|(name, value)| PyEquivalenceInputAssignment { name, value })
                .collect(),
            counterexample_outputs: value
                .counterexample_outputs
                .unwrap_or_default()
                .into_iter()
                .map(|(name, mismatch)| PyOutputMismatchEntry {
                    name,
                    lhs: mismatch.lhs,
                    rhs: mismatch.rhs,
                })
                .collect(),
            sat_recursive_calls: value.sat_stats.recursive_calls,
            sat_decisions: value.sat_stats.decisions,
            sat_backtracks: value.sat_stats.backtracks,
            sat_restarts: value.sat_stats.restarts,
            sat_elapsed_ns: value.sat_elapsed_ns,
        }
    }
}

impl From<rflux_verify::SingleStepSequentialEquivalenceReport>
    for PySingleStepSequentialEquivalenceReport
{
    fn from(value: rflux_verify::SingleStepSequentialEquivalenceReport) -> Self {
        Self {
            equivalent: value.equivalent,
            checked_outputs: value.checked_outputs,
            checked_states: value.checked_states,
            counterexample_inputs: value
                .counterexample_inputs
                .unwrap_or_default()
                .into_iter()
                .map(|(name, value)| PyEquivalenceInputAssignment { name, value })
                .collect(),
            counterexample_present_states: value
                .counterexample_present_states
                .unwrap_or_default()
                .into_iter()
                .map(|(name, value)| PyEquivalenceInputAssignment { name, value })
                .collect(),
            counterexample_outputs: value
                .counterexample_outputs
                .unwrap_or_default()
                .into_iter()
                .map(|(name, mismatch)| PyOutputMismatchEntry {
                    name,
                    lhs: mismatch.lhs,
                    rhs: mismatch.rhs,
                })
                .collect(),
            counterexample_states: value
                .counterexample_states
                .unwrap_or_default()
                .into_iter()
                .map(|(name, mismatch)| PyStateMismatchEntry {
                    name,
                    lhs_next: mismatch.lhs_next,
                    rhs_next: mismatch.rhs_next,
                    lhs_clock: mismatch.lhs_clock,
                    rhs_clock: mismatch.rhs_clock,
                })
                .collect(),
            sat_recursive_calls: value.sat_stats.recursive_calls,
            sat_decisions: value.sat_stats.decisions,
            sat_backtracks: value.sat_stats.backtracks,
            sat_restarts: value.sat_stats.restarts,
            sat_elapsed_ns: value.sat_elapsed_ns,
        }
    }
}

#[pyclass]
struct Circuit {
    #[pyo3(get, set)]
    name: String,
    netlist: Netlist,
}

#[pyclass]
#[derive(Clone)]
struct PyPinRef {
    #[pyo3(get, set)]
    node: usize,
    #[pyo3(get, set)]
    port: u16,
}

#[pymethods]
impl PyPinRef {
    #[new]
    fn new(node: usize, port: u16) -> Self {
        Self { node, port }
    }
}

#[pyclass]
#[derive(Clone)]
struct PyConnectionSpec {
    #[pyo3(get, set)]
    from_pin: PyPinRef,
    #[pyo3(get, set)]
    to_pin: PyPinRef,
}

#[pyclass]
#[derive(Clone)]
struct PyFixedNodePlacement {
    #[pyo3(get, set)]
    node: usize,
    #[pyo3(get, set)]
    x_um: f64,
    #[pyo3(get, set)]
    y_um: f64,
}

#[pyclass]
#[derive(Clone)]
struct PyBlockedRegion {
    #[pyo3(get, set)]
    min_x_um: f64,
    #[pyo3(get, set)]
    max_x_um: f64,
    #[pyo3(get, set)]
    min_y_um: f64,
    #[pyo3(get, set)]
    max_y_um: f64,
}

#[pyclass]
#[derive(Clone)]
struct PyNodeTimingConstraint {
    #[pyo3(get, set)]
    node: usize,
    #[pyo3(get, set)]
    input_arrival_ps: Option<f64>,
    #[pyo3(get, set)]
    required_ps: Option<f64>,
    #[pyo3(get, set)]
    clock_domain: Option<usize>,
}

#[pyclass]
#[derive(Clone)]
struct PyClockDomainConstraint {
    #[pyo3(get, set)]
    id: usize,
    #[pyo3(get, set)]
    period_ps: f64,
}

#[pyclass]
#[derive(Clone)]
struct PyPinTimingConstraint {
    #[pyo3(get, set)]
    pin: PyPinRef,
    #[pyo3(get, set)]
    input_arrival_ps: Option<f64>,
    #[pyo3(get, set)]
    required_ps: Option<f64>,
    #[pyo3(get, set)]
    clock_domain: Option<usize>,
}

#[pyclass]
#[derive(Clone)]
struct PyCrossingConstraint {
    #[pyo3(get, set)]
    from_domain: usize,
    #[pyo3(get, set)]
    to_domain: usize,
    #[pyo3(get, set)]
    kind: String,
    #[pyo3(get, set)]
    value_ps: Option<f64>,
    #[pyo3(get, set)]
    cycles: Option<usize>,
}

#[pymethods]
impl PyBlockedRegion {
    #[new]
    fn new(min_x_um: f64, max_x_um: f64, min_y_um: f64, max_y_um: f64) -> Self {
        Self {
            min_x_um,
            max_x_um,
            min_y_um,
            max_y_um,
        }
    }
}

#[pymethods]
impl PyNodeTimingConstraint {
    #[new]
    #[pyo3(signature = (node, input_arrival_ps=None, required_ps=None, clock_domain=None))]
    fn new(
        node: usize,
        input_arrival_ps: Option<f64>,
        required_ps: Option<f64>,
        clock_domain: Option<usize>,
    ) -> Self {
        Self {
            node,
            input_arrival_ps,
            required_ps,
            clock_domain,
        }
    }
}

#[pymethods]
impl PyClockDomainConstraint {
    #[new]
    fn new(id: usize, period_ps: f64) -> Self {
        Self { id, period_ps }
    }
}

#[pymethods]
impl PyPinTimingConstraint {
    #[new]
    #[pyo3(signature = (pin, input_arrival_ps=None, required_ps=None, clock_domain=None))]
    fn new(
        pin: PyPinRef,
        input_arrival_ps: Option<f64>,
        required_ps: Option<f64>,
        clock_domain: Option<usize>,
    ) -> Self {
        Self {
            pin,
            input_arrival_ps,
            required_ps,
            clock_domain,
        }
    }
}

#[pymethods]
impl PyCrossingConstraint {
    #[new]
    #[pyo3(signature = (from_domain, to_domain, kind, value_ps=None, cycles=None))]
    fn new(
        from_domain: usize,
        to_domain: usize,
        kind: String,
        value_ps: Option<f64>,
        cycles: Option<usize>,
    ) -> Self {
        Self {
            from_domain,
            to_domain,
            kind,
            value_ps,
            cycles,
        }
    }
}

#[pymethods]
impl PyFixedNodePlacement {
    #[new]
    fn new(node: usize, x_um: f64, y_um: f64) -> Self {
        Self { node, x_um, y_um }
    }
}

#[pymethods]
impl PyConnectionSpec {
    #[new]
    fn new(from_pin: PyPinRef, to_pin: PyPinRef) -> Self {
        Self { from_pin, to_pin }
    }
}

#[pyclass]
struct PyCompilePlan {
    #[pyo3(get, set)]
    connections: Vec<PyConnectionSpec>,
    #[pyo3(get, set)]
    balance_strategy: String,
    #[pyo3(get, set)]
    balancing_sources: Vec<PyPinRef>,
}

#[pymethods]
impl PyCompilePlan {
    #[new]
    #[pyo3(signature = (connections=None, balance_strategy=None, balancing_sources=None))]
    fn new(
        connections: Option<Vec<PyConnectionSpec>>,
        balance_strategy: Option<String>,
        balancing_sources: Option<Vec<PyPinRef>>,
    ) -> Self {
        Self {
            connections: connections.unwrap_or_default(),
            balance_strategy: balance_strategy.unwrap_or_else(|| "none".to_string()),
            balancing_sources: balancing_sources.unwrap_or_default(),
        }
    }
}

#[pyclass]
struct PyCompileReport {
    #[pyo3(get)]
    connections_applied: usize,
    #[pyo3(get)]
    splitters_inserted: usize,
    #[pyo3(get)]
    balancing_dffs_inserted: usize,
}

impl From<CompileReport> for PyCompileReport {
    fn from(value: CompileReport) -> Self {
        Self {
            connections_applied: value.connections_applied,
            splitters_inserted: value.splitters_inserted,
            balancing_dffs_inserted: value.balancing_dffs_inserted,
        }
    }
}

#[pyclass]
struct PySynthesisReport {
    #[pyo3(get)]
    connections_applied: usize,
    #[pyo3(get)]
    splitters_inserted: usize,
    #[pyo3(get)]
    balancing_dffs_inserted: usize,
    #[pyo3(get)]
    bool_gate_count_before: usize,
    #[pyo3(get)]
    bool_gate_count_after: usize,
    #[pyo3(get)]
    mapped_nodes: usize,
    #[pyo3(get)]
    total_area_um2: f64,
    #[pyo3(get)]
    path_balance_insertions: usize,
    #[pyo3(get)]
    bool_opt_compatible: bool,
    #[pyo3(get)]
    node_count: usize,
    #[pyo3(get)]
    edge_count: usize,
}

impl From<SynthesisReport> for PySynthesisReport {
    fn from(value: SynthesisReport) -> Self {
        Self {
            connections_applied: value.compile.connections_applied,
            splitters_inserted: value.compile.splitters_inserted,
            balancing_dffs_inserted: value.compile.balancing_dffs_inserted,
            bool_gate_count_before: value.bool_opt.gate_count_before,
            bool_gate_count_after: value.bool_opt.gate_count_after,
            mapped_nodes: value.tech_map.mapped_nodes,
            total_area_um2: value.tech_map.total_area_um2,
            path_balance_insertions: value.path_balance.total_insertions(),
            bool_opt_compatible: value.bool_opt_compatibility.is_compatible(),
            node_count: value.node_count,
            edge_count: value.edge_count,
        }
    }
}

#[pyclass]
struct PyLayoutReport {
    #[pyo3(get)]
    connections_applied: usize,
    #[pyo3(get)]
    splitters_inserted: usize,
    #[pyo3(get)]
    balancing_dffs_inserted: usize,
    #[pyo3(get)]
    mapped_nodes: usize,
    #[pyo3(get)]
    total_area_um2: f64,
    #[pyo3(get)]
    bool_opt_compatible: bool,
    #[pyo3(get)]
    placed_nodes: usize,
    #[pyo3(get)]
    placement_width_um: f64,
    #[pyo3(get)]
    placement_height_um: f64,
    #[pyo3(get)]
    clock_sinks: usize,
    #[pyo3(get)]
    clock_buffers: usize,
    #[pyo3(get)]
    clock_phase_count: usize,
    #[pyo3(get)]
    initial_hold_violations: usize,
    #[pyo3(get)]
    final_hold_violations: usize,
    #[pyo3(get)]
    hold_fix_applied: bool,
    #[pyo3(get)]
    worst_setup_slack_ps: f64,
    #[pyo3(get)]
    worst_hold_slack_ps: f64,
    #[pyo3(get)]
    critical_path_delay_ps: f64,
    #[pyo3(get)]
    analyzed_timing_arcs: usize,
    #[pyo3(get)]
    false_path_arcs: usize,
    #[pyo3(get)]
    setup_violations: usize,
    #[pyo3(get)]
    capture_window_violations: usize,
    #[pyo3(get)]
    timing_closure: PyTimingClosureSummary,
    #[pyo3(get)]
    timing_closure_loop: PyTimingClosureLoopReport,
    #[pyo3(get)]
    routed_nets: usize,
    #[pyo3(get)]
    total_route_length_um: f64,
    #[pyo3(get)]
    initial_total_detour_overhead_um: f64,
    #[pyo3(get)]
    total_detour_overhead_um: f64,
    #[pyo3(get)]
    detoured_routes: usize,
    #[pyo3(get)]
    detour_feedback_applied: bool,
    #[pyo3(get)]
    effective_prefer_ptl_from_length_um: f64,
    #[pyo3(get)]
    effective_detour_margin_um: f64,
    #[pyo3(get)]
    jtl_routes: usize,
    #[pyo3(get)]
    ptl_routes: usize,
    #[pyo3(get)]
    node_count: usize,
    #[pyo3(get)]
    edge_count: usize,
}

#[pyclass]
#[derive(Clone)]
struct PyTimingClosureLoopReport {
    #[pyo3(get)]
    detour_feedback_attempted: bool,
    #[pyo3(get)]
    detour_feedback_applied: bool,
    #[pyo3(get)]
    initial_total_detour_overhead_um: f64,
    #[pyo3(get)]
    final_total_detour_overhead_um: f64,
    #[pyo3(get)]
    route_delay_optimization_attempted: bool,
    #[pyo3(get)]
    route_delay_optimization_applied: bool,
    #[pyo3(get)]
    reduce_route_delay_candidate_available: bool,
    #[pyo3(get)]
    recommended_prefer_ptl_from_length_um: Option<f64>,
    #[pyo3(get)]
    recommended_detour_margin_um: Option<f64>,
    #[pyo3(get)]
    recommended_route_mode: Option<String>,
    #[pyo3(get)]
    estimated_route_length_um: Option<f64>,
    #[pyo3(get)]
    estimated_slack_deficit_ps: Option<f64>,
    #[pyo3(get)]
    reduce_route_delay_candidate_attempted: bool,
    #[pyo3(get)]
    reduce_route_delay_candidate_improved: bool,
    #[pyo3(get)]
    candidate_worst_setup_slack_ps: Option<f64>,
    #[pyo3(get)]
    candidate_setup_violations: Option<usize>,
    #[pyo3(get)]
    candidate_hold_violations: Option<usize>,
    #[pyo3(get)]
    candidate_route_mode: Option<String>,
    #[pyo3(get)]
    candidate_route_length_um: Option<f64>,
    #[pyo3(get)]
    hold_fix_attempted: bool,
    #[pyo3(get)]
    hold_fix_applied: bool,
    #[pyo3(get)]
    initial_hold_violations: usize,
    #[pyo3(get)]
    final_hold_violations: usize,
    #[pyo3(get)]
    status: String,
    #[pyo3(get)]
    next_step: String,
}

#[pyclass]
#[derive(Clone)]
struct PyTimingClosureSummary {
    #[pyo3(get)]
    closed: bool,
    #[pyo3(get)]
    status: String,
    #[pyo3(get)]
    setup_closed: bool,
    #[pyo3(get)]
    hold_closed: bool,
    #[pyo3(get)]
    capture_window_closed: bool,
    #[pyo3(get)]
    setup_violations: usize,
    #[pyo3(get)]
    hold_violations: usize,
    #[pyo3(get)]
    capture_window_violations: usize,
    #[pyo3(get)]
    failing_checks: Vec<String>,
    #[pyo3(get)]
    action_count: usize,
    #[pyo3(get)]
    primary_action: Option<PyTimingClosureAction>,
    #[pyo3(get)]
    reduce_route_delay_actions: usize,
    #[pyo3(get)]
    relax_constraint_or_improve_library_timing_actions: usize,
    #[pyo3(get)]
    add_hold_padding_actions: usize,
    #[pyo3(get)]
    adjust_sfq_phase_or_pulse_window_actions: usize,
    #[pyo3(get)]
    actions: Vec<PyTimingClosureAction>,
    #[pyo3(get)]
    next_step: String,
}

#[pyclass]
#[derive(Clone)]
struct PyTimingClosureAction {
    #[pyo3(get)]
    check: String,
    #[pyo3(get)]
    priority: usize,
    #[pyo3(get)]
    remediation_kind: String,
    #[pyo3(get)]
    from_pin: PyPinRef,
    #[pyo3(get)]
    to_pin: PyPinRef,
    #[pyo3(get)]
    slack_ps: f64,
    #[pyo3(get)]
    route_mode: String,
    #[pyo3(get)]
    route_length_um: f64,
    #[pyo3(get)]
    from_domain: Option<usize>,
    #[pyo3(get)]
    to_domain: Option<usize>,
}

#[pyclass]
#[derive(Clone)]
struct PyTimingArcReport {
    #[pyo3(get)]
    from_pin: PyPinRef,
    #[pyo3(get)]
    to_pin: PyPinRef,
    #[pyo3(get)]
    is_false_path: bool,
    #[pyo3(get)]
    route_mode: String,
    #[pyo3(get)]
    route_length_um: f64,
    #[pyo3(get)]
    from_domain: Option<usize>,
    #[pyo3(get)]
    to_domain: Option<usize>,
    #[pyo3(get)]
    launch_phase: usize,
    #[pyo3(get)]
    capture_phase: usize,
    #[pyo3(get)]
    launch_window_start_ps: f64,
    #[pyo3(get)]
    launch_window_end_ps: f64,
    #[pyo3(get)]
    capture_window_start_ps: f64,
    #[pyo3(get)]
    capture_window_end_ps: f64,
    #[pyo3(get)]
    arrival_phase_offset_ps: f64,
    #[pyo3(get)]
    capture_window_slack_ps: f64,
    #[pyo3(get)]
    capture_window_violation: bool,
    #[pyo3(get)]
    arrival_ps: f64,
    #[pyo3(get)]
    required_ps: f64,
    #[pyo3(get)]
    setup_slack_ps: f64,
    #[pyo3(get)]
    hold_slack_ps: f64,
}

#[pyclass]
struct PyTimingAnalysisReport {
    #[pyo3(get)]
    worst_setup_slack_ps: f64,
    #[pyo3(get)]
    worst_hold_slack_ps: f64,
    #[pyo3(get)]
    critical_path_delay_ps: f64,
    #[pyo3(get)]
    analyzed_timing_arcs: usize,
    #[pyo3(get)]
    false_path_arcs: usize,
    #[pyo3(get)]
    setup_violations: usize,
    #[pyo3(get)]
    hold_violations: usize,
    #[pyo3(get)]
    capture_window_violations: usize,
    #[pyo3(get)]
    detour_feedback_applied: bool,
    #[pyo3(get)]
    hold_fix_applied: bool,
    #[pyo3(get)]
    closure: PyTimingClosureSummary,
    #[pyo3(get)]
    timing_arcs: Vec<PyTimingArcReport>,
}

#[pyclass]
#[derive(Clone)]
struct PyTimingCornerAnalysisReport {
    #[pyo3(get)]
    corner_name: String,
    #[pyo3(get)]
    is_default_corner: bool,
    #[pyo3(get)]
    is_active_corner: bool,
    #[pyo3(get)]
    worst_setup_slack_ps: f64,
    #[pyo3(get)]
    worst_hold_slack_ps: f64,
    #[pyo3(get)]
    critical_path_delay_ps: f64,
    #[pyo3(get)]
    analyzed_timing_arcs: usize,
    #[pyo3(get)]
    setup_violations: usize,
    #[pyo3(get)]
    hold_violations: usize,
    #[pyo3(get)]
    capture_window_violations: usize,
    #[pyo3(get)]
    closure: PyTimingClosureSummary,
}

#[pyclass]
#[derive(Clone)]
struct PyMultiCornerTimingAnalysisReport {
    #[pyo3(get)]
    active_timing_corner: Option<String>,
    #[pyo3(get)]
    corner_count: usize,
    #[pyo3(get)]
    worst_setup_corner: String,
    #[pyo3(get)]
    worst_hold_corner: String,
    #[pyo3(get)]
    worst_critical_path_corner: String,
    #[pyo3(get)]
    worst_setup_slack_ps: f64,
    #[pyo3(get)]
    worst_hold_slack_ps: f64,
    #[pyo3(get)]
    worst_critical_path_delay_ps: f64,
    #[pyo3(get)]
    corners: Vec<PyTimingCornerAnalysisReport>,
}

#[pyclass]
#[derive(Clone)]
struct PyStatisticalTimingArcReport {
    #[pyo3(get)]
    from_pin: PyPinRef,
    #[pyo3(get)]
    to_pin: PyPinRef,
    #[pyo3(get)]
    is_false_path: bool,
    #[pyo3(get)]
    route_mode: String,
    #[pyo3(get)]
    route_length_um: f64,
    #[pyo3(get)]
    from_domain: Option<usize>,
    #[pyo3(get)]
    to_domain: Option<usize>,
    #[pyo3(get)]
    launch_phase: usize,
    #[pyo3(get)]
    capture_phase: usize,
    #[pyo3(get)]
    launch_window_start_ps: f64,
    #[pyo3(get)]
    launch_window_end_ps: f64,
    #[pyo3(get)]
    capture_window_start_ps: f64,
    #[pyo3(get)]
    capture_window_end_ps: f64,
    #[pyo3(get)]
    arrival_phase_offset_ps: f64,
    #[pyo3(get)]
    capture_window_slack_ps: f64,
    #[pyo3(get)]
    capture_window_violation: bool,
    #[pyo3(get)]
    mean_arrival_ps: f64,
    #[pyo3(get)]
    mean_required_ps: f64,
    #[pyo3(get)]
    setup_slack_ps: f64,
    #[pyo3(get)]
    hold_slack_ps: f64,
    #[pyo3(get)]
    setup_sigma_ps: f64,
    #[pyo3(get)]
    hold_sigma_ps: f64,
    #[pyo3(get)]
    pessimistic_setup_slack_ps: f64,
    #[pyo3(get)]
    pessimistic_hold_slack_ps: f64,
}

#[pyclass]
#[derive(Clone)]
struct PyStatisticalTimingAnalysisReport {
    #[pyo3(get)]
    worst_pessimistic_setup_slack_ps: f64,
    #[pyo3(get)]
    worst_pessimistic_hold_slack_ps: f64,
    #[pyo3(get)]
    analyzed_timing_arcs: usize,
    #[pyo3(get)]
    false_path_arcs: usize,
    #[pyo3(get)]
    setup_risk_violations: usize,
    #[pyo3(get)]
    hold_risk_violations: usize,
    #[pyo3(get)]
    sigma_multiplier: f64,
    #[pyo3(get)]
    timing_arcs: Vec<PyStatisticalTimingArcReport>,
}

#[pyclass]
#[derive(Clone)]
struct PyAcBiasReport {
    #[pyo3(get)]
    routed_nets: usize,
    #[pyo3(get)]
    jtl_carrier_candidates: usize,
    #[pyo3(get)]
    ptl_coupling_risk_routes: usize,
    #[pyo3(get)]
    clock_sink_count: usize,
    #[pyo3(get)]
    estimated_static_power_savings_uw: f64,
    #[pyo3(get)]
    estimated_area_overhead_ratio: f64,
    #[pyo3(get)]
    estimated_frequency_derate_ratio: f64,
    #[pyo3(get)]
    worst_setup_slack_ps: f64,
    #[pyo3(get)]
    worst_hold_slack_ps: f64,
    #[pyo3(get)]
    timing_guardband_score: f64,
    #[pyo3(get)]
    feasibility_score: f64,
    #[pyo3(get)]
    optimization_score: f64,
}

#[pyclass]
struct PyPdk {
    inner: Pdk,
}

#[pyclass]
#[derive(Clone)]
struct PyCellLibraryEntry {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    area_um2: f64,
    #[pyo3(get)]
    pipeline_stages: u8,
    #[pyo3(get)]
    intrinsic_delay_ps: f64,
    #[pyo3(get)]
    setup_ps: f64,
    #[pyo3(get)]
    hold_ps: f64,
    #[pyo3(get)]
    timing_source: String,
    #[pyo3(get)]
    has_characterization_metadata: bool,
}

#[pyclass]
#[derive(Clone)]
struct PyCellLibraryMetadata {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    version: Option<String>,
    #[pyo3(get)]
    source: Option<String>,
}

#[pyclass]
#[derive(Clone)]
struct PyCellLibrarySummary {
    #[pyo3(get)]
    cell_count: usize,
    #[pyo3(get)]
    kind_count: usize,
    kind_counts: Vec<(String, usize)>,
    #[pyo3(get)]
    named_timing_count: usize,
    #[pyo3(get)]
    kind_timing_count: usize,
    #[pyo3(get)]
    missing_timing_count: usize,
    #[pyo3(get)]
    characterized_cell_count: usize,
    #[pyo3(get)]
    named_timing_cells: Vec<String>,
    #[pyo3(get)]
    missing_timing_cells: Vec<String>,
    #[pyo3(get)]
    characterized_cells: Vec<String>,
}

impl From<rflux_tech::CellLibraryMetadata> for PyCellLibraryMetadata {
    fn from(metadata: rflux_tech::CellLibraryMetadata) -> Self {
        Self {
            name: metadata.name,
            version: metadata.version,
            source: metadata.source,
        }
    }
}

impl From<rflux_verify::BoundedSequentialEquivalenceReport>
    for PyBoundedSequentialEquivalenceReport
{
    fn from(value: rflux_verify::BoundedSequentialEquivalenceReport) -> Self {
        Self {
            equivalent: value.equivalent,
            depth: value.depth,
            checked_steps: value.checked_steps,
            unroll_mode: value.unroll_mode,
            checked_outputs: value.checked_outputs,
            checked_states: value.checked_states,
            first_failing_step: value.first_failing_step,
            steps: value
                .steps
                .into_iter()
                .map(|step| PyBoundedSequentialEquivalenceStepReport {
                    step: step.step,
                    report: step.report.into(),
                })
                .collect(),
            sat_recursive_calls: value.sat_stats.recursive_calls,
            sat_decisions: value.sat_stats.decisions,
            sat_backtracks: value.sat_stats.backtracks,
            sat_restarts: value.sat_stats.restarts,
            sat_elapsed_ns: value.sat_elapsed_ns,
        }
    }
}

impl From<rflux_tech::CellLibraryEntry> for PyCellLibraryEntry {
    fn from(entry: rflux_tech::CellLibraryEntry) -> Self {
        Self {
            name: entry.name,
            kind: sf_cell_kind_name(entry.kind).to_string(),
            area_um2: entry.area_um2,
            pipeline_stages: entry.pipeline_stages,
            intrinsic_delay_ps: entry.intrinsic_delay_ps,
            setup_ps: entry.setup_ps,
            hold_ps: entry.hold_ps,
            timing_source: entry.timing_source,
            has_characterization_metadata: entry.has_characterization_metadata,
        }
    }
}

impl From<rflux_tech::CellLibrarySummary> for PyCellLibrarySummary {
    fn from(summary: rflux_tech::CellLibrarySummary) -> Self {
        Self {
            cell_count: summary.cell_count,
            kind_count: summary.kind_count,
            kind_counts: summary
                .kind_counts
                .into_iter()
                .map(|(kind, count)| (sf_cell_kind_name(kind).to_string(), count))
                .collect(),
            named_timing_count: summary.named_timing_count,
            kind_timing_count: summary.kind_timing_count,
            missing_timing_count: summary.missing_timing_count,
            characterized_cell_count: summary.characterized_cell_count,
            named_timing_cells: summary.named_timing_cells,
            missing_timing_cells: summary.missing_timing_cells,
            characterized_cells: summary.characterized_cells,
        }
    }
}

#[pymethods]
impl PyCellLibrarySummary {
    #[getter]
    fn kind_counts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new_bound(py);
        for (kind, count) in &self.kind_counts {
            dict.set_item(kind, count)?;
        }
        Ok(dict)
    }
}

#[pymethods]
impl PyPdk {
    #[staticmethod]
    #[pyo3(signature = (name="py-minimal-pdk"))]
    fn minimal(name: &str) -> Self {
        Self {
            inner: Pdk::minimal(name),
        }
    }

    #[staticmethod]
    fn from_json(payload: &str) -> PyResult<Self> {
        Ok(Self {
            inner: Pdk::from_json(payload)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        })
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    fn to_json(&self) -> PyResult<String> {
        self.inner
            .to_json()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[getter]
    fn active_timing_corner(&self) -> Option<String> {
        self.inner.active_timing_corner.clone()
    }

    fn timing_corner_names(&self) -> Vec<String> {
        self.inner
            .timing_corner_names()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn with_active_timing_corner(&self, name: &str) -> Self {
        Self {
            inner: self.inner.with_active_timing_corner(name),
        }
    }

    #[getter]
    fn cell_library_name(&self) -> String {
        self.inner.cell_library_name().to_string()
    }

    #[getter]
    fn cell_library_version(&self) -> Option<String> {
        self.inner.cell_library_version().map(str::to_string)
    }

    #[getter]
    fn cell_library_source(&self) -> Option<String> {
        self.inner.cell_library_source().map(str::to_string)
    }

    fn cell_library_metadata(&self) -> PyCellLibraryMetadata {
        self.inner.cell_library_metadata().into()
    }

    fn cell_library_kinds(&self) -> Vec<String> {
        self.inner
            .cell_library_kinds()
            .into_iter()
            .map(|kind| sf_cell_kind_name(kind).to_string())
            .collect()
    }

    fn cell_library_entries(&self) -> Vec<PyCellLibraryEntry> {
        self.inner
            .cell_library_entries()
            .into_iter()
            .map(PyCellLibraryEntry::from)
            .collect()
    }

    fn cell_library_summary(&self) -> PyCellLibrarySummary {
        self.inner.cell_library_summary().into()
    }

    fn cell_library_entries_by_kind(&self, kind: &str) -> PyResult<Vec<PyCellLibraryEntry>> {
        Ok(self
            .inner
            .cell_library_entries_by_kind(parse_sf_cell_kind(kind)?)
            .into_iter()
            .map(PyCellLibraryEntry::from)
            .collect())
    }

    fn cell_library_entry(&self, cell_name: &str) -> Option<PyCellLibraryEntry> {
        self.inner
            .cell_library_entry(cell_name)
            .map(PyCellLibraryEntry::from)
    }

    fn merge_characterized_library_json(&self, serialized_entry: &str) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .with_characterized_library_json(serialized_entry)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        })
    }

    fn merge_characterized_library_entries(
        &self,
        serialized_entries: Vec<String>,
    ) -> PyResult<Self> {
        let references = serialized_entries
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        Ok(Self {
            inner: self
                .inner
                .merge_characterized_library_json_strings(&references)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        })
    }
}

#[pyclass]
#[derive(Clone)]
struct PyLibraryAwareDesignOptimizationReport {
    #[pyo3(get)]
    ac_bias: PyAcBiasOptimizationReport,
    #[pyo3(get)]
    baseline_statistical: PyStatisticalTimingAnalysisReport,
    #[pyo3(get)]
    optimized_statistical: PyStatisticalTimingAnalysisReport,
    #[pyo3(get)]
    baseline_constraints: PyAdvancedConstraintReport,
    #[pyo3(get)]
    optimized_constraints: PyAdvancedConstraintReport,
    #[pyo3(get)]
    characterized_cells_merged: usize,
    #[pyo3(get)]
    design_optimization_score: f64,
    #[pyo3(get)]
    baseline_cell_delay_sigma_ratio: f64,
    #[pyo3(get)]
    optimized_cell_delay_sigma_ratio: f64,
    #[pyo3(get)]
    baseline_sigma_multiplier: f64,
    #[pyo3(get)]
    optimized_sigma_multiplier: f64,
    #[pyo3(get)]
    baseline_placement_halo_scale: f64,
    #[pyo3(get)]
    optimized_placement_halo_scale: f64,
    #[pyo3(get)]
    placement_candidates_evaluated: usize,
    #[pyo3(get)]
    statistical_candidates_evaluated: usize,
}

#[pyclass]
#[derive(Clone)]
struct PyLibraryAwareAcBiasOptimizationReport {
    #[pyo3(get)]
    ac_bias: PyAcBiasOptimizationReport,
    #[pyo3(get)]
    baseline_constraints: PyAdvancedConstraintReport,
    #[pyo3(get)]
    optimized_constraints: PyAdvancedConstraintReport,
    #[pyo3(get)]
    characterized_cells_merged: usize,
    #[pyo3(get)]
    library_optimization_score: f64,
}

#[pyclass]
#[derive(Clone)]
struct PyAcBiasOptimizationReport {
    #[pyo3(get)]
    baseline: PyAcBiasReport,
    #[pyo3(get)]
    optimized: PyAcBiasReport,
    #[pyo3(get)]
    baseline_prefer_ptl_from_length_um: f64,
    #[pyo3(get)]
    optimized_prefer_ptl_from_length_um: f64,
    #[pyo3(get)]
    baseline_detour_margin_um: f64,
    #[pyo3(get)]
    optimized_detour_margin_um: f64,
    #[pyo3(get)]
    threshold_candidates_evaluated: usize,
    #[pyo3(get)]
    detour_margin_candidates_evaluated: usize,
    #[pyo3(get)]
    optimization_applied: bool,
}

#[pyclass]
#[derive(Clone)]
struct PySimulationEndpointRef {
    #[pyo3(get)]
    raw: String,
    #[pyo3(get)]
    node: String,
    #[pyo3(get)]
    port: Option<u16>,
}

#[pyclass]
#[derive(Clone)]
struct PySimulationDelayDetail {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    delay_ps: f64,
    #[pyo3(get)]
    from_ref: Option<PySimulationEndpointRef>,
    #[pyo3(get)]
    to_ref: Option<PySimulationEndpointRef>,
}

#[pyclass]
#[derive(Clone)]
struct PySimulationMeasurementDetail {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    measured_value: f64,
    #[pyo3(get)]
    at_ref: Option<PySimulationEndpointRef>,
}

#[pyclass]
#[derive(Clone)]
struct PySimulationMeasurementWarning {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    reason: String,
    #[pyo3(get)]
    at_ref: Option<PySimulationEndpointRef>,
}

#[pyclass]
#[derive(Clone)]
struct PySimulationViolationDetail {
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    detail: String,
    #[pyo3(get)]
    at_ref: Option<PySimulationEndpointRef>,
}

#[pyclass]
struct PySimulationReport {
    #[pyo3(get)]
    backend: String,
    #[pyo3(get)]
    josim_alignment_level: String,
    #[pyo3(get)]
    josim_alignment_available: bool,
    #[pyo3(get)]
    josim_next_step: String,
    #[pyo3(get)]
    josim_quality_passed: bool,
    #[pyo3(get)]
    josim_quality_status: String,
    #[pyo3(get)]
    simulated_events: usize,
    #[pyo3(get)]
    generated_deck_lines: usize,
    #[pyo3(get)]
    generated_deck_path: Option<String>,
    #[pyo3(get)]
    waveform_path: Option<String>,
    #[pyo3(get)]
    external_summary_contract: Option<String>,
    #[pyo3(get)]
    reported_violations: usize,
    #[pyo3(get)]
    reported_worst_delay_ps: Option<f64>,
    #[pyo3(get)]
    delay_details: Vec<PySimulationDelayDetail>,
    #[pyo3(get)]
    measurement_details: Vec<PySimulationMeasurementDetail>,
    #[pyo3(get)]
    measurement_warnings: Vec<PySimulationMeasurementWarning>,
    #[pyo3(get)]
    violation_details: Vec<PySimulationViolationDetail>,
    #[pyo3(get)]
    external_status_code: Option<i32>,
    #[pyo3(get)]
    external_result: Option<String>,
}

#[pyclass]
struct PyVerificationReport {
    #[pyo3(get)]
    checked_routes: usize,
    #[pyo3(get)]
    checked_ptl_routes: usize,
    #[pyo3(get)]
    structural_violations: usize,
    #[pyo3(get)]
    ptl_macro_boundary_violations: usize,
    #[pyo3(get)]
    ptl_forbidden_length_violations: usize,
    #[pyo3(get)]
    simulation_backend: String,
    #[pyo3(get)]
    josim_alignment_level: String,
    #[pyo3(get)]
    josim_alignment_available: bool,
    #[pyo3(get)]
    josim_next_step: String,
    #[pyo3(get)]
    josim_quality_passed: bool,
    #[pyo3(get)]
    josim_quality_status: String,
    #[pyo3(get)]
    simulated_events: usize,
    #[pyo3(get)]
    generated_deck_lines: usize,
    #[pyo3(get)]
    generated_deck_path: Option<String>,
    #[pyo3(get)]
    waveform_path: Option<String>,
    #[pyo3(get)]
    external_summary_contract: Option<String>,
    #[pyo3(get)]
    reported_violations: usize,
    #[pyo3(get)]
    reported_worst_delay_ps: Option<f64>,
    #[pyo3(get)]
    delay_details: Vec<PySimulationDelayDetail>,
    #[pyo3(get)]
    measurement_details: Vec<PySimulationMeasurementDetail>,
    #[pyo3(get)]
    measurement_warnings: Vec<PySimulationMeasurementWarning>,
    #[pyo3(get)]
    violation_details: Vec<PySimulationViolationDetail>,
    #[pyo3(get)]
    external_status_code: Option<i32>,
    #[pyo3(get)]
    external_result: Option<String>,
}

#[pyclass]
struct PyCompoundCellCharacterizationReport {
    #[pyo3(get)]
    cell_name: String,
    #[pyo3(get)]
    node_count: usize,
    #[pyo3(get)]
    edge_count: usize,
    #[pyo3(get)]
    mapped_nodes: usize,
    #[pyo3(get)]
    total_area_um2: f64,
    #[pyo3(get)]
    derived_intrinsic_delay_ps: f64,
    #[pyo3(get)]
    derived_setup_ps: f64,
    #[pyo3(get)]
    derived_hold_ps: f64,
    #[pyo3(get)]
    generated_cell_kind: String,
    #[pyo3(get)]
    generated_pipeline_stages: u8,
    #[pyo3(get)]
    generated_library_json: String,
    #[pyo3(get)]
    simulated_delay_ps: Option<f64>,
    #[pyo3(get)]
    simulation_backend: String,
    #[pyo3(get)]
    generated_deck_lines: usize,
    #[pyo3(get)]
    generated_deck_path: Option<String>,
    #[pyo3(get)]
    waveform_path: Option<String>,
    #[pyo3(get)]
    reported_violations: usize,
}

#[pyclass]
#[derive(Clone)]
struct PyAdvancedConstraintViolation {
    #[pyo3(get)]
    category: String,
    #[pyo3(get)]
    detail: String,
    #[pyo3(get)]
    measured_value: f64,
    #[pyo3(get)]
    limit_value: f64,
}

#[pyclass]
#[derive(Clone)]
struct PyAdvancedConstraintReport {
    #[pyo3(get)]
    estimated_thermal_load_uw: f64,
    #[pyo3(get)]
    estimated_mechanical_stress_score: f64,
    #[pyo3(get)]
    jtl_density_per_100um: f64,
    #[pyo3(get)]
    detour_overhead_ratio: f64,
    #[pyo3(get)]
    ptl_coupling_ratio: f64,
    #[pyo3(get)]
    manufacturing_hotspots: usize,
    #[pyo3(get)]
    violation_count: usize,
    #[pyo3(get)]
    violations: Vec<PyAdvancedConstraintViolation>,
}

impl From<LayoutReport> for PyLayoutReport {
    fn from(value: LayoutReport) -> Self {
        Self {
            connections_applied: value.synthesis.compile.connections_applied,
            splitters_inserted: value.synthesis.compile.splitters_inserted,
            balancing_dffs_inserted: value.synthesis.compile.balancing_dffs_inserted,
            mapped_nodes: value.synthesis.tech_map.mapped_nodes,
            total_area_um2: value.synthesis.tech_map.total_area_um2,
            bool_opt_compatible: value.synthesis.bool_opt_compatibility.is_compatible(),
            placed_nodes: value.placement.placed_nodes,
            placement_width_um: value.placement.width_um,
            placement_height_um: value.placement.height_um,
            clock_sinks: value.clock.clock_sinks,
            clock_buffers: value.clock.clock_buffers,
            clock_phase_count: value.clock.phase_count,
            initial_hold_violations: value.timing.initial_hold_violations,
            final_hold_violations: value.timing.final_hold_violations,
            hold_fix_applied: value.timing.hold_fix_applied,
            worst_setup_slack_ps: value.timing.worst_setup_slack_ps,
            worst_hold_slack_ps: value.timing.worst_hold_slack_ps,
            critical_path_delay_ps: value.timing.critical_path_delay_ps,
            analyzed_timing_arcs: value.timing.analyzed_arcs,
            false_path_arcs: value.timing.false_path_arcs,
            setup_violations: value.timing.setup_violations,
            capture_window_violations: value.timing.capture_window_violations,
            timing_closure: value.timing_closure.into(),
            timing_closure_loop: value.timing_closure_loop.into(),
            routed_nets: value.routing.routed_nets,
            total_route_length_um: value.routing.total_length_um,
            initial_total_detour_overhead_um: value.initial_total_detour_overhead_um,
            total_detour_overhead_um: value.routing.total_detour_overhead_um,
            detoured_routes: value.routing.detoured_routes,
            detour_feedback_applied: value.detour_feedback_applied,
            effective_prefer_ptl_from_length_um: value.routing.effective_prefer_ptl_from_length_um,
            effective_detour_margin_um: value.routing.effective_detour_margin_um,
            jtl_routes: value.routing.jtl_routes,
            ptl_routes: value.routing.ptl_routes,
            node_count: value.synthesis.node_count,
            edge_count: value.synthesis.edge_count,
        }
    }
}

impl From<rflux_flow::TimingClosureLoopReport> for PyTimingClosureLoopReport {
    fn from(value: rflux_flow::TimingClosureLoopReport) -> Self {
        Self {
            detour_feedback_attempted: value.detour_feedback_attempted,
            detour_feedback_applied: value.detour_feedback_applied,
            initial_total_detour_overhead_um: value.initial_total_detour_overhead_um,
            final_total_detour_overhead_um: value.final_total_detour_overhead_um,
            route_delay_optimization_attempted: value.route_delay_optimization_attempted,
            route_delay_optimization_applied: value.route_delay_optimization_applied,
            reduce_route_delay_candidate_available: value.reduce_route_delay_candidate_available,
            recommended_prefer_ptl_from_length_um: value.recommended_prefer_ptl_from_length_um,
            recommended_detour_margin_um: value.recommended_detour_margin_um,
            recommended_route_mode: value.recommended_route_mode.map(|mode| match mode {
                RouteMode::Jtl => "jtl".to_string(),
                RouteMode::Ptl => "ptl".to_string(),
            }),
            estimated_route_length_um: value.estimated_route_length_um,
            estimated_slack_deficit_ps: value.estimated_slack_deficit_ps,
            reduce_route_delay_candidate_attempted: value.reduce_route_delay_candidate_attempted,
            reduce_route_delay_candidate_improved: value.reduce_route_delay_candidate_improved,
            candidate_worst_setup_slack_ps: value.candidate_worst_setup_slack_ps,
            candidate_setup_violations: value.candidate_setup_violations,
            candidate_hold_violations: value.candidate_hold_violations,
            candidate_route_mode: value.candidate_route_mode.map(|mode| match mode {
                RouteMode::Jtl => "jtl".to_string(),
                RouteMode::Ptl => "ptl".to_string(),
            }),
            candidate_route_length_um: value.candidate_route_length_um,
            hold_fix_attempted: value.hold_fix_attempted,
            hold_fix_applied: value.hold_fix_applied,
            initial_hold_violations: value.initial_hold_violations,
            final_hold_violations: value.final_hold_violations,
            status: value.status,
            next_step: value.next_step,
        }
    }
}

impl From<rflux_flow::TimingClosureSummary> for PyTimingClosureSummary {
    fn from(value: rflux_flow::TimingClosureSummary) -> Self {
        Self {
            closed: value.closed,
            status: value.status,
            setup_closed: value.setup_closed,
            hold_closed: value.hold_closed,
            capture_window_closed: value.capture_window_closed,
            setup_violations: value.setup_violations,
            hold_violations: value.hold_violations,
            capture_window_violations: value.capture_window_violations,
            failing_checks: value.failing_checks,
            action_count: value.action_count,
            primary_action: value.primary_action.map(PyTimingClosureAction::from),
            reduce_route_delay_actions: value.reduce_route_delay_actions,
            relax_constraint_or_improve_library_timing_actions: value
                .relax_constraint_or_improve_library_timing_actions,
            add_hold_padding_actions: value.add_hold_padding_actions,
            adjust_sfq_phase_or_pulse_window_actions: value
                .adjust_sfq_phase_or_pulse_window_actions,
            actions: value
                .actions
                .into_iter()
                .map(PyTimingClosureAction::from)
                .collect(),
            next_step: value.next_step,
        }
    }
}

impl From<rflux_flow::TimingClosureAction> for PyTimingClosureAction {
    fn from(value: rflux_flow::TimingClosureAction) -> Self {
        Self {
            check: match value.check {
                rflux_flow::TimingClosureCheck::Setup => "setup".to_string(),
                rflux_flow::TimingClosureCheck::Hold => "hold".to_string(),
                rflux_flow::TimingClosureCheck::CaptureWindow => "capture_window".to_string(),
            },
            priority: value.priority,
            remediation_kind: match value.remediation_kind {
                rflux_flow::TimingClosureRemediationKind::ReduceRouteDelay => {
                    "reduce_route_delay".to_string()
                }
                rflux_flow::TimingClosureRemediationKind::RelaxConstraintOrImproveLibraryTiming => {
                    "relax_constraint_or_improve_library_timing".to_string()
                }
                rflux_flow::TimingClosureRemediationKind::AddHoldPadding => {
                    "add_hold_padding".to_string()
                }
                rflux_flow::TimingClosureRemediationKind::AdjustSfqPhaseOrPulseWindow => {
                    "adjust_sfq_phase_or_pulse_window".to_string()
                }
            },
            from_pin: PyPinRef {
                node: value.from.node.0,
                port: value.from.port,
            },
            to_pin: PyPinRef {
                node: value.to.node.0,
                port: value.to.port,
            },
            slack_ps: value.slack_ps,
            route_mode: match value.route_mode {
                RouteMode::Jtl => "jtl".to_string(),
                RouteMode::Ptl => "ptl".to_string(),
            },
            route_length_um: value.route_length_um,
            from_domain: value.from_domain,
            to_domain: value.to_domain,
        }
    }
}

impl From<TimingAnalysisReport> for PyTimingAnalysisReport {
    fn from(value: TimingAnalysisReport) -> Self {
        Self {
            worst_setup_slack_ps: value.worst_setup_slack_ps,
            worst_hold_slack_ps: value.worst_hold_slack_ps,
            critical_path_delay_ps: value.critical_path_delay_ps,
            analyzed_timing_arcs: value.analyzed_arcs,
            false_path_arcs: value.false_path_arcs,
            setup_violations: value.setup_violations,
            hold_violations: value.hold_violations,
            capture_window_violations: value.capture_window_violations,
            detour_feedback_applied: value.detour_feedback_applied,
            hold_fix_applied: value.hold_fix_applied,
            closure: value.closure.into(),
            timing_arcs: value
                .timing_arcs
                .into_iter()
                .map(|arc| PyTimingArcReport {
                    from_pin: PyPinRef {
                        node: arc.from.node.0,
                        port: arc.from.port,
                    },
                    to_pin: PyPinRef {
                        node: arc.to.node.0,
                        port: arc.to.port,
                    },
                    is_false_path: arc.is_false_path,
                    route_mode: match arc.route_mode {
                        RouteMode::Jtl => "jtl".to_string(),
                        RouteMode::Ptl => "ptl".to_string(),
                    },
                    route_length_um: arc.route_length_um,
                    from_domain: arc.from_domain,
                    to_domain: arc.to_domain,
                    launch_phase: arc.launch_phase,
                    capture_phase: arc.capture_phase,
                    launch_window_start_ps: arc.launch_window_start_ps,
                    launch_window_end_ps: arc.launch_window_end_ps,
                    capture_window_start_ps: arc.capture_window_start_ps,
                    capture_window_end_ps: arc.capture_window_end_ps,
                    arrival_phase_offset_ps: arc.arrival_phase_offset_ps,
                    capture_window_slack_ps: arc.capture_window_slack_ps,
                    capture_window_violation: arc.capture_window_violation,
                    arrival_ps: arc.arrival_ps,
                    required_ps: arc.required_ps,
                    setup_slack_ps: arc.setup_slack_ps,
                    hold_slack_ps: arc.hold_slack_ps,
                })
                .collect(),
        }
    }
}

impl From<TimingCornerAnalysisReport> for PyTimingCornerAnalysisReport {
    fn from(value: TimingCornerAnalysisReport) -> Self {
        Self {
            corner_name: value.corner_name,
            is_default_corner: value.is_default_corner,
            is_active_corner: value.is_active_corner,
            worst_setup_slack_ps: value.worst_setup_slack_ps,
            worst_hold_slack_ps: value.worst_hold_slack_ps,
            critical_path_delay_ps: value.critical_path_delay_ps,
            analyzed_timing_arcs: value.analyzed_arcs,
            setup_violations: value.setup_violations,
            hold_violations: value.hold_violations,
            capture_window_violations: value.capture_window_violations,
            closure: value.closure.into(),
        }
    }
}

impl From<MultiCornerTimingAnalysisReport> for PyMultiCornerTimingAnalysisReport {
    fn from(value: MultiCornerTimingAnalysisReport) -> Self {
        Self {
            active_timing_corner: value.active_timing_corner,
            corner_count: value.corner_count,
            worst_setup_corner: value.worst_setup_corner,
            worst_hold_corner: value.worst_hold_corner,
            worst_critical_path_corner: value.worst_critical_path_corner,
            worst_setup_slack_ps: value.worst_setup_slack_ps,
            worst_hold_slack_ps: value.worst_hold_slack_ps,
            worst_critical_path_delay_ps: value.worst_critical_path_delay_ps,
            corners: value
                .corners
                .into_iter()
                .map(PyTimingCornerAnalysisReport::from)
                .collect(),
        }
    }
}

impl From<StatisticalTimingAnalysisReport> for PyStatisticalTimingAnalysisReport {
    fn from(value: StatisticalTimingAnalysisReport) -> Self {
        Self {
            worst_pessimistic_setup_slack_ps: value.worst_pessimistic_setup_slack_ps,
            worst_pessimistic_hold_slack_ps: value.worst_pessimistic_hold_slack_ps,
            analyzed_timing_arcs: value.analyzed_arcs,
            false_path_arcs: value.false_path_arcs,
            setup_risk_violations: value.setup_risk_violations,
            hold_risk_violations: value.hold_risk_violations,
            sigma_multiplier: value.sigma_multiplier,
            timing_arcs: value
                .timing_arcs
                .into_iter()
                .map(|arc| PyStatisticalTimingArcReport {
                    from_pin: PyPinRef {
                        node: arc.from.node.0,
                        port: arc.from.port,
                    },
                    to_pin: PyPinRef {
                        node: arc.to.node.0,
                        port: arc.to.port,
                    },
                    is_false_path: arc.is_false_path,
                    route_mode: match arc.route_mode {
                        RouteMode::Jtl => "jtl".to_string(),
                        RouteMode::Ptl => "ptl".to_string(),
                    },
                    route_length_um: arc.route_length_um,
                    from_domain: arc.from_domain,
                    to_domain: arc.to_domain,
                    launch_phase: arc.launch_phase,
                    capture_phase: arc.capture_phase,
                    launch_window_start_ps: arc.launch_window_start_ps,
                    launch_window_end_ps: arc.launch_window_end_ps,
                    capture_window_start_ps: arc.capture_window_start_ps,
                    capture_window_end_ps: arc.capture_window_end_ps,
                    arrival_phase_offset_ps: arc.arrival_phase_offset_ps,
                    capture_window_slack_ps: arc.capture_window_slack_ps,
                    capture_window_violation: arc.capture_window_violation,
                    mean_arrival_ps: arc.mean_arrival_ps,
                    mean_required_ps: arc.mean_required_ps,
                    setup_slack_ps: arc.setup_slack_ps,
                    hold_slack_ps: arc.hold_slack_ps,
                    setup_sigma_ps: arc.setup_sigma_ps,
                    hold_sigma_ps: arc.hold_sigma_ps,
                    pessimistic_setup_slack_ps: arc.pessimistic_setup_slack_ps,
                    pessimistic_hold_slack_ps: arc.pessimistic_hold_slack_ps,
                })
                .collect(),
        }
    }
}

impl From<AcBiasReport> for PyAcBiasReport {
    fn from(value: AcBiasReport) -> Self {
        Self {
            routed_nets: value.routed_nets,
            jtl_carrier_candidates: value.jtl_carrier_candidates,
            ptl_coupling_risk_routes: value.ptl_coupling_risk_routes,
            clock_sink_count: value.clock_sink_count,
            estimated_static_power_savings_uw: value.estimated_static_power_savings_uw,
            estimated_area_overhead_ratio: value.estimated_area_overhead_ratio,
            estimated_frequency_derate_ratio: value.estimated_frequency_derate_ratio,
            worst_setup_slack_ps: value.worst_setup_slack_ps,
            worst_hold_slack_ps: value.worst_hold_slack_ps,
            timing_guardband_score: value.timing_guardband_score,
            feasibility_score: value.feasibility_score,
            optimization_score: value.optimization_score,
        }
    }
}

impl From<LibraryAwareDesignOptimizationReport> for PyLibraryAwareDesignOptimizationReport {
    fn from(value: LibraryAwareDesignOptimizationReport) -> Self {
        Self {
            ac_bias: value.ac_bias.into(),
            baseline_statistical: value.baseline_statistical.into(),
            optimized_statistical: value.optimized_statistical.into(),
            baseline_constraints: value.baseline_constraints.into(),
            optimized_constraints: value.optimized_constraints.into(),
            characterized_cells_merged: value.characterized_cells_merged,
            design_optimization_score: value.design_optimization_score,
            baseline_cell_delay_sigma_ratio: value
                .baseline_statistical_config
                .cell_delay_sigma_ratio,
            optimized_cell_delay_sigma_ratio: value
                .optimized_statistical_config
                .cell_delay_sigma_ratio,
            baseline_sigma_multiplier: value.baseline_statistical_config.sigma_multiplier,
            optimized_sigma_multiplier: value.optimized_statistical_config.sigma_multiplier,
            baseline_placement_halo_scale: value.baseline_placement_halo_scale,
            optimized_placement_halo_scale: value.optimized_placement_halo_scale,
            placement_candidates_evaluated: value.placement_candidates_evaluated,
            statistical_candidates_evaluated: value.statistical_candidates_evaluated,
        }
    }
}

impl From<LibraryAwareAcBiasOptimizationReport> for PyLibraryAwareAcBiasOptimizationReport {
    fn from(value: LibraryAwareAcBiasOptimizationReport) -> Self {
        Self {
            ac_bias: value.ac_bias.into(),
            baseline_constraints: value.baseline_constraints.into(),
            optimized_constraints: value.optimized_constraints.into(),
            characterized_cells_merged: value.characterized_cells_merged,
            library_optimization_score: value.library_optimization_score,
        }
    }
}

impl From<AcBiasOptimizationReport> for PyAcBiasOptimizationReport {
    fn from(value: AcBiasOptimizationReport) -> Self {
        Self {
            baseline: value.baseline.into(),
            optimized: value.optimized.into(),
            baseline_prefer_ptl_from_length_um: value.baseline_prefer_ptl_from_length_um,
            optimized_prefer_ptl_from_length_um: value.optimized_prefer_ptl_from_length_um,
            baseline_detour_margin_um: value.baseline_detour_margin_um,
            optimized_detour_margin_um: value.optimized_detour_margin_um,
            threshold_candidates_evaluated: value.threshold_candidates_evaluated,
            detour_margin_candidates_evaluated: value.detour_margin_candidates_evaluated,
            optimization_applied: value.optimization_applied,
        }
    }
}

impl From<SimulationReport> for PySimulationReport {
    fn from(value: SimulationReport) -> Self {
        let josim_gate = value.josim_quality_gate();
        Self {
            backend: simulation_backend_name(&value.backend).to_string(),
            josim_alignment_level: josim_gate.alignment_level,
            josim_alignment_available: josim_gate.external_alignment_available,
            josim_next_step: josim_gate.next_step,
            josim_quality_passed: josim_gate.passed,
            josim_quality_status: josim_gate.status,
            simulated_events: value.simulated_events,
            generated_deck_lines: value.generated_deck_lines,
            generated_deck_path: value.generated_deck_path,
            waveform_path: value.waveform_path,
            external_summary_contract: value.external_summary_contract,
            reported_violations: value.reported_violations,
            reported_worst_delay_ps: value.reported_worst_delay_ps,
            delay_details: value
                .delay_details
                .into_iter()
                .map(|detail| PySimulationDelayDetail {
                    name: detail.name,
                    delay_ps: detail.delay_ps,
                    from_ref: detail.from_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                    to_ref: detail.to_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            measurement_details: value
                .measurement_details
                .into_iter()
                .map(|detail| PySimulationMeasurementDetail {
                    name: detail.name,
                    kind: detail.kind,
                    measured_value: detail.measured_value,
                    at_ref: detail.at_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            measurement_warnings: value
                .measurement_warnings
                .into_iter()
                .map(|warning| PySimulationMeasurementWarning {
                    name: warning.name,
                    kind: warning.kind,
                    reason: warning.reason,
                    at_ref: warning.at_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            violation_details: value
                .violation_details
                .into_iter()
                .map(|detail| PySimulationViolationDetail {
                    kind: detail.kind,
                    detail: detail.detail,
                    at_ref: detail.at_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            external_status_code: value.external_status_code,
            external_result: value.external_result,
        }
    }
}

impl From<VerificationReport> for PyVerificationReport {
    fn from(value: VerificationReport) -> Self {
        let josim_gate = value.simulation.josim_quality_gate();
        Self {
            checked_routes: value.checked_routes,
            checked_ptl_routes: value.checked_ptl_routes,
            structural_violations: value.structural_violations,
            ptl_macro_boundary_violations: value.ptl_macro_boundary_violations,
            ptl_forbidden_length_violations: value.ptl_forbidden_length_violations,
            simulation_backend: simulation_backend_name(&value.simulation.backend).to_string(),
            josim_alignment_level: josim_gate.alignment_level,
            josim_alignment_available: josim_gate.external_alignment_available,
            josim_next_step: josim_gate.next_step,
            josim_quality_passed: josim_gate.passed,
            josim_quality_status: josim_gate.status,
            simulated_events: value.simulation.simulated_events,
            generated_deck_lines: value.simulation.generated_deck_lines,
            generated_deck_path: value.simulation.generated_deck_path,
            waveform_path: value.simulation.waveform_path,
            external_summary_contract: value.simulation.external_summary_contract,
            reported_violations: value.simulation.reported_violations,
            reported_worst_delay_ps: value.simulation.reported_worst_delay_ps,
            delay_details: value
                .simulation
                .delay_details
                .into_iter()
                .map(|detail| PySimulationDelayDetail {
                    name: detail.name,
                    delay_ps: detail.delay_ps,
                    from_ref: detail.from_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                    to_ref: detail.to_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            measurement_details: value
                .simulation
                .measurement_details
                .into_iter()
                .map(|detail| PySimulationMeasurementDetail {
                    name: detail.name,
                    kind: detail.kind,
                    measured_value: detail.measured_value,
                    at_ref: detail.at_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            measurement_warnings: value
                .simulation
                .measurement_warnings
                .into_iter()
                .map(|warning| PySimulationMeasurementWarning {
                    name: warning.name,
                    kind: warning.kind,
                    reason: warning.reason,
                    at_ref: warning.at_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            violation_details: value
                .simulation
                .violation_details
                .into_iter()
                .map(|detail| PySimulationViolationDetail {
                    kind: detail.kind,
                    detail: detail.detail,
                    at_ref: detail.at_ref.map(|endpoint| PySimulationEndpointRef {
                        raw: endpoint.raw,
                        node: endpoint.node,
                        port: endpoint.port,
                    }),
                })
                .collect(),
            external_status_code: value.simulation.external_status_code,
            external_result: value.simulation.external_result,
        }
    }
}

impl From<CompoundCellCharacterizationReport> for PyCompoundCellCharacterizationReport {
    fn from(value: CompoundCellCharacterizationReport) -> Self {
        Self {
            cell_name: value.cell_name,
            node_count: value.node_count,
            edge_count: value.edge_count,
            mapped_nodes: value.mapped_nodes,
            total_area_um2: value.total_area_um2,
            derived_intrinsic_delay_ps: value.derived_intrinsic_delay_ps,
            derived_setup_ps: value.derived_setup_ps,
            derived_hold_ps: value.derived_hold_ps,
            generated_cell_kind: value.generated_cell_kind,
            generated_pipeline_stages: value.generated_pipeline_stages,
            generated_library_json: value.generated_library_json,
            simulated_delay_ps: value.simulated_delay_ps,
            simulation_backend: simulation_backend_name(&value.simulation_backend).to_string(),
            generated_deck_lines: value.generated_deck_lines,
            generated_deck_path: value.generated_deck_path,
            waveform_path: value.waveform_path,
            reported_violations: value.reported_violations,
        }
    }
}

impl From<AdvancedConstraintViolation> for PyAdvancedConstraintViolation {
    fn from(value: AdvancedConstraintViolation) -> Self {
        Self {
            category: value.category,
            detail: value.detail,
            measured_value: value.measured_value,
            limit_value: value.limit_value,
        }
    }
}

impl From<AdvancedConstraintReport> for PyAdvancedConstraintReport {
    fn from(value: AdvancedConstraintReport) -> Self {
        Self {
            estimated_thermal_load_uw: value.estimated_thermal_load_uw,
            estimated_mechanical_stress_score: value.estimated_mechanical_stress_score,
            jtl_density_per_100um: value.jtl_density_per_100um,
            detour_overhead_ratio: value.detour_overhead_ratio,
            ptl_coupling_ratio: value.ptl_coupling_ratio,
            manufacturing_hotspots: value.manufacturing_hotspots,
            violation_count: value.violation_count,
            violations: value.violations.into_iter().map(Into::into).collect(),
        }
    }
}

fn simulation_backend_name(backend: &SimulationBackend) -> &'static str {
    match backend {
        SimulationBackend::EventOnly => "event_only",
        SimulationBackend::ExternalCompleted => "external_completed",
        SimulationBackend::ExternalFailed => "external_failed",
        SimulationBackend::ExternalUnavailable => "external_unavailable",
        SimulationBackend::InternalTransientCompleted => "internal_transient_completed",
        SimulationBackend::InternalTransientUnavailable => "internal_transient_unavailable",
    }
}

fn parse_simulation_mode(mode: Option<&str>) -> PyResult<SimulationMode> {
    match mode.unwrap_or("auto") {
        "auto" => Ok(SimulationMode::Auto),
        "event_only" => Ok(SimulationMode::EventOnly),
        "external_josim" => Ok(SimulationMode::ExternalJosim),
        "internal_transient" => Ok(SimulationMode::InternalTransient),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown simulation mode: {other}"
        ))),
    }
}

fn parse_balance_strategy(strategy: &str) -> PyResult<BalanceStrategy> {
    match strategy {
        "none" => Ok(BalanceStrategy::None),
        "explicit" => Ok(BalanceStrategy::Explicit),
        "all_connected_sources" => Ok(BalanceStrategy::AllConnectedSources),
        "by_sink_level" => Ok(BalanceStrategy::BySinkLevel),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown balance strategy: {strategy}"
        ))),
    }
}

fn to_pin_ref(pin: &PyPinRef) -> PinRef {
    PinRef {
        node: rflux_ir::NodeId(pin.node),
        port: pin.port,
    }
}

fn ensure_plan_nodes(netlist: &mut Netlist, plan: &PyCompilePlan) {
    let mut max_index = None::<usize>;
    let mut sink_nodes = std::collections::BTreeSet::new();
    let mut referenced_nodes = std::collections::BTreeSet::new();

    for connection in &plan.connections {
        referenced_nodes.insert(connection.from_pin.node);
        referenced_nodes.insert(connection.to_pin.node);
        sink_nodes.insert(connection.to_pin.node);
        max_index = Some(
            max_index.map_or(connection.from_pin.node.max(connection.to_pin.node), |m| {
                m.max(connection.from_pin.node).max(connection.to_pin.node)
            }),
        );
    }
    for pin in &plan.balancing_sources {
        referenced_nodes.insert(pin.node);
        max_index = Some(max_index.map_or(pin.node, |m| m.max(pin.node)));
    }

    let Some(max_index) = max_index else {
        return;
    };

    while netlist.node_count() <= max_index {
        let index = netlist.node_count();
        let kind = if sink_nodes.contains(&index) {
            NodeKind::CellInstance
        } else if referenced_nodes.contains(&index) {
            NodeKind::Port
        } else {
            NodeKind::CellInstance
        };
        let name = format!("py_node_{index}");
        netlist.add_node(kind, name);
    }
}

fn ensure_node_capacity(netlist: &mut Netlist, node_indices: impl IntoIterator<Item = usize>) {
    let max_index = node_indices.into_iter().max();
    let Some(max_index) = max_index else {
        return;
    };

    while netlist.node_count() <= max_index {
        let index = netlist.node_count();
        let name = format!("py_node_{index}");
        netlist.add_node(NodeKind::CellInstance, name);
    }
}

fn to_fixed_node_placement(fixed: &PyFixedNodePlacement) -> FixedNodePlacement {
    FixedNodePlacement {
        node: rflux_ir::NodeId(fixed.node),
        point: Point {
            x_um: fixed.x_um,
            y_um: fixed.y_um,
        },
    }
}

fn to_blocked_region(region: &PyBlockedRegion) -> BlockedRegion {
    BlockedRegion {
        min_x_um: region.min_x_um,
        max_x_um: region.max_x_um,
        min_y_um: region.min_y_um,
        max_y_um: region.max_y_um,
    }
}

fn to_timing_constraint(constraint: &PyNodeTimingConstraint) -> NodeTimingConstraint {
    NodeTimingConstraint {
        node: rflux_ir::NodeId(constraint.node),
        input_arrival_ps: constraint.input_arrival_ps,
        required_ps: constraint.required_ps,
        clock_domain: constraint.clock_domain,
    }
}

fn to_clock_domain_constraint(domain: &PyClockDomainConstraint) -> ClockDomainConstraint {
    ClockDomainConstraint {
        id: domain.id,
        period_ps: domain.period_ps,
    }
}

fn to_pin_timing_constraint(constraint: &PyPinTimingConstraint) -> PinTimingConstraint {
    PinTimingConstraint {
        pin: to_pin_ref(&constraint.pin),
        input_arrival_ps: constraint.input_arrival_ps,
        required_ps: constraint.required_ps,
        clock_domain: constraint.clock_domain,
    }
}

fn parse_crossing_constraint_kind(kind: &str) -> PyResult<CrossingConstraintKind> {
    match kind {
        "false_path" => Ok(CrossingConstraintKind::FalsePath),
        "max_delay" => Ok(CrossingConstraintKind::MaxDelay),
        "multicycle" => Ok(CrossingConstraintKind::Multicycle),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown crossing constraint kind: {kind}"
        ))),
    }
}

fn to_crossing_constraint(constraint: &PyCrossingConstraint) -> PyResult<CrossingConstraint> {
    Ok(CrossingConstraint {
        from_domain: constraint.from_domain,
        to_domain: constraint.to_domain,
        kind: parse_crossing_constraint_kind(&constraint.kind)?,
        value_ps: constraint.value_ps,
        cycles: constraint.cycles,
    })
}

fn to_flow_config(
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    min_hold_jtl_length_um: Option<f64>,
    prefer_ptl_from_length_um: Option<f64>,
    detour_margin_um: Option<f64>,
) -> PyResult<FlowConfig> {
    let synthesis_plan = if let Some(plan) = plan {
        CompilePlan {
            connections: plan
                .connections
                .iter()
                .map(|connection| ConnectionSpec {
                    from: to_pin_ref(&connection.from_pin),
                    to: to_pin_ref(&connection.to_pin),
                })
                .collect(),
            balance_strategy: parse_balance_strategy(&plan.balance_strategy)?,
            balancing_sources: plan.balancing_sources.iter().map(to_pin_ref).collect(),
        }
    } else {
        CompilePlan::default()
    };

    let blocked = blocked_regions.unwrap_or_default();
    let fixed = fixed_nodes.unwrap_or_default();
    let timing_constraints = timing_constraints.unwrap_or_default();
    let pin_timing_constraints = pin_timing_constraints.unwrap_or_default();
    let clock_domains = clock_domains.unwrap_or_default();
    let crossing_constraints = crossing_constraints.unwrap_or_default();

    Ok(FlowConfig {
        synthesis: SynthesisConfig {
            plan: synthesis_plan,
            bool_opt: BoolOptConfig::default(),
        },
        timing: TimingConfig {
            clock_period_ps: TimingConfig::default().clock_period_ps,
            input_arrival_ps: TimingConfig::default().input_arrival_ps,
            sfq_phase_count: TimingConfig::default().sfq_phase_count,
            sfq_pulse_window_ps: TimingConfig::default().sfq_pulse_window_ps,
            node_constraints: timing_constraints
                .iter()
                .map(to_timing_constraint)
                .collect(),
            pin_constraints: pin_timing_constraints
                .iter()
                .map(to_pin_timing_constraint)
                .collect(),
            clock_domains: clock_domains
                .iter()
                .map(to_clock_domain_constraint)
                .collect(),
            crossing_constraints: crossing_constraints
                .iter()
                .map(to_crossing_constraint)
                .collect::<PyResult<Vec<_>>>()?,
        },
        routing: rflux_route::RoutingConfig {
            blocked_regions: blocked.iter().map(to_blocked_region).collect(),
            prefer_ptl_from_length_um: prefer_ptl_from_length_um
                .unwrap_or(rflux_route::RoutingConfig::default().prefer_ptl_from_length_um),
            detour_margin_um: detour_margin_um
                .unwrap_or(rflux_route::RoutingConfig::default().detour_margin_um),
            ..rflux_route::RoutingConfig::default()
        },
        placement: rflux_place::PlacementConfig {
            fixed_nodes: fixed.iter().map(to_fixed_node_placement).collect(),
            blocked_regions: blocked
                .iter()
                .map(|region| rflux_place::BlockedRegion {
                    min_x_um: region.min_x_um,
                    max_x_um: region.max_x_um,
                    min_y_um: region.min_y_um,
                    max_y_um: region.max_y_um,
                })
                .collect(),
            ..rflux_place::PlacementConfig::default()
        },
        min_hold_jtl_length_um: min_hold_jtl_length_um.unwrap_or_default(),
        ..FlowConfig::default()
    })
}

fn parse_node_kind(kind: &str) -> PyResult<NodeKind> {
    match kind {
        "cell" | "cell_instance" => Ok(NodeKind::CellInstance),
        "macro" | "macro_cell" => Ok(NodeKind::MacroCell),
        "splitter" => Ok(NodeKind::Splitter),
        "dff" => Ok(NodeKind::Dff),
        "jtl" => Ok(NodeKind::Jtl),
        "ptl" => Ok(NodeKind::Ptl),
        "port" => Ok(NodeKind::Port),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown node kind: {kind}"
        ))),
    }
}

fn parse_logic_op(logic_op: &str) -> PyResult<LogicOp> {
    match logic_op {
        "buf" => Ok(LogicOp::Buf),
        "not" | "inv" => Ok(LogicOp::Not),
        "and" => Ok(LogicOp::And),
        "or" => Ok(LogicOp::Or),
        "xor" => Ok(LogicOp::Xor),
        "mux2" | "mux" => Ok(LogicOp::Mux2),
        "dffe" => Ok(LogicOp::DffEnable),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown logic op: {logic_op}"
        ))),
    }
}

fn node_kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::CellInstance => "cell_instance",
        NodeKind::MacroCell => "macro_cell",
        NodeKind::Splitter => "splitter",
        NodeKind::Dff => "dff",
        NodeKind::Jtl => "jtl",
        NodeKind::Ptl => "ptl",
        NodeKind::Port => "port",
    }
}

fn parse_sf_cell_kind(kind: &str) -> PyResult<SfCellKind> {
    match kind {
        "generic_gate" | "GenericGate" | "cell" | "cell_instance" => Ok(SfCellKind::GenericGate),
        "macro" | "Macro" | "macro_cell" => Ok(SfCellKind::Macro),
        "splitter" | "Splitter" => Ok(SfCellKind::Splitter),
        "dff" | "Dff" => Ok(SfCellKind::Dff),
        "jtl" | "Jtl" => Ok(SfCellKind::Jtl),
        "ptl" | "Ptl" => Ok(SfCellKind::Ptl),
        "port" | "Port" => Ok(SfCellKind::Port),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown cell kind: {kind}"
        ))),
    }
}

fn sf_cell_kind_name(kind: SfCellKind) -> &'static str {
    match kind {
        SfCellKind::GenericGate => "generic_gate",
        SfCellKind::Macro => "macro",
        SfCellKind::Splitter => "splitter",
        SfCellKind::Dff => "dff",
        SfCellKind::Jtl => "jtl",
        SfCellKind::Ptl => "ptl",
        SfCellKind::Port => "port",
    }
}

#[pymethods]
impl Circuit {
    #[new]
    #[pyo3(signature = (name=None))]
    fn new(name: Option<String>) -> Self {
        Self {
            name: name.unwrap_or_default(),
            netlist: Netlist::new(),
        }
    }

    #[pyo3(signature = (kind, name, logic_op=None))]
    fn add_node(&mut self, kind: &str, name: String, logic_op: Option<String>) -> PyResult<usize> {
        let node_kind = parse_node_kind(kind)?;
        let logic_op = logic_op.as_deref().map(parse_logic_op).transpose()?;
        Ok(self
            .netlist
            .add_node_with_logic(node_kind, name, logic_op)
            .0)
    }

    fn connect(
        &mut self,
        from_node: usize,
        from_port: u16,
        to_node: usize,
        to_port: u16,
    ) -> PyResult<()> {
        self.netlist
            .connect(
                PinRef {
                    node: rflux_ir::NodeId(from_node),
                    port: from_port,
                },
                PinRef {
                    node: rflux_ir::NodeId(to_node),
                    port: to_port,
                },
            )
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn node_count(&self) -> usize {
        self.netlist.node_count()
    }

    fn edge_count(&self) -> usize {
        self.netlist.edge_count()
    }

    fn nodes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let items = self.netlist.nodes().iter().map(|node| {
            (
                node.id.0,
                node_kind_name(&node.kind).to_string(),
                node.name.clone(),
            )
        });
        Ok(PyList::new_bound(py, items))
    }

    fn edges<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let items = self
            .netlist
            .edge_pairs()
            .into_iter()
            .map(|(from, to)| ((from.node.0, from.port), (to.node.0, to.port)));
        Ok(PyList::new_bound(py, items))
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.netlist)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[staticmethod]
    #[pyo3(signature = (payload, name=None))]
    fn from_json(payload: &str, name: Option<String>) -> PyResult<Self> {
        let netlist: Netlist = serde_json::from_str(payload)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            name: name.unwrap_or_default(),
            netlist,
        })
    }
}

fn build_flow_pdk(
    name: &str,
    characterized_library_json: Option<&str>,
    characterized_library_entries: Option<Vec<String>>,
) -> PyResult<Pdk> {
    let mut pdk = Pdk::minimal(name);
    if let Some(entries) = characterized_library_entries {
        let references = entries.iter().map(String::as_str).collect::<Vec<_>>();
        pdk = pdk
            .merge_characterized_library_json_strings(&references)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    }
    if let Some(json) = characterized_library_json {
        pdk = pdk
            .with_characterized_library_json(json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    }
    Ok(pdk)
}

#[pyfunction]
#[pyo3(signature = (serialized_entries, base_name="py-minimal-pdk"))]
fn merge_characterized_library(
    serialized_entries: Vec<String>,
    base_name: &str,
) -> PyResult<String> {
    let pdk = build_flow_pdk(base_name, None, Some(serialized_entries))?;
    pdk.to_json()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction]
#[pyo3(signature = (path, name=None))]
fn read_bench_file(path: &str, name: Option<String>) -> PyResult<Circuit> {
    let netlist = read_bench_netlist(path)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(Circuit {
        name: name.unwrap_or_default(),
        netlist,
    })
}

#[pyfunction]
#[pyo3(signature = (text, name=None))]
fn read_bench_text(text: &str, name: Option<String>) -> PyResult<Circuit> {
    let temp_path = std::env::temp_dir().join(format!(
        "rflux-py-bench-{}.bench",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?
            .as_nanos()
    ));
    fs::write(&temp_path, text).map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
    let result = read_netlist_as(&temp_path, NetlistInputFormat::Bench)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()));
    let _ = fs::remove_file(&temp_path);
    Ok(Circuit {
        name: name.unwrap_or_default(),
        netlist: result?,
    })
}

#[pyfunction]
fn compile_plan(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: &PyCompilePlan,
) -> PyResult<PyCompileReport> {
    ensure_plan_nodes(&mut circuit.netlist, plan);

    let rust_plan = CompilePlan {
        connections: plan
            .connections
            .iter()
            .map(|connection| ConnectionSpec {
                from: to_pin_ref(&connection.from_pin),
                to: to_pin_ref(&connection.to_pin),
            })
            .collect(),
        balance_strategy: parse_balance_strategy(&plan.balance_strategy)?,
        balancing_sources: plan.balancing_sources.iter().map(to_pin_ref).collect(),
    };

    let mut compiler = Compiler::new();
    let report = compiler
        .compile_plan(&mut circuit.netlist, &rust_plan)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None))]
fn compile_netlist(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
) -> PyResult<PySynthesisReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }

    let rust_plan = if let Some(plan) = plan {
        CompilePlan {
            connections: plan
                .connections
                .iter()
                .map(|connection| ConnectionSpec {
                    from: to_pin_ref(&connection.from_pin),
                    to: to_pin_ref(&connection.to_pin),
                })
                .collect(),
            balance_strategy: parse_balance_strategy(&plan.balance_strategy)?,
            balancing_sources: plan.balancing_sources.iter().map(to_pin_ref).collect(),
        }
    } else {
        CompilePlan::default()
    };

    let config = SynthesisConfig {
        plan: rust_plan,
        bool_opt: BoolOptConfig::default(),
    };
    let pdk = Pdk::minimal("py-minimal-pdk");

    let mut compiler = Compiler::new();
    let report = compiler
        .compile_netlist(&mut circuit.netlist, &pdk, &config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, min_hold_jtl_length_um=None, prefer_ptl_from_length_um=None, detour_margin_um=None, characterized_library_json=None, characterized_library_entries=None))]
fn compile_layout(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    min_hold_jtl_length_um: Option<f64>,
    prefer_ptl_from_length_um: Option<f64>,
    detour_margin_um: Option<f64>,
    characterized_library_json: Option<String>,
    characterized_library_entries: Option<Vec<String>>,
) -> PyResult<PyLayoutReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }
    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        min_hold_jtl_length_um,
        prefer_ptl_from_length_um,
        detour_margin_um,
    )?;
    let pdk = build_flow_pdk(
        "py-minimal-pdk",
        characterized_library_json.as_deref(),
        characterized_library_entries,
    )?;

    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut circuit.netlist, &pdk, &config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, characterized_library_json=None, characterized_library_entries=None))]
fn analyze_timing(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    characterized_library_json: Option<String>,
    characterized_library_entries: Option<Vec<String>>,
) -> PyResult<PyTimingAnalysisReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let pdk = build_flow_pdk(
        "py-minimal-pdk",
        characterized_library_json.as_deref(),
        characterized_library_entries,
    )?;

    let mut runner = FlowRunner::new();
    let report = runner
        .analyze_timing(&mut circuit.netlist, &pdk, &config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, pdk, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None))]
fn analyze_timing_corners(
    mut circuit: PyRefMut<'_, Circuit>,
    pdk: &PyPdk,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
) -> PyResult<PyMultiCornerTimingAnalysisReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;

    let mut runner = FlowRunner::new();
    let report = runner
        .analyze_timing_corners(&mut circuit.netlist, &pdk.inner, &config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, characterized_library_json=None, characterized_library_entries=None, cell_delay_sigma_ratio=None, wire_delay_sigma_ratio=None, global_cell_delay_sigma_ratio=None, global_wire_delay_sigma_ratio=None, clock_uncertainty_sigma_ps=None, cross_domain_uncertainty_sigma_ps=None, max_delay_cross_domain_uncertainty_sigma_ps=None, multicycle_cross_domain_uncertainty_sigma_ps=None, sigma_multiplier=None))]
fn analyze_timing_statistical(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    characterized_library_json: Option<String>,
    characterized_library_entries: Option<Vec<String>>,
    cell_delay_sigma_ratio: Option<f64>,
    wire_delay_sigma_ratio: Option<f64>,
    global_cell_delay_sigma_ratio: Option<f64>,
    global_wire_delay_sigma_ratio: Option<f64>,
    clock_uncertainty_sigma_ps: Option<f64>,
    cross_domain_uncertainty_sigma_ps: Option<f64>,
    max_delay_cross_domain_uncertainty_sigma_ps: Option<f64>,
    multicycle_cross_domain_uncertainty_sigma_ps: Option<f64>,
    sigma_multiplier: Option<f64>,
) -> PyResult<PyStatisticalTimingAnalysisReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let pdk = build_flow_pdk(
        "py-minimal-pdk",
        characterized_library_json.as_deref(),
        characterized_library_entries,
    )?;
    let statistical_config = StatisticalTimingConfig {
        cell_delay_sigma_ratio: cell_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().cell_delay_sigma_ratio),
        wire_delay_sigma_ratio: wire_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().wire_delay_sigma_ratio),
        global_cell_delay_sigma_ratio: global_cell_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().global_cell_delay_sigma_ratio),
        global_wire_delay_sigma_ratio: global_wire_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().global_wire_delay_sigma_ratio),
        clock_uncertainty_sigma_ps: clock_uncertainty_sigma_ps
            .unwrap_or(StatisticalTimingConfig::default().clock_uncertainty_sigma_ps),
        cross_domain_uncertainty_sigma_ps: cross_domain_uncertainty_sigma_ps
            .unwrap_or(StatisticalTimingConfig::default().cross_domain_uncertainty_sigma_ps),
        max_delay_cross_domain_uncertainty_sigma_ps: max_delay_cross_domain_uncertainty_sigma_ps
            .unwrap_or(
                StatisticalTimingConfig::default().max_delay_cross_domain_uncertainty_sigma_ps,
            ),
        multicycle_cross_domain_uncertainty_sigma_ps: multicycle_cross_domain_uncertainty_sigma_ps
            .unwrap_or(
                StatisticalTimingConfig::default().multicycle_cross_domain_uncertainty_sigma_ps,
            ),
        sigma_multiplier: sigma_multiplier
            .unwrap_or(StatisticalTimingConfig::default().sigma_multiplier),
    };

    let mut runner = FlowRunner::new();
    let report = runner
        .analyze_timing_statistical(&mut circuit.netlist, &pdk, &config, &statistical_config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None))]
fn analyze_ac_bias(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
) -> PyResult<PyAcBiasReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let pdk = Pdk::minimal("py-minimal-pdk");

    let mut runner = FlowRunner::new();
    let report = runner
        .analyze_ac_bias(&mut circuit.netlist, &pdk, &config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, characterized_library_json=None, characterized_library_entries=None))]
fn optimize_ac_bias(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    characterized_library_json: Option<String>,
    characterized_library_entries: Option<Vec<String>>,
) -> PyResult<PyAcBiasOptimizationReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let pdk = build_flow_pdk(
        "py-minimal-pdk",
        characterized_library_json.as_deref(),
        characterized_library_entries,
    )?;

    let runner_report = FlowRunner::new()
        .optimize_ac_bias(&circuit.netlist, &pdk, &config)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(runner_report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, characterized_library_entries, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, max_estimated_thermal_load_uw=8.0, max_estimated_mechanical_stress_score=0.75, max_jtl_density_per_100um=8.0, max_detour_overhead_ratio=0.35, max_ptl_coupling_ratio=0.65, cell_delay_sigma_ratio=None, wire_delay_sigma_ratio=None, global_cell_delay_sigma_ratio=None, global_wire_delay_sigma_ratio=None, clock_uncertainty_sigma_ps=None, cross_domain_uncertainty_sigma_ps=None, max_delay_cross_domain_uncertainty_sigma_ps=None, multicycle_cross_domain_uncertainty_sigma_ps=None, sigma_multiplier=None))]
fn optimize_design_with_characterized_library(
    circuit: PyRefMut<'_, Circuit>,
    characterized_library_entries: Vec<String>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    max_estimated_thermal_load_uw: f64,
    max_estimated_mechanical_stress_score: f64,
    max_jtl_density_per_100um: f64,
    max_detour_overhead_ratio: f64,
    max_ptl_coupling_ratio: f64,
    cell_delay_sigma_ratio: Option<f64>,
    wire_delay_sigma_ratio: Option<f64>,
    global_cell_delay_sigma_ratio: Option<f64>,
    global_wire_delay_sigma_ratio: Option<f64>,
    clock_uncertainty_sigma_ps: Option<f64>,
    cross_domain_uncertainty_sigma_ps: Option<f64>,
    max_delay_cross_domain_uncertainty_sigma_ps: Option<f64>,
    multicycle_cross_domain_uncertainty_sigma_ps: Option<f64>,
    sigma_multiplier: Option<f64>,
) -> PyResult<PyLibraryAwareDesignOptimizationReport> {
    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let constraint_config = AdvancedConstraintConfig {
        max_estimated_thermal_load_uw,
        max_estimated_mechanical_stress_score,
        max_jtl_density_per_100um,
        max_detour_overhead_ratio,
        max_ptl_coupling_ratio,
    };
    let statistical_config = StatisticalTimingConfig {
        cell_delay_sigma_ratio: cell_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().cell_delay_sigma_ratio),
        wire_delay_sigma_ratio: wire_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().wire_delay_sigma_ratio),
        global_cell_delay_sigma_ratio: global_cell_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().global_cell_delay_sigma_ratio),
        global_wire_delay_sigma_ratio: global_wire_delay_sigma_ratio
            .unwrap_or(StatisticalTimingConfig::default().global_wire_delay_sigma_ratio),
        clock_uncertainty_sigma_ps: clock_uncertainty_sigma_ps
            .unwrap_or(StatisticalTimingConfig::default().clock_uncertainty_sigma_ps),
        cross_domain_uncertainty_sigma_ps: cross_domain_uncertainty_sigma_ps
            .unwrap_or(StatisticalTimingConfig::default().cross_domain_uncertainty_sigma_ps),
        max_delay_cross_domain_uncertainty_sigma_ps: max_delay_cross_domain_uncertainty_sigma_ps
            .unwrap_or(
                StatisticalTimingConfig::default().max_delay_cross_domain_uncertainty_sigma_ps,
            ),
        multicycle_cross_domain_uncertainty_sigma_ps: multicycle_cross_domain_uncertainty_sigma_ps
            .unwrap_or(
                StatisticalTimingConfig::default().multicycle_cross_domain_uncertainty_sigma_ps,
            ),
        sigma_multiplier: sigma_multiplier
            .unwrap_or(StatisticalTimingConfig::default().sigma_multiplier),
    };
    let base_pdk = Pdk::minimal("py-minimal-pdk");
    let report = FlowRunner::new()
        .optimize_design_with_characterized_library(
            &circuit.netlist,
            &base_pdk,
            &config,
            &constraint_config,
            &statistical_config,
            &characterized_library_entries,
        )
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, characterized_library_entries, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, max_estimated_thermal_load_uw=8.0, max_estimated_mechanical_stress_score=0.75, max_jtl_density_per_100um=8.0, max_detour_overhead_ratio=0.35, max_ptl_coupling_ratio=0.65))]
fn optimize_ac_bias_with_characterized_library(
    circuit: PyRefMut<'_, Circuit>,
    characterized_library_entries: Vec<String>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    max_estimated_thermal_load_uw: f64,
    max_estimated_mechanical_stress_score: f64,
    max_jtl_density_per_100um: f64,
    max_detour_overhead_ratio: f64,
    max_ptl_coupling_ratio: f64,
) -> PyResult<PyLibraryAwareAcBiasOptimizationReport> {
    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let constraint_config = AdvancedConstraintConfig {
        max_estimated_thermal_load_uw,
        max_estimated_mechanical_stress_score,
        max_jtl_density_per_100um,
        max_detour_overhead_ratio,
        max_ptl_coupling_ratio,
    };
    let base_pdk = Pdk::minimal("py-minimal-pdk");
    let report = FlowRunner::new()
        .optimize_ac_bias_with_characterized_library(
            &circuit.netlist,
            &base_pdk,
            &config,
            &constraint_config,
            &characterized_library_entries,
        )
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, cell_name="compound_cell".to_string(), simulation_mode=None, external_command=None))]
fn characterize_compound_cell(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    cell_name: String,
    simulation_mode: Option<String>,
    external_command: Option<String>,
) -> PyResult<PyCompoundCellCharacterizationReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let pdk = Pdk::minimal("py-minimal-pdk");

    let mut runner = FlowRunner::new();
    let report = runner
        .characterize_compound_cell(
            &mut circuit.netlist,
            &pdk,
            &config,
            &SimulationConfig {
                mode: parse_simulation_mode(simulation_mode.as_deref())?,
                external_command,
            },
            &CompoundCellCharacterizationConfig { cell_name },
        )
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, max_estimated_thermal_load_uw=8.0, max_estimated_mechanical_stress_score=0.75, max_jtl_density_per_100um=8.0, max_detour_overhead_ratio=0.35, max_ptl_coupling_ratio=0.65))]
fn analyze_advanced_constraints(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    max_estimated_thermal_load_uw: f64,
    max_estimated_mechanical_stress_score: f64,
    max_jtl_density_per_100um: f64,
    max_detour_overhead_ratio: f64,
    max_ptl_coupling_ratio: f64,
) -> PyResult<PyAdvancedConstraintReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let pdk = Pdk::minimal("py-minimal-pdk");

    let mut runner = FlowRunner::new();
    let report = runner
        .analyze_advanced_constraints(
            &mut circuit.netlist,
            &pdk,
            &config,
            &AdvancedConstraintConfig {
                max_estimated_thermal_load_uw,
                max_estimated_mechanical_stress_score,
                max_jtl_density_per_100um,
                max_detour_overhead_ratio,
                max_ptl_coupling_ratio,
            },
        )
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (circuit, plan=None, fixed_nodes=None, blocked_regions=None, timing_constraints=None, pin_timing_constraints=None, clock_domains=None, crossing_constraints=None, simulation_mode=None, external_command=None))]
fn verify_layout(
    mut circuit: PyRefMut<'_, Circuit>,
    plan: Option<&PyCompilePlan>,
    fixed_nodes: Option<Vec<PyFixedNodePlacement>>,
    blocked_regions: Option<Vec<PyBlockedRegion>>,
    timing_constraints: Option<Vec<PyNodeTimingConstraint>>,
    pin_timing_constraints: Option<Vec<PyPinTimingConstraint>>,
    clock_domains: Option<Vec<PyClockDomainConstraint>>,
    crossing_constraints: Option<Vec<PyCrossingConstraint>>,
    simulation_mode: Option<String>,
    external_command: Option<String>,
) -> PyResult<PyVerificationReport> {
    if let Some(plan) = plan {
        ensure_plan_nodes(&mut circuit.netlist, plan);
    }
    if let Some(fixed_nodes) = &fixed_nodes {
        ensure_node_capacity(
            &mut circuit.netlist,
            fixed_nodes.iter().map(|fixed| fixed.node),
        );
    }

    let config = to_flow_config(
        plan,
        fixed_nodes,
        blocked_regions,
        timing_constraints,
        pin_timing_constraints,
        clock_domains,
        crossing_constraints,
        None,
        None,
        None,
    )?;
    let pdk = Pdk::minimal("py-minimal-pdk");

    let mut runner = FlowRunner::new();
    let report = runner
        .verify_layout(
            &mut circuit.netlist,
            &pdk,
            &config,
            &SimulationConfig {
                mode: parse_simulation_mode(simulation_mode.as_deref())?,
                external_command,
            },
        )
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (deck_text, simulation_mode=None, external_command=None))]
fn simulate_text(
    deck_text: &str,
    simulation_mode: Option<String>,
    external_command: Option<String>,
) -> PyResult<PySimulationReport> {
    let report = simulate_text_core(
        deck_text,
        &SimulationConfig {
            mode: parse_simulation_mode(simulation_mode.as_deref())?,
            external_command,
        },
    )
    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (file_path, simulation_mode=None, external_command=None))]
fn simulate_file(
    file_path: &str,
    simulation_mode: Option<String>,
    external_command: Option<String>,
) -> PyResult<PySimulationReport> {
    let report = simulate_file_core(
        file_path,
        &SimulationConfig {
            mode: parse_simulation_mode(simulation_mode.as_deref())?,
            external_command,
        },
    )
    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
fn is_supported_external_command(command: &str) -> bool {
    is_supported_external_command_core(command)
}

#[pyfunction]
fn check_equivalence(
    lhs: PyRef<'_, Circuit>,
    rhs: PyRef<'_, Circuit>,
) -> PyResult<PyCombinationalEquivalenceReport> {
    let verifier = Verifier::new();
    let report = verifier
        .check_boolean_equivalence(&lhs.netlist, &rhs.netlist)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
fn check_single_step_sequential_equivalence(
    lhs: PyRef<'_, Circuit>,
    rhs: PyRef<'_, Circuit>,
) -> PyResult<PySingleStepSequentialEquivalenceReport> {
    let verifier = Verifier::new();
    let report = verifier
        .check_single_step_sequential_equivalence(&lhs.netlist, &rhs.netlist)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pyfunction]
#[pyo3(signature = (lhs, rhs, depth=2))]
fn check_bounded_sequential_equivalence(
    lhs: PyRef<'_, Circuit>,
    rhs: PyRef<'_, Circuit>,
    depth: usize,
) -> PyResult<PyBoundedSequentialEquivalenceReport> {
    let verifier = Verifier::new();
    let report = verifier
        .check_bounded_sequential_equivalence(&lhs.netlist, &rhs.netlist, depth)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    Ok(report.into())
}

#[pymodule]
fn _core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Circuit>()?;
    m.add_class::<PyPinRef>()?;
    m.add_class::<PyConnectionSpec>()?;
    m.add_class::<PyFixedNodePlacement>()?;
    m.add_class::<PyBlockedRegion>()?;
    m.add_class::<PyNodeTimingConstraint>()?;
    m.add_class::<PyPinTimingConstraint>()?;
    m.add_class::<PyClockDomainConstraint>()?;
    m.add_class::<PyCrossingConstraint>()?;
    m.add_class::<PyCompilePlan>()?;
    m.add_class::<PyCompileReport>()?;
    m.add_class::<PySynthesisReport>()?;
    m.add_class::<PyLayoutReport>()?;
    m.add_class::<PyTimingClosureSummary>()?;
    m.add_class::<PyTimingClosureAction>()?;
    m.add_class::<PyTimingClosureLoopReport>()?;
    m.add_class::<PyTimingArcReport>()?;
    m.add_class::<PyTimingAnalysisReport>()?;
    m.add_class::<PyTimingCornerAnalysisReport>()?;
    m.add_class::<PyMultiCornerTimingAnalysisReport>()?;
    m.add_class::<PyStatisticalTimingArcReport>()?;
    m.add_class::<PyStatisticalTimingAnalysisReport>()?;
    m.add_class::<PyAcBiasReport>()?;
    m.add_class::<PyPdk>()?;
    m.add_class::<PyCellLibraryEntry>()?;
    m.add_class::<PyCellLibraryMetadata>()?;
    m.add_class::<PyCellLibrarySummary>()?;
    m.add_class::<PyAcBiasOptimizationReport>()?;
    m.add_class::<PyLibraryAwareAcBiasOptimizationReport>()?;
    m.add_class::<PyLibraryAwareDesignOptimizationReport>()?;
    m.add_class::<PySimulationEndpointRef>()?;
    m.add_class::<PySimulationDelayDetail>()?;
    m.add_class::<PySimulationMeasurementDetail>()?;
    m.add_class::<PySimulationMeasurementWarning>()?;
    m.add_class::<PySimulationViolationDetail>()?;
    m.add_class::<PySimulationReport>()?;
    m.add_class::<PyEquivalenceInputAssignment>()?;
    m.add_class::<PyOutputMismatchEntry>()?;
    m.add_class::<PyStateMismatchEntry>()?;
    m.add_class::<PyCombinationalEquivalenceReport>()?;
    m.add_class::<PySingleStepSequentialEquivalenceReport>()?;
    m.add_class::<PyBoundedSequentialEquivalenceStepReport>()?;
    m.add_class::<PyBoundedSequentialEquivalenceReport>()?;
    m.add_class::<PyVerificationReport>()?;
    m.add_class::<PyCompoundCellCharacterizationReport>()?;
    m.add_class::<PyAdvancedConstraintViolation>()?;
    m.add_class::<PyAdvancedConstraintReport>()?;
    m.add_function(wrap_pyfunction!(compile_plan, m)?)?;
    m.add_function(wrap_pyfunction!(compile_netlist, m)?)?;
    m.add_function(wrap_pyfunction!(compile_layout, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_timing, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_timing_corners, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_timing_statistical, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_ac_bias, m)?)?;
    m.add_function(wrap_pyfunction!(merge_characterized_library, m)?)?;
    m.add_function(wrap_pyfunction!(optimize_ac_bias, m)?)?;
    m.add_function(wrap_pyfunction!(
        optimize_ac_bias_with_characterized_library,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(
        optimize_design_with_characterized_library,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(characterize_compound_cell, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_advanced_constraints, m)?)?;
    m.add_function(wrap_pyfunction!(verify_layout, m)?)?;
    m.add_function(wrap_pyfunction!(simulate_text, m)?)?;
    m.add_function(wrap_pyfunction!(simulate_file, m)?)?;
    m.add_function(wrap_pyfunction!(is_supported_external_command, m)?)?;
    m.add_function(wrap_pyfunction!(check_equivalence, m)?)?;
    m.add_function(wrap_pyfunction!(
        check_single_step_sequential_equivalence,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(check_bounded_sequential_equivalence, m)?)?;
    m.add_function(wrap_pyfunction!(read_bench_file, m)?)?;
    m.add_function(wrap_pyfunction!(read_bench_text, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
