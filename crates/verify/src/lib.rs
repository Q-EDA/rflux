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
        assert!(report.sat_stats.recursive_calls >= 1);
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

        let report = verifier
            .check_single_step_sequential_equivalence(&lhs, &rhs)
            .expect("single-step sequential equivalence should run");

        assert!(!report.equivalent);
        assert!(report.counterexample_states.is_some());
        assert!(report.sat_stats.recursive_calls >= 1);
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
        assert!(report.sat_stats.recursive_calls >= 1);
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
}
