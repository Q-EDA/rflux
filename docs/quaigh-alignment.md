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
| Pattern-based XOR/MUX reconstruction from AND network | optim::infer_gates::infer_xor_mux | quaigh_alignment_reconstructs_xor_from_and_pattern, quaigh_alignment_reconstructs_mux_from_and_pattern | pass | Complemented AND cones now reconstruct to XOR/MUX in the current IR subset. |
| DFFE reconstruction from mux structure | optim::infer_gates::infer_dffe | internal_boolean_optimization_rewrites_mux_feedback_dff_to_dffe, internal_boolean_optimization_rewrites_inverted_mux_feedback_dff_to_dffe, internal_boolean_optimization_rewrites_wrapped_mux_feedback_dff_to_dffe, quaigh_alignment_sequential_fixture_cases | partial | rflux-synth now normalizes the standard `q_next = enable ? data : q_prev` mux-feedback form into explicit `DffEnable`, including the mirrored arm ordering that requires an inserted inverted-enable node, a single passthrough wrapper on the mux-to-DFF data path (`Splitter`/`Jtl`/`Ptl`), wrapped feedback/clock fixture variants, the combined inverted-enable plus wrapped-feedback case, and the combined inverted-enable plus wrapped-clock case. More general sequential motifs are still out of scope. |
| Equivalence-proof style regression corpus | equiv.rs suite | quaigh_alignment_fixture_cases + Compiler::check_boolean_equivalence_sat | partial | SAT-based combinational equivalence is integrated for fixture before/after checks, and the synth-side checker now consumes `rflux-sat::IncrementalSolver` with per-output assumptions instead of cloning a fresh solve formula per query. Full equiv.rs parity corpus is still pending. |

Sequential equivalence note:

- `Compiler::check_sequential_equivalence_sat` now provides a single-step sequential SAT check for the current `Dff`/`DffEnable` subset.
- The check shares same-named present-state variables across both netlists, compares observable outputs, and compares each same-named state's next-state and clock functions.
- State matching is currently name-based by design; mismatched state-name sets are rejected up front as an interface mismatch.
- This path is intentionally narrower than full sequential equivalence; it is designed to validate the current DFFE normalization slice without changing the existing combinational SAT interface.

## Local validation commands

- uv run cargo test -p rflux-synth
- uv run cargo test -p rflux-synth internal_boolean_optimization_deduplicates_deep_commutative_cones
- uv run cargo test -p rflux-synth quaigh_alignment_keeps_mux_data_order_semantics
- uv run cargo test -p rflux-synth quaigh_alignment_respects_xor_sharing_toggle
- uv run cargo test -p rflux-synth internal_boolean_optimization_rewrites_mux_feedback_dff_to_dffe
- uv run cargo test -p rflux-synth internal_boolean_optimization_rewrites_inverted_mux_feedback_dff_to_dffe
- uv run cargo test -p rflux-synth internal_boolean_optimization_rewrites_wrapped_mux_feedback_dff_to_dffe
- uv run cargo test -p rflux-synth sequential_sat_equivalence_finds_transition_counterexample

Fixture-driven alignment harness:

- integration test: `crates/synth/tests/quaigh_alignment_fixtures.rs`
- fixture directory: `crates/synth/tests/fixtures/quaigh_alignment/`
- per-fixture SAT check: baseline vs optimized must be combinationally equivalent
- SAT mismatch diagnostics now include both counterexample primary-input assignments and per-output lhs/rhs values (via `Compiler::check_boolean_equivalence_sat`).
- SAT reports now also include solver activity counters (`recursive_calls`, `decisions`, `unit_assignments`, `pure_literal_assignments`, `backtracks`, `restarts`).
- fixture regression now emits SAT trend lines per fixture plus an aggregate summary (`quaigh_fixture_sat_summary`) including `total_restarts` and `max_elapsed_ns`.
- fixture regression also writes a CSV artifact by default to `target/quaigh_fixture_sat_metrics.csv`.

- current fixture scenarios: 65
	- dedup_and_pair_from_bench
	- dedup_and_pair
	- flatten_and_deep
	- factor_or_of_and_common_term
	- factor_and_of_or_common_term
	- absorb_and_subset
	- absorb_or_subset
	- xor_from_and_pattern
	- mux_from_and_pattern
	- xor_from_or_pattern
	- mux_from_or_pattern
	- factor_then_xor_from_and_pattern
	- factor_then_mux_from_and_pattern
	- factor_then_xor_from_or_pattern
	- factor_then_mux_from_or_pattern
	- consensus_or_redundancy
	- consensus_and_redundancy
	- xor3_chain_from_bench
	- aoi31_from_bench
	- oai31_from_bench
	- aoi211_from_bench
	- oai211_from_bench
	- aoi311_from_bench
	- oai311_from_bench
	- aoi321_from_bench
	- oai321_from_bench
	- aoi322_from_bench
	- oai322_from_bench
	- aoi421_from_bench
	- oai421_from_bench
	- aoi422_from_bench
	- oai422_from_bench
	- aoi431_from_bench
	- oai431_from_bench
	- aoi432_from_bench
	- oai432_from_bench
	- aoi433_from_bench
	- oai433_from_bench
	- aoi441_from_bench
	- oai441_from_bench
	- aoi442_from_bench
	- oai442_from_bench
	- aoi443_from_bench
	- oai443_from_bench
	- aoi444_from_bench
	- oai444_from_bench
	- aoi2221_from_bench
	- oai2221_from_bench
	- aoi222_from_bench
	- oai222_from_bench
	- aoi221_from_bench
	- oai221_from_bench
	- aoi22_from_bench
	- oai22_from_bench
	- aoi21_from_bench
	- oai21_from_bench
	- majority3_from_bench
	- andn4_chain_from_bench
	- nand_nor_pair_from_bench
	- iscas_c17_from_bench
	- xorn4_chain_from_bench
	- xnor_pair_from_bench
	- mux_data_order_distinct
	- xor_toggle_pair_enabled
	- xor_toggle_pair (sharing disabled)

Sequential fixture alignment harness:

- integration test: `crates/synth/tests/sequential_alignment_fixtures.rs`
- fixture directory: `crates/synth/tests/fixtures/quaigh_alignment/`
- validation style: structural post-opt assertions only; these fixtures intentionally do not go through `Compiler::check_boolean_equivalence_sat` because the current SAT equivalence path is combinational-only and rejects `Dff`/`DffEnable`
- current sequential scenarios:
	- `dffe_feedback_wrapped.json` (feedback arm wrapped by a passthrough `Jtl` before re-entering the mux hold arm)
	- `dffe_clock_wrapped.json` (clock input wrapped by a passthrough `Jtl` while the mux-feedback DFFE rewrite still converges)
	- `dffe_inverted_wrapped.json` (mirrored-arm DFFE form with wrapped feedback and inserted inverted-enable normalization)
	- `dffe_inverted_clock_wrapped.json` (mirrored-arm DFFE form with wrapped clock input and inserted inverted-enable normalization)

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
| factor_and_of_or_common_term | 3 | 2 | factor AND(OR,OR) by common term |
| absorb_and_subset | 3 | 1 | absorb redundant AND subset |
| absorb_or_subset | 3 | 1 | absorb redundant OR subset |
| xor_from_and_pattern | 5 | 1 | reconstruct XOR from complemented AND structure |
| mux_from_and_pattern | 4 | 1 | reconstruct MUX from complemented AND structure |
| xor_from_or_pattern | 5 | 1 | reconstruct XOR from complemented OR structure |
| mux_from_or_pattern | 4 | 1 | reconstruct MUX from complemented OR structure |
| factor_then_xor_from_and_pattern | 5 | 2 | factor then reconstruct XOR from AND structure |
| factor_then_mux_from_and_pattern | 4 | 2 | factor then reconstruct MUX from AND structure |
| factor_then_xor_from_or_pattern | 5 | 2 | factor then reconstruct XOR from OR structure |
| factor_then_mux_from_or_pattern | 4 | 2 | factor then reconstruct MUX from OR structure |
| consensus_or_redundancy | 5 | 1 | remove OR consensus redundancy |
| consensus_and_redundancy | 5 | 1 | remove AND consensus redundancy |
| xor3_chain_from_bench | 2 | 1 | xor3 associative flattening shape |
| aoi31_from_bench | 4 | 3 | lower AOI31 into current IR, then flatten the AND side of the frontend-lowered cone |
| oai31_from_bench | 4 | 3 | lower OAI31 into current IR, then flatten the OR side of the frontend-lowered cone |
| aoi211_from_bench | 4 | 3 | lower AOI211 into current IR, then flatten the OR side of the frontend-lowered cone |
| oai211_from_bench | 4 | 3 | lower OAI211 into current IR, then flatten the AND side of the frontend-lowered cone |
| aoi311_from_bench | 5 | 3 | lower AOI311 into current IR, then flatten the OR side of the frontend-lowered cone more aggressively than the initial estimate |
| oai311_from_bench | 5 | 3 | lower OAI311 into current IR, then flatten the AND side of the frontend-lowered cone more aggressively than the initial estimate |
| aoi321_from_bench | 6 | 3 | lower AOI321 into current IR, then flatten both repeated AND and OR stages into the same three-gate shape |
| oai321_from_bench | 6 | 3 | lower OAI321 into current IR, then flatten both repeated OR and AND stages into the same three-gate shape |
| aoi322_from_bench | 6 | 5 | lower AOI322 into current IR, preserving the three product terms while flattening the outer OR side by one gate |
| oai322_from_bench | 6 | 5 | lower OAI322 into current IR, preserving the three sum terms while flattening the outer AND side by one gate |
| aoi421_from_bench | 7 | 4 | lower AOI421 into current IR, then collapse the chained four-input product side and outer OR side more aggressively than the initial estimate |
| oai421_from_bench | 7 | 4 | lower OAI421 into current IR, then collapse the chained four-input sum side and outer AND side more aggressively than the initial estimate |
| aoi422_from_bench | 8 | 5 | lower AOI422 into current IR, keeping the three product terms but flattening the outer OR side to five gates overall |
| oai422_from_bench | 8 | 5 | lower OAI422 into current IR, keeping the three sum terms but flattening the outer AND side to five gates overall |
| aoi431_from_bench | 8 | 4 | lower AOI431 into current IR, then collapse the chained four-input product side, the chained three-input product side, and the outer OR side more aggressively than the initial estimate |
| oai431_from_bench | 8 | 4 | lower OAI431 into current IR, then collapse the chained four-input sum side, the chained three-input sum side, and the outer AND side more aggressively than the initial estimate |
| aoi432_from_bench | 9 | 5 | lower AOI432 into current IR, keeping the three product terms while flattening the outer OR side and the two chained inner product sides enough to reach the same five-gate post-opt shape as AOI422 |
| oai432_from_bench | 9 | 5 | lower OAI432 into current IR, keeping the three sum terms while flattening the outer AND side and the two chained inner sum sides enough to reach the same five-gate post-opt shape as OAI422 |
| aoi433_from_bench | 10 | 5 | lower AOI433 into current IR, keeping the three product terms while flattening both three-input inner product chains and the outer OR side into the same five-gate post-opt family as AOI432 |
| oai433_from_bench | 10 | 5 | lower OAI433 into current IR, keeping the three sum terms while flattening both three-input inner sum chains and the outer AND side into the same five-gate post-opt family as OAI432 |
| aoi441_from_bench | 9 | 4 | lower AOI441 into current IR, then collapse both four-input product chains and the outer OR side aggressively enough to match the same four-gate post-opt family as AOI431 |
| oai441_from_bench | 9 | 4 | lower OAI441 into current IR, then collapse both four-input sum chains and the outer AND side aggressively enough to match the same four-gate post-opt family as OAI431 |
| aoi442_from_bench | 10 | 5 | lower AOI442 into current IR, keeping the three product terms while flattening both four-input inner product chains plus the outer OR side into the same five-gate post-opt family as AOI432 and AOI433 |
| oai442_from_bench | 10 | 5 | lower OAI442 into current IR, keeping the three sum terms while flattening both four-input inner sum chains plus the outer AND side into the same five-gate post-opt family as OAI432 and OAI433 |
| aoi443_from_bench | 11 | 5 | lower AOI443 into current IR, keeping the three product terms while flattening the two four-input inner product chains, the three-input third product chain, and the outer OR side into the same five-gate post-opt family as AOI432 through AOI442 |
| oai443_from_bench | 11 | 5 | lower OAI443 into current IR, keeping the three sum terms while flattening the two four-input inner sum chains, the three-input third sum chain, and the outer AND side into the same five-gate post-opt family as OAI432 through OAI442 |
| aoi444_from_bench | 12 | 5 | lower AOI444 into current IR, keeping the three four-input product terms while flattening the outer OR side into the same five-gate post-opt family as AOI422 through AOI443 |
| oai444_from_bench | 12 | 5 | lower OAI444 into current IR, keeping the three four-input sum terms while flattening the outer AND side into the same five-gate post-opt family as OAI422 through OAI443 |
| aoi2221_from_bench | 7 | 5 | lower AOI2221 into current IR, then flatten the OR side of the frontend-lowered cone |
| oai2221_from_bench | 7 | 5 | lower OAI2221 into current IR, then flatten the AND side of the frontend-lowered cone |
| aoi222_from_bench | 6 | 5 | lower AOI222 into current IR, then flatten the OR side of the frontend-lowered cone |
| oai222_from_bench | 6 | 5 | lower OAI222 into current IR, then flatten the AND side of the frontend-lowered cone |
| aoi221_from_bench | 5 | 4 | lower AOI221 into current IR, then flatten the OR side of the frontend-lowered cone |
| oai221_from_bench | 5 | 4 | lower OAI221 into current IR, then flatten the AND side of the frontend-lowered cone |
| aoi22_from_bench | 4 | 4 | preserve AOI22 frontend-lowered AND+AND+OR+NOT shape |
| oai22_from_bench | 4 | 4 | preserve OAI22 frontend-lowered OR+OR+AND+NOT shape |
| aoi21_from_bench | 3 | 3 | preserve AOI21 frontend-lowered AND+OR+NOT shape |
| oai21_from_bench | 3 | 3 | preserve OAI21 frontend-lowered OR+AND+NOT shape |
| majority3_from_bench | 5 | 4 | lower MAJ into current IR, then factor the majority cone |
| andn4_chain_from_bench | 3 | 1 | andn chain flattening shape |
| nand_nor_pair_from_bench | 4 | 4 | preserve NAND/NOR frontend-lowered AND+NOT and OR+NOT shapes |
| iscas_c17_from_bench | 12 | 12 | preserve imported ISCAS c17 NAND-lowered benchmark shape |
| xorn4_chain_from_bench | 3 | 1 | xorn chain flattening shape |
| xnor_pair_from_bench | 2 | 2 | preserve XNOR frontend-lowered XOR+NOT shape |
| mux_data_order_distinct | 2 | 2 | preserve non-commutative MUX order |
| xor_toggle_pair_enabled | 2 | 1 | xor sharing enabled |
| xor_toggle_pair | 2 | 2 | xor sharing disabled |

Each row above now also has a SAT equivalence assertion between baseline and optimized netlists in the fixture harness.

Trend capture note:

- run `uv run cargo test -p rflux-synth --test quaigh_alignment_fixtures -- --nocapture`
- capture the emitted `fixture=...` lines and `quaigh_fixture_sat_summary` line as the evolving baseline.
- run `uv run cargo test -p rflux-synth --test end_to_end_classic_examples -- --nocapture` to exercise classic end-to-end circuits.
- CSV output path can be overridden with `RFLUX_QUAIGH_METRICS_CSV=/your/path/metrics.csv`.

These Quaigh-alignment harness commands now have explicit CI smoke anchors instead of relying only on broad workspace coverage:

- `cargo test -p rflux-synth --test quaigh_alignment_fixtures -- --nocapture`
- `cargo test -p rflux-synth --test end_to_end_classic_examples -- --nocapture`
- `cargo test -p rflux-sat --test dimacs_end_to_end -- --nocapture`

The added `xor3_chain_from_bench`, `andn4_chain_from_bench`, and `xorn4_chain_from_bench` fixtures are mapped to Quaigh `equiv`-style operator shapes (`test_equiv_xor3`, `test_equiv_andn`, and `test_equiv_xorn`) within the current IR subset. The checked-in `aoi31_from_bench`, `oai31_from_bench`, `aoi211_from_bench`, `oai211_from_bench`, `aoi311_from_bench`, `oai311_from_bench`, `aoi321_from_bench`, `oai321_from_bench`, `aoi322_from_bench`, `oai322_from_bench`, `aoi421_from_bench`, `oai421_from_bench`, `aoi422_from_bench`, `oai422_from_bench`, `aoi431_from_bench`, `oai431_from_bench`, `aoi432_from_bench`, `oai432_from_bench`, `aoi433_from_bench`, `oai433_from_bench`, `aoi441_from_bench`, `oai441_from_bench`, `aoi442_from_bench`, `oai442_from_bench`, `aoi443_from_bench`, `oai443_from_bench`, `aoi444_from_bench`, `oai444_from_bench`, `aoi2221_from_bench`, `oai2221_from_bench`, `aoi222_from_bench`, `oai222_from_bench`, `aoi221_from_bench`, `oai221_from_bench`, `aoi22_from_bench`, `oai22_from_bench`, `aoi21_from_bench`, `oai21_from_bench`, `majority3_from_bench`, `nand_nor_pair_from_bench`, and `xnor_pair_from_bench` fixtures now also travel through the same harness, locking the current frontend-lowering contracts for `AOI31 -> NOT(OR(AND(AND(a,b), c), d))` with current post-opt AND flattening to three gates, `OAI31 -> NOT(AND(OR(OR(a,b), c), d))` with current post-opt OR flattening to three gates, `AOI211 -> NOT(OR(OR(AND(a,b), c), d))` with current post-opt OR flattening to three gates, `OAI211 -> NOT(AND(AND(OR(a,b), c), d))` with current post-opt AND flattening to three gates, `AOI311 -> NOT(OR(OR(AND(AND(a,b), c), d), e))` with current post-opt OR flattening to three gates, `OAI311 -> NOT(AND(AND(OR(OR(a,b), c), d), e))` with current post-opt AND flattening to three gates, `AOI321 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), e), f))` with current post-opt flattening to three gates, `OAI321 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), e), f))` with current post-opt flattening to three gates, `AOI322 -> NOT(OR(OR(AND(a,b,c), AND(d,e)), AND(f,g)))` with current post-opt flattening to five gates, `OAI322 -> NOT(AND(AND(OR(a,b,c), OR(d,e)), OR(f,g)))` with current post-opt flattening to five gates, `AOI421 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(e,f)), g))` with current post-opt flattening to four gates, `OAI421 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(e,f)), g))` with current post-opt flattening to four gates, `AOI422 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(e,f)), AND(g,h)))` with current post-opt flattening to five gates, `OAI422 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(e,f)), OR(g,h)))` with current post-opt flattening to five gates, `AOI431 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(AND(e,f), g)), h))` with current post-opt flattening to four gates, `OAI431 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(OR(e,f), g)), h))` with current post-opt flattening to four gates, `AOI432 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(AND(e,f), g)), AND(h,i)))` with current post-opt flattening to five gates, `OAI432 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(OR(e,f), g)), OR(h,i)))` with current post-opt flattening to five gates, `AOI433 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(AND(e,f), g)), AND(AND(h,i), j)))` with current post-opt flattening to five gates, `OAI433 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(OR(e,f), g)), OR(OR(h,i), j)))` with current post-opt flattening to five gates, `AOI441 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(AND(AND(e,f), g), h)), i))` with current post-opt flattening to four gates, `OAI441 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(OR(OR(e,f), g), h)), i))` with current post-opt flattening to four gates, `AOI442 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(AND(AND(e,f), g), h)), AND(i,j)))` with current post-opt flattening to five gates, `OAI442 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(OR(OR(e,f), g), h)), OR(i,j)))` with current post-opt flattening to five gates, `AOI443 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(AND(AND(e,f), g), h)), AND(AND(i,j), k)))` with current post-opt flattening to five gates, `OAI443 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(OR(OR(e,f), g), h)), OR(OR(i,j), k)))` with current post-opt flattening to five gates, `AOI444 -> NOT(OR(OR(AND(AND(AND(a,b), c), d), AND(AND(AND(e,f), g), h)), AND(AND(AND(i,j), k), l)))` with current post-opt flattening to five gates, `OAI444 -> NOT(AND(AND(OR(OR(OR(a,b), c), d), OR(OR(OR(e,f), g), h)), OR(OR(OR(i,j), k), l)))` with current post-opt flattening to five gates, `AOI2221 -> NOT(OR(OR(OR(AND(a,b), AND(c,d)), AND(e,f)), g))` with current post-opt OR flattening to five gates, `OAI2221 -> NOT(AND(AND(AND(OR(a,b), OR(c,d)), OR(e,f)), g))` with current post-opt AND flattening to five gates, `AOI222 -> NOT(OR(OR(AND(a,b), AND(c,d)), AND(e,f)))` with current post-opt OR flattening to five gates, `OAI222 -> NOT(AND(AND(OR(a,b), OR(c,d)), OR(e,f)))` with current post-opt AND flattening to five gates, `AOI221 -> NOT(OR(OR(AND(a,b), AND(c,d)), e))` with current post-opt OR flattening to four gates, `OAI221 -> NOT(AND(AND(OR(a,b), OR(c,d)), e))` with current post-opt AND flattening to four gates, `AOI22 -> NOT(OR(AND(a,b), AND(c,d)))`, `OAI22 -> NOT(AND(OR(a,b), OR(c,d)))`, `AOI21 -> NOT(OR(AND(a,b), c))`, `OAI21 -> NOT(AND(OR(a,b), c))`, `MAJ -> OR(OR(AND(a,b), AND(a,c)), AND(b,c))`, `NAND -> AND + NOT`, `NOR -> OR + NOT`, and `XNOR -> XOR + NOT`. The first selected ISCAS benchmark import is still seeded by `iscas_c17_from_bench`, keeping a small real benchmark in the same regression corpus while additional external ISCAS sources remain pending.

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

Additional checked-in frontend-lowering samples:

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi31.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi31_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai31.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai31_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi211.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi211_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai211.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai211_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi311.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi311_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai311.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai311_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi321.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi321_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai321.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai321_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi322.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi322_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai322.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai322_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi421.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi421_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai421.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai421_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi422.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi422_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai422.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai422_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi431.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi431_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai431.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai431_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi432.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi432_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai432.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai432_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi433.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi433_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai433.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai433_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi441.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi441_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai441.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai441_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi442.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi442_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai442.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai442_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi443.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi443_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai443.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai443_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi444.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi444_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai444.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai444_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi2221.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi2221_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai2221.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai2221_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi222.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi222_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai222.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai222_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi221.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi221_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai221.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai221_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi22.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi22_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai22.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai22_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/aoi21.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/aoi21_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/oai21.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/oai21_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/majority3.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/majority3_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/nand_nor_pair.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/nand_nor_pair_from_bench.json`

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/xnor_pair.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/xnor_pair_from_bench.json`

Selected small benchmark import:

- sample bench input: `crates/synth/tests/fixtures/quaigh_alignment/bench/iscas_c17.bench`
- generated fixture sample: `crates/synth/tests/fixtures/quaigh_alignment/iscas_c17_from_bench.json`

Example conversion command:

```bash
uv run python python/scripts/convert_quaigh_bench_to_ir_fixture.py \
  --input-bench crates/synth/tests/fixtures/quaigh_alignment/bench/dedup_and_pair.bench \
  --output-json crates/synth/tests/fixtures/quaigh_alignment/dedup_and_pair_from_bench.json \
  --overwrite
```

The converter command also has an explicit CI smoke anchor now instead of remaining a doc-only local helper:

- `uv run pytest python/tests/test_quaigh_bench_converter.py -q`

That pytest coverage now regenerates and compares every checked-in `bench/*.bench -> *_from_bench.json` pair in this fixture directory, so new frontend-lowering samples become part of the converter regression as soon as both files are added.

The CLI side now has a matching sweep-style smoke anchor for the same checked-in bench corpus:

- `uv run cargo test -p rflux-cli checked_in_bench_fixtures -- --nocapture`

That CLI filter runs both self-equivalence and `compile-netlist` diagnostics coverage across every checked-in Quaigh bench fixture. The handwritten `NAND`/`NOR` CLI tests remain as direct inline lowering regressions alongside the checked-in corpus sweep.

Bench inputs captured in diagnostics bundles are now labeled as `bench_text` / `quaigh_bench_subset` contracts instead of producing a spurious JSON inspection error, which keeps imported benchmark fixtures such as `iscas_c17.bench` clean in `run-with-diagnostics` manifests.

### Pending acceptance gates (not yet enabled)

- `quaigh_alignment_pending_reconstructs_xor_from_and_pattern`
- `quaigh_alignment_pending_reconstructs_mux_from_and_pattern`

These tests are intentionally `#[ignore]` until inversion-aware IR pattern representation is available.

## Next increments for full parity evidence

1. Add a small AND-pattern matcher pass for xor/mux reconstruction, then mirror Quaigh infer_gates examples.
2. Extend DFFE-from-mux normalization beyond the current single-register feedback motif plus one-layer passthrough wrappers to broader sequential wrappers.
3. Add an optional equivalence-check harness (or deterministic truth-table checks for bounded input sizes) to replicate more key equiv.rs scenarios.
4. Extend fixture-level benchmark import beyond the current `iscas_c17` seed to additional selected ISCAS `.bench` examples used in Quaigh docs.
