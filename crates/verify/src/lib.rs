use rflux_ir::Netlist;
use rflux_synth::{
    Compiler, EquivalenceSatProblem, SatEquivalenceReport, SequentialEquivalenceReport,
};

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
}
