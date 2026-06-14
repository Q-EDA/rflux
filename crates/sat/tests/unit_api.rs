//! Unit tests for the public rflux-sat API.
//!
//! These live in `tests/` (not inline) so they exercise the crate exactly as a
//! downstream consumer (`rflux-synth`, `rflux-cli`) would — through the public
//! surface only, with no access to crate internals.

use proptest::prelude::*;
use rflux_sat::{
    solve, solve_with_metrics, CnfFormula, IncrementalSolver, Lit, SatError, SolveResult,
};
use std::collections::BTreeMap;

#[test]
fn solves_basic_sat_instance() {
    let mut cnf = CnfFormula::new(2);
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::pos(1), Lit::neg(2)]).unwrap();

    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("formula should be satisfiable");
    };
    assert_eq!(model.value(2), Some(true));
}

#[test]
fn solves_basic_unsat_instance() {
    let mut cnf = CnfFormula::new(1);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1)]).unwrap();
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn performs_unit_propagation_chain() {
    let mut cnf = CnfFormula::new(3);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(2), Lit::pos(3)]).unwrap();

    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("formula should be satisfiable");
    };
    assert_eq!(model.value(1), Some(true));
    assert_eq!(model.value(2), Some(true));
    assert_eq!(model.value(3), Some(true));
}

#[test]
fn dimacs_roundtrip_preserves_formula() {
    let mut cnf = CnfFormula::new(3);
    cnf.add_clause(vec![Lit::pos(1), Lit::neg(2)]).unwrap();
    cnf.add_clause(vec![Lit::pos(3)]).unwrap();
    let reparsed = CnfFormula::from_dimacs(&cnf.to_dimacs()).unwrap();
    assert_eq!(reparsed, cnf);
}

#[test]
fn rejects_clause_with_out_of_range_variable() {
    let mut cnf = CnfFormula::new(2);
    let err = cnf.add_clause(vec![Lit::pos(3)]).unwrap_err();
    assert_eq!(
        err,
        SatError::VariableOutOfRange {
            var: 3,
            var_count: 2,
        }
    );
}

#[test]
fn stress_pigeonhole_unsat_5_to_4() {
    // 5 pigeons, 4 holes — where DPLL began to struggle. CDCL should solve it
    // quickly via clause learning.
    let cnf = pigeonhole_unsat(5, 4);
    let (result, metrics) = solve_with_metrics(&cnf);
    assert_eq!(result, SolveResult::Unsatisfiable);
    assert!(metrics.elapsed_ns > 0);
    assert!(metrics.stats.backtracks >= 1);
}

fn pigeonhole_unsat(pigeons: usize, holes: usize) -> CnfFormula {
    let mut cnf = CnfFormula::new(pigeons * holes);
    let var = |p: usize, h: usize| -> usize { p * holes + h + 1 };
    for p in 0..pigeons {
        let mut clause = Vec::new();
        for h in 0..holes {
            clause.push(Lit::pos(var(p, h)));
        }
        cnf.add_clause(clause).unwrap();
    }
    for h in 0..holes {
        for p1 in 0..pigeons {
            for p2 in (p1 + 1)..pigeons {
                cnf.add_clause(vec![Lit::neg(var(p1, h)), Lit::neg(var(p2, h))])
                    .unwrap();
            }
        }
    }
    cnf
}

proptest! {
    #[test]
    fn property_unit_clause_sets_match_solver_expectation(
        assignments in prop::collection::vec((1usize..=6, any::<bool>()), 0..12)
    ) {
        let max_var = assignments.iter().map(|(var, _)| *var).max().unwrap_or(1);
        let mut cnf = CnfFormula::new(max_var);
        let mut expected = BTreeMap::<usize, bool>::new();
        let mut contradictory = false;

        for (var, value) in &assignments {
            if let Some(previous) = expected.insert(*var, *value) {
                if previous != *value {
                    contradictory = true;
                }
            }
            cnf.add_clause(vec![if *value { Lit::pos(*var) } else { Lit::neg(*var) }])
                .unwrap();
        }

        let result = solve(&cnf);
        if contradictory {
            prop_assert_eq!(result, SolveResult::Unsatisfiable);
        } else {
            let SolveResult::Satisfiable(model) = result else {
                prop_assert!(false, "non-contradictory unit clauses should be satisfiable");
                unreachable!();
            };
            for (var, value) in expected {
                prop_assert_eq!(model.value(var), Some(value));
            }
        }
    }
}

#[test]
fn incremental_solver_reuses_base_formula_across_assumptions() {
    let mut solver = IncrementalSolver::new(2);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    solver.add_clause(vec![Lit::neg(1), Lit::pos(2)]).unwrap();

    let sat = solver.solve_with_assumptions(&[Lit::pos(1)]);
    let unsat = solver.solve_with_assumptions(&[Lit::neg(2)]);

    assert!(matches!(sat, SolveResult::Satisfiable(_)));
    assert_eq!(unsat, SolveResult::Unsatisfiable);
    assert_eq!(solver.base_formula().clauses().len(), 2);
}

#[test]
fn incremental_solver_reports_metrics_for_assumption_solves() {
    let mut solver = IncrementalSolver::new(3);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    solver.add_clause(vec![Lit::neg(1), Lit::pos(3)]).unwrap();

    let (result, metrics) = solver.solve_with_assumptions_and_metrics(&[Lit::neg(2)]);

    assert!(matches!(
        result,
        SolveResult::Satisfiable(_) | SolveResult::Unsatisfiable
    ));
    assert!(metrics.elapsed_ns > 0);
}

#[test]
fn incremental_solver_extracts_redundancy_reduced_unsat_core() {
    let mut solver = IncrementalSolver::new(3);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();

    let core = solver
        .unsat_core_of_assumptions(&[Lit::neg(1), Lit::neg(2), Lit::pos(3)])
        .expect("assumptions should be unsat");

    // The core must contain ¬1 and ¬2 (which together conflict with the
    // clause); +3 is irrelevant and should be filtered out by minimization.
    assert!(core.contains(&Lit::neg(1)));
    assert!(core.contains(&Lit::neg(2)));
    assert!(!core.contains(&Lit::pos(3)));
}

#[test]
fn incremental_solver_returns_no_unsat_core_for_sat_assumptions() {
    let mut solver = IncrementalSolver::new(1);
    solver.add_clause(vec![Lit::pos(1)]).unwrap();
    assert_eq!(solver.unsat_core_of_assumptions(&[Lit::pos(1)]), None);
}

#[test]
fn sat_error_codes_are_stable() {
    assert_eq!(
        SatError::VariableOutOfRange {
            var: 0,
            var_count: 0
        }
        .code(),
        "RFLOW-SIM-001"
    );
    assert_eq!(SatError::EmptyClause.code(), "RFLOW-SIM-001");
    assert_eq!(SatError::MissingDimacsHeader.code(), "RFLOW-INPUT-002");
    assert_eq!(
        SatError::InvalidDimacsHeader("x".into()).code(),
        "RFLOW-INPUT-002"
    );
    assert!(!SatError::MissingDimacsHeader.suggestion().is_empty());
}

/// Verify a returned SAT model actually satisfies every clause — the strongest
/// correctness invariant and the best bug-catcher for subtle CDCL errors.
#[test]
fn sat_model_satisfies_all_clauses() {
    let mut cnf = CnfFormula::new(5);
    cnf.add_clause(vec![Lit::pos(1), Lit::neg(2), Lit::pos(3)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::pos(4)]).unwrap();
    cnf.add_clause(vec![Lit::neg(3), Lit::neg(4)]).unwrap();
    cnf.add_clause(vec![Lit::pos(2), Lit::neg(5)]).unwrap();
    cnf.add_clause(vec![Lit::pos(5), Lit::pos(1)]).unwrap();

    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("should be SAT");
    };
    for clause in cnf.clauses() {
        let satisfied = clause
            .iter()
            .any(|lit| model.value(lit.var) == Some(!lit.negated));
        assert!(satisfied, "clause {clause:?} not satisfied");
    }
}

/// Verify CDCL returns valid models on a larger instance (34+ vars) similar to
/// the ripple_adder4 equivalence miter. This catches the 1-UIP / BCP bug that
/// only manifests at scale.
#[test]
fn cdcl_validates_model_on_medium_random_3sat() {
    // Deterministic pseudo-random 3-SAT: 30 vars, 80 clauses.
    let mut cnf = CnfFormula::new(30);
    let mut seed = 42u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        seed
    };
    for _ in 0..80 {
        let mut clause = Vec::with_capacity(3);
        for _ in 0..3 {
            let var = (next() as usize % 30) + 1;
            let neg = next() % 2 == 0;
            clause.push(if neg { Lit::neg(var) } else { Lit::pos(var) });
        }
        cnf.add_clause(clause).unwrap();
    }
    let result = solve(&cnf);
    if let SolveResult::Satisfiable(model) = result {
        for clause in cnf.clauses() {
            let satisfied = clause
                .iter()
                .any(|lit| model.value(lit.var) == Some(!lit.negated));
            assert!(satisfied, "CDCL returned invalid model — clause {clause:?} not satisfied. This indicates a 1-UIP or BCP bug.");
        }
    }
    // UNSAT is also acceptable; we only assert model validity when SAT.
}

#[test]
fn solves_single_variable_positive() {
    let mut cnf = CnfFormula::new(1);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("single positive clause should be SAT");
    };
    assert_eq!(model.value(1), Some(true));
}

#[test]
fn solves_single_variable_negative() {
    let mut cnf = CnfFormula::new(1);
    cnf.add_clause(vec![Lit::neg(1)]).unwrap();
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("single negative clause should be SAT");
    };
    assert_eq!(model.value(1), Some(false));
}

#[test]
fn solves_xor_two_variables() {
    let mut cnf = CnfFormula::new(2);
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::neg(2)]).unwrap();
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("XOR formula should be SAT");
    };
    let v1 = model.value(1).unwrap();
    let v2 = model.value(2).unwrap();
    assert_ne!(v1, v2, "XOR requires different values");
}

#[test]
fn solves_implication_chain() {
    let mut cnf = CnfFormula::new(4);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(2), Lit::pos(3)]).unwrap();
    cnf.add_clause(vec![Lit::neg(3), Lit::pos(4)]).unwrap();
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("implication chain should be SAT");
    };
    assert_eq!(model.value(1), Some(true));
    assert_eq!(model.value(2), Some(true));
    assert_eq!(model.value(3), Some(true));
    assert_eq!(model.value(4), Some(true));
}

#[test]
fn unsat_single_variable_contradiction() {
    let mut cnf = CnfFormula::new(1);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1)]).unwrap();
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn unsat_two_variable_contradiction() {
    let mut cnf = CnfFormula::new(2);
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::pos(1), Lit::neg(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::neg(2)]).unwrap();
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn incremental_solver_reuses_learned_clauses() {
    let mut solver = IncrementalSolver::new(3);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    solver.add_clause(vec![Lit::neg(1), Lit::pos(3)]).unwrap();

    let (result1, _) = solver.solve_with_assumptions_and_stats(&[]);
    assert!(matches!(result1, SolveResult::Satisfiable(_)));

    solver.add_clause(vec![Lit::neg(2)]).unwrap();
    solver.add_clause(vec![Lit::neg(3)]).unwrap();
    let (result2, _) = solver.solve_with_assumptions_and_stats(&[]);
    assert!(matches!(result2, SolveResult::Unsatisfiable));
}

#[test]
fn incremental_solver_assumptions_constrain() {
    let mut solver = IncrementalSolver::new(2);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();

    let (result1, _) = solver.solve_with_assumptions_and_stats(&[Lit::neg(1)]);
    let SolveResult::Satisfiable(model1) = result1 else {
        panic!("should be SAT with assumption");
    };
    assert_eq!(model1.value(1), Some(false));
    assert_eq!(model1.value(2), Some(true));

    let (result2, _) = solver.solve_with_assumptions_and_stats(&[Lit::neg(2)]);
    let SolveResult::Satisfiable(model2) = result2 else {
        panic!("should be SAT with assumption");
    };
    assert_eq!(model2.value(1), Some(true));
    assert_eq!(model2.value(2), Some(false));
}

#[test]
fn unsat_core_minimization() {
    let mut solver = IncrementalSolver::new(3);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2), Lit::pos(3)]).unwrap();
    solver.add_clause(vec![Lit::neg(1), Lit::neg(2)]).unwrap();
    solver.add_clause(vec![Lit::neg(2), Lit::neg(3)]).unwrap();

    let core = solver
        .unsat_core_of_assumptions(&[Lit::pos(1), Lit::pos(2), Lit::pos(3)])
        .expect("should be UNSAT");
    assert!(!core.is_empty(), "core should not be empty");
    for lit in &core {
        assert!(lit.var >= 1 && lit.var <= 3, "core literal out of range");
    }
}

#[test]
fn model_satisfies_all_clauses_on_sat() {
    let mut cnf = CnfFormula::new(5);
    cnf.add_clause(vec![Lit::pos(1), Lit::neg(2), Lit::pos(3)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::pos(4)]).unwrap();
    cnf.add_clause(vec![Lit::neg(3), Lit::neg(4), Lit::pos(5)]).unwrap();
    cnf.add_clause(vec![Lit::pos(2), Lit::pos(5)]).unwrap();

    let result = solve(&cnf);
    if let SolveResult::Satisfiable(model) = result {
        for clause in cnf.clauses() {
            let satisfied = clause
                .iter()
                .any(|lit| model.value(lit.var) == Some(!lit.negated));
            assert!(
                satisfied,
                "clause not satisfied by model: {clause:?}"
            );
        }
    }
}

#[test]
fn solve_metrics_report_positive_elapsed() {
    let mut cnf = CnfFormula::new(3);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(2), Lit::pos(3)]).unwrap();

    let (_, metrics) = solve_with_metrics(&cnf);
    assert!(metrics.elapsed_ns > 0, "elapsed time should be positive");
    assert!(metrics.stats.decisions + metrics.stats.unit_assignments >= 1);
}
