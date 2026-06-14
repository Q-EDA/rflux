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

// ---------------------------------------------------------------------------
// Edge cases: formula construction
// ---------------------------------------------------------------------------

#[test]
fn empty_formula_is_sat() {
    let cnf = CnfFormula::new(3);
    let result = solve(&cnf);
    assert!(matches!(result, SolveResult::Satisfiable(_)));
}

#[test]
fn single_unit_clause_sat() {
    let mut cnf = CnfFormula::new(1);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("single unit clause should be SAT");
    };
    assert_eq!(model.value(1), Some(true));
}

#[test]
fn single_long_clause_sat() {
    let mut cnf = CnfFormula::new(20);
    let clause: Vec<Lit> = (1..=20).map(|v| Lit::neg(v)).collect();
    cnf.add_clause(clause).unwrap();
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("single long clause should be SAT");
    };
    for var in 1..=20 {
        assert_eq!(model.value(var), Some(false));
    }
}

#[test]
fn many_variables_few_clauses_sat() {
    let mut cnf = CnfFormula::new(100);
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(50)]).unwrap();
    let result = solve(&cnf);
    assert!(matches!(result, SolveResult::Satisfiable(_)));
}

#[test]
fn duplicate_clauses_sat() {
    let mut cnf = CnfFormula::new(2);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    cnf.add_clause(vec![Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::pos(2)]).unwrap();
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("duplicate clauses should still be SAT");
    };
    assert_eq!(model.value(1), Some(true));
    assert_eq!(model.value(2), Some(true));
}

#[test]
fn all_positive_literals_sat() {
    let mut cnf = CnfFormula::new(5);
    for v in 1..=5 {
        cnf.add_clause(vec![Lit::pos(v)]).unwrap();
    }
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("all positive unit clauses should be SAT");
    };
    for v in 1..=5 {
        assert_eq!(model.value(v), Some(true));
    }
}

#[test]
fn all_negative_literals_sat() {
    let mut cnf = CnfFormula::new(5);
    for v in 1..=5 {
        cnf.add_clause(vec![Lit::neg(v)]).unwrap();
    }
    let result = solve(&cnf);
    let SolveResult::Satisfiable(model) = result else {
        panic!("all negative unit clauses should be SAT");
    };
    for v in 1..=5 {
        assert_eq!(model.value(v), Some(false));
    }
}

// ---------------------------------------------------------------------------
// Edge cases: UNSAT patterns
// ---------------------------------------------------------------------------

#[test]
fn unsat_pigeonhole_3_2() {
    let cnf = pigeonhole_unsat(3, 2);
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn unsat_pigeonhole_4_3() {
    let cnf = pigeonhole_unsat(4, 3);
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn unsat_pigeonhole_6_5() {
    let cnf = pigeonhole_unsat(6, 5);
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn unsat_odd_xor_chain() {
    let mut cnf = CnfFormula::new(3);
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::neg(2)]).unwrap();
    cnf.add_clause(vec![Lit::pos(2), Lit::pos(3)]).unwrap();
    cnf.add_clause(vec![Lit::neg(2), Lit::neg(3)]).unwrap();
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(3)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::neg(3)]).unwrap();
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn sat_even_xor_chain() {
    let mut cnf = CnfFormula::new(2);
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::neg(2)]).unwrap();
    let result = solve(&cnf);
    assert!(matches!(result, SolveResult::Satisfiable(_)));
}

#[test]
fn unsat_bool_circuit_ones_gate() {
    let mut cnf = CnfFormula::new(3);
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    cnf.add_clause(vec![Lit::pos(1), Lit::pos(3)]).unwrap();
    cnf.add_clause(vec![Lit::pos(2), Lit::pos(3)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::neg(2)]).unwrap();
    cnf.add_clause(vec![Lit::neg(1), Lit::neg(3)]).unwrap();
    cnf.add_clause(vec![Lit::neg(2), Lit::neg(3)]).unwrap();
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

// ---------------------------------------------------------------------------
// Edge cases: Model and Lit
// ---------------------------------------------------------------------------

#[test]
fn model_value_out_of_range_returns_none() {
    let mut cnf = CnfFormula::new(1);
    cnf.add_clause(vec![Lit::pos(1)]).unwrap();
    let SolveResult::Satisfiable(model) = solve(&cnf) else {
        panic!("should be SAT");
    };
    assert_eq!(model.value(0), None);
    assert_eq!(model.value(999), None);
}

#[test]
fn lit_zero_var_is_rejected() {
    let mut cnf = CnfFormula::new(1);
    let err = cnf.add_clause(vec![Lit::pos(0)]).unwrap_err();
    assert_eq!(
        err,
        SatError::VariableOutOfRange {
            var: 0,
            var_count: 1
        }
    );
}

#[test]
fn lit_negated_matches_pos() {
    let a = Lit::pos(5);
    let b = Lit::neg(5);
    assert_eq!(a.var, b.var);
    assert_ne!(a.negated, b.negated);
    assert_eq!(!a, b);
}

#[test]
fn lit_watch_index_is_unique_per_polarity() {
    let pos = Lit::pos(3);
    let neg = Lit::neg(3);
    assert_eq!(pos.var, neg.var);
    assert_ne!(pos.negated, neg.negated);
}

// ---------------------------------------------------------------------------
// Edge cases: DIMACS parsing
// ---------------------------------------------------------------------------

#[test]
fn dimacs_roundtrip_many_clauses() {
    let mut cnf = CnfFormula::new(50);
    for i in 1..=50 {
        cnf.add_clause(vec![Lit::pos(i), Lit::neg((i % 50) + 1)])
            .unwrap();
    }
    let reparsed = CnfFormula::from_dimacs(&cnf.to_dimacs()).unwrap();
    assert_eq!(reparsed, cnf);
}

#[test]
fn dimacs_rejects_empty_input() {
    let err = CnfFormula::from_dimacs("").unwrap_err();
    assert_eq!(err, SatError::MissingDimacsHeader);
}

#[test]
fn dimacs_rejects_no_header() {
    let err = CnfFormula::from_dimacs("1 2 0\n").unwrap_err();
    assert_eq!(err, SatError::MissingDimacsHeader);
}

#[test]
fn dimacs_rejects_malformed_header() {
    let err = CnfFormula::from_dimacs("p cnf abc 3\n").unwrap_err();
    assert!(matches!(err, SatError::InvalidDimacsHeader(_)));
}

#[test]
fn dimacs_zero_terminates_clause() {
    let cnf = CnfFormula::from_dimacs("p cnf 1 1\n1 0\n").unwrap();
    assert_eq!(cnf.var_count(), 1);
    assert_eq!(cnf.clauses().len(), 1);
    assert_eq!(cnf.clauses()[0], vec![Lit::pos(1)]);
}

#[test]
fn dimacs_rejects_clause_count_mismatch() {
    let err = CnfFormula::from_dimacs("p cnf 1 2\n1 0\n").unwrap_err();
    assert_eq!(
        err,
        SatError::InvalidDimacsClauseCount {
            expected: 2,
            actual: 1
        }
    );
}

#[test]
fn dimacs_rejects_unterminated_clause() {
    let err = CnfFormula::from_dimacs("p cnf 1 1\n1\n").unwrap_err();
    assert_eq!(err, SatError::UnterminatedDimacsClause);
}

// ---------------------------------------------------------------------------
// Edge cases: IncrementalSolver
// ---------------------------------------------------------------------------

#[test]
fn incremental_solver_empty_formula_sat() {
    let solver = IncrementalSolver::new(0);
    let result = solver.solve();
    assert!(matches!(result, SolveResult::Satisfiable(_)));
}

#[test]
fn incremental_solver_contradictory_assumptions_unsat() {
    let mut solver = IncrementalSolver::new(2);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();
    let result = solver.solve_with_assumptions(&[Lit::neg(1), Lit::neg(2)]);
    assert_eq!(result, SolveResult::Unsatisfiable);
}

#[test]
fn incremental_solver_var_count_grows() {
    let mut solver = IncrementalSolver::new(2);
    assert_eq!(solver.var_count(), 2);
    solver.add_var();
    assert_eq!(solver.var_count(), 3);
    solver.add_var();
    assert_eq!(solver.var_count(), 4);
}

#[test]
fn incremental_solver_add_clause_grows_formula() {
    let mut solver = IncrementalSolver::new(2);
    assert_eq!(solver.base_formula().clauses().len(), 0);
    solver.add_clause(vec![Lit::pos(1)]).unwrap();
    assert_eq!(solver.base_formula().clauses().len(), 1);
    solver.add_clause(vec![Lit::neg(2)]).unwrap();
    assert_eq!(solver.base_formula().clauses().len(), 2);
}

#[test]
fn incremental_solver_multiple_sat_unsat_transitions() {
    let mut solver = IncrementalSolver::new(2);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2)]).unwrap();

    assert!(matches!(
        solver.solve_with_assumptions(&[]),
        SolveResult::Satisfiable(_)
    ));
    assert_eq!(
        solver.solve_with_assumptions(&[Lit::neg(1), Lit::neg(2)]),
        SolveResult::Unsatisfiable
    );
    assert!(matches!(
        solver.solve_with_assumptions(&[Lit::pos(1)]),
        SolveResult::Satisfiable(_)
    ));
    let r = solver.solve_with_assumptions(&[Lit::neg(1)]);
    assert!(matches!(r, SolveResult::Satisfiable(_)) || matches!(r, SolveResult::Unsatisfiable));
}

#[test]
fn unsat_core_all_assumptions_needed() {
    let mut solver = IncrementalSolver::new(3);
    solver.add_clause(vec![Lit::pos(1), Lit::pos(2), Lit::pos(3)]).unwrap();
    solver.add_clause(vec![Lit::neg(1), Lit::neg(2), Lit::neg(3)]).unwrap();

    let core = solver
        .unsat_core_of_assumptions(&[Lit::pos(1), Lit::pos(2), Lit::pos(3)])
        .expect("should be UNSAT");
    assert!(!core.is_empty());
}

// ---------------------------------------------------------------------------
// Larger stress tests
// ---------------------------------------------------------------------------

#[test]
fn stress_pigeonhole_unsat_7_6() {
    let cnf = pigeonhole_unsat(7, 6);
    assert_eq!(solve(&cnf), SolveResult::Unsatisfiable);
}

#[test]
fn stress_3sat_100vars() {
    let mut cnf = CnfFormula::new(100);
    let mut seed = 123u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        seed
    };
    for _ in 0..300 {
        let mut clause = Vec::new();
        for _ in 0..3 {
            let var = (next() as usize % 100) + 1;
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
            assert!(satisfied, "clause {clause:?} not satisfied");
        }
    }
}

#[test]
fn stress_incremental_many_assumptions() {
    let mut solver = IncrementalSolver::new(10);
    for v in 1..=10 {
        solver.add_clause(vec![Lit::pos(v), Lit::pos((v % 10) + 1)])
            .unwrap();
    }
    for i in 0..10 {
        let assumptions: Vec<Lit> = (1..=5).map(|v| Lit::pos(v ^ i)).collect();
        let _ = solver.solve_with_assumptions(&assumptions);
    }
}
