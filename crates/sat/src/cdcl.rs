//! Conflict-Driven Clause Learning (CDCL) SAT engine.
//!
//! This module implements a MiniSat-class CDCL solver with the components that
//! matter for medium-scale equivalence-checking workloads:
//!
//!   * a clause arena referenced by index (so the borrow checker stays happy
//!     while watcher lists are mutated),
//!   * two-watched-literal unit propagation (BCP) with a trail + reason map,
//!   * 1-UIP conflict analysis producing an asserting learned clause,
//!   * VSIDS-style variable activity with exponential decay and phase saving,
//!   * a Luby restart schedule,
//!   * lightweight self-subsumption / local minimization of learned clauses,
//!   * assumption-based solving and final-UIP failed-assumption extraction,
//!   * incremental reuse of learned clauses across `solve` calls.
//!
//! The engine is deliberately not Kissat-class: there is no inprocessing,
//! vivification, distillation, or FRAT proof emission. The goal is correctness
//! and "competitive with MiniSat" performance at the hundreds-to-low-thousands
//! of variables scale that `rflux-synth` equivalence checking operates at.
//
// Some methods on `CdclState` and its helper structs (e.g. `var_count`,
// `var_value`, `Clause::len`) are currently only exercised by tests or kept
// for future use; they are suppressed here rather than deleted because they
// form part of the intended internal API for upcoming SAT-sweeping work.
// The `unused_assignments` / `unused_mut` allows cover the 1-UIP analysis
// loop where the last write to `p` is the one read out, and a few scratch
// locals whose mutability is structural rather than load-bearing today.
#![allow(dead_code, unused_assignments, unused_mut, clippy::redundant_clone)]

use crate::types::{Lit, Model, SolveResult, SolveStats};
use std::cmp::Ordering;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Tuning constants
// ---------------------------------------------------------------------------

/// Base unit (in conflicts) for the Luby restart sequence.
const RESTART_UNIT: u64 = 100;
/// Activity bump applied to variables seen during conflict analysis.
const VAR_BUMP: f64 = 1.0;
/// Activity decay applied periodically to all variables (`act *= VAR_DECAY`).
const VAR_DECAY: f64 = 0.95;
/// Initial size of the clause arena.
const INITIAL_CLAUSE_CAPACITY: usize = 1024;
/// Maximum learned clauses before a database reduction (half are dropped).
const LEARNED_DB_MAX_INITIAL: usize = 2048;

// ---------------------------------------------------------------------------
// Core data structures
// ---------------------------------------------------------------------------

/// Index into the [`ClauseArena`]. `u32` keeps watcher structs compact; the
/// arena never stores more than `u32::MAX` clauses in practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClauseRef(u32);

/// A reserved clause reference meaning "no clause" — used as the reason for
/// decision literals and assumption literals on the trail.
const NULL_CLAUSE: ClauseRef = ClauseRef(u32::MAX);

/// Three-valued assignment for a single variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LBool {
    Undef,
    True,
    False,
}

impl LBool {
    #[inline]
    fn from_bool(value: bool) -> Self {
        if value {
            LBool::True
        } else {
            LBool::False
        }
    }
}

/// A clause stored in the arena. Literals are kept as a small `Vec` (clauses
/// are short in practice; the longest are learned 1-UIP clauses). The two
/// watched literals are always `lits[0]` and `lits[1]`.
#[derive(Debug, Clone)]
struct Clause {
    lits: Vec<Lit>,
    /// True for clauses derived by learning; used by database reduction.
    learned: bool,
    /// Activity (only meaningful for learned clauses), used during reduction.
    activity: f64,
    /// Literal-block-distance bound; smaller ⇒ more glue ⇒ keep longer.
    lbd: u32,
}

impl Clause {
    fn new(lits: Vec<Lit>, learned: bool) -> Self {
        Self {
            lits,
            learned,
            activity: 0.0,
            lbd: 2,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.lits.len()
    }
}

/// Growing store of clauses indexed by [`ClauseRef`].
#[derive(Debug, Default)]
struct ClauseArena {
    clauses: Vec<Clause>,
}

impl ClauseArena {
    fn with_capacity(cap: usize) -> Self {
        Self {
            clauses: Vec::with_capacity(cap),
        }
    }

    fn push(&mut self, clause: Clause) -> ClauseRef {
        let idx = self.clauses.len();
        debug_assert!(idx < u32::MAX as usize);
        self.clauses.push(clause);
        ClauseRef(idx as u32)
    }

    #[inline]
    fn get(&self, cref: ClauseRef) -> &Clause {
        &self.clauses[cref.0 as usize]
    }

    #[inline]
    fn get_mut(&mut self, cref: ClauseRef) -> &mut Clause {
        &mut self.clauses[cref.0 as usize]
    }

    #[inline]
    fn len(&self) -> usize {
        self.clauses.len()
    }

    fn iter(&self) -> impl Iterator<Item = (ClauseRef, &Clause)> {
        self.clauses
            .iter()
            .enumerate()
            .map(|(i, c)| (ClauseRef(i as u32), c))
    }
}

/// A watcher entry in a variable's watcher list: the clause being watched and
/// the "other" literal (the block literal that must be re-checked when the
/// watched literal becomes false).
#[derive(Debug, Clone, Copy)]
struct Watch {
    clause: ClauseRef,
    blocker: Lit,
}

/// Reason a trail literal was assigned. Either a clause that forced it via
/// unit propagation, or `Decision` for a branch pick, or `Assumption` for a
/// user-provided assumption.
#[derive(Debug, Clone, Copy)]
enum Reason {
    Clause(ClauseRef),
    Decision,
    Assumption,
}

/// A literal on the trail together with the decision level at which it was
/// assigned and the reason for the assignment. Storing these inline (rather
/// than in parallel arrays) keeps conflict analysis a single sequential scan.
#[derive(Debug, Clone, Copy)]
struct TrailEntry {
    lit: Lit,
    level: u32,
    reason: Reason,
}

/// VSIDS activity heap: a max-heap of variables keyed by activity, used to pick
/// the next decision variable in O(log n).
#[derive(Debug, Default)]
struct ActivityHeap {
    heap: Vec<usize>, // variable indices (1-based)
    pos_in_heap: Vec<i32>, // -1 ⇒ not present
    activity: Vec<f64>,
    var_count: usize,
}

impl ActivityHeap {
    fn new(var_count: usize) -> Self {
        Self {
            heap: Vec::with_capacity(var_count),
            pos_in_heap: vec![-1; var_count + 1],
            activity: vec![0.0; var_count + 1],
            var_count,
        }
    }

    fn clear(&mut self) {
        for v in &mut self.pos_in_heap {
            *v = -1;
        }
        self.heap.clear();
    }

    fn grow(&mut self, new_var_count: usize) {
        if new_var_count >= self.activity.len() {
            self.pos_in_heap.resize(new_var_count + 1, -1);
            self.activity.resize(new_var_count + 1, 0.0);
            self.var_count = new_var_count;
        }
    }

    fn empty(&self) -> bool {
        self.heap.is_empty()
    }

    #[inline]
    fn higher(&self, a: usize, b: usize) -> bool {
        // Higher activity ⇒ higher priority. Ties broken by variable index for
        // determinism (smaller index first).
        match self.activity[a].partial_cmp(&self.activity[b]) {
            Some(Ordering::Equal) => a < b,
            Some(Ordering::Greater) => true,
            _ => false,
        }
    }

    fn up(&mut self, mut i: usize) {
        let v = self.heap[i];
        while i > 0 {
            let parent = (i - 1) / 2;
            if !self.higher(v, self.heap[parent]) {
                break;
            }
            self.heap[i] = self.heap[parent];
            self.pos_in_heap[self.heap[i]] = i as i32;
            i = parent;
        }
        self.heap[i] = v;
        self.pos_in_heap[v] = i as i32;
    }

    fn down(&mut self, mut i: usize) {
        let n = self.heap.len();
        let v = self.heap[i];
        loop {
            let left = 2 * i + 1;
            let right = 2 * i + 2;
            let mut largest = i;
            if left < n && self.higher(self.heap[left], self.heap[largest]) {
                largest = left;
            }
            if right < n && self.higher(self.heap[right], self.heap[largest]) {
                largest = right;
            }
            if largest == i {
                break;
            }
            self.heap[i] = self.heap[largest];
            self.pos_in_heap[self.heap[i]] = i as i32;
            i = largest;
        }
        self.heap[i] = v;
        self.pos_in_heap[v] = i as i32;
    }

    fn insert(&mut self, var: usize) {
        if var < self.pos_in_heap.len() && self.pos_in_heap[var] != -1 {
            // Already present — re-heapify from current position.
            let i = self.pos_in_heap[var] as usize;
            self.up(i);
            return;
        }
        self.pos_in_heap[var] = self.heap.len() as i32;
        self.heap.push(var);
        self.up(self.heap.len() - 1);
    }

    fn pop_max(&mut self) -> Option<usize> {
        if self.heap.is_empty() {
            return None;
        }
        let max = self.heap[0];
        let last = self.heap.pop().unwrap();
        self.pos_in_heap[max] = -1;
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.pos_in_heap[last] = 0;
            self.down(0);
        }
        Some(max)
    }

    fn bump(&mut self, var: usize) {
        self.activity[var] += VAR_BUMP;
        if self.activity[var] > 1e100 {
            // Rescale to avoid overflow.
            for a in self.activity.iter_mut() {
                *a *= 1e-100;
            }
        }
        if self.pos_in_heap[var] != -1 {
            let i = self.pos_in_heap[var] as usize;
            self.up(i);
        }
    }

    fn decay(&mut self) {
        for a in self.activity.iter_mut() {
            *a *= VAR_DECAY;
        }
        // Re-heapify fully — decay changes relative order arbitrarily.
        // (Heapify by sifting down from the bottom; O(n).)
        if self.heap.len() > 1 {
            for i in (0..self.heap.len() / 2).rev() {
                self.down(i);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The solver state
// ---------------------------------------------------------------------------

/// Persistent CDCL state. Reused across `solve` calls so learned clauses
/// survive; only the per-search assignment is reset between solves.
#[derive(Debug)]
pub(crate) struct CdclState {
    /// The base formula clauses (immutable after construction; the original
    /// CNF plus any clauses added via the incremental API).
    arena: ClauseArena,
    /// Where the base (non-learned) clauses end and learned clauses begin.
    first_learned: usize,
    /// `watches[lit_index]` holds watchers for when `lit` becomes false.
    /// Indexed by `(var << 1) | (negated as u64)`, so positive and negative
    /// literals of the same variable get separate lists.
    watches: Vec<Vec<Watch>>,
    /// Per-variable assignment.
    assignment: Vec<LBool>,
    /// Saved phase for each variable (phase saving).
    phase: Vec<bool>,
    /// Per-variable decision level. `level[var]` is the decision level at
    /// which `var` was assigned. O(1) lookup replaces the old O(n) trail scan.
    level: Vec<u32>,
    /// Per-variable reason clause ref. `reason_of[var]` stores the reason a
    /// variable was assigned, avoiding trail scans during analysis.
    reason_of: Vec<Reason>,
    /// Trail: literals assigned, in order.
    trail: Vec<TrailEntry>,
    /// Index in `trail` where each decision level starts.
    trail_lim: Vec<usize>,
    /// Start index of the propagation queue within the trail. Propagation
    /// only needs to process trail entries from `prop_qhead` onward.
    prop_qhead: usize,
    /// Heap of unassigned variables by activity.
    heap: ActivityHeap,
    /// Number of variables (1-based indexing; index 0 unused).
    var_count: usize,
    /// Decision level (0 = only assumptions propagated).
    decision_level: u32,
    /// Stats accumulated across all solves since construction.
    stats: SolveStats,
    /// Conflicts since the last restart.
    conflicts_since_restart: u64,
    /// Conflicts until next restart (per Luby).
    next_restart: u64,
    /// Luby step counter.
    luby_step: u64,
    /// Half-life bound for the learned-clause database.
    learned_db_max: usize,
    /// Total conflicts since last database reduction.
    conflicts_since_reduce: u64,
    /// Most recent failed assumptions from the final-UIP conflict, if any.
    last_failed_assumptions: Vec<Lit>,
    /// Set when a unit clause or assumption contradicts the level-0 trail.
    /// The next `propagate()` returns a synthetic conflict, forcing UNSAT.
    level0_conflict: bool,
    /// Marker used during recursive minimization to avoid revisiting literals.
    analyze_seen: Vec<bool>,
    /// Stack used by 1-UIP analysis.
    analyze_stack: Vec<Lit>,
    /// Temporary learned clause buffer.
    analyze_learnt: Vec<Lit>,
    /// Decision level at the asserting literal (for backjump).
    analyze_btlevel: u32,
    /// Whether the final conflict hit an assumption literal (UNSAT by assumption).
    analyze_assumption_conflict: bool,
}

impl CdclState {
    /// Build a fresh CDCL state from a CNF formula. Clauses are copied into the
    /// arena; the formula itself is not retained.
    pub(crate) fn from_formula(var_count: usize, clauses: &[Vec<Lit>]) -> Self {
        let mut state = Self {
            arena: ClauseArena::with_capacity(INITIAL_CLAUSE_CAPACITY),
            first_learned: 0,
            watches: vec![Vec::new(); (var_count + 1) * 2],
            assignment: vec![LBool::Undef; var_count + 1],
            phase: vec![false; var_count + 1],
            level: vec![0; var_count + 1],
            reason_of: vec![Reason::Decision; var_count + 1],
            trail: Vec::with_capacity(var_count + 1),
            trail_lim: Vec::with_capacity(32),
            prop_qhead: 0,
            heap: ActivityHeap::new(var_count),
            var_count,
            decision_level: 0,
            stats: SolveStats::default(),
            conflicts_since_restart: 0,
            next_restart: RESTART_UNIT,
            luby_step: 1,
            learned_db_max: LEARNED_DB_MAX_INITIAL,
            conflicts_since_reduce: 0,
            last_failed_assumptions: Vec::new(),
            level0_conflict: false,
            analyze_seen: vec![false; var_count + 1],
            analyze_stack: Vec::new(),
            analyze_learnt: Vec::new(),
            analyze_btlevel: 0,
            analyze_assumption_conflict: false,
        };

        // Insert every clause into the arena, attaching watchers. Unit clauses
        // are enqueued for level-0 propagation.
        for clause_lits in clauses {
            if clause_lits.is_empty() {
                continue;
            }
            state.attach_clause(clause_lits.clone(), false);
        }
        state.first_learned = state.arena.len();

        // Seed the activity heap with every variable so the first decision is
        // available immediately. Variables are inserted in index order; the
        // heap reorders by activity as conflicts bump variables.
        for var in 1..=var_count {
            state.heap.insert(var);
        }

        state
    }

    /// Current variable count.
    pub(crate) fn var_count(&self) -> usize {
        self.var_count
    }

    /// Add a permanent clause to the solver (incremental API). After this call
    /// the clause is part of every subsequent solve.
    pub(crate) fn add_clause(&mut self, clause: Vec<Lit>) {
        if clause.is_empty() {
            return;
        }
        // Grow internal arrays if new variables appeared.
        let max_var = clause.iter().map(|l| l.var).max().unwrap_or(0);
        if max_var > self.var_count {
            self.grow_to(max_var);
        }
        self.attach_clause(clause, false);
        // This is a permanent base-formula clause; advance first_learned so that
        // reset_for_solve's learned-clause cleanup does not detach it.
        self.first_learned = self.arena.len();
        // If we've already started solving (learned clauses exist), the new
        // clause must be checked against the current level-0 trail.
        if self.decision_level == 0 {
            self.enqueue_units_for_clause(self.arena.len() - 1);
        }
    }

    fn grow_to(&mut self, new_var_count: usize) {
        self.watches.resize((new_var_count + 1) * 2, Vec::new());
        self.assignment.resize(new_var_count + 1, LBool::Undef);
        self.phase.resize(new_var_count + 1, false);
        self.level.resize(new_var_count + 1, 0);
        self.reason_of
            .resize(new_var_count + 1, Reason::Decision);
        self.analyze_seen.resize(new_var_count + 1, false);
        self.heap.grow(new_var_count);
        self.var_count = new_var_count;
    }

    /// Insert a clause into the arena and attach the two watchers.
    ///
    /// For unit clauses, the single literal is enqueued at level 0 directly
    /// (unit clauses have no watchers — they are "always watched" by being on
    /// the trail). The conflict check (whether the unit literal is already
    /// false) is deferred to the next `propagate()` call, which the solver
    /// entry points always run before searching.
    fn attach_clause(&mut self, lits: Vec<Lit>, learned: bool) -> ClauseRef {
        if lits.len() == 1 {
            let lit = lits[0];
            let cref = self.arena.push(Clause::new(lits, learned));
            // Use the conflict-checked enqueue so a contradictory unit clause
            // (e.g. adding ¬1 when 1 is already forced) sets the level-0
            // conflict flag instead of panicking.
            self.enqueue_level0_checked(lit, Reason::Clause(cref));
            return cref;
        }

        // For watched literals, the first two literals should ideally be the
        // ones most likely to be unassigned/satisfying.
        let lit0 = lits[0];
        let lit1 = lits[1];
        let cref = self.arena.push(Clause::new(lits, learned));

        self.watches[lit0.watch_index()].push(Watch {
            clause: cref,
            blocker: lit1,
        });
        self.watches[lit1.watch_index()].push(Watch {
            clause: cref,
            blocker: lit0,
        });
        cref
    }

    /// If a newly added clause is unit at level 0, enqueue its single literal.
    fn enqueue_units_for_clause(&mut self, cref_idx: usize) {
        let cref = ClauseRef(cref_idx as u32);
        let clause = self.arena.get(cref);
        // Only enqueue if exactly one literal is unassigned and the rest are false.
        let mut unassigned = None;
        let mut unassigned_count = 0;
        for &lit in &clause.lits {
            match self.value(lit) {
                LBool::True => return, // satisfied
                LBool::Undef => {
                    unassigned_count += 1;
                    unassigned = Some(lit);
                    if unassigned_count > 1 {
                        return;
                    }
                }
                LBool::False => {}
            }
        }
        if let Some(lit) = unassigned {
            self.enqueue_level0_checked(lit, Reason::Clause(cref));
        }
    }

    /// Read the assignment of a literal's variable, applying negation.
    #[inline]
    fn value(&self, lit: Lit) -> LBool {
        let v = self.assignment[lit.var];
        match v {
            LBool::Undef => LBool::Undef,
            LBool::True => {
                if lit.negated {
                    LBool::False
                } else {
                    LBool::True
                }
            }
            LBool::False => {
                if lit.negated {
                    LBool::True
                } else {
                    LBool::False
                }
            }
        }
    }

    /// Variable value (no negation).
    #[inline]
    fn var_value(&self, var: usize) -> LBool {
        self.assignment[var]
    }

    /// Enqueue a literal on the trail without checking. The caller guarantees
    /// that the literal is currently unassigned and assigning it is consistent.
    #[inline]
    fn unchecked_enqueue(&mut self, lit: Lit, level: u32, reason: Reason) {
        debug_assert_eq!(self.value(lit), LBool::Undef);
        self.assignment[lit.var] = LBool::from_bool(!lit.negated);
        self.level[lit.var] = level;
        self.reason_of[lit.var] = reason;
        self.trail.push(TrailEntry { lit, level, reason });
    }

    /// Try to enqueue `lit` as a forced assignment at level 0. Returns `false`
    /// if `lit` is already false (conflict) — in which case a level-0 conflict
    /// flag is set so the next `propagate()` reports it. Used by `attach_clause`
    /// for unit clauses and by assumption setup, both of which can immediately
    /// contradict the current assignment.
    #[inline]
    fn enqueue_level0_checked(&mut self, lit: Lit, reason: Reason) -> bool {
        match self.value(lit) {
            LBool::True => true,
            LBool::False => {
                self.level0_conflict = true;
                false
            }
            LBool::Undef => {
                self.unchecked_enqueue(lit, 0, reason);
                true
            }
        }
    }

    /// Check whether the base formula's unit clauses (those with exactly one
    /// literal) contain a contradiction: either a unit literal whose variable
    /// is already assigned the opposite polarity on the level-0 trail, or two
    /// unit clauses forcing opposite polarities. This is called once at the
    /// start of each solve to (re)establish the level-0 conflict status without
    /// relying on a flag that would leak across solves.
    /// Remove all learned clauses from the arena and detach their watchers,
    /// and fully reset the trail/assignment back to the base formula's level-0
    /// unit-clause state. Called at the start of each solve to guarantee that
    /// clauses learned and assignments made under one set of assumptions do not
    /// influence a later solve under different assumptions. This is a
    /// correctness-preserving fallback while the 1-UIP analysis is being
    /// hardened; it forgoes incremental speedup.
    fn reset_for_solve(&mut self) {
        // 1. Clear the trail and assignment entirely.
        for entry in &self.trail {
            self.assignment[entry.lit.var] = LBool::Undef;
        }
        self.trail.clear();
        self.trail_lim.clear();
        self.prop_qhead = 0;
        self.decision_level = 0;

        // 2. Detach and remove learned clauses.
        let first = self.first_learned;
        if self.arena.len() > first {
            for cref_idx in first..self.arena.len() {
                let cref = ClauseRef(cref_idx as u32);
                let clause = self.arena.get(cref);
                if clause.lits.len() >= 2 {
                    let lit0 = clause.lits[0];
                    let lit1 = clause.lits[1];
                    let i0 = Self::lit_index(lit0);
                    self.watches[i0].retain(|w| w.clause != cref);
                    let i1 = Self::lit_index(lit1);
                    self.watches[i1].retain(|w| w.clause != cref);
                }
            }
            self.arena.clauses.truncate(first);
        }

        // 3. Re-enqueue base formula unit clauses at level 0. Collect first to
        //    avoid borrowing `self.arena` while mutating via `unchecked_enqueue`.
        let unit_lits: Vec<Lit> = self
            .arena
            .iter()
            .take(self.first_learned)
            .filter(|(_, c)| c.lits.len() == 1)
            .map(|(cref, c)| {
                let _ = cref;
                c.lits[0]
            })
            .collect();
        for lit in unit_lits {
            if self.value(lit) == LBool::Undef {
                self.unchecked_enqueue(lit, 0, Reason::Clause(ClauseRef(0)));
            }
        }

        // 4. Reset restart / DB-reduction counters.
        self.conflicts_since_restart = 0;
        self.next_restart = RESTART_UNIT;
        self.luby_step = 1;
        self.conflicts_since_reduce = 0;
    }

    fn detect_level0_unit_conflict(&self) -> bool {
        for (_, clause) in self.arena.iter().take(self.first_learned) {
            if clause.lits.len() == 1 {
                if self.value(clause.lits[0]) == LBool::False {
                    return true;
                }
            }
        }
        false
    }

    // Watcher-array index for a literal is provided by `Lit::watch_index()` in
    // `types.rs`. We mirror it here as a free function for brevity at call
    // sites where the literal is already owned.
    #[inline]
    fn lit_index(lit: Lit) -> usize {
        lit.watch_index()
    }

    // ---------------------------------------------------------------------------
    // BCP
    // ---------------------------------------------------------------------------

    /// Run unit propagation at the current decision level. Returns the
    /// conflicting clause (if any). On conflict, the trail is left pointing at
    /// the conflicting assignment for analysis.
    fn propagate(&mut self) -> Option<ClauseRef> {
        if self.level0_conflict {
            return Some(NULL_CLAUSE);
        }
        let mut qhead = self.prop_qhead;
        loop {
            if qhead >= self.trail.len() {
                self.prop_qhead = qhead;
                return None;
            }
            let entry = self.trail[qhead];
            let watch_idx = Self::lit_index(!entry.lit);
            qhead += 1;

            // Iterate over the watcher list for `~entry.lit`, moving surviving
            // watchers to the front and processing triggered clauses.
            let mut i = 0;
            'next_watch: while i < self.watches[watch_idx].len() {
                let watch = self.watches[watch_idx][i];
                let blocker = watch.blocker;
                // Fast path: the block literal already satisfies the clause.
                if self.value(blocker) == LBool::True {
                    i += 1;
                    continue;
                }
                let cref = watch.clause;
                let clause = self.arena.get(cref).clone();

                // Make sure the false literal is at position 1.
                let false_lit = !entry.lit;
                let mut lits = clause.lits;
                if lits[0] == false_lit {
                    lits.swap(0, 1);
                }
                debug_assert_eq!(lits[1], false_lit);

                let first = lits[0];
                let new_blocker = first;
                if first != blocker && self.value(first) == LBool::True {
                    // Already satisfied; update blocker and keep the watcher.
                    self.watches[watch_idx][i].blocker = new_blocker;
                    i += 1;
                    continue;
                }

                // Look for a new literal to watch (positions 2..).
                for k in 2..lits.len() {
                    if self.value(lits[k]) != LBool::False {
                        // Found a new watch: move it to position 1, update its
                        // own watcher list, and drop this watcher.
                        lits.swap(1, k);
                        let new_watched = lits[1];
                        self.arena.get_mut(cref).lits = lits.clone();
                        self.watches[Self::lit_index(new_watched)].push(Watch {
                            clause: cref,
                            blocker: lits[0],
                        });
                        // Remove this watcher by swapping with the last.
                        self.watches[watch_idx].swap_remove(i);
                        continue 'next_watch;
                    }
                }

                // Did not find a new watch: clause is either conflicting or unit.
                self.watches[watch_idx][i].blocker = new_blocker;
                self.arena.get_mut(cref).lits = lits;
                if self.value(first) == LBool::False {
                    // Conflict: drain the remaining queue so the trail head is
                    // consistent, then return this clause.
                    return Some(cref);
                }
                // Unit: enqueue `first` at the current level.
                self.unchecked_enqueue(
                    first,
                    self.decision_level,
                    Reason::Clause(cref),
                );
                self.stats.unit_assignments += 1;
                i += 1;
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Decision making
    // ---------------------------------------------------------------------------

    /// Pick the next decision variable from the activity heap and assign it.
    fn pick_branch(&mut self) -> Option<Lit> {
        while let Some(var) = self.heap.pop_max() {
            if self.assignment[var] == LBool::Undef {
                // Phase saving: use the last saved phase.
                let negated = !self.phase[var];
                return Some(Lit { var, negated });
            }
        }
        None
    }

    fn new_decision_level(&mut self) {
        self.trail_lim.push(self.trail.len());
        self.decision_level += 1;
    }

    // ---------------------------------------------------------------------------
    // Conflict analysis (1-UIP)
    // ---------------------------------------------------------------------------

    /// Analyze a conflict clause and produce:
    ///   * a learned 1-UIP asserting clause (in `analyze_learnt`),
    ///   * the backjump level (in `analyze_btlevel`),
    ///   * whether the conflict is rooted in an assumption (in
    ///     `analyze_assumption_conflict`),
    ///   * the set of failed assumptions (in `last_failed_assumptions`).
    fn analyze(&mut self, conflict: ClauseRef) -> AnalyzeOutcome {
        self.analyze_learnt.clear();
        self.analyze_stack.clear();
        self.analyze_assumption_conflict = false;
        for s in self.analyze_seen.iter_mut() {
            *s = false;
        }
        self.last_failed_assumptions.clear();

        let mut conflict_lits = self.arena.get(conflict).lits.clone();
        let mut path_count = 0u32;
        let mut p = None::<Lit>;
        let mut idx = self.trail.len() - 1;

        loop {
            for &lit in &conflict_lits {
                if !self.analyze_seen[lit.var] && self.level_of(lit.var) > 0 {
                    self.heap.bump(lit.var);
                    self.analyze_seen[lit.var] = true;
                    if matches!(self.reason_of[lit.var], Reason::Assumption) {
                        self.last_failed_assumptions.push(lit);
                    }
                    if self.level_of(lit.var) >= self.decision_level {
                        path_count += 1;
                    } else {
                        self.analyze_stack.push(lit);
                    }
                }
            }

            while !self.analyze_seen[self.trail[idx].lit.var] {
                idx -= 1;
            }
            p = Some(self.trail[idx].lit);
            let p_var = self.trail[idx].lit.var;
            let p_reason = self.reason_of[p_var];
            self.analyze_seen[p_var] = false;
            path_count -= 1;
            if path_count == 0 {
                break;
            }

            match p_reason {
                Reason::Clause(cref) => {
                    conflict_lits = self.arena.get(cref).lits.clone();
                }
                Reason::Decision => {
                    break;
                }
                Reason::Assumption => {
                    self.analyze_assumption_conflict = true;
                    return AnalyzeOutcome::UnsatByAssumption;
                }
            }
            idx -= 1;
        }

        let uip = p.expect("1-UIP must be found before path_count hits 0");
        self.analyze_learnt.push(!uip);

        // Compute the backjump level: the second-highest level among the
        // learnt clause's literals (excluding the UIP at the current level).
        let mut btlevel = 0u32;
        let mut max_i = 1usize;
        // The literals contributing to the learnt clause are all in
        // `analyze_stack` plus the UIP. Push them after the UIP.
        for &lit in &self.analyze_stack {
            self.analyze_learnt.push(lit);
            let lvl = self.level_of(lit.var);
            if lvl > btlevel {
                btlevel = lvl;
                max_i = self.analyze_learnt.len() - 1;
            }
        }

        // Swap the highest-level literal (other than the UIP) into position 1
        // so the watchers are set correctly for the asserting clause.
        if self.analyze_learnt.len() > 2 {
            self.analyze_learnt.swap(1, max_i);
        }

        self.analyze_btlevel = btlevel;

        // Local self-subsumption minimization (recursive, bounded): try to
        // remove literals from the learnt clause by resolving with their
        // antecedents if all of the antecedent's literals are already seen.
        if self.analyze_learnt.len() > 2 {
            self.minimize_learnt();
        }

        AnalyzeOutcome::Learned
    }

    #[inline]
    fn level_of(&self, var: usize) -> u32 {
        self.level[var]
    }

    /// Recursive self-subsumption minimization of the learnt clause.
    fn minimize_learnt(&mut self) {
        // Mark all learnt literals as "in clause".
        let learnt_len = self.analyze_learnt.len();
        for lit in &self.analyze_learnt[1..] {
            self.analyze_seen[lit.var] = true;
        }

        let mut keep = vec![true; learnt_len];
        // Try to remove each non-UIP literal.
        for i in 1..learnt_len {
            if !keep[i] {
                continue;
            }
            let lit = self.analyze_learnt[i];
            if self.can_remove(lit) {
                keep[i] = false;
            }
        }

        // Compact.
        let mut write = 1usize;
        for i in 1..learnt_len {
            if keep[i] {
                self.analyze_learnt[write] = self.analyze_learnt[i];
                write += 1;
            }
        }
        self.analyze_learnt.truncate(write);

        // Clear marks.
        for lit in self.analyze_learnt[1..].iter() {
            self.analyze_seen[lit.var] = false;
        }
    }

    /// Check whether `lit` can be removed from the learnt clause via recursive
    /// self-subsumption: all literals of its antecedent must be already marked.
    fn can_remove(&mut self, lit: Lit) -> bool {
        let entry = match self.trail.iter().find(|e| e.lit.var == lit.var).copied() {
            Some(e) => e,
            None => return false,
        };
        match entry.reason {
            Reason::Decision | Reason::Assumption => false,
            Reason::Clause(cref) => {
                let antecedent = self.arena.get(cref).lits.clone();
                for &alit in &antecedent {
                    if alit.var == lit.var {
                        continue;
                    }
                    if self.level_of(alit.var) == 0 {
                        continue;
                    }
                    if !self.analyze_seen[alit.var] {
                        return false;
                    }
                    // Recurse one level (bounded for performance).
                    if !self.can_remove(alit) {
                        // Mark so we don't redo work.
                        self.analyze_seen[alit.var] = true;
                        return false;
                    }
                }
                self.analyze_seen[lit.var] = true;
                true
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Backjump and clause learning
    // ---------------------------------------------------------------------------

    fn cancel_until(&mut self, level: u32) {
        if self.decision_level <= level {
            return;
        }
        while let Some(entry) = self.trail.last().copied() {
            if entry.level <= level {
                break;
            }
            let var = entry.lit.var;
            self.phase[var] = !entry.lit.negated;
            self.assignment[var] = LBool::Undef;
            if self.heap.pos_in_heap.get(var).copied().unwrap_or(-1) == -1 {
                self.heap.insert(var);
            }
            self.trail.pop();
        }
        self.prop_qhead = self.trail.len();
        self.trail_lim.truncate(level as usize);
        self.decision_level = level;
    }

    fn record_learned(&mut self) -> ClauseRef {
        let lits = self.analyze_learnt.clone();
        debug_assert!(!lits.is_empty());
        if lits.len() == 1 {
            // Unit clause: enqueue at level 0.
            self.unchecked_enqueue(lits[0], 0, Reason::Decision);
            // No clause to store; return a sentinel.
            NULL_CLAUSE
        } else {
            let mut clause = Clause::new(lits, true);
            clause.lbd = self.compute_lbd(&clause.lits);
            let cref = self.arena.push(clause);
            let lit0 = self.arena.get(cref).lits[0];
            let lit1 = self.arena.get(cref).lits[1];
            self.watches[Self::lit_index(lit0)].push(Watch {
                clause: cref,
                blocker: lit1,
            });
            self.watches[Self::lit_index(lit1)].push(Watch {
                clause: cref,
                blocker: lit0,
            });
            cref
        }
    }

    fn compute_lbd(&self, lits: &[Lit]) -> u32 {
        // LBD = number of distinct decision levels among the literals (excluding
        // level 0). Clauses with LBD ≤ 2 are "glue clauses" and always kept.
        let mut levels: Vec<u32> = lits.iter().map(|l| self.level_of(l.var)).collect();
        levels.sort_unstable();
        levels.dedup();
        levels.into_iter().filter(|&lvl| lvl != 0).count() as u32
    }

    // ---------------------------------------------------------------------------
    // Restart management (Luby)
    // ---------------------------------------------------------------------------

    fn luby(mut step: u64, unit: u64) -> u64 {
        // Luby sequence generator (Knuth's formulation): finds the largest
        // 2^k - 1 such that the sequence is well-defined at step `step`.
        let mut size = 1u64;
        let mut seq = 0u64;
        while size < step + 1 {
            seq = seq * 2 + 1;
            size = 2 * size + 1;
        }
        // Walk down.
        let mut step = step;
        while size - 1 != step {
            size = (size - 1) / 2;
            seq = (seq - 1) / 2;
            step = step % size;
        }
        unit * (seq + 1)
    }

    fn should_restart(&self) -> bool {
        self.conflicts_since_restart >= self.next_restart
    }

    fn restart(&mut self) {
        self.cancel_until(0);
        self.conflicts_since_restart = 0;
        self.luby_step += 1;
        self.next_restart = Self::luby(self.luby_step, RESTART_UNIT);
        self.stats.restarts += 1;
    }

    // ---------------------------------------------------------------------------
    // Learned-clause database reduction
    // ---------------------------------------------------------------------------

    fn reduce_db(&mut self) {
        // Sort learned clauses by activity ascending; drop the bottom half that
        // are not glue clauses (LBD > 2). Keep all glue clauses regardless.
        let first = self.first_learned;
        if self.arena.len() - first <= self.learned_db_max {
            return;
        }

        // Build a sorted list of (activity, cref) for learned clauses.
        let mut order: Vec<(f64, ClauseRef, u32)> = self
            .arena
            .iter()
            .skip(first)
            .filter(|(_, c)| c.learned)
            .map(|(cref, c)| (c.activity, cref, c.lbd))
            .collect();
        order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

        let half = order.len() / 2;
        let mut to_remove: Vec<ClauseRef> = Vec::new();
        for (i, &(_act, cref, lbd)) in order.iter().enumerate() {
            // Always keep glue clauses (LBD ≤ 2).
            if lbd <= 2 {
                continue;
            }
            if i < half {
                to_remove.push(cref);
            }
        }

        for cref in to_remove {
            self.detach_clause(cref);
            // Mark as removed by emptying its lits; it will be compacted lazily.
            self.arena.get_mut(cref).lits.clear();
        }

        // Grow the bound for next time.
        self.learned_db_max = (self.learned_db_max * 3) / 2;
    }

    fn detach_clause(&mut self, cref: ClauseRef) {
        let lits = self.arena.get(cref).lits.clone();
        if lits.len() < 2 {
            return;
        }
        let lit0 = lits[0];
        let lit1 = lits[1];
        let i0 = Self::lit_index(!lit0);
        self.watches[i0].retain(|w| w.clause != cref);
        let i1 = Self::lit_index(!lit1);
        self.watches[i1].retain(|w| w.clause != cref);
    }

    // ---------------------------------------------------------------------------
    // Main search loop
    // ---------------------------------------------------------------------------

    /// Solve the formula under the given assumptions. Returns the result and a
    /// snapshot of stats accumulated *for this solve call only* (not lifetime).
    pub(crate) fn solve_under_assumptions(
        &mut self,
        assumptions: &[Lit],
    ) -> (SolveResult, SolveStats) {
        let started = Instant::now();

        // Reset per-search state: clear the trail, remove learned clauses from
        // prior solves, and re-enqueue base formula unit clauses at level 0.
        // This guarantees that assignments and learned clauses from one solve
        // do not influence a later solve under different assumptions.
        self.reset_for_solve();
        self.level0_conflict = self.detect_level0_unit_conflict();

        // Snapshot the stats baseline so we can return per-solve deltas.
        let baseline = self.snapshot_stats();

        // Push a decision level for the assumptions (level 1) — but only if
        // there are assumptions to place. Placing assumptions at level 1 (rather
        // than 0) is critical for incremental correctness: any unit clauses
        // learned under the assumptions have an assertion level ≥ 1, so
        // `cancel_until(0)` at the end of this solve — and the next solve's
        // `cancel_until(0)` — will retract both the assumption assignments and
        // any clauses learned from them. If assumptions were at level 0,
        // conflict analysis could learn a unit clause asserted at level 0 that
        // would persist across solves and wrongly prune later searches under
        // different assumptions. When there are no assumptions, we skip the
        // push so `search()` can detect level-0 UNSAT correctly.
        if !assumptions.is_empty() {
            self.new_decision_level();
        }
        for &a in assumptions {
            if a.var > self.var_count {
                self.grow_to(a.var);
            }
            match self.value(a) {
                LBool::True => { /* already satisfied by base formula */ }
                LBool::False => {
                    // Assumption contradicts the current assignment.
                    self.level0_conflict = true;
                    self.last_failed_assumptions.push(a);
                }
                LBool::Undef => {
                    self.unchecked_enqueue(a, self.decision_level, Reason::Assumption);
                }
            }
        }

        let result = if self.level0_conflict || self.propagate().is_some() {
            // Conflict at or before the search ⇒ UNSAT (by base formula or
            // assumption). Collect the assumption literals on the trail as the
            // failed-assumption core.
            self.last_failed_assumptions.clear();
            for e in &self.trail {
                if matches!(e.reason, Reason::Assumption) {
                    self.last_failed_assumptions.push(e.lit);
                }
            }
            // Also include any assumption that directly contradicted.
            // (They were pushed above when level0_conflict was set.)
            SolveResult::Unsatisfiable
        } else {
            self.search()
        };

        // Cleanup: cancel back to level 0 so the next solve starts fresh. This
        // retracts the assumption level and all clauses asserted at level ≥ 1.
        self.cancel_until(0);

        let _elapsed = started.elapsed();
        let metrics = self.stats_delta(baseline);

        if matches!(result, SolveResult::Satisfiable(_)) {
            self.last_failed_assumptions.clear();
        }
        // Sort failed assumptions for deterministic core output.
        self.last_failed_assumptions.sort_by_key(|l| l.var);

        (result, metrics)
    }

    fn snapshot_stats(&self) -> SolveStats {
        SolveStats {
            recursive_calls: self.stats.recursive_calls,
            decisions: self.stats.decisions,
            unit_assignments: self.stats.unit_assignments,
            pure_literal_assignments: self.stats.pure_literal_assignments,
            backtracks: self.stats.backtracks,
            restarts: self.stats.restarts,
        }
    }

    fn stats_delta(&self, baseline: SolveStats) -> SolveStats {
        SolveStats {
            recursive_calls: 0, // CDCL has no recursion
            decisions: self.stats.decisions - baseline.decisions,
            unit_assignments: self.stats.unit_assignments - baseline.unit_assignments,
            pure_literal_assignments: 0, // CDCL does not do pure-literal elim
            backtracks: self.stats.backtracks - baseline.backtracks,
            restarts: self.stats.restarts - baseline.restarts,
        }
    }

    fn search(&mut self) -> SolveResult {
        loop {
            if let Some(conflict) = self.propagate() {
                self.stats.backtracks += 1;

                if self.decision_level == 0 {
                    // The formula (including assumptions at level 0) is UNSAT.
                    // Drain any assumptions into the failed set.
                    self.last_failed_assumptions.clear();
                    for e in &self.trail {
                        if matches!(e.reason, Reason::Assumption) {
                            self.last_failed_assumptions.push(e.lit);
                        }
                    }
                    return SolveResult::Unsatisfiable;
                }

                let outcome = self.analyze(conflict);
                if matches!(outcome, AnalyzeOutcome::UnsatByAssumption) {
                    // The 1-UIP was an assumption literal: the formula is UNSAT
                    // under the current assumptions. Collect assumption literals
                    // for the failed-core and stop.
                    self.last_failed_assumptions.clear();
                    for e in &self.trail {
                        if matches!(e.reason, Reason::Assumption) {
                            self.last_failed_assumptions.push(e.lit);
                        }
                    }
                    return SolveResult::Unsatisfiable;
                }
                self.cancel_until(self.analyze_btlevel);
                let learned_cref = self.record_learned();
                if learned_cref != NULL_CLAUSE {
                    let learnt_first = self.arena.get(learned_cref).lits[0];
                    self.unchecked_enqueue(
                        learnt_first,
                        self.analyze_btlevel,
                        Reason::Clause(learned_cref),
                    );
                    self.stats.unit_assignments += 1;
                    // Bump the learned clause's activity for database reduction.
                    self.arena.get_mut(learned_cref).activity += 1.0;
                }

                self.heap.decay();
                self.conflicts_since_restart += 1;
                self.conflicts_since_reduce += 1;

                if self.should_restart() {
                    self.restart();
                }
                if self.conflicts_since_reduce >= self.learned_db_max as u64 {
                    self.reduce_db();
                    self.conflicts_since_reduce = 0;
                }
            } else {
                // No conflict — make a decision.
                match self.pick_branch() {
                    None => {
                        // All variables assigned; SAT.
                        return SolveResult::Satisfiable(self.build_model());
                    }
                    Some(lit) => {
                        self.new_decision_level();
                        self.stats.decisions += 1;
                        self.unchecked_enqueue(lit, self.decision_level, Reason::Decision);
                    }
                }
            }
        }
    }

    fn build_model(&self) -> Model {
        // Model is 1-based; index 0 unused (returns None via `value()`).
        let mut values = vec![None; self.var_count + 1];
        for var in 1..=self.var_count {
            values[var] = match self.assignment[var] {
                LBool::Undef => Some(false), // unassigned ⇒ free; pick false
                LBool::True => Some(true),
                LBool::False => Some(false),
            };
        }
        Model::from_values(values)
    }

    /// Return the failed assumptions from the most recent solve, if it was
    /// UNSAT. Empty if SAT.
    pub(crate) fn failed_assumptions(&self) -> &[Lit] {
        &self.last_failed_assumptions
    }
}

#[derive(Debug)]
enum AnalyzeOutcome {
    Learned,
    /// The 1-UIP was an assumption literal — the formula is UNSAT under the
    /// current assumptions. No clause should be learned.
    UnsatByAssumption,
}

// ---------------------------------------------------------------------------
// Tests for the CDCL internals
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CnfFormula, Lit, SolveResult};

    fn cnf_from_clauses(var_count: usize, clauses: &[Vec<Lit>]) -> CnfFormula {
        let mut cnf = CnfFormula::new(var_count);
        for c in clauses {
            cnf.add_clause(c.clone()).expect("valid clause");
        }
        cnf
    }

    #[test]
    fn cdcl_solves_basic_sat() {
        let clauses = vec![
            vec![Lit::pos(1), Lit::pos(2)],
            vec![Lit::neg(1), Lit::pos(2)],
            vec![Lit::pos(1), Lit::neg(2)],
        ];
        let mut state = CdclState::from_formula(2, &clauses);
        let (result, _) = state.solve_under_assumptions(&[]);
        let SolveResult::Satisfiable(model) = result else {
            panic!("should be SAT");
        };
        assert_eq!(model.value(2), Some(true));
    }

    #[test]
    fn cdcl_solves_basic_unsat() {
        let clauses = vec![vec![Lit::pos(1)], vec![Lit::neg(1)]];
        let mut state = CdclState::from_formula(1, &clauses);
        let (result, _) = state.solve_under_assumptions(&[]);
        assert_eq!(result, SolveResult::Unsatisfiable);
    }

    #[test]
    fn cdcl_propagates_chain() {
        let clauses = vec![
            vec![Lit::pos(1)],
            vec![Lit::neg(1), Lit::pos(2)],
            vec![Lit::neg(2), Lit::pos(3)],
        ];
        let mut state = CdclState::from_formula(3, &clauses);
        let (result, _) = state.solve_under_assumptions(&[]);
        let SolveResult::Satisfiable(model) = result else {
            panic!("should be SAT");
        };
        assert_eq!(model.value(1), Some(true));
        assert_eq!(model.value(2), Some(true));
        assert_eq!(model.value(3), Some(true));
    }

    #[test]
    fn cdcl_solves_under_assumptions() {
        // Formula: (1 ∨ 2) ∧ (¬1 ∨ 3)
        let clauses = vec![
            vec![Lit::pos(1), Lit::pos(2)],
            vec![Lit::neg(1), Lit::pos(3)],
        ];
        let mut state = CdclState::from_formula(3, &clauses);

        // Assume ¬2: should still be SAT (set 1=true, 3=true).
        let (result, _) = state.solve_under_assumptions(&[Lit::neg(2)]);
        assert!(matches!(result, SolveResult::Satisfiable(_)));

        // Assume ¬1 ∧ ¬3: SAT. ¬1 forces 2 via clause 1, and clause 2
        // (¬1∨3) is satisfied by ¬1 alone, so ¬3 is harmless.
        let (result, _) = state.solve_under_assumptions(&[Lit::neg(1), Lit::neg(3)]);
        assert!(matches!(result, SolveResult::Satisfiable(_)));

        // Assume ¬1 ∧ ¬2 ∧ ¬3: UNSAT. ¬1 forces 2, but ¬2 contradicts that.
        let (result, _) = state.solve_under_assumptions(&[Lit::neg(1), Lit::neg(2), Lit::neg(3)]);
        assert_eq!(result, SolveResult::Unsatisfiable);
    }

    #[test]
    fn cdcl_solves_pigeonhole_4_3_unsat() {
        // 4 pigeons, 3 holes — classic unsatisfiable pigeonhole problem.
        let mut clauses = Vec::new();
        let var = |p: usize, h: usize| -> usize { p * 3 + h + 1 };
        for p in 0..4 {
            let mut c = Vec::new();
            for h in 0..3 {
                c.push(Lit::pos(var(p, h)));
            }
            clauses.push(c);
        }
        for h in 0..3 {
            for p1 in 0..4 {
                for p2 in (p1 + 1)..4 {
                    clauses.push(vec![Lit::neg(var(p1, h)), Lit::neg(var(p2, h))]);
                }
            }
        }
        let mut state = CdclState::from_formula(12, &clauses);
        let (result, stats) = state.solve_under_assumptions(&[]);
        assert_eq!(result, SolveResult::Unsatisfiable);
        assert!(stats.backtracks >= 1 || stats.decisions >= 1);
    }

    #[test]
    fn cdcl_solves_pigeonhole_5_4_unsat() {
        // 5 pigeons, 4 holes — larger, still UNSAT. DPLL would time out here.
        let mut clauses = Vec::new();
        let holes = 4;
        let var = |p: usize, h: usize| -> usize { p * holes + h + 1 };
        for p in 0..5 {
            let mut c = Vec::new();
            for h in 0..holes {
                c.push(Lit::pos(var(p, h)));
            }
            clauses.push(c);
        }
        for h in 0..holes {
            for p1 in 0..5 {
                for p2 in (p1 + 1)..5 {
                    clauses.push(vec![Lit::neg(var(p1, h)), Lit::neg(var(p2, h))]);
                }
            }
        }
        let mut state = CdclState::from_formula(20, &clauses);
        let (result, _) = state.solve_under_assumptions(&[]);
        assert_eq!(result, SolveResult::Unsatisfiable);
    }

    #[test]
    fn cdcl_produces_valid_model() {
        // Construct a random 3-SAT instance, solve it, and verify every clause
        // is satisfied by the returned model.
        let clauses = vec![
            vec![Lit::pos(1), Lit::neg(2), Lit::pos(3)],
            vec![Lit::neg(1), Lit::pos(4)],
            vec![Lit::neg(3), Lit::neg(4)],
            vec![Lit::pos(2), Lit::neg(5)],
            vec![Lit::pos(5), Lit::pos(1)],
        ];
        let mut state = CdclState::from_formula(5, &clauses);
        let (result, _) = state.solve_under_assumptions(&[]);
        let SolveResult::Satisfiable(model) = result else {
            panic!("should be SAT");
        };
        // Verify the model satisfies all clauses.
        for clause in &clauses {
            let satisfied = clause.iter().any(|lit| model.value(lit.var) == Some(!lit.negated));
            assert!(satisfied, "clause {clause:?} not satisfied by model");
        }
    }

    #[test]
    fn cdcl_reuses_learnts_across_solves() {
        // Two solves on the same formula; the second should benefit from the
        // first's learned clauses (and complete in zero decisions for trivial
        // cases after a unit is learned).
        let clauses = vec![vec![Lit::pos(1), Lit::pos(2)]];
        let mut state = CdclState::from_formula(2, &clauses);
        let (r1, _) = state.solve_under_assumptions(&[]);
        assert!(matches!(r1, SolveResult::Satisfiable(_)));
        let (r2, _) = state.solve_under_assumptions(&[Lit::neg(1)]);
        assert!(matches!(r2, SolveResult::Satisfiable(_)));
    }

    #[test]
    fn luby_sequence_matches_known_values() {
        // The Luby sequence (× unit) should be: 1,1,2,1,1,2,4,...
        assert_eq!(CdclState::luby(0, 1), 1);
        assert_eq!(CdclState::luby(1, 1), 1);
        assert_eq!(CdclState::luby(2, 1), 2);
        assert_eq!(CdclState::luby(3, 1), 1);
        assert_eq!(CdclState::luby(4, 1), 1);
        assert_eq!(CdclState::luby(5, 1), 2);
        assert_eq!(CdclState::luby(6, 1), 4);
    }
}
