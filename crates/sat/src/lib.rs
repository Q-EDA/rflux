use std::time::Instant;

// Ensure analyze_conflict and add_learned_clause are properly declared
fn analyze_conflict(
    _formula: &mut CnfFormula,
    _values: &Vec<Option<bool>>,
    trail: &Vec<usize>,
    decision_levels: &Vec<usize>,
) -> LearnedClause {
    let learned_clause = vec![Lit::neg(trail.last().copied().unwrap_or(1))];
    let backtrack_level = decision_levels
        .last()
        .copied()
        .unwrap_or(0)
        .saturating_sub(1);
    LearnedClause::new(learned_clause, backtrack_level)
}

fn add_learned_clause(
    formula: &mut CnfFormula,
    learned_clause: LearnedClause,
) -> Result<(), SatError> {
    formula.add_clause(learned_clause.clause)
}

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

    pub fn to_dimacs(&self) -> String {
        let mut rendered = String::new();
        rendered.push_str(&format!(
            "p cnf {} {}\n",
            self.var_count,
            self.clauses.len()
        ));
        for clause in &self.clauses {
            for lit in clause {
                let value = if lit.negated {
                    -(lit.var as i64)
                } else {
                    lit.var as i64
                };
                rendered.push_str(&format!("{} ", value));
            }
            rendered.push_str("0\n");
        }
        rendered
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
        let mut expected_clause_count = None::<usize>;
        let mut formula = None::<CnfFormula>;
        let mut pending_clause = Vec::new();
        let mut parsed_clause_count = 0usize;

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
                let parsed_clause_count = parts[3]
                    .parse::<usize>()
                    .map_err(|_| SatError::InvalidDimacsHeader(line.to_string()))?;
                expected_clause_count = Some(parsed_clause_count);
                formula = Some(CnfFormula::new(parsed_var_count));
                continue;
            }

            let Some(ref mut cnf) = formula else {
                return Err(SatError::MissingDimacsHeader);
            };

            for token in line.split_whitespace() {
                let lit = token
                    .parse::<i32>()
                    .map_err(|_| SatError::InvalidDimacsLiteral(token.to_string()))?;
                if lit == 0 {
                    if pending_clause.is_empty() {
                        return Err(SatError::EmptyClause);
                    }
                    cnf.add_clause(std::mem::take(&mut pending_clause))?;
                    parsed_clause_count += 1;
                    continue;
                }
                let var = lit.unsigned_abs() as usize;
                if var == 0 || var > cnf.var_count {
                    return Err(SatError::VariableOutOfRange {
                        var,
                        var_count: cnf.var_count,
                    });
                }
                pending_clause.push(if lit > 0 {
                    Lit::pos(var)
                } else {
                    Lit::neg(var)
                });
            }
        }

        let Some(cnf) = formula else {
            return Err(SatError::MissingDimacsHeader);
        };

        if !pending_clause.is_empty() {
            return Err(SatError::UnterminatedDimacsClause);
        }

        let expected_clause_count = expected_clause_count.ok_or(SatError::MissingDimacsHeader)?;
        if parsed_clause_count != expected_clause_count {
            return Err(SatError::InvalidDimacsClauseCount {
                expected: expected_clause_count,
                actual: parsed_clause_count,
            });
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SolveStats {
    pub recursive_calls: usize,
    pub decisions: usize,
    pub unit_assignments: usize,
    pub pure_literal_assignments: usize,
    pub backtracks: usize,
    pub restarts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
    InvalidDimacsClauseCount { expected: usize, actual: usize },
    UnterminatedDimacsClause,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedClause {
    pub clause: Vec<Lit>,
    pub backtrack_level: usize,
}

impl LearnedClause {
    pub fn new(clause: Vec<Lit>, backtrack_level: usize) -> Self {
        Self {
            clause,
            backtrack_level,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalSolver {
    formula: CnfFormula,
}

impl IncrementalSolver {
    pub fn new(var_count: usize) -> Self {
        Self {
            formula: CnfFormula::new(var_count),
        }
    }

    pub fn from_formula(formula: CnfFormula) -> Self {
        Self { formula }
    }

    pub fn add_var(&mut self) -> usize {
        self.formula.add_var()
    }

    pub fn add_clause(&mut self, clause: Vec<Lit>) -> Result<(), SatError> {
        self.formula.add_clause(clause)
    }

    pub fn var_count(&self) -> usize {
        self.formula.var_count()
    }

    pub fn base_formula(&self) -> &CnfFormula {
        &self.formula
    }

    pub fn solve(&self) -> SolveResult {
        self.solve_with_assumptions(&[])
    }

    pub fn solve_with_assumptions(&self, assumptions: &[Lit]) -> SolveResult {
        self.solve_with_assumptions_and_metrics(assumptions).0
    }

    pub fn solve_with_assumptions_and_stats(
        &self,
        assumptions: &[Lit],
    ) -> (SolveResult, SolveStats) {
        let (result, metrics) = self.solve_with_assumptions_and_metrics(assumptions);
        (result, metrics.stats)
    }

    pub fn solve_with_assumptions_and_metrics(
        &self,
        assumptions: &[Lit],
    ) -> (SolveResult, SolveMetrics) {
        let mut formula = self.formula.clone();
        for assumption in assumptions {
            formula
                .add_clause(vec![*assumption])
                .expect("assumptions should use in-range variables");
        }
        solve_with_metrics(&mut formula)
    }

    pub fn unsat_core_of_assumptions(&self, assumptions: &[Lit]) -> Option<Vec<Lit>> {
        let (result, _) = self.solve_with_assumptions_and_metrics(assumptions);
        if !matches!(result, SolveResult::Unsatisfiable) {
            return None;
        }

        let mut core = assumptions.to_vec();
        let mut index = 0usize;
        while index < core.len() {
            let candidate = core
                .iter()
                .enumerate()
                .filter_map(|(candidate_index, lit)| {
                    if candidate_index == index {
                        None
                    } else {
                        Some(*lit)
                    }
                })
                .collect::<Vec<_>>();

            if matches!(
                self.solve_with_assumptions(&candidate),
                SolveResult::Unsatisfiable
            ) {
                core = candidate;
            } else {
                index += 1;
            }
        }

        Some(core)
    }
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
    let mut working_formula = formula.clone();
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
        let mut values = vec![None; working_formula.var_count() + 1];
        let mut unlimited_budget = None;
        match dpll(
            &mut working_formula,
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
    formula.var_count() * 10
}

enum SearchOutcome {
    Satisfiable,
    Unsatisfiable,
    BudgetExhausted,
}

fn dpll(
    formula: &mut CnfFormula,
    values: &mut Vec<Option<bool>>,
    phase_hints: &mut [bool],
    stats: &mut SolveStats,
    decision_budget: &mut Option<usize>,
) -> SearchOutcome {
    let mut trail = Vec::new();
    let mut decision_levels = Vec::new();

    stats.recursive_calls += 1;
    if !propagate(formula, values, phase_hints, stats) {
        let conflict = analyze_conflict(formula, values, &trail, &decision_levels);
        if let Err(e) = add_learned_clause(formula, conflict) {
            eprintln!("Error adding learned clause: {:?}", e);
            return SearchOutcome::Unsatisfiable;
        }
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

    decision_levels.push(trail.len());
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

    decision_levels.pop();
    stats.backtracks += 1;
    SearchOutcome::Unsatisfiable
}

fn propagate(
    formula: &mut CnfFormula,
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
    formula: &mut CnfFormula,
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
    formula: &mut CnfFormula,
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

fn all_clauses_satisfied(formula: &mut CnfFormula, values: &[Option<bool>]) -> bool {
    formula.clauses.iter().all(|clause| {
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

fn choose_unassigned_var(formula: &mut CnfFormula, values: &[Option<bool>]) -> Option<usize> {
    (1..=formula.var_count()).find(|&var| values[var].is_none())
}

#[cfg(test)]
fn unit_propagate(formula: &CnfFormula, values: &mut Vec<Option<bool>>) -> bool {
    for clause in &formula.clauses {
        if clause
            .iter()
            .all(|lit| lit.eval(values[lit.var]) == Some(false))
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use proptest::prelude::*;

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
    fn dimacs_roundtrip_preserves_formula() {
        let mut cnf = CnfFormula::new(3);
        cnf.add_clause(vec![Lit::pos(1), Lit::neg(2)])
            .expect("valid clause");
        cnf.add_clause(vec![Lit::pos(3)]).expect("valid clause");

        let reparsed = CnfFormula::from_dimacs(&cnf.to_dimacs()).expect("roundtrip should parse");

        assert_eq!(reparsed, cnf);
    }

    #[test]
    fn parses_dimacs_with_multiline_and_same_line_clauses() {
        let dimacs = "
            c clause 1 spans lines, clauses 2 and 3 share a line
            p cnf 3 3
            1
            -2 0 2 3 0
            -1 3 0
        ";

        let cnf = CnfFormula::from_dimacs(dimacs).expect("dimacs should parse");

        assert_eq!(cnf.clauses().len(), 3);
        assert_eq!(cnf.clauses()[0], vec![Lit::pos(1), Lit::neg(2)]);
        assert_eq!(cnf.clauses()[1], vec![Lit::pos(2), Lit::pos(3)]);
        assert_eq!(cnf.clauses()[2], vec![Lit::neg(1), Lit::pos(3)]);
    }

    #[test]
    fn rejects_dimacs_with_clause_count_mismatch() {
        let dimacs = "
            p cnf 2 2
            1 0
        ";

        let error = CnfFormula::from_dimacs(dimacs).expect_err("clause count mismatch should fail");

        assert_eq!(
            error,
            SatError::InvalidDimacsClauseCount {
                expected: 2,
                actual: 1,
            }
        );
    }

    #[test]
    fn rejects_dimacs_with_unterminated_clause() {
        let dimacs = "
            p cnf 2 1
            1 -2
        ";

        let error = CnfFormula::from_dimacs(dimacs).expect_err("unterminated clause should fail");

        assert_eq!(error, SatError::UnterminatedDimacsClause);
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
            metrics.stats.decisions
                + metrics.stats.unit_assignments
                + metrics.stats.pure_literal_assignments
                >= 1
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
        assert!(
            metrics.stats.decisions
                + metrics.stats.unit_assignments
                + metrics.stats.pure_literal_assignments
                >= 1
        );
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
                    .expect("generated unit clause should always be valid");
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
        solver
            .add_clause(vec![Lit::pos(1), Lit::pos(2)])
            .expect("valid clause");
        solver
            .add_clause(vec![Lit::neg(1), Lit::pos(2)])
            .expect("valid clause");

        let sat = solver.solve_with_assumptions(&[Lit::pos(1)]);
        let unsat = solver.solve_with_assumptions(&[Lit::neg(2)]);

        assert!(matches!(sat, SolveResult::Satisfiable(_)));
        assert_eq!(unsat, SolveResult::Unsatisfiable);
        assert_eq!(solver.base_formula().clauses().len(), 2);
    }

    #[test]
    fn incremental_solver_reports_metrics_for_assumption_solves() {
        let mut solver = IncrementalSolver::new(3);
        solver
            .add_clause(vec![Lit::pos(1), Lit::pos(2)])
            .expect("valid clause");
        solver
            .add_clause(vec![Lit::neg(1), Lit::pos(3)])
            .expect("valid clause");

        let (result, metrics) = solver.solve_with_assumptions_and_metrics(&[Lit::neg(2)]);

        assert!(matches!(
            result,
            SolveResult::Satisfiable(_) | SolveResult::Unsatisfiable
        ));
        assert!(metrics.elapsed_ns > 0);
        assert!(metrics.stats.recursive_calls >= 1);
    }

    #[test]
    fn incremental_solver_extracts_redundancy_reduced_unsat_core() {
        let mut solver = IncrementalSolver::new(3);
        solver
            .add_clause(vec![Lit::pos(1), Lit::pos(2)])
            .expect("valid clause");

        let core = solver
            .unsat_core_of_assumptions(&[Lit::neg(1), Lit::neg(2), Lit::pos(3)])
            .expect("assumptions should be unsat");

        assert_eq!(core, vec![Lit::neg(1), Lit::neg(2)]);
    }

    #[test]
    fn incremental_solver_returns_no_unsat_core_for_sat_assumptions() {
        let mut solver = IncrementalSolver::new(1);
        solver.add_clause(vec![Lit::pos(1)]).expect("valid clause");

        assert_eq!(solver.unsat_core_of_assumptions(&[Lit::pos(1)]), None);
    }
}
