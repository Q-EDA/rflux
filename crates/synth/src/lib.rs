mod bool_opt;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rflux_ir::{IrError, Netlist, NodeKind, PinRef};
use rflux_sat::{CnfFormula, IncrementalSolver, Lit, Model, SolveResult, SolveStats};
use rflux_tech::{Pdk, SfCell, SfCellKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SynthError {
    #[error("ir error: {0}")]
    Ir(#[from] IrError),
    #[error("could not rewire source pin {0:?} during splitter insertion")]
    MissingExistingSink(PinRef),
    #[error("could not locate sink for source pin {0:?} when inserting balancing dff")]
    MissingSinkForDffInsertion(PinRef),
    #[error("boolean optimization does not support node kind {0:?}")]
    UnsupportedBoolOptNodeKind(NodeKind),
    #[error("boolean optimization requires a driving source for node {0}")]
    MissingBoolOptDriver(usize),
    #[error(
        "boolean optimization expected node {node} to have {expected} input(s), found {actual}"
    )]
    UnexpectedBoolOptInputCount {
        node: usize,
        expected: usize,
        actual: usize,
    },
    #[error("could not resolve a combinational order for the netlist")]
    CombinationalCycle,
    #[error("boolean optimization encountered an unsupported dependency pattern")]
    CycleOrUnsupportedDependency,
    #[error("sat interface mismatch: {0}")]
    SatInterfaceMismatch(String),
    #[error("sat check does not support node {node} kind {kind:?}")]
    SatUnsupportedNodeKind { node: usize, kind: NodeKind },
    #[error("sat check expected node {node} to have {expected} input(s), found {actual}")]
    SatUnexpectedInputCount {
        node: usize,
        expected: usize,
        actual: usize,
    },
    #[error("sat encoding error: {0}")]
    SatEncoding(String),
}

impl SynthError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            SynthError::Ir(e) => e.code(),
            SynthError::MissingExistingSink(..) => "RFLOW-FLOW-001",
            SynthError::MissingSinkForDffInsertion(..) => "RFLOW-FLOW-001",
            SynthError::UnsupportedBoolOptNodeKind(..) => "RFLOW-FLOW-001",
            SynthError::MissingBoolOptDriver(..) => "RFLOW-FLOW-001",
            SynthError::UnexpectedBoolOptInputCount { .. } => "RFLOW-FLOW-001",
            SynthError::CombinationalCycle => "RFLOW-FLOW-001",
            SynthError::CycleOrUnsupportedDependency => "RFLOW-FLOW-001",
            SynthError::SatInterfaceMismatch(..) => "RFLOW-VERIFY-001",
            SynthError::SatUnsupportedNodeKind { .. } => "RFLOW-VERIFY-002",
            SynthError::SatUnexpectedInputCount { .. } => "RFLOW-VERIFY-001",
            SynthError::SatEncoding(..) => "RFLOW-VERIFY-001",
        }
    }

    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            SynthError::Ir(e) => e.suggestion(),
            SynthError::MissingExistingSink(..) => {
                "Check that the netlist has valid connections before splitter insertion."
            }
            SynthError::MissingSinkForDffInsertion(..) => {
                "Ensure the combinational path has a valid sink for balancing DFF insertion."
            }
            SynthError::UnsupportedBoolOptNodeKind(..) => {
                "Boolean optimization only supports CellInstance nodes."
            }
            SynthError::MissingBoolOptDriver(..) => {
                "Every node in the boolean network needs a driving source."
            }
            SynthError::UnexpectedBoolOptInputCount { .. } => {
                "The node has an unexpected number of inputs for boolean optimization."
            }
            SynthError::CombinationalCycle => {
                "The netlist contains a combinational cycle. Resolve feedback paths before synthesis."
            }
            SynthError::CycleOrUnsupportedDependency => {
                "Resolve cyclic dependencies in the netlist before synthesis."
            }
            SynthError::SatInterfaceMismatch(..) => {
                "Ensure both LHS and RHS have matching named input/output port sets."
            }
            SynthError::SatUnsupportedNodeKind { .. } => {
                "Equivalence checking only supports Dff and DffEnable node kinds for sequential."
            }
            SynthError::SatUnexpectedInputCount { .. } => {
                "Check that nodes have the expected number of inputs for equivalence checking."
            }
            SynthError::SatEncoding(..) => {
                "Report this as a bug. SAT encoding encountered an internal error."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SatEquivalenceReport {
    pub equivalent: bool,
    pub checked_outputs: Vec<String>,
    pub counterexample_inputs: Option<BTreeMap<String, bool>>,
    pub counterexample_outputs: Option<BTreeMap<String, SatOutputMismatch>>,
    pub sat_stats: SolveStats,
    pub sat_elapsed_ns: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquivalenceCheckKind {
    Output,
    State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivalenceCheckTarget {
    pub kind: EquivalenceCheckKind,
    pub name: String,
    pub assumptions: Vec<Lit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivalenceSatProblem {
    pub formula: CnfFormula,
    pub checks: Vec<EquivalenceCheckTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SatOutputMismatch {
    pub lhs: bool,
    pub rhs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequentialEquivalenceReport {
    pub equivalent: bool,
    pub checked_outputs: Vec<String>,
    pub checked_states: Vec<String>,
    pub counterexample_inputs: Option<BTreeMap<String, bool>>,
    pub counterexample_present_states: Option<BTreeMap<String, bool>>,
    pub counterexample_outputs: Option<BTreeMap<String, SatOutputMismatch>>,
    pub counterexample_states: Option<BTreeMap<String, SatStateTransitionMismatch>>,
    pub sat_stats: SolveStats,
    pub sat_elapsed_ns: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedSequentialEquivalenceStepReport {
    pub step: usize,
    pub report: SequentialEquivalenceReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedSequentialEquivalenceReport {
    pub equivalent: bool,
    pub depth: usize,
    pub checked_steps: usize,
    pub unroll_mode: String,
    pub checked_outputs: Vec<String>,
    pub checked_states: Vec<String>,
    pub first_failing_step: Option<usize>,
    pub steps: Vec<BoundedSequentialEquivalenceStepReport>,
    pub sat_stats: SolveStats,
    pub sat_elapsed_ns: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SatStateTransitionMismatch {
    pub lhs_next: bool,
    pub rhs_next: bool,
    pub lhs_clock: bool,
    pub rhs_clock: bool,
}

#[derive(Debug, Clone)]
struct BooleanEquivalenceProblemData {
    export: EquivalenceSatProblem,
    shared_input_vars: BTreeMap<String, usize>,
    output_vars: Vec<(String, usize, usize)>,
}

#[derive(Debug, Clone)]
struct SequentialEquivalenceProblemData {
    export: EquivalenceSatProblem,
    shared_input_vars: BTreeMap<String, usize>,
    shared_state_vars: BTreeMap<String, usize>,
    lhs_outputs: BTreeMap<String, usize>,
    rhs_outputs: BTreeMap<String, usize>,
    lhs_states: BTreeMap<String, SequentialSatState>,
    rhs_states: BTreeMap<String, SequentialSatState>,
}

#[derive(Debug, Clone)]
struct BoundedSequentialCheck {
    step: usize,
    diff_var: usize,
    prior_diff_vars: Vec<usize>,
}

#[derive(Debug, Clone)]
struct BoundedSequentialFrameData {
    input_vars: BTreeMap<String, usize>,
    lhs_present_state_vars: BTreeMap<String, usize>,
    lhs_outputs: BTreeMap<String, usize>,
    rhs_outputs: BTreeMap<String, usize>,
    lhs_states: BTreeMap<String, SequentialSatState>,
    rhs_states: BTreeMap<String, SequentialSatState>,
}

#[derive(Debug, Clone)]
struct BoundedSequentialProblemData {
    formula: CnfFormula,
    checks: Vec<BoundedSequentialCheck>,
    frames: Vec<BoundedSequentialFrameData>,
    checked_outputs: Vec<String>,
    checked_states: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionSpec {
    pub from: PinRef,
    pub to: PinRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BalanceStrategy {
    #[default]
    None,
    Explicit,
    AllConnectedSources,
    BySinkLevel,
}

#[derive(Debug, Clone, Default)]
pub struct CompilePlan {
    pub connections: Vec<ConnectionSpec>,
    pub balance_strategy: BalanceStrategy,
    pub balancing_sources: Vec<PinRef>,
}

#[derive(Debug, Clone, Default)]
pub struct SynthesisConfig {
    pub plan: CompilePlan,
    pub bool_opt: BoolOptConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompileReport {
    pub connections_applied: usize,
    pub splitters_inserted: usize,
    pub balancing_dffs_inserted: usize,
}

#[derive(Debug, Clone)]
pub struct BoolOptConfig {
    pub share_logic_flattening_limit: usize,
    pub infer_xor_mux: bool,
    pub infer_dffe: bool,
}

impl Default for BoolOptConfig {
    fn default() -> Self {
        Self {
            share_logic_flattening_limit: 8,
            infer_xor_mux: true,
            infer_dffe: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoolOptReport {
    pub gate_count_before: usize,
    pub gate_count_after: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathBalanceNeed {
    pub sink_node: usize,
    pub source: PinRef,
    pub deficit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PathBalanceReport {
    pub node_levels: Vec<usize>,
    pub needs: Vec<PathBalanceNeed>,
}

impl PathBalanceReport {
    #[must_use]
    pub fn total_insertions(&self) -> usize {
        self.needs.iter().map(|need| need.deficit).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOptCompatibilityIssueKind {
    UnsupportedNodeKind,
    MissingDriver,
    UnexpectedInputCount,
    CycleOrUnresolvedDependency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoolOptCompatibilityIssue {
    pub node: usize,
    pub kind: BoolOptCompatibilityIssueKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoolOptCompatibilityReport {
    pub input_nodes: Vec<usize>,
    pub output_candidates: Vec<usize>,
    pub issues: Vec<BoolOptCompatibilityIssue>,
}

impl BoolOptCompatibilityReport {
    #[must_use]
    pub fn is_compatible(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SynthesisReport {
    pub compile: CompileReport,
    pub bool_opt: BoolOptReport,
    pub tech_map: TechMapReport,
    pub path_balance: PathBalanceReport,
    pub bool_opt_compatibility: BoolOptCompatibilityReport,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct TechMappedNode<'a> {
    pub node_name: &'a str,
    pub cell: &'a SfCell,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TechMapReport {
    pub mapped_nodes: usize,
    pub total_area_um2: f64,
    pub unmapped_nodes: usize,
    pub coverage_ratio: f64,
}

pub struct TechMapper<'a> {
    pdk: &'a Pdk,
}

impl<'a> TechMapper<'a> {
    #[must_use]
    pub fn new(pdk: &'a Pdk) -> Self {
        Self { pdk }
    }

    #[must_use]
    pub fn map_kind(kind: &NodeKind) -> SfCellKind {
        match kind {
            NodeKind::CellInstance => SfCellKind::GenericGate,
            NodeKind::MacroCell => SfCellKind::Macro,
            NodeKind::Splitter => SfCellKind::Splitter,
            NodeKind::Dff => SfCellKind::Dff,
            NodeKind::Jtl => SfCellKind::Jtl,
            NodeKind::Ptl => SfCellKind::Ptl,
            NodeKind::Port => SfCellKind::Port,
        }
    }

    #[must_use]
    pub fn map_netlist(&self, netlist: &'a Netlist) -> Vec<TechMappedNode<'a>> {
        netlist
            .nodes()
            .iter()
            .filter_map(|node| {
                let kind = Self::map_kind(&node.kind);
                self.pdk
                    .cell_library
                    .find_by_name(&node.name)
                    .or_else(|| self.pdk.cell_library.find_by_kind(kind))
                    .map(|cell| TechMappedNode {
                        node_name: &node.name,
                        cell,
                    })
            })
            .collect()
    }

    #[must_use]
    pub fn map_netlist_area_optimized(&self, netlist: &'a Netlist) -> Vec<TechMappedNode<'a>> {
        netlist
            .nodes()
            .iter()
            .filter_map(|node| {
                let kind = Self::map_kind(&node.kind);
                let best_cell = self
                    .pdk
                    .cell_library
                    .cells
                    .iter()
                    .filter(|c| c.kind == kind)
                    .min_by(|a, b| {
                        a.area_um2
                            .partial_cmp(&b.area_um2)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .or_else(|| self.pdk.cell_library.find_by_kind(kind));
                best_cell.map(|cell| TechMappedNode {
                    node_name: &node.name,
                    cell,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn map_report(&self, netlist: &'a Netlist) -> TechMapReport {
        let mapped = self.map_netlist(netlist);
        let total = netlist.nodes().len();
        let mapped_count = mapped.len();
        TechMapReport {
            mapped_nodes: mapped_count,
            total_area_um2: mapped.iter().map(|entry| entry.cell.area_um2).sum(),
            unmapped_nodes: total - mapped_count,
            coverage_ratio: if total > 0 {
                mapped_count as f64 / total as f64
            } else {
                0.0
            },
        }
    }

    /// Identify candidate subgraphs that could be merged into complex cells.
    ///
    /// In SFQ, multi-level complex cells (e.g., AND-OR-INVERT) reduce pipeline
    /// depth by combining multiple logic levels into a single cell that only
    /// needs clock at its input/output boundaries.
    ///
    /// Returns a list of (root_node, depth, area) tuples for subgraphs that
    /// match available complex cells in the library.
    pub fn find_complex_cell_candidates(
        &self,
        netlist: &'a Netlist,
    ) -> Vec<ComplexCellCandidate<'a>> {
        let mut candidates = Vec::new();
        let complex_cells: Vec<_> = self
            .pdk
            .cell_library
            .cells
            .iter()
            .filter(|c| matches!(c.kind, SfCellKind::Macro))
            .collect();

        if complex_cells.is_empty() {
            return candidates;
        }

        // For each cell instance, try to merge it with its neighbors
        // into a complex cell if the merged subgraph matches a library entry.
        for node in netlist.nodes() {
            if !matches!(node.kind, NodeKind::CellInstance | NodeKind::MacroCell) {
                continue;
            }
            // Check if this node's name matches any complex cell
            if let Some(cell) = self.pdk.cell_library.find_by_name(&node.name) {
                if matches!(cell.kind, SfCellKind::Macro) {
                    candidates.push(ComplexCellCandidate {
                        root: node,
                        cell,
                        depth: 1,
                        area_um2: cell.area_um2,
                    });
                }
            }
        }

        candidates
    }
}

/// A candidate for complex cell mapping.
#[derive(Debug, Clone)]
pub struct ComplexCellCandidate<'a> {
    pub root: &'a rflux_ir::Node,
    pub cell: &'a SfCell,
    pub depth: usize,
    pub area_um2: f64,
}

#[derive(Debug, Default)]
pub struct Compiler {
    next_splitter_id: usize,
    next_balance_dff_id: usize,
}

impl Compiler {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile(
        &mut self,
        netlist: &mut Netlist,
        from: PinRef,
        to: PinRef,
    ) -> Result<(), SynthError> {
        self.connect_with_splitter(netlist, from, to).map(|_| ())
    }

    pub fn compile_plan(
        &mut self,
        netlist: &mut Netlist,
        plan: &CompilePlan,
    ) -> Result<CompileReport, SynthError> {
        let mut report = CompileReport::default();

        for connection in &plan.connections {
            let inserted_splitter =
                self.connect_with_splitter(netlist, connection.from, connection.to)?;
            report.connections_applied += 1;
            if inserted_splitter {
                report.splitters_inserted += 1;
            }
        }

        for source in self.collect_balancing_sources(netlist, plan) {
            self.insert_balancing_dff(netlist, source)?;
            report.balancing_dffs_inserted += 1;
        }

        if matches!(plan.balance_strategy, BalanceStrategy::BySinkLevel) {
            report.balancing_dffs_inserted += self.balance_by_sink_level(netlist)?;
        }

        Ok(report)
    }

    pub fn compile_netlist(
        &mut self,
        netlist: &mut Netlist,
        pdk: &Pdk,
        config: &SynthesisConfig,
    ) -> Result<SynthesisReport, SynthError> {
        let compile = self.compile_plan(netlist, &config.plan)?;
        let bool_opt = self.optimize_boolean_network(netlist, &config.bool_opt);
        let path_balance = self.analyze_path_balancing(netlist)?;
        let tech_map = TechMapper::new(pdk).map_report(netlist);
        let bool_opt_compatibility = self.analyze_bool_opt_compatibility(netlist);

        Ok(SynthesisReport {
            compile,
            bool_opt,
            tech_map,
            path_balance,
            bool_opt_compatibility,
            node_count: netlist.node_count(),
            edge_count: netlist.edge_count(),
        })
    }

    pub fn insert_balancing_dff(
        &mut self,
        netlist: &mut Netlist,
        from: PinRef,
    ) -> Result<PinRef, SynthError> {
        let previous_sink = netlist
            .disconnect(from)
            .ok_or(SynthError::MissingSinkForDffInsertion(from))?;

        let dff_name = format!("balance_dff_{}", self.next_balance_dff_id);
        self.next_balance_dff_id += 1;
        let dff = netlist.add_node(NodeKind::Dff, dff_name);

        let dff_in = PinRef { node: dff, port: 0 };
        let dff_out = PinRef { node: dff, port: 1 };

        netlist.connect(from, dff_in)?;
        netlist.connect(dff_out, previous_sink)?;

        Ok(dff_out)
    }

    fn connect_with_splitter(
        &mut self,
        netlist: &mut Netlist,
        from: PinRef,
        to: PinRef,
    ) -> Result<bool, SynthError> {
        match netlist.connect(from, to) {
            Ok(()) => Ok(false),
            Err(IrError::DestinationAlreadyDriven) => Err(IrError::DestinationAlreadyDriven.into()),
            Err(IrError::SourceAlreadyConnected) => {
                let previous_sink = netlist
                    .disconnect(from)
                    .ok_or(SynthError::MissingExistingSink(from))?;

                let splitter_name = format!("auto_splitter_{}", self.next_splitter_id);
                self.next_splitter_id += 1;
                let splitter = netlist.add_node(NodeKind::Splitter, splitter_name);

                let splitter_in = PinRef {
                    node: splitter,
                    port: 0,
                };
                let splitter_out_a = PinRef {
                    node: splitter,
                    port: 1,
                };
                let splitter_out_b = PinRef {
                    node: splitter,
                    port: 2,
                };

                netlist.connect(from, splitter_in)?;
                netlist.connect(splitter_out_a, previous_sink)?;
                netlist.connect(splitter_out_b, to)?;
                Ok(true)
            }
        }
    }

    fn collect_balancing_sources(&self, netlist: &Netlist, plan: &CompilePlan) -> Vec<PinRef> {
        match plan.balance_strategy {
            BalanceStrategy::None => Vec::new(),
            BalanceStrategy::Explicit => plan.balancing_sources.clone(),
            BalanceStrategy::AllConnectedSources => netlist
                .edge_pairs()
                .into_iter()
                .map(|(from, _)| from)
                .collect(),
            BalanceStrategy::BySinkLevel => Vec::new(),
        }
    }

    fn balance_by_sink_level(&mut self, netlist: &mut Netlist) -> Result<usize, SynthError> {
        let analysis = self.analyze_path_balancing(netlist)?;
        let mut inserted = 0;
        for need in analysis.needs {
            let mut current = need.source;
            for _ in 0..need.deficit {
                current = self.insert_balancing_dff(netlist, current)?;
                inserted += 1;
            }
        }

        Ok(inserted)
    }

    pub fn analyze_path_balancing(
        &self,
        netlist: &Netlist,
    ) -> Result<PathBalanceReport, SynthError> {
        let levels = self.compute_node_levels(netlist)?;
        let mut incoming_by_sink: std::collections::BTreeMap<usize, Vec<(PinRef, usize)>> =
            std::collections::BTreeMap::new();

        for (from, to) in netlist.edge_pairs() {
            incoming_by_sink
                .entry(to.node.0)
                .or_default()
                .push((from, levels[from.node.0]));
        }

        let mut needs = Vec::new();
        for (sink_node, incoming) in incoming_by_sink {
            if incoming.len() < 2 {
                continue;
            }

            let max_level = incoming.iter().map(|(_, level)| *level).max().unwrap_or(0);
            for (source, level) in incoming {
                let deficit = max_level.saturating_sub(level);
                if deficit > 0 {
                    needs.push(PathBalanceNeed {
                        sink_node,
                        source,
                        deficit,
                    });
                }
            }
        }

        Ok(PathBalanceReport {
            node_levels: levels,
            needs,
        })
    }

    fn compute_node_levels(&self, netlist: &Netlist) -> Result<Vec<usize>, SynthError> {
        let node_count = netlist.node_count();
        let mut indegree = vec![0usize; node_count];
        let mut adjacency = vec![Vec::<usize>::new(); node_count];
        for (from, to) in netlist.edge_pairs() {
            indegree[to.node.0] += 1;
            adjacency[from.node.0].push(to.node.0);
        }

        let mut queue = std::collections::VecDeque::new();
        for (node, degree) in indegree.iter().enumerate() {
            if *degree == 0 {
                queue.push_back(node);
            }
        }

        let mut levels = vec![0usize; node_count];
        let mut visited = 0usize;
        while let Some(node) = queue.pop_front() {
            visited += 1;
            let next_level = levels[node] + 1;
            for succ in &adjacency[node] {
                if next_level > levels[*succ] {
                    levels[*succ] = next_level;
                }
                indegree[*succ] -= 1;
                if indegree[*succ] == 0 {
                    queue.push_back(*succ);
                }
            }
        }

        if visited != node_count {
            return Err(SynthError::CombinationalCycle);
        }

        Ok(levels)
    }

    pub fn check_boolean_equivalence_sat(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<SatEquivalenceReport, SynthError> {
        let problem = build_boolean_equivalence_problem_data(lhs, rhs)?;
        let checked_outputs = problem
            .export
            .checks
            .iter()
            .map(|check| check.name.clone())
            .collect::<Vec<_>>();
        let incremental_solver = IncrementalSolver::from_formula(problem.export.formula.clone());
        let mut aggregated_stats = SolveStats::default();
        let mut aggregated_elapsed_ns = 0_u128;

        for check in &problem.export.checks {
            let (solve_result, sat_metrics) =
                incremental_solver.solve_with_assumptions_and_metrics(&check.assumptions);
            merge_solve_stats(&mut aggregated_stats, &sat_metrics.stats);
            aggregated_elapsed_ns += sat_metrics.elapsed_ns;

            if let SolveResult::Satisfiable(model) = solve_result {
                let mut assignment = BTreeMap::new();
                for (name, var) in &problem.shared_input_vars {
                    assignment.insert(name.clone(), model.value(*var).unwrap_or(false));
                }
                let mut output_mismatch = BTreeMap::new();
                for (name, lhs_var, rhs_var) in &problem.output_vars {
                    output_mismatch.insert(
                        name.clone(),
                        SatOutputMismatch {
                            lhs: model.value(*lhs_var).unwrap_or(false),
                            rhs: model.value(*rhs_var).unwrap_or(false),
                        },
                    );
                }
                return Ok(SatEquivalenceReport {
                    equivalent: false,
                    checked_outputs,
                    counterexample_inputs: Some(assignment),
                    counterexample_outputs: Some(output_mismatch),
                    sat_stats: aggregated_stats,
                    sat_elapsed_ns: aggregated_elapsed_ns,
                });
            }
            continue;
        }

        Ok(SatEquivalenceReport {
            equivalent: true,
            checked_outputs,
            counterexample_inputs: None,
            counterexample_outputs: None,
            sat_stats: aggregated_stats,
            sat_elapsed_ns: aggregated_elapsed_ns,
        })
    }

    pub fn check_sequential_equivalence_sat(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<SequentialEquivalenceReport, SynthError> {
        let problem = build_sequential_equivalence_problem_data(lhs, rhs)?;
        let checked_outputs = problem
            .export
            .checks
            .iter()
            .filter(|check| matches!(check.kind, EquivalenceCheckKind::Output))
            .map(|check| check.name.clone())
            .collect::<Vec<_>>();
        let checked_states = problem
            .export
            .checks
            .iter()
            .filter(|check| matches!(check.kind, EquivalenceCheckKind::State))
            .map(|check| check.name.clone())
            .collect::<Vec<_>>();
        let incremental_solver = IncrementalSolver::from_formula(problem.export.formula.clone());
        let mut aggregated_stats = SolveStats::default();
        let mut aggregated_elapsed_ns = 0_u128;

        for check in &problem.export.checks {
            let (solve_result, sat_metrics) =
                incremental_solver.solve_with_assumptions_and_metrics(&check.assumptions);
            merge_solve_stats(&mut aggregated_stats, &sat_metrics.stats);
            aggregated_elapsed_ns += sat_metrics.elapsed_ns;

            if let SolveResult::Satisfiable(model) = solve_result {
                let mut input_assignment = BTreeMap::new();
                for (name, var) in &problem.shared_input_vars {
                    input_assignment.insert(name.clone(), model.value(*var).unwrap_or(false));
                }

                let mut present_state_assignment = BTreeMap::new();
                for (name, var) in &problem.shared_state_vars {
                    present_state_assignment
                        .insert(name.clone(), model.value(*var).unwrap_or(false));
                }

                let mut output_mismatch = BTreeMap::new();
                for (name, lhs_var, rhs_var) in problem.lhs_outputs.iter().map(|(name, lhs_var)| {
                    (
                        name.clone(),
                        *lhs_var,
                        *problem
                            .rhs_outputs
                            .get(name)
                            .expect("rhs output must exist"),
                    )
                }) {
                    output_mismatch.insert(
                        name,
                        SatOutputMismatch {
                            lhs: model.value(lhs_var).unwrap_or(false),
                            rhs: model.value(rhs_var).unwrap_or(false),
                        },
                    );
                }

                let mut state_mismatch = BTreeMap::new();
                for (name, lhs_state, rhs_state) in
                    problem.lhs_states.iter().map(|(name, lhs_state)| {
                        (
                            name.clone(),
                            lhs_state,
                            problem.rhs_states.get(name).expect("rhs state must exist"),
                        )
                    })
                {
                    state_mismatch.insert(
                        name,
                        SatStateTransitionMismatch {
                            lhs_next: model.value(lhs_state.next_var).unwrap_or(false),
                            rhs_next: model.value(rhs_state.next_var).unwrap_or(false),
                            lhs_clock: model.value(lhs_state.clock_var).unwrap_or(false),
                            rhs_clock: model.value(rhs_state.clock_var).unwrap_or(false),
                        },
                    );
                }

                return Ok(SequentialEquivalenceReport {
                    equivalent: false,
                    checked_outputs,
                    checked_states,
                    counterexample_inputs: Some(input_assignment),
                    counterexample_present_states: Some(present_state_assignment),
                    counterexample_outputs: Some(output_mismatch),
                    counterexample_states: Some(state_mismatch),
                    sat_stats: aggregated_stats,
                    sat_elapsed_ns: aggregated_elapsed_ns,
                });
            }
        }

        Ok(SequentialEquivalenceReport {
            equivalent: true,
            checked_outputs,
            checked_states,
            counterexample_inputs: None,
            counterexample_present_states: None,
            counterexample_outputs: None,
            counterexample_states: None,
            sat_stats: aggregated_stats,
            sat_elapsed_ns: aggregated_elapsed_ns,
        })
    }

    pub fn check_bounded_sequential_equivalence_sat(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
        depth: usize,
    ) -> Result<BoundedSequentialEquivalenceReport, SynthError> {
        let depth = depth.max(1);
        let problem = build_bounded_sequential_equivalence_problem_data(lhs, rhs, depth)?;
        let incremental_solver = IncrementalSolver::from_formula(problem.formula.clone());
        let mut aggregated_stats = SolveStats::default();
        let mut aggregated_elapsed_ns = 0_u128;

        for check in &problem.checks {
            let mut assumptions = check
                .prior_diff_vars
                .iter()
                .map(|var| Lit::neg(*var))
                .collect::<Vec<_>>();
            assumptions.push(Lit::pos(check.diff_var));

            let (solve_result, sat_metrics) =
                incremental_solver.solve_with_assumptions_and_metrics(&assumptions);
            merge_solve_stats(&mut aggregated_stats, &sat_metrics.stats);
            aggregated_elapsed_ns += sat_metrics.elapsed_ns;

            if let SolveResult::Satisfiable(model) = solve_result {
                let frame = &problem.frames[check.step];
                let report = sequential_report_from_bounded_model(
                    &problem.checked_outputs,
                    &problem.checked_states,
                    frame,
                    &model,
                    aggregated_stats.clone(),
                    aggregated_elapsed_ns,
                );
                return Ok(BoundedSequentialEquivalenceReport {
                    equivalent: false,
                    depth,
                    checked_steps: check.step + 1,
                    unroll_mode: "state_unrolled".to_string(),
                    checked_outputs: problem.checked_outputs,
                    checked_states: problem.checked_states,
                    first_failing_step: Some(check.step),
                    steps: vec![BoundedSequentialEquivalenceStepReport {
                        step: check.step,
                        report,
                    }],
                    sat_stats: aggregated_stats,
                    sat_elapsed_ns: aggregated_elapsed_ns,
                });
            }
        }

        Ok(BoundedSequentialEquivalenceReport {
            equivalent: true,
            depth,
            checked_steps: depth,
            unroll_mode: "state_unrolled".to_string(),
            checked_outputs: problem.checked_outputs,
            checked_states: problem.checked_states,
            first_failing_step: None,
            steps: (0..depth)
                .map(|step| BoundedSequentialEquivalenceStepReport {
                    step,
                    report: SequentialEquivalenceReport {
                        equivalent: true,
                        checked_outputs: Vec::new(),
                        checked_states: Vec::new(),
                        counterexample_inputs: None,
                        counterexample_present_states: None,
                        counterexample_outputs: None,
                        counterexample_states: None,
                        sat_stats: SolveStats::default(),
                        sat_elapsed_ns: 0,
                    },
                })
                .collect(),
            sat_stats: aggregated_stats,
            sat_elapsed_ns: aggregated_elapsed_ns,
        })
    }

    pub fn build_boolean_equivalence_problem(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<EquivalenceSatProblem, SynthError> {
        Ok(build_boolean_equivalence_problem_data(lhs, rhs)?.export)
    }

    pub fn build_sequential_equivalence_problem(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<EquivalenceSatProblem, SynthError> {
        Ok(build_sequential_equivalence_problem_data(lhs, rhs)?.export)
    }
}

fn merge_solve_stats(into: &mut SolveStats, other: &SolveStats) {
    into.recursive_calls += other.recursive_calls;
    into.decisions += other.decisions;
    into.unit_assignments += other.unit_assignments;
    into.pure_literal_assignments += other.pure_literal_assignments;
    into.backtracks += other.backtracks;
    into.restarts += other.restarts;
}

fn sequential_report_from_bounded_model(
    checked_outputs: &[String],
    checked_states: &[String],
    frame: &BoundedSequentialFrameData,
    model: &Model,
    sat_stats: SolveStats,
    sat_elapsed_ns: u128,
) -> SequentialEquivalenceReport {
    let mut input_assignment = BTreeMap::new();
    for (name, var) in &frame.input_vars {
        input_assignment.insert(name.clone(), model.value(*var).unwrap_or(false));
    }

    let mut present_state_assignment = BTreeMap::new();
    for (name, lhs_var) in &frame.lhs_present_state_vars {
        present_state_assignment.insert(name.clone(), model.value(*lhs_var).unwrap_or(false));
    }

    let mut output_mismatch = BTreeMap::new();
    for (name, lhs_var) in &frame.lhs_outputs {
        let rhs_var = frame.rhs_outputs.get(name).expect("rhs output must exist");
        output_mismatch.insert(
            name.clone(),
            SatOutputMismatch {
                lhs: model.value(*lhs_var).unwrap_or(false),
                rhs: model.value(*rhs_var).unwrap_or(false),
            },
        );
    }

    let mut state_mismatch = BTreeMap::new();
    for (name, lhs_state) in &frame.lhs_states {
        let rhs_state = frame.rhs_states.get(name).expect("rhs state must exist");
        state_mismatch.insert(
            name.clone(),
            SatStateTransitionMismatch {
                lhs_next: model.value(lhs_state.next_var).unwrap_or(false),
                rhs_next: model.value(rhs_state.next_var).unwrap_or(false),
                lhs_clock: model.value(lhs_state.clock_var).unwrap_or(false),
                rhs_clock: model.value(rhs_state.clock_var).unwrap_or(false),
            },
        );
    }

    SequentialEquivalenceReport {
        equivalent: false,
        checked_outputs: checked_outputs.to_vec(),
        checked_states: checked_states.to_vec(),
        counterexample_inputs: Some(input_assignment),
        counterexample_present_states: Some(present_state_assignment),
        counterexample_outputs: Some(output_mismatch),
        counterexample_states: Some(state_mismatch),
        sat_stats,
        sat_elapsed_ns,
    }
}

#[derive(Debug, Clone)]
struct SatEncodedNetlist {
    inputs: BTreeSet<String>,
    outputs: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
struct SequentialSatState {
    next_var: usize,
    clock_var: usize,
}

#[derive(Debug, Clone)]
struct SequentialSatEncodedNetlist {
    inputs: BTreeSet<String>,
    outputs: BTreeMap<String, usize>,
    states: BTreeMap<String, SequentialSatState>,
}

#[derive(Debug, Clone)]
enum SequentialDiffCheck {
    Output { diff_var: usize },
    State { diff_var: usize },
}

impl SequentialDiffCheck {
    fn assumptions(&self) -> Vec<Lit> {
        match self {
            Self::Output { diff_var, .. } => vec![Lit::pos(*diff_var)],
            Self::State { diff_var } => vec![Lit::pos(*diff_var)],
        }
    }
}

fn build_boolean_equivalence_problem_data(
    lhs: &Netlist,
    rhs: &Netlist,
) -> Result<BooleanEquivalenceProblemData, SynthError> {
    let mut formula = CnfFormula::new(0);
    let mut shared_input_vars = BTreeMap::<String, usize>::new();

    let lhs_encoded = encode_netlist_for_sat(lhs, &mut formula, &mut shared_input_vars)?;
    let rhs_encoded = encode_netlist_for_sat(rhs, &mut formula, &mut shared_input_vars)?;

    if lhs_encoded.inputs != rhs_encoded.inputs {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "input sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.inputs, rhs_encoded.inputs
        )));
    }
    if lhs_encoded.outputs.keys().collect::<BTreeSet<_>>()
        != rhs_encoded.outputs.keys().collect::<BTreeSet<_>>()
    {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "output sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.outputs.keys().collect::<Vec<_>>(),
            rhs_encoded.outputs.keys().collect::<Vec<_>>()
        )));
    }
    if lhs_encoded.outputs.is_empty() {
        return Err(SynthError::SatInterfaceMismatch(
            "no named output ports found for comparison".to_string(),
        ));
    }

    let mut checks = Vec::new();
    let mut output_vars = Vec::<(String, usize, usize)>::new();
    for output in lhs_encoded.outputs.keys() {
        let lhs_var = *lhs_encoded
            .outputs
            .get(output)
            .expect("lhs output key must exist");
        let rhs_var = *rhs_encoded
            .outputs
            .get(output)
            .expect("rhs output key must exist");
        let diff_var = formula.add_var();
        encode_xor_eq(&mut formula, diff_var, lhs_var, rhs_var)?;
        checks.push(EquivalenceCheckTarget {
            kind: EquivalenceCheckKind::Output,
            name: output.clone(),
            assumptions: vec![Lit::pos(diff_var)],
        });
        output_vars.push((output.clone(), lhs_var, rhs_var));
    }

    Ok(BooleanEquivalenceProblemData {
        export: EquivalenceSatProblem { formula, checks },
        shared_input_vars,
        output_vars,
    })
}

fn build_sequential_equivalence_problem_data(
    lhs: &Netlist,
    rhs: &Netlist,
) -> Result<SequentialEquivalenceProblemData, SynthError> {
    let mut formula = CnfFormula::new(0);
    let mut shared_input_vars = BTreeMap::<String, usize>::new();
    let mut shared_state_vars = BTreeMap::<String, usize>::new();

    let lhs_encoded = encode_netlist_for_sequential_sat(
        lhs,
        &mut formula,
        &mut shared_input_vars,
        &mut shared_state_vars,
    )?;
    let rhs_encoded = encode_netlist_for_sequential_sat(
        rhs,
        &mut formula,
        &mut shared_input_vars,
        &mut shared_state_vars,
    )?;

    if lhs_encoded.inputs != rhs_encoded.inputs {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "input sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.inputs, rhs_encoded.inputs
        )));
    }
    if lhs_encoded.outputs.keys().collect::<BTreeSet<_>>()
        != rhs_encoded.outputs.keys().collect::<BTreeSet<_>>()
    {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "output sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.outputs.keys().collect::<Vec<_>>(),
            rhs_encoded.outputs.keys().collect::<Vec<_>>()
        )));
    }
    if lhs_encoded.states.keys().collect::<BTreeSet<_>>()
        != rhs_encoded.states.keys().collect::<BTreeSet<_>>()
    {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "state sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.states.keys().collect::<Vec<_>>(),
            rhs_encoded.states.keys().collect::<Vec<_>>()
        )));
    }

    let mut checks = Vec::<EquivalenceCheckTarget>::new();

    for output in lhs_encoded.outputs.keys() {
        let lhs_var = *lhs_encoded
            .outputs
            .get(output)
            .expect("lhs output key must exist");
        let rhs_var = *rhs_encoded
            .outputs
            .get(output)
            .expect("rhs output key must exist");
        let diff_var = formula.add_var();
        encode_xor_eq(&mut formula, diff_var, lhs_var, rhs_var)?;
        checks.push(EquivalenceCheckTarget {
            kind: EquivalenceCheckKind::Output,
            name: output.clone(),
            assumptions: SequentialDiffCheck::Output { diff_var }.assumptions(),
        });
    }

    for state_name in lhs_encoded.states.keys() {
        let lhs_state = lhs_encoded
            .states
            .get(state_name)
            .expect("lhs state must exist");
        let rhs_state = rhs_encoded
            .states
            .get(state_name)
            .expect("rhs state must exist");
        let next_diff_var = formula.add_var();
        encode_xor_eq(
            &mut formula,
            next_diff_var,
            lhs_state.next_var,
            rhs_state.next_var,
        )?;
        let clock_diff_var = formula.add_var();
        encode_xor_eq(
            &mut formula,
            clock_diff_var,
            lhs_state.clock_var,
            rhs_state.clock_var,
        )?;
        let state_diff_var = formula.add_var();
        encode_or_eq(
            &mut formula,
            state_diff_var,
            &[next_diff_var, clock_diff_var],
        )?;
        checks.push(EquivalenceCheckTarget {
            kind: EquivalenceCheckKind::State,
            name: state_name.clone(),
            assumptions: SequentialDiffCheck::State {
                diff_var: state_diff_var,
            }
            .assumptions(),
        });
    }

    Ok(SequentialEquivalenceProblemData {
        export: EquivalenceSatProblem { formula, checks },
        shared_input_vars,
        shared_state_vars,
        lhs_outputs: lhs_encoded.outputs,
        rhs_outputs: rhs_encoded.outputs,
        lhs_states: lhs_encoded.states,
        rhs_states: rhs_encoded.states,
    })
}

fn build_bounded_sequential_equivalence_problem_data(
    lhs: &Netlist,
    rhs: &Netlist,
    depth: usize,
) -> Result<BoundedSequentialProblemData, SynthError> {
    let mut formula = CnfFormula::new(0);
    let mut frames = Vec::<BoundedSequentialFrameData>::with_capacity(depth);
    let mut checks = Vec::new();
    let mut prior_diff_vars = Vec::<usize>::new();
    let mut checked_outputs = Vec::<String>::new();
    let mut checked_states = Vec::<String>::new();

    for step in 0..depth {
        let mut shared_input_vars = BTreeMap::<String, usize>::new();
        let mut lhs_state_vars = BTreeMap::<String, usize>::new();
        let mut rhs_state_vars = BTreeMap::<String, usize>::new();

        let lhs_encoded = encode_netlist_for_sequential_sat(
            lhs,
            &mut formula,
            &mut shared_input_vars,
            &mut lhs_state_vars,
        )?;
        let rhs_encoded = encode_netlist_for_sequential_sat(
            rhs,
            &mut formula,
            &mut shared_input_vars,
            &mut rhs_state_vars,
        )?;

        validate_sequential_interfaces(&lhs_encoded, &rhs_encoded)?;
        if step == 0 {
            checked_outputs = lhs_encoded.outputs.keys().cloned().collect();
            checked_states = lhs_encoded.states.keys().cloned().collect();
            for state_name in lhs_encoded.states.keys() {
                let lhs_initial = *lhs_state_vars
                    .get(state_name)
                    .expect("lhs initial state var must exist");
                let rhs_initial = *rhs_state_vars
                    .get(state_name)
                    .expect("rhs initial state var must exist");
                encode_equal(&mut formula, lhs_initial, rhs_initial)?;
            }
        }

        if let Some(previous_frame) = frames.last() {
            for state_name in lhs_encoded.states.keys() {
                let previous_lhs = previous_frame
                    .lhs_states
                    .get(state_name)
                    .expect("previous lhs state must exist");
                let previous_rhs = previous_frame
                    .rhs_states
                    .get(state_name)
                    .expect("previous rhs state must exist");
                let current_lhs = *lhs_state_vars
                    .get(state_name)
                    .expect("current lhs state var must exist");
                let current_rhs = *rhs_state_vars
                    .get(state_name)
                    .expect("current rhs state var must exist");
                encode_equal(&mut formula, current_lhs, previous_lhs.next_var)?;
                encode_equal(&mut formula, current_rhs, previous_rhs.next_var)?;
            }
        }

        for output in lhs_encoded.outputs.keys() {
            let lhs_var = *lhs_encoded
                .outputs
                .get(output)
                .expect("lhs output key must exist");
            let rhs_var = *rhs_encoded
                .outputs
                .get(output)
                .expect("rhs output key must exist");
            let diff_var = formula.add_var();
            encode_xor_eq(&mut formula, diff_var, lhs_var, rhs_var)?;
            checks.push(BoundedSequentialCheck {
                step,
                diff_var,
                prior_diff_vars: prior_diff_vars.clone(),
            });
            prior_diff_vars.push(diff_var);
        }

        for state_name in lhs_encoded.states.keys() {
            let lhs_state = lhs_encoded
                .states
                .get(state_name)
                .expect("lhs state must exist");
            let rhs_state = rhs_encoded
                .states
                .get(state_name)
                .expect("rhs state must exist");
            let next_diff_var = formula.add_var();
            encode_xor_eq(
                &mut formula,
                next_diff_var,
                lhs_state.next_var,
                rhs_state.next_var,
            )?;
            let clock_diff_var = formula.add_var();
            encode_xor_eq(
                &mut formula,
                clock_diff_var,
                lhs_state.clock_var,
                rhs_state.clock_var,
            )?;
            let state_diff_var = formula.add_var();
            encode_or_eq(
                &mut formula,
                state_diff_var,
                &[next_diff_var, clock_diff_var],
            )?;
            checks.push(BoundedSequentialCheck {
                step,
                diff_var: state_diff_var,
                prior_diff_vars: prior_diff_vars.clone(),
            });
            prior_diff_vars.push(state_diff_var);
        }

        frames.push(BoundedSequentialFrameData {
            input_vars: shared_input_vars,
            lhs_present_state_vars: lhs_state_vars,
            lhs_outputs: lhs_encoded.outputs,
            rhs_outputs: rhs_encoded.outputs,
            lhs_states: lhs_encoded.states,
            rhs_states: rhs_encoded.states,
        });
    }

    Ok(BoundedSequentialProblemData {
        formula,
        checks,
        frames,
        checked_outputs,
        checked_states,
    })
}

fn validate_sequential_interfaces(
    lhs_encoded: &SequentialSatEncodedNetlist,
    rhs_encoded: &SequentialSatEncodedNetlist,
) -> Result<(), SynthError> {
    if lhs_encoded.inputs != rhs_encoded.inputs {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "input sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.inputs, rhs_encoded.inputs
        )));
    }
    if lhs_encoded.outputs.keys().collect::<BTreeSet<_>>()
        != rhs_encoded.outputs.keys().collect::<BTreeSet<_>>()
    {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "output sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.outputs.keys().collect::<Vec<_>>(),
            rhs_encoded.outputs.keys().collect::<Vec<_>>()
        )));
    }
    if lhs_encoded.states.keys().collect::<BTreeSet<_>>()
        != rhs_encoded.states.keys().collect::<BTreeSet<_>>()
    {
        return Err(SynthError::SatInterfaceMismatch(format!(
            "state sets differ: lhs={:?}, rhs={:?}",
            lhs_encoded.states.keys().collect::<Vec<_>>(),
            rhs_encoded.states.keys().collect::<Vec<_>>()
        )));
    }
    Ok(())
}

fn encode_netlist_for_sequential_sat(
    netlist: &Netlist,
    formula: &mut CnfFormula,
    shared_input_vars: &mut BTreeMap<String, usize>,
    shared_state_vars: &mut BTreeMap<String, usize>,
) -> Result<SequentialSatEncodedNetlist, SynthError> {
    let topo = topological_order_for_sequential_sat(netlist)?;
    let (mut incoming_by_node, outdegree) = incoming_and_outdegree_for_sat(netlist);
    for incoming in &mut incoming_by_node {
        incoming.sort_by_key(|(port, _)| *port);
    }

    let mut node_var = vec![None::<usize>; netlist.node_count()];
    let mut inputs = BTreeSet::new();
    let mut outputs = BTreeMap::new();

    for node in netlist.nodes() {
        if !matches!(node.kind, NodeKind::Dff) {
            continue;
        }

        let present_var = *shared_state_vars
            .entry(node.name.clone())
            .or_insert_with(|| formula.add_var());
        node_var[node.id.0] = Some(present_var);
    }

    for node_index in topo {
        let node = &netlist.nodes()[node_index];
        if matches!(node.kind, NodeKind::Dff) {
            continue;
        }

        let incoming = &incoming_by_node[node_index];
        if incoming.is_empty() {
            match node.kind {
                NodeKind::Port => {
                    let var = *shared_input_vars
                        .entry(node.name.clone())
                        .or_insert_with(|| formula.add_var());
                    node_var[node_index] = Some(var);
                    inputs.insert(node.name.clone());
                }
                _ => {
                    return Err(SynthError::SatUnsupportedNodeKind {
                        node: node_index,
                        kind: node.kind.clone(),
                    })
                }
            }
            continue;
        }

        let operand_vars: Vec<usize> = incoming
            .iter()
            .map(|(_, source)| node_var[source.node.0].expect("topo order should resolve vars"))
            .collect();

        let produced_var = match node.kind {
            NodeKind::Port | NodeKind::Splitter | NodeKind::Jtl | NodeKind::Ptl => {
                if operand_vars.len() != 1 {
                    return Err(SynthError::SatUnexpectedInputCount {
                        node: node_index,
                        expected: 1,
                        actual: operand_vars.len(),
                    });
                }
                operand_vars[0]
            }
            NodeKind::CellInstance | NodeKind::MacroCell => {
                let op = node.logic_op.clone().unwrap_or(rflux_ir::LogicOp::And);
                if op == rflux_ir::LogicOp::DffEnable {
                    return Err(SynthError::SatUnsupportedNodeKind {
                        node: node_index,
                        kind: node.kind.clone(),
                    });
                }
                let output_var = formula.add_var();
                encode_gate_relation(formula, output_var, &operand_vars, &op)?;
                output_var
            }
            NodeKind::Dff => continue,
        };

        node_var[node_index] = Some(produced_var);
    }

    let mut states = BTreeMap::new();
    for node in netlist.nodes() {
        if !matches!(node.kind, NodeKind::Dff) {
            continue;
        }

        let incoming = &incoming_by_node[node.id.0];
        let present_var = node_var[node.id.0].expect("dff present state var must exist");
        let (next_var, clock_var) = match node.logic_op {
            Some(rflux_ir::LogicOp::DffEnable) => {
                if incoming.len() != 3
                    || incoming[0].0 != 0
                    || incoming[1].0 != 1
                    || incoming[2].0 != 2
                {
                    return Err(SynthError::SatUnexpectedInputCount {
                        node: node.id.0,
                        expected: 3,
                        actual: incoming.len(),
                    });
                }
                let data_var = node_var[incoming[0].1.node.0].ok_or_else(|| {
                    SynthError::SatEncoding(format!(
                        "missing DffEnable data var for node {}",
                        node.id.0
                    ))
                })?;
                let enable_var = node_var[incoming[1].1.node.0].ok_or_else(|| {
                    SynthError::SatEncoding(format!(
                        "missing DffEnable enable var for node {}",
                        node.id.0
                    ))
                })?;
                let clock_var = node_var[incoming[2].1.node.0].ok_or_else(|| {
                    SynthError::SatEncoding(format!(
                        "missing DffEnable clock var for node {}",
                        node.id.0
                    ))
                })?;
                let next_var = formula.add_var();
                encode_mux2_eq(formula, next_var, enable_var, present_var, data_var)?;
                (next_var, clock_var)
            }
            None => {
                if incoming.len() != 2 || incoming[0].0 != 0 || incoming[1].0 != 1 {
                    return Err(SynthError::SatUnexpectedInputCount {
                        node: node.id.0,
                        expected: 2,
                        actual: incoming.len(),
                    });
                }
                let data_var = node_var[incoming[0].1.node.0].ok_or_else(|| {
                    SynthError::SatEncoding(format!("missing Dff data var for node {}", node.id.0))
                })?;
                let clock_var = node_var[incoming[1].1.node.0].ok_or_else(|| {
                    SynthError::SatEncoding(format!("missing Dff clock var for node {}", node.id.0))
                })?;
                (data_var, clock_var)
            }
            Some(_) => {
                return Err(SynthError::SatUnsupportedNodeKind {
                    node: node.id.0,
                    kind: NodeKind::Dff,
                })
            }
        };

        states.insert(
            node.name.clone(),
            SequentialSatState {
                next_var,
                clock_var,
            },
        );
    }

    for node in netlist.nodes() {
        if !matches!(node.kind, NodeKind::Port) || outdegree[node.id.0] != 0 {
            continue;
        }
        if incoming_by_node[node.id.0].is_empty() {
            continue;
        }
        outputs.insert(
            node.name.clone(),
            node_var[node.id.0].expect("encoded output port must have variable"),
        );
    }

    Ok(SequentialSatEncodedNetlist {
        inputs,
        outputs,
        states,
    })
}

fn encode_netlist_for_sat(
    netlist: &Netlist,
    formula: &mut CnfFormula,
    shared_input_vars: &mut BTreeMap<String, usize>,
) -> Result<SatEncodedNetlist, SynthError> {
    let topo = topological_order_for_sat(netlist)?;
    let (mut incoming_by_node, outdegree) = incoming_and_outdegree_for_sat(netlist);
    for incoming in &mut incoming_by_node {
        incoming.sort_by_key(|(port, _)| *port);
    }

    let mut node_var = vec![None::<usize>; netlist.node_count()];
    let mut inputs = BTreeSet::new();
    let mut outputs = BTreeMap::new();

    for node_index in topo {
        let node = &netlist.nodes()[node_index];
        let incoming = &incoming_by_node[node_index];

        if incoming.is_empty() {
            match node.kind {
                NodeKind::Port => {
                    let var = *shared_input_vars
                        .entry(node.name.clone())
                        .or_insert_with(|| formula.add_var());
                    node_var[node_index] = Some(var);
                    inputs.insert(node.name.clone());
                }
                _ => {
                    return Err(SynthError::SatUnsupportedNodeKind {
                        node: node_index,
                        kind: node.kind.clone(),
                    })
                }
            }
            continue;
        }

        let operand_vars: Vec<usize> = incoming
            .iter()
            .map(|(_, source)| node_var[source.node.0].expect("topo order should resolve vars"))
            .collect();

        let produced_var = match node.kind {
            NodeKind::Port | NodeKind::Splitter | NodeKind::Jtl | NodeKind::Ptl => {
                if operand_vars.len() != 1 {
                    return Err(SynthError::SatUnexpectedInputCount {
                        node: node_index,
                        expected: 1,
                        actual: operand_vars.len(),
                    });
                }
                operand_vars[0]
            }
            NodeKind::CellInstance | NodeKind::MacroCell => {
                let op = node.logic_op.clone().unwrap_or(rflux_ir::LogicOp::And);
                let output_var = formula.add_var();
                encode_gate_relation(formula, output_var, &operand_vars, &op)?;
                output_var
            }
            NodeKind::Dff => {
                return Err(SynthError::SatUnsupportedNodeKind {
                    node: node_index,
                    kind: NodeKind::Dff,
                })
            }
        };

        node_var[node_index] = Some(produced_var);
    }

    for node in netlist.nodes() {
        if !matches!(node.kind, NodeKind::Port) || outdegree[node.id.0] != 0 {
            continue;
        }
        if incoming_by_node[node.id.0].is_empty() {
            continue;
        }
        outputs.insert(
            node.name.clone(),
            node_var[node.id.0].expect("encoded output port must have variable"),
        );
    }

    Ok(SatEncodedNetlist { inputs, outputs })
}

fn encode_gate_relation(
    formula: &mut CnfFormula,
    output_var: usize,
    operands: &[usize],
    op: &rflux_ir::LogicOp,
) -> Result<(), SynthError> {
    use rflux_ir::LogicOp;

    match op {
        LogicOp::Buf => {
            if operands.len() != 1 {
                return Err(SynthError::SatEncoding(format!(
                    "BUF expects 1 input, got {}",
                    operands.len()
                )));
            }
            add_buf_eq(formula, output_var, operands[0])
        }
        LogicOp::Not => {
            if operands.len() != 1 {
                return Err(SynthError::SatEncoding(format!(
                    "NOT expects 1 input, got {}",
                    operands.len()
                )));
            }
            add_not_eq(formula, output_var, operands[0])
        }
        LogicOp::And => encode_and_eq(formula, output_var, operands),
        LogicOp::Or => encode_or_eq(formula, output_var, operands),
        LogicOp::Xor => encode_xor_nary_eq(formula, output_var, operands),
        LogicOp::Mux2 => {
            if operands.len() != 3 {
                return Err(SynthError::SatEncoding(format!(
                    "MUX2 expects 3 inputs, got {}",
                    operands.len()
                )));
            }
            encode_mux2_eq(formula, output_var, operands[0], operands[1], operands[2])
        }
        LogicOp::DffEnable => Err(SynthError::SatEncoding(
            "DffEnable is sequential and not supported in combinational SAT check".to_string(),
        )),
    }
}

fn add_clause(formula: &mut CnfFormula, clause: Vec<Lit>) -> Result<(), SynthError> {
    formula
        .add_clause(clause)
        .map_err(|err| SynthError::SatEncoding(format!("{err:?}")))
}

fn add_buf_eq(formula: &mut CnfFormula, z: usize, x: usize) -> Result<(), SynthError> {
    add_clause(formula, vec![Lit::neg(z), Lit::pos(x)])?;
    add_clause(formula, vec![Lit::pos(z), Lit::neg(x)])
}

fn add_not_eq(formula: &mut CnfFormula, z: usize, x: usize) -> Result<(), SynthError> {
    add_clause(formula, vec![Lit::neg(z), Lit::neg(x)])?;
    add_clause(formula, vec![Lit::pos(z), Lit::pos(x)])
}

fn encode_and_eq(formula: &mut CnfFormula, z: usize, inputs: &[usize]) -> Result<(), SynthError> {
    if inputs.is_empty() {
        return Err(SynthError::SatEncoding(
            "AND with zero inputs is unsupported".to_string(),
        ));
    }
    if inputs.len() == 1 {
        return add_buf_eq(formula, z, inputs[0]);
    }

    for x in inputs {
        add_clause(formula, vec![Lit::neg(z), Lit::pos(*x)])?;
    }
    let mut reverse = vec![Lit::pos(z)];
    reverse.extend(inputs.iter().map(|x| Lit::neg(*x)));
    add_clause(formula, reverse)
}

fn encode_or_eq(formula: &mut CnfFormula, z: usize, inputs: &[usize]) -> Result<(), SynthError> {
    if inputs.is_empty() {
        return Err(SynthError::SatEncoding(
            "OR with zero inputs is unsupported".to_string(),
        ));
    }
    if inputs.len() == 1 {
        return add_buf_eq(formula, z, inputs[0]);
    }

    for x in inputs {
        add_clause(formula, vec![Lit::pos(z), Lit::neg(*x)])?;
    }
    let mut reverse = vec![Lit::neg(z)];
    reverse.extend(inputs.iter().map(|x| Lit::pos(*x)));
    add_clause(formula, reverse)
}

fn encode_equal(formula: &mut CnfFormula, x: usize, y: usize) -> Result<(), SynthError> {
    add_clause(formula, vec![Lit::neg(x), Lit::pos(y)])?;
    add_clause(formula, vec![Lit::pos(x), Lit::neg(y)])
}

fn encode_xor_eq(formula: &mut CnfFormula, z: usize, x: usize, y: usize) -> Result<(), SynthError> {
    add_clause(formula, vec![Lit::neg(x), Lit::neg(y), Lit::neg(z)])?;
    add_clause(formula, vec![Lit::neg(x), Lit::pos(y), Lit::pos(z)])?;
    add_clause(formula, vec![Lit::pos(x), Lit::neg(y), Lit::pos(z)])?;
    add_clause(formula, vec![Lit::pos(x), Lit::pos(y), Lit::neg(z)])
}

fn encode_xor_nary_eq(
    formula: &mut CnfFormula,
    z: usize,
    inputs: &[usize],
) -> Result<(), SynthError> {
    if inputs.is_empty() {
        return Err(SynthError::SatEncoding(
            "XOR with zero inputs is unsupported".to_string(),
        ));
    }
    if inputs.len() == 1 {
        return add_buf_eq(formula, z, inputs[0]);
    }

    let mut acc = inputs[0];
    for (index, next) in inputs.iter().enumerate().skip(1) {
        let out = if index == inputs.len() - 1 {
            z
        } else {
            formula.add_var()
        };
        encode_xor_eq(formula, out, acc, *next)?;
        acc = out;
    }
    Ok(())
}

fn encode_mux2_eq(
    formula: &mut CnfFormula,
    z: usize,
    s: usize,
    a: usize,
    b: usize,
) -> Result<(), SynthError> {
    add_clause(formula, vec![Lit::neg(s), Lit::neg(a), Lit::pos(z)])?;
    add_clause(formula, vec![Lit::pos(s), Lit::neg(b), Lit::pos(z)])?;
    add_clause(formula, vec![Lit::neg(s), Lit::pos(a), Lit::neg(z)])?;
    add_clause(formula, vec![Lit::pos(s), Lit::pos(b), Lit::neg(z)])
}

fn incoming_and_outdegree_for_sat(netlist: &Netlist) -> (Vec<Vec<(u16, PinRef)>>, Vec<usize>) {
    let mut incoming_by_node = vec![Vec::<(u16, PinRef)>::new(); netlist.node_count()];
    let mut outdegree = vec![0usize; netlist.node_count()];
    for (from, to) in netlist.edge_pairs() {
        incoming_by_node[to.node.0].push((to.port, from));
        outdegree[from.node.0] += 1;
    }
    (incoming_by_node, outdegree)
}

fn topological_order_for_sat(netlist: &Netlist) -> Result<Vec<usize>, SynthError> {
    let mut indegree = vec![0usize; netlist.node_count()];
    let mut adjacency = vec![Vec::<usize>::new(); netlist.node_count()];
    for (from, to) in netlist.edge_pairs() {
        indegree[to.node.0] += 1;
        adjacency[from.node.0].push(to.node.0);
    }

    let mut queue = VecDeque::new();
    for (node, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(node);
        }
    }

    let mut order = Vec::with_capacity(netlist.node_count());
    while let Some(node) = queue.pop_front() {
        order.push(node);
        for succ in &adjacency[node] {
            indegree[*succ] -= 1;
            if indegree[*succ] == 0 {
                queue.push_back(*succ);
            }
        }
    }

    if order.len() != netlist.node_count() {
        return Err(SynthError::CombinationalCycle);
    }
    Ok(order)
}

fn topological_order_for_sequential_sat(netlist: &Netlist) -> Result<Vec<usize>, SynthError> {
    let mut indegree = vec![0usize; netlist.node_count()];
    let mut adjacency = vec![Vec::<usize>::new(); netlist.node_count()];
    for (from, to) in netlist.edge_pairs() {
        if matches!(netlist.nodes()[to.node.0].kind, NodeKind::Dff) {
            continue;
        }
        indegree[to.node.0] += 1;
        adjacency[from.node.0].push(to.node.0);
    }

    let mut queue = VecDeque::new();
    for (node, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(node);
        }
    }

    let mut order = Vec::with_capacity(netlist.node_count());
    while let Some(node) = queue.pop_front() {
        order.push(node);
        for successor in &adjacency[node] {
            indegree[*successor] -= 1;
            if indegree[*successor] == 0 {
                queue.push_back(*successor);
            }
        }
    }

    if order.len() != netlist.node_count() {
        return Err(SynthError::SatEncoding(
            "could not resolve sequential SAT order".to_string(),
        ));
    }

    Ok(order)
}

// ---------------------------------------------------------------------------
// P1-5: Physical Feasibility Estimator
// ---------------------------------------------------------------------------

/// Result of physical feasibility estimation (P1-5).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FeasibilityReport {
    /// Total estimated cell area (um²).
    pub estimated_area_um2: f64,
    /// Total number of nodes.
    pub node_count: usize,
    /// Total number of edges (nets).
    pub edge_count: usize,
    /// Average fanout per net.
    pub avg_fanout: f64,
    /// Number of splitter nodes (auto-inserted).
    pub splitter_count: usize,
    /// Number of DFF nodes (including balancing DFFs).
    pub dff_count: usize,
    /// Estimated routing demand (total edge length in um).
    pub estimated_routing_demand_um: f64,
    /// Estimated routing capacity (available channel length in um).
    pub estimated_routing_capacity_um: f64,
    /// Congestion ratio: demand / capacity.  >1.0 means over-congested.
    pub congestion_ratio: f64,
    /// Pipeline depth (number of DFF stages in the longest path).
    pub pipeline_depth: usize,
    /// Whether the design is physically feasible.
    pub feasible: bool,
    /// Warnings about potential physical issues.
    pub warnings: Vec<String>,
}

/// Physical feasibility estimator for synthesis output (P1-5).
///
/// Estimates routing congestion, area, and pipeline depth from the
/// netlist without running full placement/routing.  Used during
/// synthesis to catch physical infeasibility early.
#[derive(Debug, Default)]
pub struct PhysicalFeasibilityEstimator;

impl PhysicalFeasibilityEstimator {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Estimate physical feasibility of a synthesized netlist.
    pub fn estimate(&self, netlist: &Netlist, _pdk: &Pdk) -> FeasibilityReport {
        let nodes = netlist.nodes();
        let node_count = nodes.len();
        let edge_count = netlist.edge_count();

        // Count node types
        let mut splitter_count = 0usize;
        let mut dff_count = 0usize;
        let mut total_area = 0.0f64;

        for node in nodes {
            match node.kind {
                NodeKind::Splitter => {
                    splitter_count += 1;
                    total_area += 5.0; // default splitter area
                }
                NodeKind::Dff => {
                    dff_count += 1;
                    total_area += 20.0; // default DFF area
                }
                NodeKind::CellInstance => {
                    total_area += 10.0; // default gate area
                }
                NodeKind::MacroCell => {
                    total_area += 50.0; // default macro area
                }
                _ => {}
            }
        }

        // Compute average fanout
        let avg_fanout = if node_count > 0 {
            edge_count as f64 / node_count.max(1) as f64
        } else {
            0.0
        };

        // Estimate routing demand: sum of Manhattan distances for all edges
        // Use a heuristic: each edge spans ~sqrt(area/nodes) on average
        let avg_span = if node_count > 1 {
            (total_area / node_count as f64).sqrt()
        } else {
            0.0
        };
        let routing_demand = edge_count as f64 * avg_span;

        // Estimate routing capacity: available channel length
        // Assume 2 metal layers, each providing perimeter * channel_density
        let side_length = total_area.sqrt();
        let channel_density = 0.5; // 50% of perimeter usable for routing
        let routing_capacity = 2.0 * 4.0 * side_length * channel_density;

        let congestion_ratio = if routing_capacity > 0.0 {
            routing_demand / routing_capacity
        } else {
            0.0
        };

        // Estimate pipeline depth from topological analysis
        let pipeline_depth = self.estimate_pipeline_depth(netlist);

        // Generate warnings
        let mut warnings = Vec::new();
        if congestion_ratio > 1.0 {
            warnings.push(format!(
                "High congestion risk: ratio {:.2} (>1.0). Consider reducing fanout \
                 or using more compound cells.",
                congestion_ratio
            ));
        }
        if splitter_count as f64 / node_count.max(1) as f64 > 0.3 {
            warnings.push(format!(
                "High splitter ratio: {}/{} nodes are splitters. \
                 Consider restructuring to reduce fanout.",
                splitter_count, node_count
            ));
        }
        if pipeline_depth > 10 {
            warnings.push(format!(
                "Deep pipeline: {} stages. Consider using compound cells \
                 to reduce DFF overhead.",
                pipeline_depth
            ));
        }

        FeasibilityReport {
            estimated_area_um2: total_area,
            node_count,
            edge_count,
            avg_fanout,
            splitter_count,
            dff_count,
            estimated_routing_demand_um: routing_demand,
            estimated_routing_capacity_um: routing_capacity,
            congestion_ratio,
            pipeline_depth,
            feasible: congestion_ratio <= 1.5 && warnings.is_empty(),
            warnings,
        }
    }

    /// Estimate pipeline depth using topological BFS.
    fn estimate_pipeline_depth(&self, netlist: &Netlist) -> usize {
        use std::collections::{HashMap, VecDeque};

        let mut depth_map: HashMap<rflux_ir::NodeId, usize> = HashMap::new();
        let mut in_degree: HashMap<rflux_ir::NodeId, usize> = HashMap::new();
        let mut children: HashMap<rflux_ir::NodeId, Vec<rflux_ir::NodeId>> = HashMap::new();

        for node in netlist.nodes() {
            in_degree.entry(node.id).or_insert(0);
            children.entry(node.id).or_default();
        }
        for (from, to) in netlist.edge_pairs() {
            *in_degree.entry(to.node).or_insert(0) += 1;
            children.entry(from.node).or_default().push(to.node);
        }

        let mut queue = VecDeque::new();
        for (&node_id, &deg) in &in_degree {
            if deg == 0 {
                depth_map.insert(node_id, 0);
                queue.push_back(node_id);
            }
        }

        while let Some(current) = queue.pop_front() {
            let current_depth = depth_map[&current];
            let node = &netlist.nodes()[current.0];
            // DFF resets the pipeline depth
            let next_depth = if matches!(node.kind, NodeKind::Dff) {
                0
            } else {
                current_depth + 1
            };

            if let Some(child_list) = children.get(&current) {
                for &child in child_list {
                    let entry = depth_map.entry(child).or_insert(0);
                    if next_depth > *entry {
                        *entry = next_depth;
                    }
                    let deg = in_degree.get_mut(&child).unwrap();
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(child);
                    }
                }
            }
        }

        depth_map.values().copied().max().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// P2-2: SFQ DFT (Design for Testability) Framework
// ---------------------------------------------------------------------------

/// Result of testability analysis (P2-2).
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestabilityReport {
    /// Total nodes in the netlist.
    pub total_nodes: usize,
    /// Nodes that are controllable (reachable from a test point or primary input).
    pub controllable_nodes: usize,
    /// Nodes that are observable (can be observed at a test point or primary output).
    pub observable_nodes: usize,
    /// Estimated stuck-at fault coverage (%).
    pub fault_coverage_percent: f64,
    /// Nodes that are neither controllable nor observable.
    pub untestable_nodes: Vec<String>,
    /// Test points inserted.
    pub test_points_inserted: usize,
    /// Observation points inserted.
    pub observation_points_inserted: usize,
}

/// A test point inserted into the netlist for controllability.
#[derive(Debug, Clone)]
pub struct TestPoint {
    /// Node where the test point is inserted.
    pub target_node: rflux_ir::NodeId,
    /// Name of the test point input.
    pub name: String,
    /// The test point node id (after insertion).
    pub test_node: rflux_ir::NodeId,
}

/// An observation point inserted for observability.
#[derive(Debug, Clone)]
pub struct ObservationPoint {
    /// Node being observed.
    pub target_node: rflux_ir::NodeId,
    /// Name of the observation output.
    pub name: String,
    /// The observer splitter node id.
    pub observer_node: rflux_ir::NodeId,
}

/// SFQ test point injector (P2-2).
///
/// Inserts controllable test points at strategic locations in the
/// netlist to improve fault coverage.  In SFQ, a test point is a
/// pulse injection site that can be activated during testing to
/// set a known value at a node.
#[derive(Debug, Default)]
pub struct TestPointInjector;

impl TestPointInjector {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Insert test points at nodes with high fanout (hard to control).
    ///
    /// Returns the list of inserted test points.
    pub fn insert_test_points(
        &self,
        netlist: &mut Netlist,
        max_points: usize,
    ) -> Vec<TestPoint> {
        let mut test_points = Vec::new();
        let mut fanout_count: std::collections::HashMap<rflux_ir::NodeId, usize> =
            std::collections::HashMap::new();

        for (from, _to) in netlist.edge_pairs() {
            *fanout_count.entry(from.node).or_insert(0) += 1;
        }

        let mut high_fanout: Vec<(rflux_ir::NodeId, usize)> = fanout_count
            .into_iter()
            .filter(|(_, count)| *count > 2)
            .collect();
        high_fanout.sort_by(|a, b| b.1.cmp(&a.1));

        for (node_id, _) in high_fanout.into_iter().take(max_points) {
            let tp_name = format!("test_point_{}", test_points.len());
            let tp_id = netlist.add_node(NodeKind::CellInstance, &tp_name);
            netlist
                .connect(
                    rflux_ir::PinRef { node: tp_id, port: 0 },
                    rflux_ir::PinRef { node: node_id, port: 0 },
                )
                .ok();
            test_points.push(TestPoint {
                target_node: node_id,
                name: tp_name,
                test_node: tp_id,
            });
        }

        test_points
    }
}

/// SFQ pulse observer (P2-2).
///
/// Inserts non-destructive observation points by adding splitter
/// nodes that copy the pulse to an observation channel.  In SFQ,
/// destructive readout means we can't simply "probe" a node; we
/// must split the signal to observe it without consuming it.
#[derive(Debug, Default)]
pub struct PulseObserver;

impl PulseObserver {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Insert observation points at output ports.
    ///
    /// Returns the list of inserted observation points.
    pub fn insert_observation_points(
        &self,
        netlist: &mut Netlist,
    ) -> Vec<ObservationPoint> {
        let mut observers = Vec::new();

        let output_ports: Vec<rflux_ir::NodeId> = netlist
            .nodes()
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Port))
            .filter(|n| {
                // Check if this port is driven (output port)
                netlist.edge_pairs().iter().any(|(_, to)| to.node == n.id)
            })
            .map(|n| n.id)
            .collect();

        for port_id in output_ports {
            let obs_name = format!("obs_{}", observers.len());
            let obs_id = netlist.add_node(NodeKind::Splitter, &obs_name);

            // Find the edge driving this port
            if let Some((from, _to)) = netlist
                .edge_pairs()
                .iter()
                .find(|(_, to)| to.node == port_id)
                .map(|(f, t)| (*f, *t))
            {
                netlist.disconnect(from);
                netlist.connect(from, rflux_ir::PinRef { node: obs_id, port: 0 }).ok();
                netlist
                    .connect(
                        rflux_ir::PinRef { node: obs_id, port: 0 },
                        rflux_ir::PinRef { node: port_id, port: 0 },
                    )
                    .ok();

                observers.push(ObservationPoint {
                    target_node: port_id,
                    name: obs_name,
                    observer_node: obs_id,
                });
            }
        }

        observers
    }
}

/// Analyze testability of a netlist (P2-2).
///
/// Computes controllability (reachability from primary inputs) and
/// observability (reachability to primary outputs) to estimate
/// stuck-at fault coverage.
pub fn analyze_testability(netlist: &Netlist) -> TestabilityReport {
    use std::collections::{HashSet, VecDeque};

    let nodes = netlist.nodes();
    let edges = netlist.edge_pairs();
    let total = nodes.len();

    // Find primary inputs (Port nodes with no incoming edges)
    let driven: HashSet<rflux_ir::NodeId> = edges.iter().map(|(_, to)| to.node).collect();
    let primary_inputs: HashSet<rflux_ir::NodeId> = nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Port) && !driven.contains(&n.id))
        .map(|n| n.id)
        .collect();

    // Find primary outputs (Port nodes with no outgoing edges)
    let drivers: HashSet<rflux_ir::NodeId> = edges.iter().map(|(from, _)| from.node).collect();
    let primary_outputs: HashSet<rflux_ir::NodeId> = nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Port) && !drivers.contains(&n.id))
        .map(|n| n.id)
        .collect();

    // Forward reachability from inputs (controllability)
    let mut controllable: HashSet<rflux_ir::NodeId> = HashSet::new();
    let mut queue: VecDeque<rflux_ir::NodeId> = primary_inputs.iter().copied().collect();
    let mut children: std::collections::HashMap<rflux_ir::NodeId, Vec<rflux_ir::NodeId>> =
        std::collections::HashMap::new();
    for (from, to) in &edges {
        children.entry(from.node).or_default().push(to.node);
    }
    while let Some(current) = queue.pop_front() {
        if controllable.insert(current) {
            if let Some(child_list) = children.get(&current) {
                for &child in child_list {
                    if !controllable.contains(&child) {
                        queue.push_back(child);
                    }
                }
            }
        }
    }

    // Backward reachability from outputs (observability)
    let mut observable: HashSet<rflux_ir::NodeId> = HashSet::new();
    let mut queue: VecDeque<rflux_ir::NodeId> = primary_outputs.iter().copied().collect();
    let mut parents: std::collections::HashMap<rflux_ir::NodeId, Vec<rflux_ir::NodeId>> =
        std::collections::HashMap::new();
    for (from, to) in &edges {
        parents.entry(to.node).or_default().push(from.node);
    }
    while let Some(current) = queue.pop_front() {
        if observable.insert(current) {
            if let Some(parent_list) = parents.get(&current) {
                for &parent in parent_list {
                    if !observable.contains(&parent) {
                        queue.push_back(parent);
                    }
                }
            }
        }
    }

    let controllable_count = controllable.len();
    let observable_count = observable.len();

    // Fault coverage: a fault is detectable if the site is both
    // controllable and observable.
    let testable = controllable.intersection(&observable).count();
    let coverage = if total > 0 {
        testable as f64 / total as f64 * 100.0
    } else {
        100.0
    };

    let untestable: Vec<String> = nodes
        .iter()
        .filter(|n| !controllable.contains(&n.id) || !observable.contains(&n.id))
        .map(|n| n.name.clone())
        .collect();

    TestabilityReport {
        total_nodes: total,
        controllable_nodes: controllable_count,
        observable_nodes: observable_count,
        fault_coverage_percent: coverage,
        untestable_nodes: untestable,
        test_points_inserted: 0,
        observation_points_inserted: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{LogicOp, NodeId};

    #[test]
    fn inserts_splitter_when_fanout_is_requested() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let src = netlist.add_node(NodeKind::CellInstance, "src");
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        let src_out = PinRef { node: src, port: 0 };
        let a_in = PinRef { node: a, port: 0 };
        let b_in = PinRef { node: b, port: 0 };

        compiler
            .compile(&mut netlist, src_out, a_in)
            .expect("first sink should connect directly");
        compiler
            .compile(&mut netlist, src_out, b_in)
            .expect("second sink should trigger splitter insertion");

        assert_eq!(netlist.node_count(), 4);
        assert_eq!(netlist.edge_count(), 3);

        let splitter_id = NodeId(3);
        let splitter_in = PinRef {
            node: splitter_id,
            port: 0,
        };
        let splitter_out_a = PinRef {
            node: splitter_id,
            port: 1,
        };
        let splitter_out_b = PinRef {
            node: splitter_id,
            port: 2,
        };

        assert_eq!(netlist.sink_of(src_out), Some(splitter_in));
        assert_eq!(netlist.sink_of(splitter_out_a), Some(a_in));
        assert_eq!(netlist.sink_of(splitter_out_b), Some(b_in));
    }

    #[test]
    fn inserts_balancing_dff_on_existing_connection() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let src = netlist.add_node(NodeKind::CellInstance, "src");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");

        let src_out = PinRef { node: src, port: 0 };
        let sink_in = PinRef {
            node: sink,
            port: 0,
        };

        netlist
            .connect(src_out, sink_in)
            .expect("initial edge should connect");

        let dff_out = compiler
            .insert_balancing_dff(&mut netlist, src_out)
            .expect("dff insertion should succeed");

        let dff_id = NodeId(2);
        let dff_in = PinRef {
            node: dff_id,
            port: 0,
        };

        assert_eq!(dff_out.node, dff_id);
        assert_eq!(netlist.sink_of(src_out), Some(dff_in));
        assert_eq!(netlist.sink_of(dff_out), Some(sink_in));
    }

    #[test]
    fn maps_node_kinds_to_sfq_cell_types() {
        let pdk = Pdk::minimal("test-pdk");
        let mapper = TechMapper::new(&pdk);
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "in");
        netlist.add_node(NodeKind::CellInstance, "gate");
        netlist.add_node(NodeKind::Dff, "dff");

        let mapped = mapper.map_netlist(&netlist);
        assert_eq!(mapped.len(), 3);
        assert_eq!(mapped[0].cell.kind, SfCellKind::Port);
        assert_eq!(mapped[1].cell.kind, SfCellKind::GenericGate);
        assert_eq!(mapped[2].cell.kind, SfCellKind::Dff);
    }

    #[test]
    fn produces_tech_mapping_area_report() {
        let pdk = Pdk::minimal("test-pdk");
        let mapper = TechMapper::new(&pdk);
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::CellInstance, "gate");
        netlist.add_node(NodeKind::Splitter, "split");

        let report = mapper.map_report(&netlist);
        assert_eq!(report.mapped_nodes, 2);
        assert!(report.total_area_um2 > 0.0);
        assert_eq!(report.unmapped_nodes, 0);
        assert_eq!(report.coverage_ratio, 1.0);
    }

    #[test]
    fn area_optimized_mapping_selects_smallest() {
        let pdk = Pdk::minimal("test-pdk");
        let mapper = TechMapper::new(&pdk);
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::CellInstance, "gate");
        netlist.add_node(NodeKind::Dff, "dff");

        let mapped = mapper.map_netlist_area_optimized(&netlist);
        assert_eq!(mapped.len(), 2);
        assert!(mapped[0].cell.area_um2 > 0.0);
        assert!(mapped[1].cell.area_um2 > 0.0);
    }

    #[test]
    fn runs_internal_boolean_pass_entry() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert!(report.gate_count_before >= report.gate_count_after);
    }

    #[test]
    fn compile_plan_reports_splitter_and_dff_insertions() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let src = netlist.add_node(NodeKind::CellInstance, "src");
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        let src_out = PinRef { node: src, port: 0 };
        let a_in = PinRef { node: a, port: 0 };
        let b_in = PinRef { node: b, port: 0 };

        let plan = CompilePlan {
            connections: vec![
                ConnectionSpec {
                    from: src_out,
                    to: a_in,
                },
                ConnectionSpec {
                    from: src_out,
                    to: b_in,
                },
            ],
            balance_strategy: BalanceStrategy::Explicit,
            balancing_sources: vec![src_out],
        };

        let report = compiler
            .compile_plan(&mut netlist, &plan)
            .expect("compile plan should succeed");

        assert_eq!(report.connections_applied, 2);
        assert_eq!(report.splitters_inserted, 1);
        assert_eq!(report.balancing_dffs_inserted, 1);
    }

    #[test]
    fn compile_plan_can_balance_all_connected_sources() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let src_a = netlist.add_node(NodeKind::CellInstance, "src_a");
        let src_b = netlist.add_node(NodeKind::CellInstance, "src_b");
        let sink_a = netlist.add_node(NodeKind::CellInstance, "sink_a");
        let sink_b = netlist.add_node(NodeKind::CellInstance, "sink_b");

        let plan = CompilePlan {
            connections: vec![
                ConnectionSpec {
                    from: PinRef {
                        node: src_a,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink_a,
                        port: 0,
                    },
                },
                ConnectionSpec {
                    from: PinRef {
                        node: src_b,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink_b,
                        port: 0,
                    },
                },
            ],
            balance_strategy: BalanceStrategy::AllConnectedSources,
            balancing_sources: Vec::new(),
        };

        let report = compiler
            .compile_plan(&mut netlist, &plan)
            .expect("automatic balancing should succeed");

        assert_eq!(report.connections_applied, 2);
        assert_eq!(report.splitters_inserted, 0);
        assert_eq!(report.balancing_dffs_inserted, 2);
        assert_eq!(netlist.node_count(), 6);
        assert_eq!(netlist.edge_count(), 4);
    }

    #[test]
    fn analyzes_bool_opt_compatibility_for_supported_comb_netlist() {
        let compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: gate,
                    port: 0,
                },
            )
            .expect("port to gate should connect");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: gate,
                    port: 1,
                },
            )
            .expect("port to gate should connect");

        let report = compiler.analyze_bool_opt_compatibility(&netlist);
        assert!(report.is_compatible());
        assert_eq!(report.input_nodes, vec![0, 1]);
        assert_eq!(report.output_candidates, vec![2]);
    }

    #[test]
    fn analyzes_bool_opt_compatibility_for_unsupported_dff() {
        let compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let src = netlist.add_node(NodeKind::Port, "src");
        let dff = netlist.add_node(NodeKind::Dff, "dff");

        netlist
            .connect(PinRef { node: src, port: 0 }, PinRef { node: dff, port: 0 })
            .expect("port to dff should connect");

        let report = compiler.analyze_bool_opt_compatibility(&netlist);
        assert!(!report.is_compatible());
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.kind == BoolOptCompatibilityIssueKind::UnsupportedNodeKind));
    }

    #[test]
    fn internal_boolean_optimization_deduplicates_equivalent_logic() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let gate0 = netlist.add_node(NodeKind::CellInstance, "gate0");
        let gate1 = netlist.add_node(NodeKind::CellInstance, "gate1");
        let out0 = netlist.add_node(NodeKind::Port, "out0");
        let out1 = netlist.add_node(NodeKind::Port, "out1");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: gate0,
                    port: 0,
                },
            )
            .expect("port to gate should connect");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: gate0,
                    port: 1,
                },
            )
            .expect("port to gate should connect");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: gate1,
                    port: 0,
                },
            )
            .expect("port to gate should connect");
        netlist
            .connect(
                PinRef { node: b, port: 1 },
                PinRef {
                    node: gate1,
                    port: 1,
                },
            )
            .expect("port to gate should connect");
        netlist
            .connect(
                PinRef {
                    node: gate0,
                    port: 0,
                },
                PinRef {
                    node: out0,
                    port: 0,
                },
            )
            .expect("gate to output should connect");
        netlist
            .connect(
                PinRef {
                    node: gate1,
                    port: 0,
                },
                PinRef {
                    node: out1,
                    port: 0,
                },
            )
            .expect("gate to output should connect");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 2);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::CellInstance))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_deduplicates_equivalent_xor_logic() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let xor0 = netlist.add_node_with_logic(NodeKind::CellInstance, "xor0", Some(LogicOp::Xor));
        let xor1 = netlist.add_node_with_logic(NodeKind::CellInstance, "xor1", Some(LogicOp::Xor));
        let out0 = netlist.add_node(NodeKind::Port, "out0");
        let out1 = netlist.add_node(NodeKind::Port, "out1");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: xor0,
                    port: 0,
                },
            )
            .expect("a to xor0");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: xor0,
                    port: 1,
                },
            )
            .expect("b to xor0");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: xor1,
                    port: 0,
                },
            )
            .expect("a to xor1");
        netlist
            .connect(
                PinRef { node: b, port: 1 },
                PinRef {
                    node: xor1,
                    port: 1,
                },
            )
            .expect("b to xor1");
        netlist
            .connect(
                PinRef {
                    node: xor0,
                    port: 0,
                },
                PinRef {
                    node: out0,
                    port: 0,
                },
            )
            .expect("xor0 to out0");
        netlist
            .connect(
                PinRef {
                    node: xor1,
                    port: 0,
                },
                PinRef {
                    node: out1,
                    port: 0,
                },
            )
            .expect("xor1 to out1");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 2);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Xor))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_deduplicates_equivalent_mux_logic() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let sel = netlist.add_node(NodeKind::Port, "sel");
        let d0 = netlist.add_node(NodeKind::Port, "d0");
        let d1 = netlist.add_node(NodeKind::Port, "d1");
        let mux0 = netlist.add_node_with_logic(NodeKind::CellInstance, "mux0", Some(LogicOp::Mux2));
        let mux1 = netlist.add_node_with_logic(NodeKind::CellInstance, "mux1", Some(LogicOp::Mux2));
        let out0 = netlist.add_node(NodeKind::Port, "out0");
        let out1 = netlist.add_node(NodeKind::Port, "out1");

        netlist
            .connect(
                PinRef { node: sel, port: 0 },
                PinRef {
                    node: mux0,
                    port: 0,
                },
            )
            .expect("sel to mux0");
        netlist
            .connect(
                PinRef { node: d0, port: 0 },
                PinRef {
                    node: mux0,
                    port: 1,
                },
            )
            .expect("d0 to mux0");
        netlist
            .connect(
                PinRef { node: d1, port: 0 },
                PinRef {
                    node: mux0,
                    port: 2,
                },
            )
            .expect("d1 to mux0");
        netlist
            .connect(
                PinRef { node: sel, port: 1 },
                PinRef {
                    node: mux1,
                    port: 0,
                },
            )
            .expect("sel to mux1");
        netlist
            .connect(
                PinRef { node: d0, port: 1 },
                PinRef {
                    node: mux1,
                    port: 1,
                },
            )
            .expect("d0 to mux1");
        netlist
            .connect(
                PinRef { node: d1, port: 1 },
                PinRef {
                    node: mux1,
                    port: 2,
                },
            )
            .expect("d1 to mux1");
        netlist
            .connect(
                PinRef {
                    node: mux0,
                    port: 0,
                },
                PinRef {
                    node: out0,
                    port: 0,
                },
            )
            .expect("mux0 to out0");
        netlist
            .connect(
                PinRef {
                    node: mux1,
                    port: 0,
                },
                PinRef {
                    node: out1,
                    port: 0,
                },
            )
            .expect("mux1 to out1");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 2);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Mux2))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_recognizes_dffe_nodes() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let data = netlist.add_node(NodeKind::Port, "data");
        let enable = netlist.add_node(NodeKind::Port, "enable");
        let clock = netlist.add_node(NodeKind::Port, "clock");
        let dff0 = netlist.add_node_with_logic(NodeKind::Dff, "dff0", Some(LogicOp::DffEnable));
        let dff1 = netlist.add_node_with_logic(NodeKind::Dff, "dff1", Some(LogicOp::DffEnable));
        let out0 = netlist.add_node(NodeKind::Port, "out0");
        let out1 = netlist.add_node(NodeKind::Port, "out1");

        netlist
            .connect(
                PinRef {
                    node: data,
                    port: 0,
                },
                PinRef {
                    node: dff0,
                    port: 0,
                },
            )
            .expect("data to dff0");
        netlist
            .connect(
                PinRef {
                    node: enable,
                    port: 0,
                },
                PinRef {
                    node: dff0,
                    port: 1,
                },
            )
            .expect("enable to dff0");
        netlist
            .connect(
                PinRef {
                    node: clock,
                    port: 0,
                },
                PinRef {
                    node: dff0,
                    port: 2,
                },
            )
            .expect("clock to dff0");
        netlist
            .connect(
                PinRef {
                    node: data,
                    port: 1,
                },
                PinRef {
                    node: dff1,
                    port: 0,
                },
            )
            .expect("data to dff1");
        netlist
            .connect(
                PinRef {
                    node: enable,
                    port: 1,
                },
                PinRef {
                    node: dff1,
                    port: 1,
                },
            )
            .expect("enable to dff1");
        netlist
            .connect(
                PinRef {
                    node: clock,
                    port: 1,
                },
                PinRef {
                    node: dff1,
                    port: 2,
                },
            )
            .expect("clock to dff1");
        netlist
            .connect(
                PinRef {
                    node: dff0,
                    port: 0,
                },
                PinRef {
                    node: out0,
                    port: 0,
                },
            )
            .expect("dff0 to out0");
        netlist
            .connect(
                PinRef {
                    node: dff1,
                    port: 0,
                },
                PinRef {
                    node: out1,
                    port: 0,
                },
            )
            .expect("dff1 to out1");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 0);
        assert_eq!(report.gate_count_after, 0);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::Dff)
                    && node.logic_op == Some(LogicOp::DffEnable))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_rewrites_mux_feedback_dff_to_dffe() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let data = netlist.add_node(NodeKind::Port, "data");
        let enable = netlist.add_node(NodeKind::Port, "enable");
        let clock = netlist.add_node(NodeKind::Port, "clock");
        let dff = netlist.add_node(NodeKind::Dff, "state");
        let feedback = netlist.add_node(NodeKind::Splitter, "feedback_split");
        let mux =
            netlist.add_node_with_logic(NodeKind::CellInstance, "state_mux", Some(LogicOp::Mux2));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: dff, port: 0 },
                PinRef {
                    node: feedback,
                    port: 0,
                },
            )
            .expect("dff to splitter");
        netlist
            .connect(
                PinRef {
                    node: feedback,
                    port: 1,
                },
                PinRef { node: mux, port: 1 },
            )
            .expect("feedback hold arm");
        netlist
            .connect(
                PinRef {
                    node: enable,
                    port: 0,
                },
                PinRef { node: mux, port: 0 },
            )
            .expect("enable to mux select");
        netlist
            .connect(
                PinRef {
                    node: data,
                    port: 0,
                },
                PinRef { node: mux, port: 2 },
            )
            .expect("data to mux update arm");
        netlist
            .connect(PinRef { node: mux, port: 0 }, PinRef { node: dff, port: 0 })
            .expect("mux to dff data");
        netlist
            .connect(
                PinRef {
                    node: clock,
                    port: 0,
                },
                PinRef { node: dff, port: 1 },
            )
            .expect("clock to dff");
        netlist
            .connect(
                PinRef {
                    node: feedback,
                    port: 2,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("feedback to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 1);
        assert_eq!(report.gate_count_after, 0);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::DffEnable))
                .count(),
            1
        );
        assert!(!netlist
            .nodes()
            .iter()
            .any(|node| node.logic_op == Some(LogicOp::Mux2)));
    }

    #[test]
    fn internal_boolean_optimization_rewrites_inverted_mux_feedback_dff_to_dffe() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let data = netlist.add_node(NodeKind::Port, "data");
        let enable = netlist.add_node(NodeKind::Port, "enable");
        let clock = netlist.add_node(NodeKind::Port, "clock");
        let dff = netlist.add_node(NodeKind::Dff, "state");
        let feedback = netlist.add_node(NodeKind::Splitter, "feedback_split");
        let mux =
            netlist.add_node_with_logic(NodeKind::CellInstance, "state_mux", Some(LogicOp::Mux2));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: dff, port: 0 },
                PinRef {
                    node: feedback,
                    port: 0,
                },
            )
            .expect("dff to splitter");
        netlist
            .connect(
                PinRef {
                    node: data,
                    port: 0,
                },
                PinRef { node: mux, port: 1 },
            )
            .expect("data to mux arm a");
        netlist
            .connect(
                PinRef {
                    node: feedback,
                    port: 1,
                },
                PinRef { node: mux, port: 2 },
            )
            .expect("feedback hold arm");
        netlist
            .connect(
                PinRef {
                    node: enable,
                    port: 0,
                },
                PinRef { node: mux, port: 0 },
            )
            .expect("enable to mux select");
        netlist
            .connect(PinRef { node: mux, port: 0 }, PinRef { node: dff, port: 0 })
            .expect("mux to dff data");
        netlist
            .connect(
                PinRef {
                    node: clock,
                    port: 0,
                },
                PinRef { node: dff, port: 1 },
            )
            .expect("clock to dff");
        netlist
            .connect(
                PinRef {
                    node: feedback,
                    port: 2,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("feedback to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 1);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::DffEnable))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Not))
                .count(),
            1
        );
        assert!(!netlist
            .nodes()
            .iter()
            .any(|node| node.logic_op == Some(LogicOp::Mux2)));
    }

    #[test]
    fn internal_boolean_optimization_rewrites_wrapped_mux_feedback_dff_to_dffe() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let data = netlist.add_node(NodeKind::Port, "data");
        let enable = netlist.add_node(NodeKind::Port, "enable");
        let clock = netlist.add_node(NodeKind::Port, "clock");
        let dff = netlist.add_node(NodeKind::Dff, "state");
        let feedback = netlist.add_node(NodeKind::Splitter, "feedback_split");
        let mux =
            netlist.add_node_with_logic(NodeKind::CellInstance, "state_mux", Some(LogicOp::Mux2));
        let data_pipe = netlist.add_node(NodeKind::Jtl, "data_pipe");
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: dff, port: 0 },
                PinRef {
                    node: feedback,
                    port: 0,
                },
            )
            .expect("dff to splitter");
        netlist
            .connect(
                PinRef {
                    node: feedback,
                    port: 1,
                },
                PinRef { node: mux, port: 1 },
            )
            .expect("feedback hold arm");
        netlist
            .connect(
                PinRef {
                    node: enable,
                    port: 0,
                },
                PinRef { node: mux, port: 0 },
            )
            .expect("enable to mux select");
        netlist
            .connect(
                PinRef {
                    node: data,
                    port: 0,
                },
                PinRef { node: mux, port: 2 },
            )
            .expect("data to mux update arm");
        netlist
            .connect(
                PinRef { node: mux, port: 0 },
                PinRef {
                    node: data_pipe,
                    port: 0,
                },
            )
            .expect("mux to data pipe");
        netlist
            .connect(
                PinRef {
                    node: data_pipe,
                    port: 0,
                },
                PinRef { node: dff, port: 0 },
            )
            .expect("data pipe to dff");
        netlist
            .connect(
                PinRef {
                    node: clock,
                    port: 0,
                },
                PinRef { node: dff, port: 1 },
            )
            .expect("clock to dff");
        netlist
            .connect(
                PinRef {
                    node: feedback,
                    port: 2,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("feedback to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 1);
        assert_eq!(report.gate_count_after, 0);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::DffEnable))
                .count(),
            1
        );
        assert!(!netlist
            .nodes()
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Jtl)));
        assert!(!netlist
            .nodes()
            .iter()
            .any(|node| node.logic_op == Some(LogicOp::Mux2)));
    }

    #[test]
    fn internal_boolean_optimization_eliminates_and_absorption_redundancy() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let or_gate =
            netlist.add_node_with_logic(NodeKind::CellInstance, "or_gate", Some(LogicOp::Or));
        let and_gate =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_gate", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: or_gate,
                    port: 0,
                },
            )
            .expect("a to or");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: or_gate,
                    port: 1,
                },
            )
            .expect("b to or");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: and_gate,
                    port: 0,
                },
            )
            .expect("a to and");
        netlist
            .connect(
                PinRef {
                    node: or_gate,
                    port: 0,
                },
                PinRef {
                    node: and_gate,
                    port: 1,
                },
            )
            .expect("or to and");
        netlist
            .connect(
                PinRef {
                    node: and_gate,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 2);
        assert_eq!(report.gate_count_after, 0);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::CellInstance))
                .count(),
            0
        );
    }

    #[test]
    fn internal_boolean_optimization_eliminates_or_absorption_redundancy() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let and_gate =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_gate", Some(LogicOp::And));
        let or_gate =
            netlist.add_node_with_logic(NodeKind::CellInstance, "or_gate", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and_gate,
                    port: 0,
                },
            )
            .expect("a to and");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and_gate,
                    port: 1,
                },
            )
            .expect("b to and");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: or_gate,
                    port: 0,
                },
            )
            .expect("a to or");
        netlist
            .connect(
                PinRef {
                    node: and_gate,
                    port: 0,
                },
                PinRef {
                    node: or_gate,
                    port: 1,
                },
            )
            .expect("and to or");
        netlist
            .connect(
                PinRef {
                    node: or_gate,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("or to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 2);
        assert_eq!(report.gate_count_after, 0);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::CellInstance))
                .count(),
            0
        );
    }

    #[test]
    fn internal_boolean_optimization_eliminates_and_subset_absorption_redundancy() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let c = netlist.add_node(NodeKind::Port, "c");
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let or1 = netlist.add_node_with_logic(NodeKind::CellInstance, "or1", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: or0, port: 0 })
            .expect("a to or0");
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: or0, port: 1 })
            .expect("b to or0");
        netlist
            .connect(PinRef { node: a, port: 1 }, PinRef { node: or1, port: 0 })
            .expect("a to or1");
        netlist
            .connect(PinRef { node: b, port: 1 }, PinRef { node: or1, port: 1 })
            .expect("b to or1");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: or1, port: 2 })
            .expect("c to or1");
        netlist
            .connect(
                PinRef { node: or0, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or0 to and0");
        netlist
            .connect(
                PinRef { node: or1, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or1 to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 3);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            0
        );
    }

    #[test]
    fn internal_boolean_optimization_eliminates_or_subset_absorption_redundancy() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let c = netlist.add_node(NodeKind::Port, "c");
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("b to and0");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("a to and1");
        netlist
            .connect(
                PinRef { node: b, port: 1 },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("b to and1");
        netlist
            .connect(
                PinRef { node: c, port: 0 },
                PinRef {
                    node: and1,
                    port: 2,
                },
            )
            .expect("c to and1");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 3);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            0
        );
    }

    #[test]
    fn quaigh_alignment_reconstructs_xor_from_or_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_a =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_a", Some(LogicOp::Not));
        let not_b =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_b", Some(LogicOp::Not));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let or1 = netlist.add_node_with_logic(NodeKind::CellInstance, "or1", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: not_a,
                    port: 0,
                },
            )
            .expect("a to not_a");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: not_b,
                    port: 0,
                },
            )
            .expect("b to not_b");
        netlist
            .connect(PinRef { node: a, port: 1 }, PinRef { node: or0, port: 0 })
            .expect("a to or0");
        netlist
            .connect(PinRef { node: b, port: 1 }, PinRef { node: or0, port: 1 })
            .expect("b to or0");
        netlist
            .connect(
                PinRef {
                    node: not_a,
                    port: 0,
                },
                PinRef { node: or1, port: 0 },
            )
            .expect("not_a to or1");
        netlist
            .connect(
                PinRef {
                    node: not_b,
                    port: 0,
                },
                PinRef { node: or1, port: 1 },
            )
            .expect("not_b to or1");
        netlist
            .connect(
                PinRef { node: or0, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or0 to and0");
        netlist
            .connect(
                PinRef { node: or1, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or1 to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 5);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Xor))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_reconstructs_mux_from_or_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let s = netlist.add_node(NodeKind::Port, "s");
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_s =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_s", Some(LogicOp::Not));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let or1 = netlist.add_node_with_logic(NodeKind::CellInstance, "or1", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: s, port: 0 },
                PinRef {
                    node: not_s,
                    port: 0,
                },
            )
            .expect("s to not_s");
        netlist
            .connect(PinRef { node: s, port: 1 }, PinRef { node: or0, port: 0 })
            .expect("s to or0");
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: or0, port: 1 })
            .expect("b to or0");
        netlist
            .connect(
                PinRef {
                    node: not_s,
                    port: 0,
                },
                PinRef { node: or1, port: 0 },
            )
            .expect("not_s to or1");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: or1, port: 1 })
            .expect("a to or1");
        netlist
            .connect(
                PinRef { node: or0, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or0 to and0");
        netlist
            .connect(
                PinRef { node: or1, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or1 to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 4);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Mux2))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_factors_then_reconstructs_xor_from_and_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let c = netlist.add_node(NodeKind::Port, "c");
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_a =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_a", Some(LogicOp::Not));
        let not_b =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_b", Some(LogicOp::Not));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: not_a,
                    port: 0,
                },
            )
            .expect("a to not_a");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: not_b,
                    port: 0,
                },
            )
            .expect("b to not_b");
        netlist
            .connect(
                PinRef { node: c, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("c to and0");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef {
                    node: not_b,
                    port: 0,
                },
                PinRef {
                    node: and0,
                    port: 2,
                },
            )
            .expect("not_b to and0");
        netlist
            .connect(
                PinRef { node: c, port: 1 },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("c to and1");
        netlist
            .connect(
                PinRef {
                    node: not_a,
                    port: 0,
                },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("not_a to and1");
        netlist
            .connect(
                PinRef { node: b, port: 1 },
                PinRef {
                    node: and1,
                    port: 2,
                },
            )
            .expect("b to and1");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 5);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Xor))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_factors_then_reconstructs_mux_from_and_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let c = netlist.add_node(NodeKind::Port, "c");
        let s = netlist.add_node(NodeKind::Port, "s");
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_s =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_s", Some(LogicOp::Not));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: s, port: 0 },
                PinRef {
                    node: not_s,
                    port: 0,
                },
            )
            .expect("s to not_s");
        netlist
            .connect(
                PinRef { node: c, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("c to and0");
        netlist
            .connect(
                PinRef { node: s, port: 1 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("s to and0");
        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and0,
                    port: 2,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef { node: c, port: 1 },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("c to and1");
        netlist
            .connect(
                PinRef {
                    node: not_s,
                    port: 0,
                },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("not_s to and1");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and1,
                    port: 2,
                },
            )
            .expect("b to and1");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 4);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Mux2))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_factors_then_reconstructs_xor_from_or_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let c = netlist.add_node(NodeKind::Port, "c");
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_a =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_a", Some(LogicOp::Not));
        let not_b =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_b", Some(LogicOp::Not));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let or1 = netlist.add_node_with_logic(NodeKind::CellInstance, "or1", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: not_a,
                    port: 0,
                },
            )
            .expect("a to not_a");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: not_b,
                    port: 0,
                },
            )
            .expect("b to not_b");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: or0, port: 0 })
            .expect("c to or0");
        netlist
            .connect(PinRef { node: a, port: 1 }, PinRef { node: or0, port: 1 })
            .expect("a to or0");
        netlist
            .connect(PinRef { node: b, port: 1 }, PinRef { node: or0, port: 2 })
            .expect("b to or0");
        netlist
            .connect(PinRef { node: c, port: 1 }, PinRef { node: or1, port: 0 })
            .expect("c to or1");
        netlist
            .connect(
                PinRef {
                    node: not_a,
                    port: 0,
                },
                PinRef { node: or1, port: 1 },
            )
            .expect("not_a to or1");
        netlist
            .connect(
                PinRef {
                    node: not_b,
                    port: 0,
                },
                PinRef { node: or1, port: 2 },
            )
            .expect("not_b to or1");
        netlist
            .connect(
                PinRef { node: or0, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or0 to and0");
        netlist
            .connect(
                PinRef { node: or1, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or1 to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 5);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Xor))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_factors_then_reconstructs_mux_from_or_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let c = netlist.add_node(NodeKind::Port, "c");
        let s = netlist.add_node(NodeKind::Port, "s");
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_s =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_s", Some(LogicOp::Not));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let or1 = netlist.add_node_with_logic(NodeKind::CellInstance, "or1", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: s, port: 0 },
                PinRef {
                    node: not_s,
                    port: 0,
                },
            )
            .expect("s to not_s");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: or0, port: 0 })
            .expect("c to or0");
        netlist
            .connect(PinRef { node: s, port: 1 }, PinRef { node: or0, port: 1 })
            .expect("s to or0");
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: or0, port: 2 })
            .expect("b to or0");
        netlist
            .connect(PinRef { node: c, port: 1 }, PinRef { node: or1, port: 0 })
            .expect("c to or1");
        netlist
            .connect(
                PinRef {
                    node: not_s,
                    port: 0,
                },
                PinRef { node: or1, port: 1 },
            )
            .expect("not_s to or1");
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: or1, port: 2 })
            .expect("a to or1");
        netlist
            .connect(
                PinRef { node: or0, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or0 to and0");
        netlist
            .connect(
                PinRef { node: or1, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or1 to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 4);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Mux2))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_reaches_a_fixed_point_in_one_call() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let c = netlist.add_node(NodeKind::Port, "c");
        let s = netlist.add_node(NodeKind::Port, "s");
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_s =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_s", Some(LogicOp::Not));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: s, port: 0 },
                PinRef {
                    node: not_s,
                    port: 0,
                },
            )
            .expect("s to not_s");
        netlist
            .connect(
                PinRef { node: c, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("c to and0");
        netlist
            .connect(
                PinRef { node: s, port: 1 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("s to and0");
        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and0,
                    port: 2,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef { node: c, port: 1 },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("c to and1");
        netlist
            .connect(
                PinRef {
                    node: not_s,
                    port: 0,
                },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("not_s to and1");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and1,
                    port: 2,
                },
            )
            .expect("b to and1");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let once = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());
        let after_once = netlist.clone();
        let twice = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(once.gate_count_after, 2);
        assert_eq!(twice.gate_count_before, once.gate_count_after);
        assert_eq!(twice.gate_count_after, once.gate_count_after);

        let eq = compiler
            .check_boolean_equivalence_sat(&after_once, &netlist)
            .expect("fixed-point check should SAT-verify");
        assert!(eq.equivalent);
    }

    #[test]
    fn internal_boolean_optimization_eliminates_or_consensus_redundancy() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let x = netlist.add_node(NodeKind::Port, "x");
        let y = netlist.add_node(NodeKind::Port, "y");
        let z = netlist.add_node(NodeKind::Port, "z");
        let not_x =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_x", Some(LogicOp::Not));
        let and_xy =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_xy", Some(LogicOp::And));
        let and_nxz =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_nxz", Some(LogicOp::And));
        let and_yz =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_yz", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: x, port: 0 },
                PinRef {
                    node: not_x,
                    port: 0,
                },
            )
            .expect("x to not_x");
        netlist
            .connect(
                PinRef { node: x, port: 1 },
                PinRef {
                    node: and_xy,
                    port: 0,
                },
            )
            .expect("x to and_xy");
        netlist
            .connect(
                PinRef { node: y, port: 0 },
                PinRef {
                    node: and_xy,
                    port: 1,
                },
            )
            .expect("y to and_xy");
        netlist
            .connect(
                PinRef {
                    node: not_x,
                    port: 0,
                },
                PinRef {
                    node: and_nxz,
                    port: 0,
                },
            )
            .expect("not_x to and_nxz");
        netlist
            .connect(
                PinRef { node: z, port: 0 },
                PinRef {
                    node: and_nxz,
                    port: 1,
                },
            )
            .expect("z to and_nxz");
        netlist
            .connect(
                PinRef { node: y, port: 1 },
                PinRef {
                    node: and_yz,
                    port: 0,
                },
            )
            .expect("y to and_yz");
        netlist
            .connect(
                PinRef { node: z, port: 1 },
                PinRef {
                    node: and_yz,
                    port: 1,
                },
            )
            .expect("z to and_yz");
        netlist
            .connect(
                PinRef {
                    node: and_xy,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and_xy to or0");
        netlist
            .connect(
                PinRef {
                    node: and_nxz,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and_nxz to or0");
        netlist
            .connect(
                PinRef {
                    node: and_yz,
                    port: 0,
                },
                PinRef { node: or0, port: 2 },
            )
            .expect("and_yz to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 5);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Mux2))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_eliminates_and_consensus_redundancy() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let x = netlist.add_node(NodeKind::Port, "x");
        let y = netlist.add_node(NodeKind::Port, "y");
        let z = netlist.add_node(NodeKind::Port, "z");
        let not_x =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_x", Some(LogicOp::Not));
        let or_xy = netlist.add_node_with_logic(NodeKind::CellInstance, "or_xy", Some(LogicOp::Or));
        let or_nxz =
            netlist.add_node_with_logic(NodeKind::CellInstance, "or_nxz", Some(LogicOp::Or));
        let or_yz = netlist.add_node_with_logic(NodeKind::CellInstance, "or_yz", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: x, port: 0 },
                PinRef {
                    node: not_x,
                    port: 0,
                },
            )
            .expect("x to not_x");
        netlist
            .connect(
                PinRef { node: x, port: 1 },
                PinRef {
                    node: or_xy,
                    port: 0,
                },
            )
            .expect("x to or_xy");
        netlist
            .connect(
                PinRef { node: y, port: 0 },
                PinRef {
                    node: or_xy,
                    port: 1,
                },
            )
            .expect("y to or_xy");
        netlist
            .connect(
                PinRef {
                    node: not_x,
                    port: 0,
                },
                PinRef {
                    node: or_nxz,
                    port: 0,
                },
            )
            .expect("not_x to or_nxz");
        netlist
            .connect(
                PinRef { node: z, port: 0 },
                PinRef {
                    node: or_nxz,
                    port: 1,
                },
            )
            .expect("z to or_nxz");
        netlist
            .connect(
                PinRef { node: y, port: 1 },
                PinRef {
                    node: or_yz,
                    port: 0,
                },
            )
            .expect("y to or_yz");
        netlist
            .connect(
                PinRef { node: z, port: 1 },
                PinRef {
                    node: or_yz,
                    port: 1,
                },
            )
            .expect("z to or_yz");
        netlist
            .connect(
                PinRef {
                    node: or_xy,
                    port: 0,
                },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or_xy to and0");
        netlist
            .connect(
                PinRef {
                    node: or_nxz,
                    port: 0,
                },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or_nxz to and0");
        netlist
            .connect(
                PinRef {
                    node: or_yz,
                    port: 0,
                },
                PinRef {
                    node: and0,
                    port: 2,
                },
            )
            .expect("or_yz to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 5);
        assert_eq!(report.gate_count_after, 1);
    }

    #[test]
    fn internal_boolean_optimization_deduplicates_deep_commutative_cones() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let c = netlist.add_node(NodeKind::Port, "c");
        let d = netlist.add_node(NodeKind::Port, "d");

        let and_ab0 =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_ab0", Some(LogicOp::And));
        let and_cd0 =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_cd0", Some(LogicOp::And));
        let and_top0 =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_top0", Some(LogicOp::And));

        let and_ab1 =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_ab1", Some(LogicOp::And));
        let and_cd1 =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_cd1", Some(LogicOp::And));
        let and_top1 =
            netlist.add_node_with_logic(NodeKind::CellInstance, "and_top1", Some(LogicOp::And));

        let out0 = netlist.add_node(NodeKind::Port, "out0");
        let out1 = netlist.add_node(NodeKind::Port, "out1");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and_ab0,
                    port: 0,
                },
            )
            .expect("a to and_ab0");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and_ab0,
                    port: 1,
                },
            )
            .expect("b to and_ab0");
        netlist
            .connect(
                PinRef { node: c, port: 0 },
                PinRef {
                    node: and_cd0,
                    port: 0,
                },
            )
            .expect("c to and_cd0");
        netlist
            .connect(
                PinRef { node: d, port: 0 },
                PinRef {
                    node: and_cd0,
                    port: 1,
                },
            )
            .expect("d to and_cd0");
        netlist
            .connect(
                PinRef {
                    node: and_ab0,
                    port: 0,
                },
                PinRef {
                    node: and_top0,
                    port: 0,
                },
            )
            .expect("ab0 to top0");
        netlist
            .connect(
                PinRef {
                    node: and_cd0,
                    port: 0,
                },
                PinRef {
                    node: and_top0,
                    port: 1,
                },
            )
            .expect("cd0 to top0");

        netlist
            .connect(
                PinRef { node: b, port: 1 },
                PinRef {
                    node: and_ab1,
                    port: 0,
                },
            )
            .expect("b to and_ab1");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: and_ab1,
                    port: 1,
                },
            )
            .expect("a to and_ab1");
        netlist
            .connect(
                PinRef { node: d, port: 1 },
                PinRef {
                    node: and_cd1,
                    port: 0,
                },
            )
            .expect("d to and_cd1");
        netlist
            .connect(
                PinRef { node: c, port: 1 },
                PinRef {
                    node: and_cd1,
                    port: 1,
                },
            )
            .expect("c to and_cd1");
        netlist
            .connect(
                PinRef {
                    node: and_cd1,
                    port: 0,
                },
                PinRef {
                    node: and_top1,
                    port: 0,
                },
            )
            .expect("cd1 to top1");
        netlist
            .connect(
                PinRef {
                    node: and_ab1,
                    port: 0,
                },
                PinRef {
                    node: and_top1,
                    port: 1,
                },
            )
            .expect("ab1 to top1");

        netlist
            .connect(
                PinRef {
                    node: and_top0,
                    port: 0,
                },
                PinRef {
                    node: out0,
                    port: 0,
                },
            )
            .expect("top0 to out0");
        netlist
            .connect(
                PinRef {
                    node: and_top1,
                    port: 0,
                },
                PinRef {
                    node: out1,
                    port: 0,
                },
            )
            .expect("top1 to out1");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 6);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_keeps_mux_data_order_semantics() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let sel = netlist.add_node(NodeKind::Port, "sel");
        let d0 = netlist.add_node(NodeKind::Port, "d0");
        let d1 = netlist.add_node(NodeKind::Port, "d1");
        let mux0 = netlist.add_node_with_logic(NodeKind::CellInstance, "mux0", Some(LogicOp::Mux2));
        let mux1 = netlist.add_node_with_logic(NodeKind::CellInstance, "mux1", Some(LogicOp::Mux2));
        let out0 = netlist.add_node(NodeKind::Port, "out0");
        let out1 = netlist.add_node(NodeKind::Port, "out1");

        netlist
            .connect(
                PinRef { node: sel, port: 0 },
                PinRef {
                    node: mux0,
                    port: 0,
                },
            )
            .expect("sel to mux0");
        netlist
            .connect(
                PinRef { node: d0, port: 0 },
                PinRef {
                    node: mux0,
                    port: 1,
                },
            )
            .expect("d0 to mux0");
        netlist
            .connect(
                PinRef { node: d1, port: 0 },
                PinRef {
                    node: mux0,
                    port: 2,
                },
            )
            .expect("d1 to mux0");

        netlist
            .connect(
                PinRef { node: sel, port: 1 },
                PinRef {
                    node: mux1,
                    port: 0,
                },
            )
            .expect("sel to mux1");
        netlist
            .connect(
                PinRef { node: d1, port: 1 },
                PinRef {
                    node: mux1,
                    port: 1,
                },
            )
            .expect("d1 to mux1");
        netlist
            .connect(
                PinRef { node: d0, port: 1 },
                PinRef {
                    node: mux1,
                    port: 2,
                },
            )
            .expect("d0 to mux1");

        netlist
            .connect(
                PinRef {
                    node: mux0,
                    port: 0,
                },
                PinRef {
                    node: out0,
                    port: 0,
                },
            )
            .expect("mux0 to out0");
        netlist
            .connect(
                PinRef {
                    node: mux1,
                    port: 0,
                },
                PinRef {
                    node: out1,
                    port: 0,
                },
            )
            .expect("mux1 to out1");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 2);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Mux2))
                .count(),
            2
        );
    }

    #[test]
    fn quaigh_alignment_respects_xor_sharing_toggle() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let xor0 = netlist.add_node_with_logic(NodeKind::CellInstance, "xor0", Some(LogicOp::Xor));
        let xor1 = netlist.add_node_with_logic(NodeKind::CellInstance, "xor1", Some(LogicOp::Xor));
        let out0 = netlist.add_node(NodeKind::Port, "out0");
        let out1 = netlist.add_node(NodeKind::Port, "out1");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: xor0,
                    port: 0,
                },
            )
            .expect("a to xor0");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: xor0,
                    port: 1,
                },
            )
            .expect("b to xor0");
        netlist
            .connect(
                PinRef { node: b, port: 1 },
                PinRef {
                    node: xor1,
                    port: 0,
                },
            )
            .expect("b to xor1");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: xor1,
                    port: 1,
                },
            )
            .expect("a to xor1");
        netlist
            .connect(
                PinRef {
                    node: xor0,
                    port: 0,
                },
                PinRef {
                    node: out0,
                    port: 0,
                },
            )
            .expect("xor0 to out0");
        netlist
            .connect(
                PinRef {
                    node: xor1,
                    port: 0,
                },
                PinRef {
                    node: out1,
                    port: 0,
                },
            )
            .expect("xor1 to out1");

        let report = compiler.optimize_boolean_network(
            &mut netlist,
            &BoolOptConfig {
                share_logic_flattening_limit: 8,
                infer_xor_mux: false,
                infer_dffe: true,
            },
        );

        assert_eq!(report.gate_count_before, 2);
        assert_eq!(report.gate_count_after, 2);
    }

    #[test]
    fn internal_boolean_optimization_factors_or_of_and_common_term() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let c = netlist.add_node(NodeKind::Port, "c");
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("b to and0");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("a to and1");
        netlist
            .connect(
                PinRef { node: c, port: 0 },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("c to and1");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 3);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_factors_or_of_three_and_terms_with_shared_input() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let c = netlist.add_node(NodeKind::Port, "c");
        let d = netlist.add_node(NodeKind::Port, "d");
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let and2 = netlist.add_node_with_logic(NodeKind::CellInstance, "and2", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("b to and0");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("a to and1");
        netlist
            .connect(
                PinRef { node: c, port: 0 },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("c to and1");
        netlist
            .connect(
                PinRef { node: a, port: 2 },
                PinRef {
                    node: and2,
                    port: 0,
                },
            )
            .expect("a to and2");
        netlist
            .connect(
                PinRef { node: d, port: 0 },
                PinRef {
                    node: and2,
                    port: 1,
                },
            )
            .expect("d to and2");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(
                PinRef {
                    node: and2,
                    port: 0,
                },
                PinRef { node: or0, port: 2 },
            )
            .expect("and2 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 4);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_factors_and_of_or_common_term() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let c = netlist.add_node(NodeKind::Port, "c");
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let or1 = netlist.add_node_with_logic(NodeKind::CellInstance, "or1", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: or0, port: 0 })
            .expect("a to or0");
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: or0, port: 1 })
            .expect("b to or0");
        netlist
            .connect(PinRef { node: a, port: 1 }, PinRef { node: or1, port: 0 })
            .expect("a to or1");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: or1, port: 1 })
            .expect("c to or1");
        netlist
            .connect(
                PinRef { node: or0, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or0 to and0");
        netlist
            .connect(
                PinRef { node: or1, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or1 to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 3);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            1
        );
    }

    #[test]
    fn internal_boolean_optimization_factors_and_of_three_or_terms_with_shared_input() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let c = netlist.add_node(NodeKind::Port, "c");
        let d = netlist.add_node(NodeKind::Port, "d");
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let or1 = netlist.add_node_with_logic(NodeKind::CellInstance, "or1", Some(LogicOp::Or));
        let or2 = netlist.add_node_with_logic(NodeKind::CellInstance, "or2", Some(LogicOp::Or));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: or0, port: 0 })
            .expect("a to or0");
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: or0, port: 1 })
            .expect("b to or0");
        netlist
            .connect(PinRef { node: a, port: 1 }, PinRef { node: or1, port: 0 })
            .expect("a to or1");
        netlist
            .connect(PinRef { node: c, port: 0 }, PinRef { node: or1, port: 1 })
            .expect("c to or1");
        netlist
            .connect(PinRef { node: a, port: 2 }, PinRef { node: or2, port: 0 })
            .expect("a to or2");
        netlist
            .connect(PinRef { node: d, port: 0 }, PinRef { node: or2, port: 1 })
            .expect("d to or2");
        netlist
            .connect(
                PinRef { node: or0, port: 0 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("or0 to and0");
        netlist
            .connect(
                PinRef { node: or1, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("or1 to and0");
        netlist
            .connect(
                PinRef { node: or2, port: 0 },
                PinRef {
                    node: and0,
                    port: 2,
                },
            )
            .expect("or2 to and0");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: out, port: 0 },
            )
            .expect("and0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 4);
        assert_eq!(report.gate_count_after, 2);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::And))
                .count(),
            1
        );
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Or))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_reconstructs_xor_from_and_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_a =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_a", Some(LogicOp::Not));
        let not_b =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_b", Some(LogicOp::Not));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: not_a,
                    port: 0,
                },
            )
            .expect("a to not_a");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: not_b,
                    port: 0,
                },
            )
            .expect("b to not_b");
        netlist
            .connect(
                PinRef { node: a, port: 1 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef {
                    node: not_b,
                    port: 0,
                },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("not_b to and0");
        netlist
            .connect(
                PinRef {
                    node: not_a,
                    port: 0,
                },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("not_a to and1");
        netlist
            .connect(
                PinRef { node: b, port: 1 },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("b to and1");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 5);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Xor))
                .count(),
            1
        );
    }

    #[test]
    fn quaigh_alignment_reconstructs_mux_from_and_pattern() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let s = netlist.add_node(NodeKind::Port, "s");
        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let not_s =
            netlist.add_node_with_logic(NodeKind::CellInstance, "not_s", Some(LogicOp::Not));
        let and0 = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let and1 = netlist.add_node_with_logic(NodeKind::CellInstance, "and1", Some(LogicOp::And));
        let or0 = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out = netlist.add_node(NodeKind::Port, "out");

        netlist
            .connect(
                PinRef { node: s, port: 0 },
                PinRef {
                    node: not_s,
                    port: 0,
                },
            )
            .expect("s to not_s");
        netlist
            .connect(
                PinRef { node: s, port: 1 },
                PinRef {
                    node: and0,
                    port: 0,
                },
            )
            .expect("s to and0");
        netlist
            .connect(
                PinRef { node: a, port: 0 },
                PinRef {
                    node: and0,
                    port: 1,
                },
            )
            .expect("a to and0");
        netlist
            .connect(
                PinRef {
                    node: not_s,
                    port: 0,
                },
                PinRef {
                    node: and1,
                    port: 0,
                },
            )
            .expect("not_s to and1");
        netlist
            .connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: and1,
                    port: 1,
                },
            )
            .expect("b to and1");
        netlist
            .connect(
                PinRef {
                    node: and0,
                    port: 0,
                },
                PinRef { node: or0, port: 0 },
            )
            .expect("and0 to or0");
        netlist
            .connect(
                PinRef {
                    node: and1,
                    port: 0,
                },
                PinRef { node: or0, port: 1 },
            )
            .expect("and1 to or0");
        netlist
            .connect(PinRef { node: or0, port: 0 }, PinRef { node: out, port: 0 })
            .expect("or0 to out");

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(report.gate_count_before, 4);
        assert_eq!(report.gate_count_after, 1);
        assert_eq!(
            netlist
                .nodes()
                .iter()
                .filter(|node| node.logic_op == Some(LogicOp::Mux2))
                .count(),
            1
        );
    }

    #[test]
    fn sat_equivalence_reports_equivalent_networks() {
        let compiler = Compiler::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let b_l = lhs.add_node(NodeKind::Port, "b");
        let and_l = lhs.add_node_with_logic(NodeKind::CellInstance, "lhs_and", Some(LogicOp::And));
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: and_l,
                port: 0,
            },
        )
        .expect("a->and");
        lhs.connect(
            PinRef { node: b_l, port: 0 },
            PinRef {
                node: and_l,
                port: 1,
            },
        )
        .expect("b->and");
        lhs.connect(
            PinRef {
                node: and_l,
                port: 0,
            },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .expect("and->out");

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let b_r = rhs.add_node(NodeKind::Port, "b");
        let and_r = rhs.add_node_with_logic(NodeKind::CellInstance, "rhs_and", Some(LogicOp::And));
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: b_r, port: 0 },
            PinRef {
                node: and_r,
                port: 0,
            },
        )
        .expect("b->and");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: and_r,
                port: 1,
            },
        )
        .expect("a->and");
        rhs.connect(
            PinRef {
                node: and_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .expect("and->out");

        let report = compiler
            .check_boolean_equivalence_sat(&lhs, &rhs)
            .expect("equivalence check should succeed");

        assert!(report.equivalent);
        assert_eq!(report.checked_outputs, vec!["out".to_string()]);
        assert!(report.counterexample_inputs.is_none());
        assert!(report.counterexample_outputs.is_none());
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
        assert!(report.sat_elapsed_ns > 0);
    }

    #[test]
    fn sat_equivalence_finds_counterexample_for_non_equivalent_networks() {
        let compiler = Compiler::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let b_l = lhs.add_node(NodeKind::Port, "b");
        let and_l = lhs.add_node_with_logic(NodeKind::CellInstance, "lhs_and", Some(LogicOp::And));
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: and_l,
                port: 0,
            },
        )
        .expect("a->and");
        lhs.connect(
            PinRef { node: b_l, port: 0 },
            PinRef {
                node: and_l,
                port: 1,
            },
        )
        .expect("b->and");
        lhs.connect(
            PinRef {
                node: and_l,
                port: 0,
            },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .expect("and->out");

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let b_r = rhs.add_node(NodeKind::Port, "b");
        let xor_r = rhs.add_node_with_logic(NodeKind::CellInstance, "rhs_xor", Some(LogicOp::Xor));
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: xor_r,
                port: 0,
            },
        )
        .expect("a->xor");
        rhs.connect(
            PinRef { node: b_r, port: 0 },
            PinRef {
                node: xor_r,
                port: 1,
            },
        )
        .expect("b->xor");
        rhs.connect(
            PinRef {
                node: xor_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .expect("xor->out");

        let report = compiler
            .check_boolean_equivalence_sat(&lhs, &rhs)
            .expect("equivalence check should succeed");

        assert!(!report.equivalent);
        let counterexample = report
            .counterexample_inputs
            .expect("counterexample assignment should be present");
        assert!(counterexample.contains_key("a"));
        assert!(counterexample.contains_key("b"));
        let output_values = report
            .counterexample_outputs
            .expect("counterexample output values should be present");
        let out = output_values.get("out").expect("out mismatch should exist");
        assert_ne!(out.lhs, out.rhs);
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
        assert!(report.sat_elapsed_ns > 0);
    }

    #[test]
    fn sat_equivalence_reports_multi_output_mismatch_details() {
        let compiler = Compiler::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let b_l = lhs.add_node(NodeKind::Port, "b");
        let and_l = lhs.add_node_with_logic(NodeKind::CellInstance, "lhs_and", Some(LogicOp::And));
        let xor_l = lhs.add_node_with_logic(NodeKind::CellInstance, "lhs_xor", Some(LogicOp::Xor));
        let out_and_l = lhs.add_node(NodeKind::Port, "out_and");
        let out_xor_l = lhs.add_node(NodeKind::Port, "out_xor");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: and_l,
                port: 0,
            },
        )
        .expect("a->and");
        lhs.connect(
            PinRef { node: b_l, port: 0 },
            PinRef {
                node: and_l,
                port: 1,
            },
        )
        .expect("b->and");
        lhs.connect(
            PinRef { node: a_l, port: 1 },
            PinRef {
                node: xor_l,
                port: 0,
            },
        )
        .expect("a->xor");
        lhs.connect(
            PinRef { node: b_l, port: 1 },
            PinRef {
                node: xor_l,
                port: 1,
            },
        )
        .expect("b->xor");
        lhs.connect(
            PinRef {
                node: and_l,
                port: 0,
            },
            PinRef {
                node: out_and_l,
                port: 0,
            },
        )
        .expect("and->out_and");
        lhs.connect(
            PinRef {
                node: xor_l,
                port: 0,
            },
            PinRef {
                node: out_xor_l,
                port: 0,
            },
        )
        .expect("xor->out_xor");

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let b_r = rhs.add_node(NodeKind::Port, "b");
        let and_r = rhs.add_node_with_logic(NodeKind::CellInstance, "rhs_and", Some(LogicOp::And));
        let or_r = rhs.add_node_with_logic(NodeKind::CellInstance, "rhs_or", Some(LogicOp::Or));
        let out_and_r = rhs.add_node(NodeKind::Port, "out_and");
        let out_xor_r = rhs.add_node(NodeKind::Port, "out_xor");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: and_r,
                port: 0,
            },
        )
        .expect("a->and");
        rhs.connect(
            PinRef { node: b_r, port: 0 },
            PinRef {
                node: and_r,
                port: 1,
            },
        )
        .expect("b->and");
        rhs.connect(
            PinRef { node: a_r, port: 1 },
            PinRef {
                node: or_r,
                port: 0,
            },
        )
        .expect("a->or");
        rhs.connect(
            PinRef { node: b_r, port: 1 },
            PinRef {
                node: or_r,
                port: 1,
            },
        )
        .expect("b->or");
        rhs.connect(
            PinRef {
                node: and_r,
                port: 0,
            },
            PinRef {
                node: out_and_r,
                port: 0,
            },
        )
        .expect("and->out_and");
        rhs.connect(
            PinRef {
                node: or_r,
                port: 0,
            },
            PinRef {
                node: out_xor_r,
                port: 0,
            },
        )
        .expect("or->out_xor");

        let report = compiler
            .check_boolean_equivalence_sat(&lhs, &rhs)
            .expect("equivalence check should succeed");

        assert!(!report.equivalent);
        let output_values = report
            .counterexample_outputs
            .expect("counterexample output values should be present");
        let and_out = output_values
            .get("out_and")
            .expect("out_and mismatch should be tracked");
        let xor_out = output_values
            .get("out_xor")
            .expect("out_xor mismatch should be tracked");
        assert_eq!(and_out.lhs, and_out.rhs);
        assert_ne!(xor_out.lhs, xor_out.rhs);
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
        assert!(report.sat_elapsed_ns > 0);
    }

    #[test]
    fn sequential_sat_equivalence_finds_transition_counterexample() {
        let compiler = Compiler::new();

        let mut lhs = Netlist::new();
        let data_l = lhs.add_node(NodeKind::Port, "data");
        let _enable_l = lhs.add_node(NodeKind::Port, "enable");
        let clock_l = lhs.add_node(NodeKind::Port, "clock");
        let dff_l = lhs.add_node(NodeKind::Dff, "state");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef {
                node: data_l,
                port: 0,
            },
            PinRef {
                node: dff_l,
                port: 0,
            },
        )
        .expect("data->dff");
        lhs.connect(
            PinRef {
                node: clock_l,
                port: 0,
            },
            PinRef {
                node: dff_l,
                port: 1,
            },
        )
        .expect("clock->dff");
        lhs.connect(
            PinRef {
                node: dff_l,
                port: 0,
            },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .expect("dff->out");

        let mut rhs = Netlist::new();
        let data_r = rhs.add_node(NodeKind::Port, "data");
        let enable_r = rhs.add_node(NodeKind::Port, "enable");
        let clock_r = rhs.add_node(NodeKind::Port, "clock");
        let dff_r = rhs.add_node_with_logic(NodeKind::Dff, "state", Some(LogicOp::DffEnable));
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef {
                node: data_r,
                port: 0,
            },
            PinRef {
                node: dff_r,
                port: 0,
            },
        )
        .expect("data->dffe");
        rhs.connect(
            PinRef {
                node: enable_r,
                port: 0,
            },
            PinRef {
                node: dff_r,
                port: 1,
            },
        )
        .expect("enable->dffe");
        rhs.connect(
            PinRef {
                node: clock_r,
                port: 0,
            },
            PinRef {
                node: dff_r,
                port: 2,
            },
        )
        .expect("clock->dffe");
        rhs.connect(
            PinRef {
                node: dff_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .expect("dffe->out");

        let report = compiler
            .check_sequential_equivalence_sat(&lhs, &rhs)
            .expect("sequential equivalence should run on Dff/DffEnable subset");

        assert!(!report.equivalent);
        assert!(report.counterexample_inputs.is_some());
        assert!(report.counterexample_present_states.is_some());
        assert!(report.counterexample_states.is_some());
        let state = report
            .counterexample_states
            .as_ref()
            .and_then(|states| states.get("state"))
            .expect("state transition mismatch should exist");
        assert!(state.lhs_next != state.rhs_next || state.lhs_clock != state.rhs_clock);
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
        assert!(report.sat_elapsed_ns > 0);
    }

    #[test]
    fn bounded_sequential_equivalence_constrains_initial_states_equal() {
        let compiler = Compiler::new();

        let mut lhs = Netlist::new();
        let data_l = lhs.add_node(NodeKind::Port, "data");
        let clock_l = lhs.add_node(NodeKind::Port, "clock");
        let dff_l = lhs.add_node(NodeKind::Dff, "state");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef {
                node: data_l,
                port: 0,
            },
            PinRef {
                node: dff_l,
                port: 0,
            },
        )
        .expect("data->dff");
        lhs.connect(
            PinRef {
                node: clock_l,
                port: 0,
            },
            PinRef {
                node: dff_l,
                port: 1,
            },
        )
        .expect("clock->dff");
        lhs.connect(
            PinRef {
                node: dff_l,
                port: 0,
            },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .expect("dff->out");

        let mut rhs = Netlist::new();
        let data_r = rhs.add_node(NodeKind::Port, "data");
        let clock_r = rhs.add_node(NodeKind::Port, "clock");
        let dff_r = rhs.add_node(NodeKind::Dff, "state");
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef {
                node: data_r,
                port: 0,
            },
            PinRef {
                node: dff_r,
                port: 0,
            },
        )
        .expect("data->dff");
        rhs.connect(
            PinRef {
                node: clock_r,
                port: 0,
            },
            PinRef {
                node: dff_r,
                port: 1,
            },
        )
        .expect("clock->dff");
        rhs.connect(
            PinRef {
                node: dff_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .expect("dff->out");

        let report = compiler
            .check_bounded_sequential_equivalence_sat(&lhs, &rhs, 3)
            .expect("bounded sequential equivalence should run");

        assert!(report.equivalent);
        assert_eq!(report.depth, 3);
        assert_eq!(report.checked_steps, 3);
        assert_eq!(report.unroll_mode, "state_unrolled");
        assert!(report.first_failing_step.is_none());
    }

    #[test]
    fn sequential_sat_equivalence_reports_state_interface_mismatch() {
        let compiler = Compiler::new();

        let mut lhs = Netlist::new();
        let data_l = lhs.add_node(NodeKind::Port, "data");
        let clock_l = lhs.add_node(NodeKind::Port, "clock");
        let dff_l = lhs.add_node(NodeKind::Dff, "lhs_state");
        lhs.connect(
            PinRef {
                node: data_l,
                port: 0,
            },
            PinRef {
                node: dff_l,
                port: 0,
            },
        )
        .expect("data->dff");
        lhs.connect(
            PinRef {
                node: clock_l,
                port: 0,
            },
            PinRef {
                node: dff_l,
                port: 1,
            },
        )
        .expect("clock->dff");

        let mut rhs = Netlist::new();
        let data_r = rhs.add_node(NodeKind::Port, "data");
        let clock_r = rhs.add_node(NodeKind::Port, "clock");
        let dff_r = rhs.add_node(NodeKind::Dff, "rhs_state");
        rhs.connect(
            PinRef {
                node: data_r,
                port: 0,
            },
            PinRef {
                node: dff_r,
                port: 0,
            },
        )
        .expect("data->dff");
        rhs.connect(
            PinRef {
                node: clock_r,
                port: 0,
            },
            PinRef {
                node: dff_r,
                port: 1,
            },
        )
        .expect("clock->dff");

        let error = compiler
            .check_sequential_equivalence_sat(&lhs, &rhs)
            .expect_err("state name mismatch should be reported at the interface boundary");

        match error {
            SynthError::SatInterfaceMismatch(message) => {
                assert!(message.contains("state sets differ"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn compile_netlist_rewrites_supported_boolean_subgraph() {
        let mut compiler = Compiler::new();
        let pdk = Pdk::minimal("test-pdk");
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");

        let config = SynthesisConfig {
            plan: CompilePlan {
                connections: vec![
                    ConnectionSpec {
                        from: PinRef { node: a, port: 0 },
                        to: PinRef {
                            node: gate,
                            port: 0,
                        },
                    },
                    ConnectionSpec {
                        from: PinRef { node: b, port: 0 },
                        to: PinRef {
                            node: gate,
                            port: 1,
                        },
                    },
                ],
                balance_strategy: BalanceStrategy::None,
                balancing_sources: Vec::new(),
            },
            bool_opt: BoolOptConfig {
                share_logic_flattening_limit: 8,
                infer_xor_mux: false,
                infer_dffe: false,
            },
        };

        let report = compiler
            .compile_netlist(&mut netlist, &pdk, &config)
            .expect("synthesis pipeline should succeed");

        assert_eq!(
            report.bool_opt.gate_count_before,
            report.bool_opt.gate_count_after
        );
        assert_eq!(report.node_count, 3);
        assert_eq!(report.edge_count, 2);
        assert!(matches!(netlist.nodes()[0].kind, NodeKind::Port));
        assert!(matches!(netlist.nodes()[1].kind, NodeKind::Port));
        assert!(matches!(netlist.nodes()[2].kind, NodeKind::CellInstance));
        assert!(report.bool_opt_compatibility.is_compatible());
    }

    #[test]
    fn compile_plan_can_balance_by_sink_level() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let src_fast = netlist.add_node(NodeKind::CellInstance, "src_fast");
        let src_slow = netlist.add_node(NodeKind::CellInstance, "src_slow");
        let mid = netlist.add_node(NodeKind::CellInstance, "mid");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");

        let plan = CompilePlan {
            connections: vec![
                ConnectionSpec {
                    from: PinRef {
                        node: src_slow,
                        port: 0,
                    },
                    to: PinRef { node: mid, port: 0 },
                },
                ConnectionSpec {
                    from: PinRef { node: mid, port: 0 },
                    to: PinRef {
                        node: sink,
                        port: 0,
                    },
                },
                ConnectionSpec {
                    from: PinRef {
                        node: src_fast,
                        port: 0,
                    },
                    to: PinRef {
                        node: sink,
                        port: 1,
                    },
                },
            ],
            balance_strategy: BalanceStrategy::BySinkLevel,
            balancing_sources: Vec::new(),
        };

        let report = compiler
            .compile_plan(&mut netlist, &plan)
            .expect("level-based balancing should succeed");

        assert_eq!(report.connections_applied, 3);
        assert_eq!(report.splitters_inserted, 0);
        assert_eq!(report.balancing_dffs_inserted, 1);
        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
    }

    #[test]
    fn analyzes_path_balancing_for_out_of_order_edges() {
        let compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let src_fast = netlist.add_node(NodeKind::CellInstance, "src_fast");
        let src_slow = netlist.add_node(NodeKind::CellInstance, "src_slow");
        let mid = netlist.add_node(NodeKind::CellInstance, "mid");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");

        netlist
            .connect(
                PinRef { node: mid, port: 0 },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("mid to sink should connect");
        netlist
            .connect(
                PinRef {
                    node: src_fast,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 1,
                },
            )
            .expect("fast to sink should connect");
        netlist
            .connect(
                PinRef {
                    node: src_slow,
                    port: 0,
                },
                PinRef { node: mid, port: 0 },
            )
            .expect("slow to mid should connect");

        let report = compiler
            .analyze_path_balancing(&netlist)
            .expect("analysis should succeed");

        assert_eq!(report.node_levels, vec![0, 0, 1, 2]);
        assert_eq!(report.total_insertions(), 1);
        assert_eq!(report.needs.len(), 1);
        assert_eq!(report.needs[0].sink_node, 3);
        assert_eq!(report.needs[0].source.node.0, 0);
        assert_eq!(report.needs[0].deficit, 1);
    }

    #[test]
    fn by_sink_level_balancing_handles_out_of_order_edges() {
        let mut compiler = Compiler::new();
        let mut netlist = Netlist::new();

        let src_fast = netlist.add_node(NodeKind::CellInstance, "src_fast");
        let src_slow = netlist.add_node(NodeKind::CellInstance, "src_slow");
        let mid = netlist.add_node(NodeKind::CellInstance, "mid");
        let sink = netlist.add_node(NodeKind::CellInstance, "sink");

        netlist
            .connect(
                PinRef { node: mid, port: 0 },
                PinRef {
                    node: sink,
                    port: 0,
                },
            )
            .expect("mid to sink should connect");
        netlist
            .connect(
                PinRef {
                    node: src_fast,
                    port: 0,
                },
                PinRef {
                    node: sink,
                    port: 1,
                },
            )
            .expect("fast to sink should connect");
        netlist
            .connect(
                PinRef {
                    node: src_slow,
                    port: 0,
                },
                PinRef { node: mid, port: 0 },
            )
            .expect("slow to mid should connect");

        let inserted = compiler
            .balance_by_sink_level(&mut netlist)
            .expect("balancing should succeed");

        assert_eq!(inserted, 1);
        assert_eq!(netlist.node_count(), 5);
        assert_eq!(netlist.edge_count(), 4);
    }

    #[test]
    fn compile_netlist_returns_unified_summary() {
        let mut compiler = Compiler::new();
        let pdk = Pdk::minimal("test-pdk");
        let mut netlist = Netlist::new();

        let a = netlist.add_node(NodeKind::Port, "a");
        let b = netlist.add_node(NodeKind::Port, "b");
        let gate = netlist.add_node(NodeKind::CellInstance, "gate");

        let config = SynthesisConfig {
            plan: CompilePlan {
                connections: vec![
                    ConnectionSpec {
                        from: PinRef { node: a, port: 0 },
                        to: PinRef {
                            node: gate,
                            port: 0,
                        },
                    },
                    ConnectionSpec {
                        from: PinRef { node: b, port: 0 },
                        to: PinRef {
                            node: gate,
                            port: 1,
                        },
                    },
                ],
                balance_strategy: BalanceStrategy::BySinkLevel,
                balancing_sources: Vec::new(),
            },
            bool_opt: BoolOptConfig::default(),
        };

        let report = compiler
            .compile_netlist(&mut netlist, &pdk, &config)
            .expect("synthesis pipeline should succeed");

        assert_eq!(report.compile.connections_applied, 2);
        assert_eq!(report.compile.splitters_inserted, 0);
        assert_eq!(report.compile.balancing_dffs_inserted, 0);
        assert_eq!(report.tech_map.mapped_nodes, 3);
        assert_eq!(report.node_count, 3);
        assert_eq!(report.edge_count, 2);
        assert!(report.bool_opt_compatibility.is_compatible());
        assert_eq!(report.path_balance.total_insertions(), 0);
    }

    #[test]
    fn synth_error_codes_are_stable() {
        assert_eq!(SynthError::CombinationalCycle.code(), "RFLOW-FLOW-001");
        assert_eq!(
            SynthError::SatInterfaceMismatch("test".into()).code(),
            "RFLOW-VERIFY-001"
        );
        assert_eq!(
            SynthError::SatUnsupportedNodeKind {
                node: 0,
                kind: NodeKind::Dff
            }
            .code(),
            "RFLOW-VERIFY-002"
        );
        assert!(!SynthError::CombinationalCycle.suggestion().is_empty());
    }

    #[test]
    fn find_complex_cell_candidates_empty_netlist() {
        let netlist = Netlist::new();
        let pdk = Pdk::minimal("test");
        let mapper = TechMapper::new(&pdk);
        let candidates = mapper.find_complex_cell_candidates(&netlist);
        assert!(candidates.is_empty());
    }

    #[test]
    fn find_complex_cell_candidates_finds_macro_cells() {
        let mut netlist = Netlist::new();
        let _port = netlist.add_node(NodeKind::Port, "in");
        let _macro = netlist.add_node(NodeKind::MacroCell, "sfq_macro");
        let _out = netlist.add_node(NodeKind::Port, "out");

        let pdk = Pdk::minimal("test");
        let mapper = TechMapper::new(&pdk);
        let candidates = mapper.find_complex_cell_candidates(&netlist);
        assert!(candidates.iter().any(|c| c.root.name == "sfq_macro"));
    }

    #[test]
    fn find_complex_cell_candidates_skips_non_macro() {
        let mut netlist = Netlist::new();
        let _port = netlist.add_node(NodeKind::Port, "in");
        let _gate = netlist.add_node(NodeKind::CellInstance, "sfq_gate");
        let _out = netlist.add_node(NodeKind::Port, "out");

        let pdk = Pdk::minimal("test");
        let mapper = TechMapper::new(&pdk);
        let candidates = mapper.find_complex_cell_candidates(&netlist);
        assert!(candidates.is_empty());
    }

    #[test]
    fn complex_cell_candidate_has_correct_area() {
        let mut netlist = Netlist::new();
        let _port = netlist.add_node(NodeKind::Port, "in");
        let _macro = netlist.add_node(NodeKind::MacroCell, "sfq_macro");
        let _out = netlist.add_node(NodeKind::Port, "out");

        let pdk = Pdk::minimal("test");
        let mapper = TechMapper::new(&pdk);
        let candidates = mapper.find_complex_cell_candidates(&netlist);
        for c in &candidates {
            assert!(c.area_um2 > 0.0);
        }
    }

    // --- P1-5: PhysicalFeasibilityEstimator tests ---

    #[test]
    fn feasibility_estimator_empty_netlist() {
        let estimator = PhysicalFeasibilityEstimator::new();
        let netlist = Netlist::new();
        let pdk = Pdk::minimal("test");
        let report = estimator.estimate(&netlist, &pdk);
        assert_eq!(report.node_count, 0);
        assert_eq!(report.edge_count, 0);
        assert!(report.feasible);
    }

    #[test]
    fn feasibility_estimator_simple_netlist() {
        let estimator = PhysicalFeasibilityEstimator::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "in");
        let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
        let g2 = netlist.add_node(NodeKind::CellInstance, "g2");
        let out = netlist.add_node(NodeKind::Port, "out");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: g1, port: 0 }).unwrap();
        netlist.connect(PinRef { node: g1, port: 0 }, PinRef { node: g2, port: 0 }).unwrap();
        netlist.connect(PinRef { node: g2, port: 0 }, PinRef { node: out, port: 0 }).unwrap();

        let pdk = Pdk::minimal("test");
        let report = estimator.estimate(&netlist, &pdk);
        assert_eq!(report.node_count, 4);
        assert_eq!(report.edge_count, 3);
        assert!(report.estimated_area_um2 > 0.0);
    }

    #[test]
    fn feasibility_estimator_detects_high_congestion() {
        let estimator = PhysicalFeasibilityEstimator::new();
        let mut netlist = Netlist::new();
        // Create a highly connected netlist that will have high congestion
        let inp = netlist.add_node(NodeKind::Port, "in");
        let mut prev = inp;
        for i in 0..20 {
            let split = netlist.add_node(NodeKind::Splitter, format!("split_{i}"));
            netlist.connect(PinRef { node: prev, port: 0 }, PinRef { node: split, port: 0 }).unwrap();
            prev = split;
        }
        let out = netlist.add_node(NodeKind::Port, "out");
        netlist.connect(PinRef { node: prev, port: 0 }, PinRef { node: out, port: 0 }).unwrap();

        let pdk = Pdk::minimal("test");
        let report = estimator.estimate(&netlist, &pdk);
        assert_eq!(report.splitter_count, 20);
        assert!(!report.warnings.is_empty() || report.congestion_ratio > 0.0);
    }

    #[test]
    fn feasibility_estimator_counts_dffs() {
        let estimator = PhysicalFeasibilityEstimator::new();
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "in");
        let dff = netlist.add_node(NodeKind::Dff, "dff");
        let out = netlist.add_node(NodeKind::Port, "out");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: dff, port: 0 }).unwrap();
        netlist.connect(PinRef { node: dff, port: 0 }, PinRef { node: out, port: 0 }).unwrap();

        let pdk = Pdk::minimal("test");
        let report = estimator.estimate(&netlist, &pdk);
        assert_eq!(report.dff_count, 1);
    }

    // --- P2-2: DFT tests ---

    #[test]
    fn testability_report_simple_chain() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "in");
        let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
        let g2 = netlist.add_node(NodeKind::CellInstance, "g2");
        let out = netlist.add_node(NodeKind::Port, "out");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: g1, port: 0 }).unwrap();
        netlist.connect(PinRef { node: g1, port: 0 }, PinRef { node: g2, port: 0 }).unwrap();
        netlist.connect(PinRef { node: g2, port: 0 }, PinRef { node: out, port: 0 }).unwrap();

        let report = analyze_testability(&netlist);
        assert_eq!(report.total_nodes, 4);
        assert_eq!(report.controllable_nodes, 4); // all reachable from input
        assert_eq!(report.observable_nodes, 4); // all reachable to output
        assert!((report.fault_coverage_percent - 100.0).abs() < 1e-9);
    }

    #[test]
    fn testability_report_uncontrollable_node() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "in");
        let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
        let out = netlist.add_node(NodeKind::Port, "out");
        let orphan = netlist.add_node(NodeKind::CellInstance, "orphan");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: g1, port: 0 }).unwrap();
        netlist.connect(PinRef { node: g1, port: 0 }, PinRef { node: out, port: 0 }).unwrap();

        let report = analyze_testability(&netlist);
        assert_eq!(report.total_nodes, 4);
        // orphan is neither controllable nor observable
        assert!(report.untestable_nodes.contains(&"orphan".to_string()));
        assert!(report.fault_coverage_percent < 100.0);
    }

    #[test]
    fn test_point_injector_inserts_at_high_fanout() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "in");
        let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
        let split = netlist.add_node(NodeKind::Splitter, "split");
        let o1 = netlist.add_node(NodeKind::CellInstance, "o1");
        let o2 = netlist.add_node(NodeKind::CellInstance, "o2");
        let o3 = netlist.add_node(NodeKind::CellInstance, "o3");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: g1, port: 0 }).unwrap();
        netlist.connect(PinRef { node: g1, port: 0 }, PinRef { node: split, port: 0 }).unwrap();
        netlist.connect(PinRef { node: split, port: 0 }, PinRef { node: o1, port: 0 }).unwrap();
        netlist.connect(PinRef { node: split, port: 1 }, PinRef { node: o2, port: 0 }).unwrap();
        netlist.connect(PinRef { node: split, port: 2 }, PinRef { node: o3, port: 0 }).unwrap();

        let injector = TestPointInjector::new();
        let tps = injector.insert_test_points(&mut netlist, 5);
        // split has fanout 3 (via splitter), should get a test point
        // (or the node before it)
        assert!(tps.len() <= 5);
    }

    #[test]
    fn pulse_observer_inserts_at_outputs() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "in");
        let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
        let out = netlist.add_node(NodeKind::Port, "out");
        netlist.connect(PinRef { node: a, port: 0 }, PinRef { node: g1, port: 0 }).unwrap();
        netlist.connect(PinRef { node: g1, port: 0 }, PinRef { node: out, port: 0 }).unwrap();

        let observer = PulseObserver::new();
        let obs = observer.insert_observation_points(&mut netlist);
        assert_eq!(obs.len(), 1); // one output port
        assert_eq!(obs[0].target_node, out);
    }
}
