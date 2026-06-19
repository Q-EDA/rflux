use rflux_ir::Netlist;
use rflux_synth::{
    Compiler, EquivalenceSatProblem, SatEquivalenceReport, SequentialEquivalenceReport,
};
use rflux_timing::{TimingArcReport, TimingConfig, TimingReport};
use serde::Serialize;
use thiserror::Error;

pub use rflux_synth::{
    BoundedSequentialEquivalenceReport, BoundedSequentialEquivalenceStepReport,
    EquivalenceCheckKind, EquivalenceCheckTarget,
    EquivalenceSatProblem as ExportedEquivalenceSatProblem,
    SatEquivalenceReport as CombinationalEquivalenceReport,
    SequentialEquivalenceReport as SingleStepSequentialEquivalenceReport,
};

#[derive(Debug, Default)]
pub struct Verifier {
    compiler: Compiler,
}

impl Verifier {
    #[must_use]
    pub fn new() -> Self {
        Self {
            compiler: Compiler::new(),
        }
    }

    pub fn check_boolean_equivalence(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<SatEquivalenceReport, SynthError> {
        self.compiler.check_boolean_equivalence_sat(lhs, rhs)
    }

    pub fn check_single_step_sequential_equivalence(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<SequentialEquivalenceReport, SynthError> {
        self.compiler.check_sequential_equivalence_sat(lhs, rhs)
    }

    pub fn check_bounded_sequential_equivalence(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
        depth: usize,
    ) -> Result<BoundedSequentialEquivalenceReport, SynthError> {
        self.compiler
            .check_bounded_sequential_equivalence_sat(lhs, rhs, depth)
    }

    pub fn build_boolean_equivalence_problem(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<EquivalenceSatProblem, SynthError> {
        self.compiler.build_boolean_equivalence_problem(lhs, rhs)
    }

    pub fn build_single_step_sequential_equivalence_problem(
        &self,
        lhs: &Netlist,
        rhs: &Netlist,
    ) -> Result<EquivalenceSatProblem, SynthError> {
        self.compiler.build_sequential_equivalence_problem(lhs, rhs)
    }
}

pub use rflux_synth::{SatOutputMismatch, SatStateTransitionMismatch, SynthError};

// ---------------------------------------------------------------------------
// P0-3: Timing-Functional Joint Verification
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum TimingFunctionalError {
    #[error("timing report has no arcs to verify")]
    NoArcs,
}

impl TimingFunctionalError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            TimingFunctionalError::NoArcs => "RFLOW-VERIFY-001",
        }
    }

    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            TimingFunctionalError::NoArcs => {
                "Run STA before timing-functional verification. Ensure the netlist has edges."
            }
        }
    }
}

/// Configuration for timing-functional joint verification.
#[derive(Debug, Clone, Serialize)]
pub struct TimingFunctionalConfig {
    /// Maximum allowed delay difference (ps) between two combinational
    /// paths converging at the same DFF.  Defaults to
    /// `clock_period_ps - sfq_pulse_window_ps`.
    pub path_balance_tolerance_ps: Option<f64>,
    /// If true, treat capture-window violations as functional errors.
    pub capture_window_as_functional: bool,
    /// If true, treat hold violations as functional errors.
    pub hold_as_functional: bool,
}

impl Default for TimingFunctionalConfig {
    fn default() -> Self {
        Self {
            path_balance_tolerance_ps: None,
            capture_window_as_functional: true,
            hold_as_functional: true,
        }
    }
}

/// A single functional violation detected by the joint verifier.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionalViolation {
    /// Human-readable description of the violation.
    pub description: String,
    /// The timing arc that caused this violation (from, to pins).
    pub arc_from: rflux_ir::PinRef,
    pub arc_to: rflux_ir::PinRef,
    /// Severity: "error" or "warning".
    pub severity: String,
    /// Category: "pulse_alignment", "path_balance", "hold", "capture_window".
    pub category: String,
    /// The offending value (e.g., negative slack, excessive delay diff).
    pub value_ps: f64,
}

/// Result of timing-functional joint verification.
#[derive(Debug, Clone, Serialize)]
pub struct TimingFunctionalReport {
    pub total_arcs_checked: usize,
    pub functional_violations: Vec<FunctionalViolation>,
    pub pulse_alignment_violations: usize,
    pub path_balance_violations: usize,
    pub hold_functional_violations: usize,
    pub capture_window_functional_violations: usize,
    pub passed: bool,
}

/// Verifier that checks whether timing results guarantee functional
/// correctness in SFQ circuits (P0-3).
///
/// In SFQ, a timing violation is not just a performance issue — it
/// is a **functional** error because pulses are consumed on read
/// (destructive readout).  If a pulse arrives outside the capture
/// window, the receiving gate sees the wrong value.
#[derive(Debug, Default)]
pub struct TimingFunctionalVerifier;

impl TimingFunctionalVerifier {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Run timing-functional joint verification.
    ///
    /// Checks:
    /// 1. **Pulse alignment**: launch window overlaps capture window
    ///    for every non-false-path arc.
    /// 2. **Path balance**: combinational path delay differences
    ///    converging at DFFs are within tolerance.
    /// 3. **Hold violations** as functional errors (pulse arrives too
    ///    early, consumed in wrong clock phase).
    /// 4. **Capture window violations** as functional errors.
    pub fn verify(
        &self,
        netlist: &Netlist,
        timing: &TimingReport,
        timing_config: &TimingConfig,
        config: &TimingFunctionalConfig,
    ) -> Result<TimingFunctionalReport, TimingFunctionalError> {
        if timing.arcs.is_empty() {
            return Err(TimingFunctionalError::NoArcs);
        }

        let tolerance_ps = config
            .path_balance_tolerance_ps
            .unwrap_or(timing_config.clock_period_ps - timing_config.sfq_pulse_window_ps);

        let mut violations = Vec::new();

        // Check 1 & 3 & 4: per-arc checks
        for arc in &timing.arcs {
            if arc.is_false_path {
                continue;
            }

            // Pulse alignment: launch window must overlap capture window
            let launch_start = arc.launch_window_start_ps;
            let launch_end = arc.launch_window_end_ps;
            let capture_start = arc.capture_window_start_ps;
            let capture_end = arc.capture_window_end_ps;
            let overlaps =
                launch_start <= capture_end && capture_start <= launch_end;
            if !overlaps {
                let gap = if launch_end < capture_start {
                    capture_start - launch_end
                } else {
                    launch_start - capture_end
                };
                violations.push(FunctionalViolation {
                    description: format!(
                        "Pulse alignment failure: launch window [{:.1}, {:.1}] ps \
                         does not overlap capture window [{:.1}, {:.1}] ps (gap: {:.1} ps)",
                        launch_start, launch_end, capture_start, capture_end, gap
                    ),
                    arc_from: arc.from,
                    arc_to: arc.to,
                    severity: "error".to_string(),
                    category: "pulse_alignment".to_string(),
                    value_ps: -gap,
                });
            }

            // Capture window violation
            if config.capture_window_as_functional && arc.capture_window_violation {
                violations.push(FunctionalViolation {
                    description: format!(
                        "Capture window violation: arrival outside capture window"
                    ),
                    arc_from: arc.from,
                    arc_to: arc.to,
                    severity: "error".to_string(),
                    category: "capture_window".to_string(),
                    value_ps: arc.capture_window_slack_ps,
                });
            }

            // Hold violation as functional error
            if config.hold_as_functional && arc.hold_slack_ps < 0.0 {
                violations.push(FunctionalViolation {
                    description: format!(
                        "Hold violation (functional): hold slack {:.1} ps < 0",
                        arc.hold_slack_ps
                    ),
                    arc_from: arc.from,
                    arc_to: arc.to,
                    severity: "error".to_string(),
                    category: "hold".to_string(),
                    value_ps: arc.hold_slack_ps,
                });
            }
        }

        // Check 2: path balance — find DFF input arcs and compare
        // arrival times for paths converging at the same DFF.
        let dff_input_arcs: Vec<&TimingArcReport> = timing
            .arcs
            .iter()
            .filter(|a| {
                !a.is_false_path
                    && netlist.nodes().get(a.to.node.0).map_or(false, |n| {
                        matches!(n.kind, rflux_ir::NodeKind::Dff)
                    })
            })
            .collect();

        // Group by destination DFF node
        let mut by_dff: std::collections::HashMap<
            rflux_ir::NodeId,
            Vec<&TimingArcReport>,
        > = std::collections::HashMap::new();
        for arc in &dff_input_arcs {
            by_dff.entry(arc.to.node).or_default().push(arc);
        }

        for (_dff_id, arcs) in &by_dff {
            if arcs.len() < 2 {
                continue;
            }
            // Compare all pairs of arrival times at this DFF
            for i in 0..arcs.len() {
                for j in (i + 1)..arcs.len() {
                    let diff = (arcs[i].arrival_ps - arcs[j].arrival_ps).abs();
                    if diff > tolerance_ps {
                        violations.push(FunctionalViolation {
                            description: format!(
                                "Path balance violation at DFF: arrival difference {:.1} ps \
                                 exceeds tolerance {:.1} ps (path A: {}->{}, path B: {}->{})",
                                diff,
                                tolerance_ps,
                                netlist.nodes()[arcs[i].from.node.0].name,
                                netlist.nodes()[arcs[i].to.node.0].name,
                                netlist.nodes()[arcs[j].from.node.0].name,
                                netlist.nodes()[arcs[j].to.node.0].name,
                            ),
                            arc_from: arcs[i].from,
                            arc_to: arcs[i].to,
                            severity: "error".to_string(),
                            category: "path_balance".to_string(),
                            value_ps: diff - tolerance_ps,
                        });
                    }
                }
            }
        }

        let pulse_alignment_violations = violations
            .iter()
            .filter(|v| v.category == "pulse_alignment")
            .count();
        let path_balance_violations = violations
            .iter()
            .filter(|v| v.category == "path_balance")
            .count();
        let hold_functional_violations =
            violations.iter().filter(|v| v.category == "hold").count();
        let capture_window_functional_violations = violations
            .iter()
            .filter(|v| v.category == "capture_window")
            .count();
        let passed = violations.is_empty();

        Ok(TimingFunctionalReport {
            total_arcs_checked: timing.arcs.len(),
            functional_violations: violations,
            pulse_alignment_violations,
            path_balance_violations,
            hold_functional_violations,
            capture_window_functional_violations,
            passed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{LogicOp, NodeKind, PinRef};

    #[test]
    fn verifier_reports_combinational_equivalence() {
        let verifier = Verifier::new();

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

        let report = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .expect("combinational equivalence should succeed");

        assert!(report.equivalent);
        assert_eq!(report.checked_outputs, vec!["out".to_string()]);
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
    }

    #[test]
    fn verifier_reports_sequential_counterexample() {
        let verifier = Verifier::new();

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

        // DEBUG: dump the equivalence problem DIMACS for offline CDCL diagnosis.
        let problem = verifier.build_single_step_sequential_equivalence_problem(&lhs, &rhs).expect("build problem");
        eprintln!("SEQ_MITER vars={} clauses={}", problem.formula.var_count(), problem.formula.clauses().len());
        eprintln!("SEQ_MITER checks={}", problem.checks.len());
        for (i, c) in problem.checks.iter().enumerate() {
            eprintln!("SEQ_CHECK[{}] kind={:?} name={} assumptions={:?}", i, c.kind, c.name, c.assumptions);
        }
        std::fs::write("/tmp/seq_miter.cnf", problem.formula.to_dimacs()).expect("write dimacs");
        eprintln!("SEQ_DIMACS_START\n{}\nSEQ_DIMACS_END", problem.formula.to_dimacs());

        let report = verifier
            .check_single_step_sequential_equivalence(&lhs, &rhs)
            .expect("single-step sequential equivalence should run");

        assert!(!report.equivalent);
        assert!(report.counterexample_states.is_some());
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
    }

    #[test]
    fn verifier_reports_bounded_sequential_counterexample() {
        let verifier = Verifier::new();

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

        let report = verifier
            .check_bounded_sequential_equivalence(&lhs, &rhs, 4)
            .expect("bounded sequential equivalence should run");

        assert_eq!(report.depth, 4);
        assert!(!report.equivalent);
        assert_eq!(report.first_failing_step, Some(0));
        assert_eq!(report.steps.len(), 1);
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
    }

    #[test]
    fn verifier_exports_boolean_equivalence_problem() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .expect("a->out");

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let split_r = rhs.add_node(NodeKind::Splitter, "split");
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: split_r,
                port: 0,
            },
        )
        .expect("a->split");
        rhs.connect(
            PinRef {
                node: split_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .expect("split->out");

        let problem = verifier
            .build_boolean_equivalence_problem(&lhs, &rhs)
            .expect("export should succeed");

        assert_eq!(problem.checks.len(), 1);
        assert_eq!(problem.checks[0].kind, EquivalenceCheckKind::Output);
        assert_eq!(problem.checks[0].name, "out");
        assert!(problem.formula.to_dimacs().starts_with("p cnf "));
    }

    #[test]
    fn verifier_default_creates_valid_instance() {
        let verifier = Verifier::default();
        let mut lhs = Netlist::new();
        let a = lhs.add_node(NodeKind::Port, "a");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a, port: 0 },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .unwrap();
        let mut rhs = Netlist::new();
        let b = rhs.add_node(NodeKind::Port, "a");
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: b, port: 0 },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .unwrap();
        let report = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .expect("single-port passthrough should be equivalent");
        assert!(report.equivalent);
    }

    #[test]
    fn verifier_detects_non_equivalent_combinational_circuits() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let b_l = lhs.add_node(NodeKind::Port, "b");
        let and_l = lhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: and_l,
                port: 0,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef { node: b_l, port: 0 },
            PinRef {
                node: and_l,
                port: 1,
            },
        )
        .unwrap();
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
        .unwrap();

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let b_r = rhs.add_node(NodeKind::Port, "b");
        let or_r = rhs.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: or_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef { node: b_r, port: 0 },
            PinRef {
                node: or_r,
                port: 1,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef {
                node: or_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .unwrap();

        let report = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .expect("equivalence check should run");
        assert!(!report.equivalent);
        assert!(report.sat_stats.decisions + report.sat_stats.unit_assignments >= 1);
    }

    #[test]
    fn verifier_sequential_equivalent_circuits_report_equivalent() {
        let verifier = Verifier::new();

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
        .unwrap();
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
        .unwrap();
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
        .unwrap();

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
        .unwrap();
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
        .unwrap();
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
        .unwrap();

        // DEBUG: dump the equivalence problem DIMACS for offline CDCL diagnosis.
        let problem = verifier.build_single_step_sequential_equivalence_problem(&lhs, &rhs).expect("build problem");
        eprintln!("SEQ_MITER vars={} clauses={}", problem.formula.var_count(), problem.formula.clauses().len());
        eprintln!("SEQ_MITER checks={}", problem.checks.len());
        for (i, c) in problem.checks.iter().enumerate() {
            eprintln!("SEQ_CHECK[{}] kind={:?} name={} assumptions={:?}", i, c.kind, c.name, c.assumptions);
        }
        std::fs::write("/tmp/seq_miter.cnf", problem.formula.to_dimacs()).expect("write dimacs");
        eprintln!("SEQ_DIMACS_START\n{}\nSEQ_DIMACS_END", problem.formula.to_dimacs());

        let report = verifier
            .check_single_step_sequential_equivalence(&lhs, &rhs)
            .expect("single-step sequential equivalence should run");
        assert!(report.equivalent);
    }

    #[test]
    fn verifier_builds_sequential_equivalence_problem() {
        let verifier = Verifier::new();

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
        .unwrap();
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
        .unwrap();
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
        .unwrap();

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
        .unwrap();
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
        .unwrap();
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
        .unwrap();

        let problem = verifier
            .build_single_step_sequential_equivalence_problem(&lhs, &rhs)
            .expect("export should succeed");
        assert!(!problem.checks.is_empty());
        assert!(problem.formula.to_dimacs().starts_with("p cnf "));
    }

    #[test]
    fn bounded_equivalent_circuits_pass() {
        let verifier = Verifier::new();

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
        .unwrap();
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
        .unwrap();
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
        .unwrap();

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
        .unwrap();
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
        .unwrap();
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
        .unwrap();

        let report = verifier
            .check_bounded_sequential_equivalence(&lhs, &rhs, 3)
            .expect("bounded sequential equivalence should run");
        assert!(report.equivalent);
        assert_eq!(report.depth, 3);
    }

    #[test]
    fn verifier_detects_xor_vs_xnor_non_equivalence() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let b_l = lhs.add_node(NodeKind::Port, "b");
        let xor_l = lhs.add_node_with_logic(NodeKind::CellInstance, "xor0", Some(LogicOp::Xor));
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: xor_l,
                port: 0,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef { node: b_l, port: 0 },
            PinRef {
                node: xor_l,
                port: 1,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef {
                node: xor_l,
                port: 0,
            },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .unwrap();

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let b_r = rhs.add_node(NodeKind::Port, "b");
        let xor_r = rhs.add_node_with_logic(NodeKind::CellInstance, "xor0", Some(LogicOp::Xor));
        let not_r = rhs.add_node_with_logic(NodeKind::CellInstance, "not0", Some(LogicOp::Not));
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: xor_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef { node: b_r, port: 0 },
            PinRef {
                node: xor_r,
                port: 1,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef {
                node: xor_r,
                port: 0,
            },
            PinRef {
                node: not_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef {
                node: not_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .unwrap();

        let report = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .expect("equivalence check should run");
        assert!(!report.equivalent);
    }

    #[test]
    fn verifier_multi_level_circuit_equivalence() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let b_l = lhs.add_node(NodeKind::Port, "b");
        let and_l = lhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let or_l = lhs.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: and_l,
                port: 0,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef { node: b_l, port: 0 },
            PinRef {
                node: and_l,
                port: 1,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef {
                node: and_l,
                port: 0,
            },
            PinRef {
                node: or_l,
                port: 0,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef { node: a_l, port: 1 },
            PinRef {
                node: or_l,
                port: 1,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef {
                node: or_l,
                port: 0,
            },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .unwrap();

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let b_r = rhs.add_node(NodeKind::Port, "b");
        let and_r = rhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let or_r = rhs.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: and_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef { node: b_r, port: 0 },
            PinRef {
                node: and_r,
                port: 1,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef {
                node: and_r,
                port: 0,
            },
            PinRef {
                node: or_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef { node: a_r, port: 1 },
            PinRef {
                node: or_r,
                port: 1,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef {
                node: or_r,
                port: 0,
            },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .unwrap();

        let report = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .expect("equivalence check should run");
        assert!(report.equivalent);
        assert_eq!(report.checked_outputs, vec!["out".to_string()]);
    }

    #[test]
    fn verifier_multiple_output_equivalence() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let and_l = lhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out1_l = lhs.add_node(NodeKind::Port, "out1");
        let out2_l = lhs.add_node(NodeKind::Port, "out2");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: and_l,
                port: 0,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef { node: a_l, port: 1 },
            PinRef {
                node: and_l,
                port: 1,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef {
                node: and_l,
                port: 0,
            },
            PinRef {
                node: out1_l,
                port: 0,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef { node: a_l, port: 2 },
            PinRef {
                node: out2_l,
                port: 0,
            },
        )
        .unwrap();

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let and_r = rhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
        let out1_r = rhs.add_node(NodeKind::Port, "out1");
        let out2_r = rhs.add_node(NodeKind::Port, "out2");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: and_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef { node: a_r, port: 1 },
            PinRef {
                node: and_r,
                port: 1,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef {
                node: and_r,
                port: 0,
            },
            PinRef {
                node: out1_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef { node: a_r, port: 2 },
            PinRef {
                node: out2_r,
                port: 0,
            },
        )
        .unwrap();

        let report = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .expect("equivalence check should run");
        assert!(report.equivalent);
        assert_eq!(report.checked_outputs.len(), 2);
    }

    #[test]
    fn verifier_splitter_passthrough_equivalence() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let split_l = lhs.add_node(NodeKind::Splitter, "split");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(
            PinRef { node: a_l, port: 0 },
            PinRef {
                node: split_l,
                port: 0,
            },
        )
        .unwrap();
        lhs.connect(
            PinRef {
                node: split_l,
                port: 0,
            },
            PinRef {
                node: out_l,
                port: 0,
            },
        )
        .unwrap();

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(
            PinRef { node: a_r, port: 0 },
            PinRef {
                node: out_r,
                port: 0,
            },
        )
        .unwrap();

        let report = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .expect("equivalence check should run");
        assert!(report.equivalent);
    }

    #[test]
    fn bounded_non_equivalent_detected() {
        let verifier = Verifier::new();

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
        .unwrap();
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
        .unwrap();
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
        .unwrap();

        let mut rhs = Netlist::new();
        let data_r = rhs.add_node(NodeKind::Port, "data");
        let clock_r = rhs.add_node(NodeKind::Port, "clock");
        let dff_r = rhs.add_node(NodeKind::Dff, "state");
        let and_r = rhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
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
        .unwrap();
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
        .unwrap();
        rhs.connect(
            PinRef {
                node: dff_r,
                port: 0,
            },
            PinRef {
                node: and_r,
                port: 0,
            },
        )
        .unwrap();
        rhs.connect(
            PinRef {
                node: data_r,
                port: 1,
            },
            PinRef {
                node: and_r,
                port: 1,
            },
        )
        .unwrap();
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
        .unwrap();

        let report = verifier
            .check_bounded_sequential_equivalence(&lhs, &rhs, 3)
            .expect("bounded check should run");
        assert!(!report.equivalent);
    }

    // --- P0-3: TimingFunctionalVerifier tests ---

    use rflux_timing::{TimingArcReport, TimingConfig};
    use rflux_route::RouteMode;
    use rflux_tech::SfCellKind;

    fn make_timing_arc(
        from: rflux_ir::PinRef,
        to: rflux_ir::PinRef,
        launch_start: f64,
        launch_end: f64,
        capture_start: f64,
        capture_end: f64,
        arrival: f64,
        required: f64,
        setup_slack: f64,
        hold_slack: f64,
    ) -> TimingArcReport {
        TimingArcReport {
            from,
            to,
            is_false_path: false,
            driver_kind: SfCellKind::GenericGate,
            route_mode: RouteMode::Jtl,
            route_length_um: 10.0,
            cell_delay_ps: 2.0,
            wire_delay_ps: 1.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: launch_start,
            launch_window_end_ps: launch_end,
            capture_window_start_ps: capture_start,
            capture_window_end_ps: capture_end,
            arrival_phase_offset_ps: 0.0,
            capture_window_slack_ps: capture_end - arrival,
            capture_window_violation: arrival > capture_end || arrival < capture_start,
            arrival_ps: arrival,
            required_ps: required,
            setup_slack_ps: setup_slack,
            hold_slack_ps: hold_slack,
            pulse_envelope: None,
            pulse_degradation_violation: false,
            ocv_early_arrival_ps: None,
            ocv_late_arrival_ps: None,
            ocv_early_slack_ps: None,
            ocv_late_slack_ps: None,
        }
    }

    #[test]
    fn timing_functional_verifier_passes_when_windows_overlap() {
        let verifier = TimingFunctionalVerifier::new();
        let netlist = Netlist::new();
        let from = rflux_ir::PinRef { node: rflux_ir::NodeId(0), port: 0 };
        let to = rflux_ir::PinRef { node: rflux_ir::NodeId(1), port: 0 };
        let timing = TimingReport {
            arcs: vec![make_timing_arc(from, to, 0.0, 4.0, 2.0, 6.0, 3.0, 8.0, 5.0, 3.0)],
            worst_setup_slack_ps: 5.0,
            worst_hold_slack_ps: 3.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: 0.0,
            critical_path_delay_ps: 3.0,
            setup_violations: 0,
            hold_violations: 0,
            capture_window_violations: 0,
            analyzed_arcs: 1,
            false_path_arcs: 0,
            extraction_report: None,
            noise_margin: None,
            path_report: None,
        };
        let report = verifier
            .verify(&netlist, &timing, &TimingConfig::default(), &TimingFunctionalConfig::default())
            .expect("verify");
        assert!(report.passed);
        assert_eq!(report.functional_violations.len(), 0);
    }

    #[test]
    fn timing_functional_verifier_detects_pulse_misalignment() {
        let verifier = TimingFunctionalVerifier::new();
        let netlist = Netlist::new();
        let from = rflux_ir::PinRef { node: rflux_ir::NodeId(0), port: 0 };
        let to = rflux_ir::PinRef { node: rflux_ir::NodeId(1), port: 0 };
        // Launch window [0, 4], capture window [10, 14] — no overlap
        let timing = TimingReport {
            arcs: vec![make_timing_arc(from, to, 0.0, 4.0, 10.0, 14.0, 3.0, 12.0, 9.0, 3.0)],
            worst_setup_slack_ps: 9.0,
            worst_hold_slack_ps: 3.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: 0.0,
            critical_path_delay_ps: 3.0,
            setup_violations: 0,
            hold_violations: 0,
            capture_window_violations: 0,
            analyzed_arcs: 1,
            false_path_arcs: 0,
            extraction_report: None,
            noise_margin: None,
            path_report: None,
        };
        let report = verifier
            .verify(&netlist, &timing, &TimingConfig::default(), &TimingFunctionalConfig::default())
            .expect("verify");
        assert!(!report.passed);
        assert_eq!(report.pulse_alignment_violations, 1);
        assert_eq!(report.functional_violations[0].category, "pulse_alignment");
    }

    #[test]
    fn timing_functional_verifier_detects_hold_as_functional() {
        let verifier = TimingFunctionalVerifier::new();
        let netlist = Netlist::new();
        let from = rflux_ir::PinRef { node: rflux_ir::NodeId(0), port: 0 };
        let to = rflux_ir::PinRef { node: rflux_ir::NodeId(1), port: 0 };
        // Windows overlap, arrival inside capture window, but hold slack is negative
        let timing = TimingReport {
            arcs: vec![make_timing_arc(from, to, 0.0, 4.0, 2.0, 6.0, 3.0, 8.0, 5.0, -2.0)],
            worst_setup_slack_ps: 5.0,
            worst_hold_slack_ps: -2.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: 2.0,
            critical_path_delay_ps: 1.0,
            setup_violations: 0,
            hold_violations: 1,
            capture_window_violations: 0,
            analyzed_arcs: 1,
            false_path_arcs: 0,
            extraction_report: None,
            noise_margin: None,
            path_report: None,
        };
        let report = verifier
            .verify(&netlist, &timing, &TimingConfig::default(), &TimingFunctionalConfig::default())
            .expect("verify");
        assert!(!report.passed);
        assert_eq!(report.hold_functional_violations, 1);
        assert_eq!(report.functional_violations[0].category, "hold");
    }

    #[test]
    fn timing_functional_verifier_skips_false_paths() {
        let verifier = TimingFunctionalVerifier::new();
        let netlist = Netlist::new();
        let from = rflux_ir::PinRef { node: rflux_ir::NodeId(0), port: 0 };
        let to = rflux_ir::PinRef { node: rflux_ir::NodeId(1), port: 0 };
        let mut arc = make_timing_arc(from, to, 0.0, 4.0, 10.0, 14.0, 3.0, 12.0, 9.0, -2.0);
        arc.is_false_path = true;
        let timing = TimingReport {
            arcs: vec![arc],
            worst_setup_slack_ps: 9.0,
            worst_hold_slack_ps: -2.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: 2.0,
            critical_path_delay_ps: 3.0,
            setup_violations: 0,
            hold_violations: 1,
            capture_window_violations: 0,
            analyzed_arcs: 0,
            false_path_arcs: 1,
            extraction_report: None,
            noise_margin: None,
            path_report: None,
        };
        let report = verifier
            .verify(&netlist, &timing, &TimingConfig::default(), &TimingFunctionalConfig::default())
            .expect("verify");
        assert!(report.passed);
    }

    #[test]
    fn timing_functional_verifier_returns_error_for_empty_arcs() {
        let verifier = TimingFunctionalVerifier::new();
        let netlist = Netlist::new();
        let timing = TimingReport {
            arcs: vec![],
            worst_setup_slack_ps: 0.0,
            worst_hold_slack_ps: 0.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: 0.0,
            critical_path_delay_ps: 0.0,
            setup_violations: 0,
            hold_violations: 0,
            capture_window_violations: 0,
            analyzed_arcs: 0,
            false_path_arcs: 0,
            extraction_report: None,
            noise_margin: None,
            path_report: None,
        };
        let err = verifier
            .verify(&netlist, &timing, &TimingConfig::default(), &TimingFunctionalConfig::default())
            .unwrap_err();
        assert!(matches!(err, TimingFunctionalError::NoArcs));
    }

    #[test]
    fn timing_functional_verifier_configurable_hold_check() {
        let verifier = TimingFunctionalVerifier::new();
        let netlist = Netlist::new();
        let from = rflux_ir::PinRef { node: rflux_ir::NodeId(0), port: 0 };
        let to = rflux_ir::PinRef { node: rflux_ir::NodeId(1), port: 0 };
        let timing = TimingReport {
            arcs: vec![make_timing_arc(from, to, 0.0, 4.0, 2.0, 6.0, 3.0, 8.0, 5.0, -2.0)],
            worst_setup_slack_ps: 5.0,
            worst_hold_slack_ps: -2.0,
            total_negative_setup_slack_ps: 0.0,
            total_negative_hold_slack_ps: 2.0,
            critical_path_delay_ps: 3.0,
            setup_violations: 0,
            hold_violations: 1,
            capture_window_violations: 0,
            analyzed_arcs: 1,
            false_path_arcs: 0,
            extraction_report: None,
            noise_margin: None,
            path_report: None,
        };
        // Disable hold check
        let config = TimingFunctionalConfig {
            hold_as_functional: false,
            ..TimingFunctionalConfig::default()
        };
        let report = verifier
            .verify(&netlist, &timing, &TimingConfig::default(), &config)
            .expect("verify");
        assert!(report.passed);
        assert_eq!(report.hold_functional_violations, 0);
    }
}
