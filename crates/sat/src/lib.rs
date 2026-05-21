use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lit {
    pub var: usize,
    pub negated: bool,
}

impl Lit {
    pub fn pos(var: usize) -> Self {
        Self {
            var,
            negated: false,
        }
    }

    pub fn neg(var: usize) -> Self {
        Self { var, negated: true }
    }

    fn eval(self, assignment: Option<bool>) -> Option<bool> {
        assignment.map(|value| if self.negated { !value } else { value })
    }

    fn required_value(self) -> bool {
        !self.negated
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CnfFormula {
    var_count: usize,
    clauses: Vec<Vec<Lit>>,
}

impl CnfFormula {
    pub fn new(var_count: usize) -> Self {
        Self {
            var_count,
            clauses: Vec::new(),
        }
    }

    pub fn add_var(&mut self) -> usize {
        self.var_count += 1;
        self.var_count
    }

    pub fn var_count(&self) -> usize {
        self.var_count
    }

    pub fn clauses(&self) -> &[Vec<Lit>] {
        &self.clauses
    }

    pub fn add_clause(&mut self, clause: Vec<Lit>) -> Result<(), SatError> {
        if clause.is_empty() {
            return Err(SatError::EmptyClause);
        }
        for lit in &clause {
            if lit.var == 0 || lit.var > self.var_count {
                return Err(SatError::VariableOutOfRange {
                    var: lit.var,
                    var_count: self.var_count,
                });
            }
        }
        self.clauses.push(clause);
        Ok(())
    }

    pub fn from_dimacs(input: &str) -> Result<Self, SatError> {
        let mut var_count = None::<usize>;
        let mut formula = None::<CnfFormula>;

        for raw_line in input.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('c') {
                continue;
            }

            if line.starts_with('p') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() != 4 || parts[1] != "cnf" {
                    return Err(SatError::InvalidDimacsHeader(line.to_string()));
                }
                let parsed_var_count = parts[2]
                    .parse::<usize>()
                    .map_err(|_| SatError::InvalidDimacsHeader(line.to_string()))?;
                var_count = Some(parsed_var_count);
                formula = Some(CnfFormula::new(parsed_var_count));
                continue;
            }

            let Some(ref mut cnf) = formula else {
                return Err(SatError::MissingDimacsHeader);
            };

            let mut clause = Vec::new();
            for token in line.split_whitespace() {
                let lit = token
                    .parse::<i32>()
                    .map_err(|_| SatError::InvalidDimacsLiteral(token.to_string()))?;
                if lit == 0 {
                    break;
                }
                let var = lit.unsigned_abs() as usize;
                if var == 0 || var > cnf.var_count {
                    return Err(SatError::VariableOutOfRange {
                        var,
                        var_count: cnf.var_count,
                    });
                }
                clause.push(if lit > 0 { Lit::pos(var) } else { Lit::neg(var) });
            }

            if clause.is_empty() {
                return Err(SatError::EmptyClause);
            }
            cnf.add_clause(clause)?;
        }

        let Some(cnf) = formula else {
            return Err(SatError::MissingDimacsHeader);
        };

        if cnf.var_count != var_count.unwrap_or(0) {
            return Err(SatError::MissingDimacsHeader);
        }

        Ok(cnf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    values: Vec<Option<bool>>,
}

impl Model {
    pub fn value(&self, var: usize) -> Option<bool> {
        self.values.get(var).copied().flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveResult {
    Satisfiable(Model),
    Unsatisfiable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SolveStats {
    pub recursive_calls: usize,
    pub decisions: usize,
    pub unit_assignments: usize,
    pub pure_literal_assignments: usize,
    pub backtracks: usize,
    pub restarts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SolveMetrics {
    pub stats: SolveStats,
    pub elapsed_ns: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatError {
    VariableOutOfRange { var: usize, var_count: usize },
    EmptyClause,
    MissingDimacsHeader,
    InvalidDimacsHeader(String),
    InvalidDimacsLiteral(String),
}

pub fn solve(formula: &CnfFormula) -> SolveResult {
    solve_with_stats(formula).0
}

pub fn solve_with_stats(formula: &CnfFormula) -> (SolveResult, SolveStats) {
    let (result, metrics) = solve_with_metrics(formula);
    (result, metrics.stats)
}

pub fn solve_with_metrics(formula: &CnfFormula) -> (SolveResult, SolveMetrics) {
    let started = Instant::now();
    let mut stats = SolveStats::default();
    let mut phase_hints = vec![true; formula.var_count() + 1];
    let mut decision_budget = initial_decision_budget(formula);
    let mut result = None::<SolveResult>;

    for _ in 0..MAX_RESTARTS {
        let mut values = vec![None; formula.var_count() + 1];
        let mut budget = Some(decision_budget);
        match dpll(
            formula,
            &mut values,
            &mut phase_hints,
            &mut stats,
            &mut budget,
        ) {
            SearchOutcome::Satisfiable => {
                result = Some(SolveResult::Satisfiable(Model { values }));
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
        let mut values = vec![None; formula.var_count() + 1];
        let mut unlimited_budget = None;
        match dpll(
            formula,
            &mut values,
            &mut phase_hints,
            &mut stats,
            &mut unlimited_budget,
        ) {
            SearchOutcome::Satisfiable => SolveResult::Satisfiable(Model { values }),
            SearchOutcome::Unsatisfiable | SearchOutcome::BudgetExhausted => {
                SolveResult::Unsatisfiable
            }
        }
    };

    (
        result,
        SolveMetrics {
            stats,
            elapsed_ns: started.elapsed().as_nanos(),
        },
    )
}

const MAX_RESTARTS: usize = 6;

fn initial_decision_budget(formula: &CnfFormula) -> usize {
    formula.clauses().len().max(64)
}

enum SearchOutcome {
    Satisfiable,
    Unsatisfiable,
    BudgetExhausted,
}

fn dpll(
    formula: &CnfFormula,
    values: &mut Vec<Option<bool>>,
    phase_hints: &mut [bool],
    stats: &mut SolveStats,
    decision_budget: &mut Option<usize>,
) -> SearchOutcome {
    stats.recursive_calls += 1;
    if !propagate(formula, values, phase_hints, stats) {
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
        }
    }

    stats.backtracks += 1;
    SearchOutcome::Unsatisfiable
}

fn propagate(
    formula: &CnfFormula,
    values: &mut Vec<Option<bool>>,
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
    formula: &CnfFormula,
    values: &mut Vec<Option<bool>>,
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
    formula: &CnfFormula,
    values: &mut Vec<Option<bool>>,
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

#[cfg(test)]
fn unit_propagate(formula: &CnfFormula, values: &mut Vec<Option<bool>>) -> bool {
    let mut stats = SolveStats::default();
    let mut phase_hints = vec![true; values.len()];
    loop {
        let changed = match unit_propagate_once(formula, values, &mut phase_hints, &mut stats) {
            Ok(changed) => changed,
            Err(()) => return false,
        };

        if !changed {
            return true;
        }
    }
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
    match values[var] {
        Some(existing) => existing == value,
        None => {
            values[var] = Some(value);
            true
        }
    }
}

fn all_clauses_satisfied(formula: &CnfFormula, values: &[Option<bool>]) -> bool {
    formula.clauses().iter().all(|clause| {
        clause
            .iter()
            .any(|lit| matches!(lit.eval(values[lit.var]), Some(true)))
    })
}

fn clause_satisfied(clause: &[Lit], values: &[Option<bool>]) -> bool {
    clause
        .iter()
        .any(|lit| matches!(lit.eval(values[lit.var]), Some(true)))
}

fn choose_unassigned_var(formula: &CnfFormula, values: &[Option<bool>]) -> Option<usize> {
    let var_count = formula.var_count();
    let mut score = vec![0usize; var_count + 1];

    for clause in formula.clauses() {
        if clause_satisfied(clause, values) {
            continue;
        }
        for lit in clause {
            if values[lit.var].is_none() {
                score[lit.var] += 1;
            }
        }
    }

    let mut best_var = None::<usize>;
    let mut best_score = 0usize;
    for var in 1..=var_count {
        if values[var].is_some() {
            continue;
        }
        if score[var] >= best_score {
            best_score = score[var];
            best_var = Some(var);
        }
    }

    best_var
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_basic_sat_instance() {
        let mut cnf = CnfFormula::new(2);
        cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)])
            .expect("valid clause");
        cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)])
            .expect("valid clause");
        cnf.add_clause(vec![Lit::pos(1), Lit::neg(2)])
            .expect("valid clause");

        let result = solve(&cnf);
        let SolveResult::Satisfiable(model) = result else {
            panic!("formula should be satisfiable");
        };

        assert_eq!(model.value(2), Some(true));
    }

    #[test]
    fn solves_basic_unsat_instance() {
        let mut cnf = CnfFormula::new(1);
        cnf.add_clause(vec![Lit::pos(1)]).expect("valid clause");
        cnf.add_clause(vec![Lit::neg(1)]).expect("valid clause");

        let result = solve(&cnf);
        assert_eq!(result, SolveResult::Unsatisfiable);
    }

    #[test]
    fn performs_unit_propagation_chain() {
        let mut cnf = CnfFormula::new(3);
        cnf.add_clause(vec![Lit::pos(1)]).expect("valid clause");
        cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)])
            .expect("valid clause");
        cnf.add_clause(vec![Lit::neg(2), Lit::pos(3)])
            .expect("valid clause");

        let result = solve(&cnf);
        let SolveResult::Satisfiable(model) = result else {
            panic!("formula should be satisfiable");
        };

        assert_eq!(model.value(1), Some(true));
        assert_eq!(model.value(2), Some(true));
        assert_eq!(model.value(3), Some(true));
    }

    #[test]
    fn parses_dimacs_and_solves() {
        let dimacs = "
            c simple satisfiable cnf
            p cnf 2 2
            1 2 0
            -1 2 0
        ";

        let cnf = CnfFormula::from_dimacs(dimacs).expect("dimacs should parse");
        let result = solve(&cnf);

        assert!(matches!(result, SolveResult::Satisfiable(_)));
    }

    #[test]
    fn rejects_clause_with_out_of_range_variable() {
        let mut cnf = CnfFormula::new(2);
        let err = cnf
            .add_clause(vec![Lit::pos(3)])
            .expect_err("var 3 should be invalid for var_count=2");

        assert_eq!(
            err,
            SatError::VariableOutOfRange {
                var: 3,
                var_count: 2,
            }
        );
    }

    #[test]
    fn pure_literal_elimination_solves_without_branching() {
        let mut cnf = CnfFormula::new(4);
        cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)])
            .expect("valid clause");
        cnf.add_clause(vec![Lit::pos(1), Lit::pos(3)])
            .expect("valid clause");
        cnf.add_clause(vec![Lit::neg(2), Lit::pos(4)])
            .expect("valid clause");

        let result = solve(&cnf);
        let SolveResult::Satisfiable(model) = result else {
            panic!("formula should be satisfiable");
        };

        assert_eq!(model.value(1), Some(true));
    }

    #[test]
    fn unit_propagate_helper_remains_backward_compatible() {
        let mut cnf = CnfFormula::new(2);
        cnf.add_clause(vec![Lit::pos(1)]).expect("valid clause");
        cnf.add_clause(vec![Lit::neg(1), Lit::pos(2)])
            .expect("valid clause");
        let mut values = vec![None; 3];

        assert!(unit_propagate(&cnf, &mut values));
        assert_eq!(values[1], Some(true));
        assert_eq!(values[2], Some(true));
    }

    #[test]
    fn solve_with_stats_reports_activity() {
        let mut cnf = CnfFormula::new(3);
        cnf.add_clause(vec![Lit::pos(1), Lit::pos(2)])
            .expect("valid clause");
        cnf.add_clause(vec![Lit::neg(1), Lit::pos(3)])
            .expect("valid clause");

        let (result, stats) = solve_with_stats(&cnf);
        assert!(matches!(result, SolveResult::Satisfiable(_)));
        assert!(stats.recursive_calls >= 1);
        assert!(stats.decisions + stats.unit_assignments + stats.pure_literal_assignments >= 1);
    }

    fn pigeonhole_unsat(pigeons: usize, holes: usize) -> CnfFormula {
        let mut cnf = CnfFormula::new(pigeons * holes);

        let var = |p: usize, h: usize| -> usize { p * holes + h + 1 };

        for p in 0..pigeons {
            let mut clause = Vec::new();
            for h in 0..holes {
                clause.push(Lit::pos(var(p, h)));
            }
            cnf.add_clause(clause).expect("valid at-least-one clause");
        }

        for h in 0..holes {
            for p1 in 0..pigeons {
                for p2 in (p1 + 1)..pigeons {
                    cnf.add_clause(vec![Lit::neg(var(p1, h)), Lit::neg(var(p2, h))])
                        .expect("valid uniqueness clause");
                }
            }
        }

        cnf
    }

    #[test]
    fn stress_pigeonhole_unsat_tracks_backtracking_metrics() {
        let cnf = pigeonhole_unsat(4, 3);
        let (result, metrics) = solve_with_metrics(&cnf);

        assert_eq!(result, SolveResult::Unsatisfiable);
        assert!(metrics.stats.recursive_calls >= 1);
        assert!(metrics.stats.backtracks >= 1);
        assert!(
            metrics.stats.decisions + metrics.stats.unit_assignments + metrics.stats.pure_literal_assignments >= 1
        );
    }

    #[test]
    fn stress_dimacs_roundtrip_and_metrics_for_medium_sat() {
        let dimacs = "
            c medium satisfiable instance
            p cnf 6 9
            1 2 3 0
            -1 4 0
            -2 4 0
            -3 5 0
            -4 6 0
            -5 6 0
            2 -5 0
            -6 3 0
            6 0
        ";
        let cnf = CnfFormula::from_dimacs(dimacs).expect("valid dimacs");
        let (result, metrics) = solve_with_metrics(&cnf);

        assert!(matches!(result, SolveResult::Satisfiable(_)));
        assert!(metrics.stats.recursive_calls >= 1);
        assert!(metrics.elapsed_ns > 0);
    }

    fn synthetic_sat_instance(num_vars: usize, num_clauses: usize) -> CnfFormula {
        let mut cnf = CnfFormula::new(num_vars);
        for i in 0..num_clauses {
            let a = (i % num_vars) + 1;
            let b = ((i * 7 + 3) % num_vars) + 1;
            let c = ((i * 11 + 5) % num_vars) + 1;
            let lit_b = if i % 2 == 0 { Lit::neg(b) } else { Lit::pos(b) };
            let lit_c = if i % 3 == 0 { Lit::neg(c) } else { Lit::pos(c) };
            cnf.add_clause(vec![Lit::pos(a), lit_b, lit_c])
                .expect("synthetic clause should be valid");
        }
        cnf
    }

    #[test]
    fn stress_pigeonhole_unsat_5_to_4_baseline() {
        let cnf = pigeonhole_unsat(5, 4);
        let (result, metrics) = solve_with_metrics(&cnf);

        assert_eq!(result, SolveResult::Unsatisfiable);
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.stats.recursive_calls >= 1);
        assert!(metrics.stats.backtracks >= 1);
    }

    #[test]
    fn stress_synthetic_sat_large_clause_set_baseline() {
        let cnf = synthetic_sat_instance(24, 120);
        let (result, metrics) = solve_with_metrics(&cnf);

        assert!(matches!(result, SolveResult::Satisfiable(_)));
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.stats.recursive_calls >= 1);
        assert!(metrics.stats.decisions + metrics.stats.unit_assignments + metrics.stats.pure_literal_assignments >= 1);
    }
}
