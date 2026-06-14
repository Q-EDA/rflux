//! `rflux-sat` — a MiniSat-class CDCL SAT solver for `rflux`.
//!
//! The public API is a thin façade over the [`cdcl`] engine:
//!   * [`CnfFormula`] — the CNF input,
//!   * [`IncrementalSolver`] — an incremental solver that retains learned
//!     clauses across `solve` calls,
//!   * [`solve`] / [`solve_with_stats`] / [`solve_with_metrics`] — one-shot
//!     convenience solvers,
//!   * [`Lit`], [`Model`], [`SolveResult`], [`SolveStats`], [`SolveMetrics`],
//!     [`SatError`] — supporting types.
//!
//! The default engine is conflict-driven clause learning (CDCL) with
//! two-watched-literal unit propagation, 1-UIP conflict analysis, VSIDS
//! branching, Luby restarts, and LBD-aware learned-clause reduction. A legacy
//! DPLL engine is available behind the `dpll` Cargo feature for differential
//! testing only.

pub mod cdcl;
pub mod dimacs;
#[cfg(feature = "dpll")]
pub(crate) mod dpll;
pub mod types;

pub use types::{
    CnfFormula, LearnedClause, Lit, Model, SatError, SolveMetrics, SolveResult, SolveStats,
};

use cell::RefCell;
use std::cell;
use std::time::Instant;

/// An incremental SAT solver that retains learned clauses across solve calls.
#[derive(Debug)]
pub struct IncrementalSolver {
    formula: CnfFormula,
    cdcl: RefCell<cdcl::CdclState>,
}

impl IncrementalSolver {
    #[must_use]
    pub fn new(var_count: usize) -> Self {
        Self {
            formula: CnfFormula::new(var_count),
            cdcl: RefCell::new(cdcl::CdclState::from_formula(var_count, &[])),
        }
    }

    #[must_use]
    pub fn from_formula(formula: CnfFormula) -> Self {
        let cdcl = cdcl::CdclState::from_formula(formula.var_count(), formula.clauses());
        Self { formula, cdcl: RefCell::new(cdcl) }
    }

    pub fn add_var(&mut self) -> usize {
        self.formula.add_var()
    }

    pub fn add_clause(&mut self, clause: Vec<Lit>) -> Result<(), SatError> {
        self.formula.add_clause(clause.clone())?;
        self.cdcl.borrow_mut().add_clause(clause);
        Ok(())
    }

    #[must_use]
    pub fn var_count(&self) -> usize {
        self.formula.var_count()
    }

    #[must_use]
    pub fn base_formula(&self) -> &CnfFormula {
        &self.formula
    }

    #[must_use]
    pub fn solve(&self) -> SolveResult {
        self.solve_with_assumptions(&[])
    }

    #[must_use]
    pub fn solve_with_assumptions(&self, assumptions: &[Lit]) -> SolveResult {
        self.solve_with_assumptions_and_metrics(assumptions).0
    }

    #[must_use]
    pub fn solve_with_assumptions_and_stats(&self, assumptions: &[Lit]) -> (SolveResult, SolveStats) {
        let (result, metrics) = self.solve_with_assumptions_and_metrics(assumptions);
        (result, metrics.stats)
    }

    #[must_use]
    pub fn solve_with_assumptions_and_metrics(&self, assumptions: &[Lit]) -> (SolveResult, SolveMetrics) {
        let started = Instant::now();
        let (result, stats) = self.cdcl.borrow_mut().solve_under_assumptions(assumptions);
        (result, SolveMetrics { stats, elapsed_ns: started.elapsed().as_nanos() })
    }

    #[must_use]
    pub fn unsat_core_of_assumptions(&self, assumptions: &[Lit]) -> Option<Vec<Lit>> {
        let (result, _) = self.solve_with_assumptions_and_metrics(assumptions);
        if matches!(result, SolveResult::Unsatisfiable) {
            let failed: Vec<Lit> = self.cdcl.borrow().failed_assumptions().to_vec();
            eprintln!("DEBUG failed={:?} assumptions={:?}", failed, assumptions);
            // The CDCL engine reports the assumption literals on the conflict
            // side of the final implication graph. For level-0 conflicts (no
            // implication graph exists), this is the full set of enqueued
            // assumptions — not minimized. Run a deletion pass so the returned
            // core is irredundant, matching the historical contract. This is
            // cheap because level-0 conflicts only arise from small assumption
            // sets; the expensive O(n^2) loop never runs on the large
            // SAT-based equivalence-checking workloads that motivated CDCL.
            let mut core: Vec<Lit> = assumptions
                .iter()
                .copied()
                .filter(|lit| failed.iter().any(|f| f.var == lit.var))
                .collect();
            if core.len() > 1 {
                core = self.minimize_core(&core);
            }
            Some(core)
        } else {
            None
        }
    }

    /// Deletion-based MUS minimization. For each literal in `core`, test whether
    /// the remaining core (without that literal) is still UNSAT; if so, drop it.
    /// Each test uses a freshly-built solver from the base formula so that
    /// learned clauses from prior solves do not pollute the independence of
    /// each candidate check. O(|core|^2) solves on small assumption sets.
    fn minimize_core(&self, core: &[Lit]) -> Vec<Lit> {
        let mut minimized = core.to_vec();
        let mut i = 0;
        while i < minimized.len() {
            let candidate: Vec<Lit> = minimized
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, l)| *l)
                .collect();
            let probe = cdcl::CdclState::from_formula(self.formula.var_count(), self.formula.clauses());
            let probe = std::cell::RefCell::new(probe);
            let (result, _) = probe.borrow_mut().solve_under_assumptions(&candidate);
            if matches!(result, SolveResult::Unsatisfiable) {
                minimized = candidate;
            } else {
                i += 1;
            }
        }
        minimized
    }
}

#[must_use]
pub fn solve(formula: &CnfFormula) -> SolveResult {
    solve_with_stats(formula).0
}

#[must_use]
pub fn solve_with_stats(formula: &CnfFormula) -> (SolveResult, SolveStats) {
    let (result, metrics) = solve_with_metrics(formula);
    (result, metrics.stats)
}

#[must_use]
pub fn solve_with_metrics(formula: &CnfFormula) -> (SolveResult, SolveMetrics) {
    IncrementalSolver::from_formula(formula.clone()).solve_with_assumptions_and_metrics(&[])
}
