//! Public types of `rflux-sat`.
//!
//! These types form the stable public API surface. Their names, field names,
//! and method signatures MUST NOT change without coordination with downstream
//! crates (`rflux-synth`, `rflux-cli`) — the field names are part of the JSON
//! and Python ABI (see `solve_stats_to_json` in cli and the PyO3 bindings).

use std::fmt;

// ---------------------------------------------------------------------------
// Literal
// ---------------------------------------------------------------------------

/// A boolean literal: a variable and a polarity.
///
/// Variables use **1-based** indexing throughout `rflux-sat`: variable `0` is
/// reserved/unused, and a valid variable `v` satisfies `1 <= v <= var_count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lit {
    /// 1-based variable index.
    pub var: usize,
    /// If `true`, this literal is the negation of variable `var`.
    pub negated: bool,
}

impl Lit {
    /// Construct a positive literal of `var`.
    #[must_use]
    pub fn pos(var: usize) -> Self {
        Self {
            var,
            negated: false,
        }
    }

    /// Construct a negative literal of `var`.
    #[must_use]
    pub fn neg(var: usize) -> Self {
        Self {
            var,
            negated: true,
        }
    }

    /// Evaluate this literal against an optional boolean assignment of `var`.
    #[allow(dead_code)]
    fn eval(self, assignment: Option<bool>) -> Option<bool> {
        assignment.map(|value| if self.negated { !value } else { value })
    }

    /// The value `var` must take for this literal to be true.
    #[allow(dead_code)]
    fn required_value(self) -> bool {
        !self.negated
    }

    /// Index used for the watcher array: `(var << 1) | negated`.
    #[inline]
    pub(crate) fn watch_index(self) -> usize {
        (self.var << 1) | (self.negated as usize)
    }
}

impl std::ops::Not for Lit {
    type Output = Self;
    /// Negate this literal.
    #[inline]
    fn not(self) -> Self {
        Self {
            var: self.var,
            negated: !self.negated,
        }
    }
}

// ---------------------------------------------------------------------------
// Formula
// ---------------------------------------------------------------------------

/// A CNF formula: a variable count plus a list of clauses.
///
/// Each clause is a disjunction of [`Lit`]s; the formula is the conjunction of
/// all clauses. Variables are 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CnfFormula {
    var_count: usize,
    clauses: Vec<Vec<Lit>>,
}

impl CnfFormula {
    /// Construct an empty formula over `var_count` variables.
    #[must_use]
    pub fn new(var_count: usize) -> Self {
        Self {
            var_count,
            clauses: Vec::new(),
        }
    }

    /// Add a fresh variable and return its new index.
    pub fn add_var(&mut self) -> usize {
        self.var_count += 1;
        self.var_count
    }

    /// The number of variables in this formula.
    #[must_use]
    pub fn var_count(&self) -> usize {
        self.var_count
    }

    /// Borrow the clauses as a slice.
    #[must_use]
    pub fn clauses(&self) -> &[Vec<Lit>] {
        &self.clauses
    }

    /// Render this formula as a DIMACS CNF string.
    #[must_use]
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
                rendered.push_str(&format!("{value} "));
            }
            rendered.push_str("0\n");
        }
        rendered
    }

    /// Add a clause to the formula.
    ///
    /// Returns an error if the clause is empty or references a variable outside
    /// `1..=var_count`.
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

    /// Parse a DIMACS CNF string into a formula.
    pub fn from_dimacs(input: &str) -> Result<Self, SatError> {
        crate::dimacs::parse_dimacs(input)
    }
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// A satisfying assignment. Variables are 1-based; `value(0)` returns `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// `values[var]` is the assignment of variable `var` (1-based; index 0
    /// unused). `None` means "don't care" (free variable).
    values: Vec<Option<bool>>,
}

impl Model {
    /// Construct a model from a raw value vector. Exposed for the solver
    /// internals (CDCL/DPLL) to build a model from their own representation.
    #[must_use]
    pub(crate) fn from_values(values: Vec<Option<bool>>) -> Self {
        Self { values }
    }

    /// The assignment of `var`, or `None` if `var` is free or out of range.
    #[must_use]
    pub fn value(&self, var: usize) -> Option<bool> {
        self.values.get(var).copied().flatten()
    }
}

// ---------------------------------------------------------------------------
// Result + statistics
// ---------------------------------------------------------------------------

/// The outcome of a SAT solve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveResult {
    /// The formula is satisfiable; the payload is a satisfying model.
    Satisfiable(Model),
    /// The formula is unsatisfiable.
    Unsatisfiable,
}

/// Solver-internal counters, exposed for telemetry and regression testing.
///
/// # Field-name stability
///
/// The field names are part of the public ABI: `rflux-cli` emits them verbatim
/// as JSON keys, `rflux-synth`'s `merge_solve_stats` reads them all by name,
/// and the PyO3 bindings forward four of them to Python. Renaming any field
/// breaks downstream consumers without a compile error.
///
/// Under the CDCL engine, some DPLL-specific concepts are reported as their
/// CDCL analogues (see `[Unreleased]` in CHANGELOG.md for the mapping):
///   * `recursive_calls` and `pure_literal_assignments` are always `0` (CDCL
///     has no recursion and does not perform pure-literal elimination).
///   * `decisions` is the count of decision literals chosen by the search.
///   * `unit_assignments` is the count of literals forced by unit propagation
///     (BCP).
///   * `backtracks` is the count of conflict-driven backjumps.
///   * `restarts` is the count of Luby restarts performed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SolveStats {
    /// Always `0` under CDCL (retained for ABI compatibility).
    pub recursive_calls: usize,
    /// Number of decision literals chosen by the search.
    pub decisions: usize,
    /// Number of literals forced by unit propagation (BCP).
    pub unit_assignments: usize,
    /// Always `0` under CDCL (retained for ABI compatibility).
    pub pure_literal_assignments: usize,
    /// Number of conflict-driven backjumps.
    pub backtracks: usize,
    /// Number of Luby restarts performed.
    pub restarts: usize,
}

/// A [`SolveStats`] snapshot plus wall-clock elapsed time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SolveMetrics {
    /// The accumulated stats.
    pub stats: SolveStats,
    /// Wall-clock nanoseconds spent in the solve.
    pub elapsed_ns: u128,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can arise while building or validating a CNF formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatError {
    /// A clause references a variable outside `1..=var_count`.
    VariableOutOfRange { var: usize, var_count: usize },
    /// An empty clause was added. An empty clause makes the formula UNSAT.
    EmptyClause,
    /// DIMACS input had no `p cnf ...` header.
    MissingDimacsHeader,
    /// DIMACS header was malformed.
    InvalidDimacsHeader(String),
    /// A DIMACS literal token could not be parsed as a non-zero integer.
    InvalidDimacsLiteral(String),
    /// The DIMACS header's clause count does not match the parsed count.
    InvalidDimacsClauseCount { expected: usize, actual: usize },
    /// A DIMACS clause was not terminated by a `0`.
    UnterminatedDimacsClause,
}

impl SatError {
    /// A stable, machine-readable error code.
    ///
    /// These codes are part of the contract: `sat_error_codes_are_stable` pins
    /// them and downstream tooling reads them. Do not change.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            SatError::VariableOutOfRange { .. } => "RFLOW-SIM-001",
            SatError::EmptyClause => "RFLOW-SIM-001",
            SatError::MissingDimacsHeader => "RFLOW-INPUT-002",
            SatError::InvalidDimacsHeader(..) => "RFLOW-INPUT-002",
            SatError::InvalidDimacsLiteral(..) => "RFLOW-INPUT-002",
            SatError::InvalidDimacsClauseCount { .. } => "RFLOW-INPUT-002",
            SatError::UnterminatedDimacsClause => "RFLOW-INPUT-002",
        }
    }

    /// A short human-readable suggestion for resolving this error.
    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            SatError::VariableOutOfRange { .. } => {
                "The DIMACS variable index exceeds the declared variable count."
            }
            SatError::EmptyClause => {
                "An empty clause makes the formula unsatisfiable by definition."
            }
            SatError::MissingDimacsHeader => {
                "DIMACS files must start with a 'p cnf <vars> <clauses>' header."
            }
            SatError::InvalidDimacsHeader(..) => {
                "Verify the DIMACS header format: 'p cnf <num_vars> <num_clauses>'."
            }
            SatError::InvalidDimacsLiteral(..) => {
                "DIMACS literals must be non-zero integers."
            }
            SatError::InvalidDimacsClauseCount { .. } => {
                "The declared clause count does not match the actual number of clauses."
            }
            SatError::UnterminatedDimacsClause => {
                "Each DIMACS clause must end with a 0 terminator."
            }
        }
    }
}

impl fmt::Display for SatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SatError::VariableOutOfRange { var, var_count } => write!(
                f,
                "variable {var} is out of range 1..={var_count}"
            ),
            SatError::EmptyClause => write!(f, "empty clause"),
            SatError::MissingDimacsHeader => write!(f, "missing DIMACS header"),
            SatError::InvalidDimacsHeader(line) => {
                write!(f, "invalid DIMACS header: {line}")
            }
            SatError::InvalidDimacsLiteral(tok) => {
                write!(f, "invalid DIMACS literal: {tok}")
            }
            SatError::InvalidDimacsClauseCount { expected, actual } => write!(
                f,
                "DIMACS clause count mismatch: header says {expected}, parsed {actual}"
            ),
            SatError::UnterminatedDimacsClause => write!(f, "unterminated DIMACS clause"),
        }
    }
}

impl std::error::Error for SatError {}

// ---------------------------------------------------------------------------
// LearnedClause (legacy, retained for compatibility)
// ---------------------------------------------------------------------------

/// A learned clause with its asserting backjump level.
///
/// This type was historically part of the public API and is retained to avoid
/// breaking callers that construct or pattern-match it. It is not used by the
/// CDCL engine's public surface; the CDCL engine stores learned clauses
/// internally in its clause arena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedClause {
    /// The literals of the learned clause.
    pub clause: Vec<Lit>,
    /// The decision level to backjump to after asserting this clause.
    pub backtrack_level: usize,
}

impl LearnedClause {
    /// Construct a learned clause.
    #[must_use]
    pub fn new(clause: Vec<Lit>, backtrack_level: usize) -> Self {
        Self {
            clause,
            backtrack_level,
        }
    }
}
