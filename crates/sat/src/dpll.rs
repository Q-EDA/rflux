//! Legacy DPLL solver, retained as a differential-testing backend.
//!
//! This is the original recursive DPLL with pure-literal elimination that
//! shipped as `rflux-sat`'s only solver before the CDCL engine. It is compiled
//! only under the `dpll` Cargo feature, so default builds of `rflux-sat` carry
//! no DPLL code at all; the feature exists purely so that tests can run both
//! engines against the same CNF and assert identical SAT/UNSAT outcomes.
//!
//! The implementation is deliberately left byte-for-byte equivalent to the
//! pre-CDCL `lib.rs` (minus the `analyze_conflict`/`add_learned_clause` stubs,
//! which were dead code that pretended to do clause learning but actually
//! returned `vec![¬trail.last()]`). Keeping the legacy behavior unchanged is
//! what makes it a trustworthy differential reference.

#![cfg(feature = "dpll")]

use crate::types::{CnfFormula, Lit, Model, SolveResult, SolveStats};

const MAX_RESTARTS: usize = 6;

enum SearchOutcome {
    Satisfiable,
    Unsatisfiable,
    BudgetExhausted,
}

/// Solve `formula` under `assumptions` using legacy DPLL.
///
/// Returns the result plus per-call stats. This entry point mirrors the
/// historical `solve_with_metrics` behavior (clone formula, add assumptions as
/// unit clauses, run DPLL with restarts) and is used only by the differential
/// test harness in `tests/differential.rs`.
pub(crate) fn solve_dpll(
    formula: &CnfFormula,
    assumptions: &[Lit],
) -> (SolveResult, SolveStats) {
    let mut working_formula = formula.clone();
    for assumption in assumptions {
        working_formula
            .add_clause(vec![*assumption])
            .expect("assumptions should use in-range variables");
    }

    let mut stats = SolveStats::default();
    let mut phase_hints = vec![true; working_formula.var_count() + 1];
    let mut decision_budget = initial_decision_budget(&working_formula);
    let mut result = None::<SolveResult>;

    for _ in 0..MAX_RESTARTS {
        let mut values = vec![None; working_formula.var_count() + 1];
        let mut budget = Some(decision_budget);
        match dpll(
            &mut working_formula,
            &mut values,
            &mut phase_hints,
            &mut stats,
            &mut budget,
        ) {
            SearchOutcome::Satisfiable => {
                result = Some(SolveResult::Satisfiable(Model::from_values(values)));
                break;
            }
            SearchOutcome::Unsatisfiable => {
                result = Some(SolveResult::Unsatisfiable);
                break;
            }
            SearchOutcome::BudgetExhausted => {
                stats.restarts += 1;
                decision_budget = decision_budget.saturating_mul(2);
            }
        }
    }

    let result = if let Some(result) = result {
        result
    } else {
        let mut values = vec![None; working_formula.var_count() + 1];
        let mut unlimited_budget = None;
        match dpll(
            &mut working_formula,
            &mut values,
            &mut phase_hints,
            &mut stats,
            &mut unlimited_budget,
        ) {
            SearchOutcome::Satisfiable => SolveResult::Satisfiable(Model::from_values(values)),
            SearchOutcome::Unsatisfiable | SearchOutcome::BudgetExhausted => {
                SolveResult::Unsatisfiable
            }
        }
    };

    (result, stats)
}

fn initial_decision_budget(formula: &CnfFormula) -> usize {
    formula.var_count() * 10
}

fn dpll(
    formula: &mut CnfFormula,
    values: &mut Vec<Option<bool>>,
    phase_hints: &mut [bool],
    stats: &mut SolveStats,
    decision_budget: &mut Option<usize>,
) -> SearchOutcome {
    let mut trail = Vec::new();

    stats.recursive_calls += 1;
    if !propagate(formula, values, phase_hints, stats) {
        stats.backtracks += 1;
        return SearchOutcome::Unsatisfiable;
    }

    if all_clauses_satisfied(formula, values) {
        return SearchOutcome::Satisfiable;
    }

    let Some(branch_var) = choose_unassigned_var(formula, values) else {
        return SearchOutcome::Satisfiable;
    };

    if let Some(budget) = decision_budget.as_mut() {
        if *budget == 0 {
            return SearchOutcome::BudgetExhausted;
        }
        *budget -= 1;
    }

    let preferred = phase_hints[branch_var];
    let trials = [preferred, !preferred];

    for trial in trials {
        stats.decisions += 1;
        let mut branch_values = values.clone();
        if assign_with_phase(&mut branch_values, phase_hints, branch_var, trial) {
            trail.push(branch_var);
            match dpll(
                formula,
                &mut branch_values,
                phase_hints,
                stats,
                decision_budget,
            ) {
                SearchOutcome::Satisfiable => {
                    *values = branch_values;
                    return SearchOutcome::Satisfiable;
                }
                SearchOutcome::Unsatisfiable => {}
                SearchOutcome::BudgetExhausted => return SearchOutcome::BudgetExhausted,
            }
            trail.pop();
        }
    }

    stats.backtracks += 1;
    SearchOutcome::Unsatisfiable
}

fn propagate(
    formula: &mut CnfFormula,
    values: &mut [Option<bool>],
    phase_hints: &mut [bool],
    stats: &mut SolveStats,
) -> bool {
    loop {
        let mut changed = false;

        let unit_changed = match unit_propagate_once(formula, values, phase_hints, stats) {
            Ok(changed) => changed,
            Err(()) => return false,
        };
        changed |= unit_changed;

        let pure_changed = match pure_literal_eliminate_once(formula, values, phase_hints, stats) {
            Ok(changed) => changed,
            Err(()) => return false,
        };
        changed |= pure_changed;

        if !changed {
            return true;
        }
    }
}

fn unit_propagate_once(
    formula: &mut CnfFormula,
    values: &mut [Option<bool>],
    phase_hints: &mut [bool],
    stats: &mut SolveStats,
) -> Result<bool, ()> {
    let mut changed = false;

    for clause in formula.clauses() {
        let mut has_satisfied = false;
        let mut last_unassigned = None::<Lit>;
        let mut unassigned_count = 0usize;

        for lit in clause {
            match lit.eval(values[lit.var]) {
                Some(true) => {
                    has_satisfied = true;
                    break;
                }
                Some(false) => {}
                None => {
                    unassigned_count += 1;
                    last_unassigned = Some(*lit);
                }
            }
        }

        if has_satisfied {
            continue;
        }

        if unassigned_count == 0 {
            return Err(());
        }

        if unassigned_count == 1 {
            let lit = last_unassigned.expect("unit literal must exist");
            if !assign_with_phase(values, phase_hints, lit.var, lit.required_value()) {
                return Err(());
            }
            stats.unit_assignments += 1;
            changed = true;
        }
    }

    Ok(changed)
}

fn pure_literal_eliminate_once(
    formula: &mut CnfFormula,
    values: &mut [Option<bool>],
    phase_hints: &mut [bool],
    stats: &mut SolveStats,
) -> Result<bool, ()> {
    let var_count = values.len().saturating_sub(1);
    let mut pos_seen = vec![false; var_count + 1];
    let mut neg_seen = vec![false; var_count + 1];

    for clause in formula.clauses() {
        if clause_satisfied(clause, values) {
            continue;
        }

        let mut any_unassigned = false;
        for lit in clause {
            if values[lit.var].is_some() {
                continue;
            }
            any_unassigned = true;
            if lit.negated {
                neg_seen[lit.var] = true;
            } else {
                pos_seen[lit.var] = true;
            }
        }

        if !any_unassigned {
            return Err(());
        }
    }

    let mut changed = false;
    for var in 1..=var_count {
        if values[var].is_some() {
            continue;
        }
        match (pos_seen[var], neg_seen[var]) {
            (true, false) => {
                if !assign_with_phase(values, phase_hints, var, true) {
                    return Err(());
                }
                stats.pure_literal_assignments += 1;
                changed = true;
            }
            (false, true) => {
                if !assign_with_phase(values, phase_hints, var, false) {
                    return Err(());
                }
                stats.pure_literal_assignments += 1;
                changed = true;
            }
            _ => {}
        }
    }

    Ok(changed)
}

fn assign_with_phase(
    values: &mut [Option<bool>],
    phase_hints: &mut [bool],
    var: usize,
    value: bool,
) -> bool {
    phase_hints[var] = value;
    assign(values, var, value)
}

fn assign(values: &mut [Option<bool>], var: usize, value: bool) -> bool {
    if let Some(existing) = values[var] {
        existing == value
    } else {
        values[var] = Some(value);
        true
    }
}

fn all_clauses_satisfied(formula: &mut CnfFormula, values: &[Option<bool>]) -> bool {
    formula
        .clauses()
        .iter()
        .all(|clause| clause_satisfied(clause, values))
}

fn clause_satisfied(clause: &[Lit], values: &[Option<bool>]) -> bool {
    clause
        .iter()
        .any(|lit| matches!(lit.eval(values[lit.var]), Some(true)))
}

fn choose_unassigned_var(formula: &mut CnfFormula, values: &[Option<bool>]) -> Option<usize> {
    (1..=formula.var_count()).find(|&var| values[var].is_none())
}
