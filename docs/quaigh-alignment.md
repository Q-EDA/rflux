# Quaigh Alignment Matrix (rflux-synth)

Date: 2026-05-21

## Scope

This document tracks behavior alignment between rflux-synth boolean optimization and Quaigh.

Upstream reference:
- crate: quaigh 0.0.6
- repository: https://github.com/Coloquinte/quaigh

## Upstream anchors used

The following upstream modules/tests were used as reference anchors:
- src/optim/share_logic.rs: test_flatten_and, test_flatten_xor, test_share_and
- src/network/network.rs: test_dedup
- src/optim/infer_gates.rs: infer_xor_mux, infer_dffe
- src/network/gates.rs: canonicalization rules for And/Xor/Mux
- src/equiv.rs: test_equiv_xor, test_equiv_mux, test_equiv_xor3, test_equiv_andn, test_equiv_xorn

## Alignment status

Legend:
- pass: behavior validated by rflux-synth unit test
- partial: partially aligned, constrained by current IR/optimizer shape
- gap: not yet implemented in rflux-synth

| Behavior area | Quaigh reference | rflux-synth test | Status | Notes |
|---|---|---|---|---|
| Commutative dedup and sharing | network::test_dedup, share_logic::test_share_and | internal_boolean_optimization_deduplicates_equivalent_logic | pass | Equivalent cones merge to one gate. |
| Deep AND flatten plus sharing | share_logic::test_flatten_and | internal_boolean_optimization_deduplicates_deep_commutative_cones | pass | Current flattening config converges to a single shared AND gate. |
| XOR sharing control | optim::infer_xor_mux flow + share_logic flattening | quaigh_alignment_respects_xor_sharing_toggle | pass | infer_xor_mux controls sharing behavior in current implementation. |
| MUX canonical non-commutative ordering | gates canonical mux checks | quaigh_alignment_keeps_mux_data_order_semantics | pass | Distinct data-order muxes are not wrongly merged. |
| Absorption simplification | (normalization simplification family) | internal_boolean_optimization_eliminates_and_absorption_redundancy, internal_boolean_optimization_eliminates_or_absorption_redundancy | pass | Added in rflux-synth as extra simplification beyond prior baseline. |
| Pattern-based factoring from AND/OR structures | share_logic and normalization-related rewrites | internal_boolean_optimization_factors_or_of_and_common_term | partial | rflux-synth now factors OR(AND(a,b),AND(a,c)) => AND(a, OR(b,c)) on a safe subset. |
| Pattern-based XOR/MUX reconstruction from AND network | optim::infer_gates::infer_xor_mux | none | gap | Full Quaigh-style XOR/MUX reconstruction from complemented AND structures is still missing in current IR model. |
| DFFE reconstruction from mux structure | optim::infer_gates::infer_dffe | none | gap | rflux-synth currently reuses explicit DffEnable nodes; no structural reconstruction pass yet. |
| Equivalence-proof style regression corpus | equiv.rs suite | quaigh_alignment_fixture_cases + Compiler::check_boolean_equivalence_sat | partial | SAT-based combinational equivalence is now integrated for fixture before/after checks; full equiv.rs parity corpus is still pending. |

## Local validation commands

- uv run cargo test -p rflux-synth
- uv run cargo test -p rflux-synth internal_boolean_optimization_deduplicates_deep_commutative_cones
- uv run cargo test -p rflux-synth quaigh_alignment_keeps_mux_data_order_semantics
- uv run cargo test -p rflux-synth quaigh_alignment_respects_xor_sharing_toggle

Fixture-driven alignment harness:

- integration test: `crates/synth/tests/quaigh_alignment_fixtures.rs`
- fixture directory: `crates/synth/tests/fixtures/quaigh_alignment/`
- per-fixture SAT check: baseline vs optimized must be combinationally equivalent
- SAT mismatch diagnostics now include both counterexample primary-input assignments and per-output lhs/rhs values (via `Compiler::check_boolean_equivalence_sat`).
- SAT reports now also include solver activity counters (`recursive_calls`, `decisions`, `unit_assignments`, `pure_literal_assignments`, `backtracks`, `restarts`).
- fixture regression now emits SAT trend lines per fixture plus an aggregate summary (`quaigh_fixture_sat_summary`) including `total_restarts` and `max_elapsed_ns`.
- fixture regression also writes a CSV artifact by default to `target/quaigh_fixture_sat_metrics.csv`.
- current fixture scenarios: 10
	- dedup_and_pair_from_bench
	- dedup_and_pair
	- flatten_and_deep
	- factor_or_of_and_common_term
	- xor3_chain_from_bench
	- andn4_chain_from_bench
	- xorn4_chain_from_bench
	- mux_data_order_distinct
	- xor_toggle_pair_enabled
	- xor_toggle_pair (sharing disabled)

Classic end-to-end examples:

- integration test: `crates/synth/tests/end_to_end_classic_examples.rs`
- fixture directory: `crates/synth/tests/fixtures/classic_examples/`
- current classic scenarios:
	- `classic_and8_chain.json` (AND chain flattening)
	- `classic_xor8_chain.json` (XOR parity chain flattening)
	- `classic_mux4_tree.json` (4:1 MUX tree, no unsafe merge)
	- `classic_mux8_tree.json` (8:1 MUX tree, deeper selector layering)
	- `classic_majority3.json` (majority-of-3 canonical SOP form)
	- `classic_dual_product4.json` (commutative dual-cone sharing to a single product)
	- `classic_full_adder.json` (sum/carry combinational datapath)
	- `classic_ripple_adder4.json` (4-bit ripple-carry adder, multi-output SAT regression)
- validation: gate count monotonicity + SAT equivalence report availability (`sat_stats`, `sat_elapsed_ns`).

### Measured fixture gate-count results

| Fixture | Before | After | Expected behavior |
|---|---:|---:|---|
| dedup_and_pair_from_bench | 2 | 1 | dedup shared AND cone |
| dedup_and_pair | 2 | 1 | dedup shared AND cone |
| flatten_and_deep | 3 | 1 | deep AND flatten + merge |
| factor_or_of_and_common_term | 3 | 2 | factor OR(AND,AND) by common term |
| xor3_chain_from_bench | 2 | 1 | xor3 associative flattening shape |
| andn4_chain_from_bench | 3 | 1 | andn chain flattening shape |
| xorn4_chain_from_bench | 3 | 1 | xorn chain flattening shape |
| mux_data_order_distinct | 2 | 2 | preserve non-commutative MUX order |
| xor_toggle_pair_enabled | 2 | 1 | xor sharing enabled |
| xor_toggle_pair | 2 | 2 | xor sharing disabled |

Each row above now also has a SAT equivalence assertion between baseline and optimized netlists in the fixture harness.

Trend capture note:

- run `uv run cargo test -p rflux-synth --test quaigh_alignment_fixtures -- --nocapture`
- capture the emitted `fixture=...` lines and `quaigh_fixture_sat_summary` line as the evolving baseline.
- run `uv run cargo test -p rflux-synth --test end_to_end_classic_examples -- --nocapture` to exercise classic end-to-end circuits.
- CSV output path can be overridden with `RFLUX_QUAIGH_METRICS_CSV=/your/path/metrics.csv`.

The added `xor3_chain_from_bench`, `andn4_chain_from_bench`, and `xorn4_chain_from_bench` fixtures are mapped to Quaigh `equiv`-style operator shapes (`test_equiv_xor3`, `test_equiv_andn`, and `test_equiv_xorn`) within the current IR subset.

SAT stress baseline direction:

- `rflux-sat` now includes stress tests for:
	- pigeonhole UNSAT (4->3 and larger 5->4 baselines)
	- DIMACS medium SAT instances
	- deterministic synthetic SAT large-clause baseline (24 vars, 120 clauses)

SAT DIMACS end-to-end classics:

- integration test: `crates/sat/tests/dimacs_end_to_end.rs`
- fixture directory: `crates/sat/tests/fixtures/`
- current DIMACS scenarios:
	- `sat_3var_implication.cnf`
	- `sat_exactly_one_4.cnf`
	- `unsat_unit_contradiction.cnf`
	- `unsat_pigeonhole_3_2.cnf`
- validation: fixture file parse + expected SAT/UNSAT + solver metrics availability (`recursive_calls`, `elapsed_ns`).
- output artifact: default CSV at `target/dimacs_sat_metrics.csv` with per-fixture rows and summary row (including `restarts`).
- override output path: set `RFLUX_DIMACS_METRICS_CSV=/your/path/dimacs_metrics.csv`.

### Bench-to-fixture conversion path

- converter script: `python/scripts/convert_quaigh_bench_to_ir_fixture.py`
- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/dedup_and_pair.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/dedup_and_pair_from_bench.json`

Example conversion command:

```bash
uv run python python/scripts/convert_quaigh_bench_to_ir_fixture.py \
  --input-bench crates/synth/tests/fixtures/quaigh_alignment/bench/dedup_and_pair.bench \
  --output-json crates/synth/tests/fixtures/quaigh_alignment/dedup_and_pair_from_bench.json \
  --overwrite
```

### Pending acceptance gates (not yet enabled)

- `quaigh_alignment_pending_reconstructs_xor_from_and_pattern`
- `quaigh_alignment_pending_reconstructs_mux_from_and_pattern`

These tests are intentionally `#[ignore]` until inversion-aware IR pattern representation is available.

## Next increments for full parity evidence

1. Add a small AND-pattern matcher pass for xor/mux reconstruction, then mirror Quaigh infer_gates examples.
2. Add dffe-from-mux reconstruction parity tests.
3. Add an optional equivalence-check harness (or deterministic truth-table checks for bounded input sizes) to replicate key equiv.rs scenarios.
4. Add fixture-level benchmark import for selected ISCAS .bench examples used in Quaigh docs.
