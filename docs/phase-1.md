# Phase 1 Completion

Date: 2026-05-19

## Status

Phase 1 goals are complete for the current scaffolded scope.

Exit criteria now covered:

- logic-synthesis entry points exist in Rust and Python
- splitter insertion and path-balancing DFF insertion are executable and tested
- a minimal SFQ technology-mapping/report path is in place
- internal pure-Rust boolean optimization and compatibility analysis exist for the supported combinational subset
- the unified synthesis pipeline returns concrete reports instead of placeholders

## Implemented in this step

- Added `rflux-synth` crate to workspace.
- Implemented a minimal `Compiler::compile()` driver API.
- Implemented splitter auto-insertion pass for fanout requests:
  - first sink connects directly
  - second sink triggers insertion of an auto-generated `Splitter` node
  - rewires previous sink through splitter outputs
- Added synthesis unit test for fanout-to-splitter rewrite.
- Extended `rflux-ir::Netlist` with pass-friendly helpers:
  - `sink_of()`
  - `disconnect()`
  - `is_input_driven()`
  - `nodes()`
- Added path-balancing primitive pass: insert a DFF on an existing connection.
- Added executable SFQ technology mapping against `rflux-tech::Pdk` cell library:
  - concrete `SfCellLibrary` in `rflux-tech`
  - node-to-cell mapping in `rflux-synth`
  - area report generation (`TechMapReport`)
- Added an internal pure-Rust boolean optimization path:
  - shared-subexpression merging for equivalent combinational cones
  - bounded AND flattening driven by `BoolOptConfig`
  - compatibility analysis for the currently supported combinational subset
  - no external C dependency, keeping the core wasm-friendly
- Added `compile_plan` batch API with `CompileReport`:
  - batch connection application
  - splitter insertion accounting
  - balancing strategy selection (`None`, `Explicit`, `AllConnectedSources`)
  - explicit balancing-DFF insertion list
- Added synth <-> io smoke test: JSON IR read/write around a synth pass.
- Added Python phase-1 facade `rflux.compile(circuit)` (placeholder passthrough).
- Added Python-side plan types aligned with Rust synthesis API:
  - `PinRef`
  - `ConnectionSpec`
  - `CompilePlan`
  - `BalanceStrategy`
  - `compile_plan(circuit, plan)`
- Added `BySinkLevel` balancing strategy in Rust and Python.
- Upgraded `BySinkLevel` from a single-pass heuristic to a graph-based longest-path analysis:
  - explicit `PathBalanceReport`
  - stable under out-of-order edge insertion
  - reusable imbalance analysis before DFF insertion
- Added a unified `compile_netlist(...)` synthesis pipeline in `rflux-synth`:
  - compile plan execution
  - internal boolean-network rewrite on the supported combinational subset
  - post-pass path balancing summary
  - boolean optimization summary
  - technology mapping summary
  - boolean-optimization compatibility summary
- Added Python `compile_plan_report()` facade:
  - prefers real PyO3 backend when available
  - falls back to a local report model that mirrors current Rust behavior
- Upgraded Python `Circuit` integration:
  - extension-backed `Circuit` now owns a real Rust `Netlist`
  - `compile_plan` mutates the circuit's netlist state
  - Python can query `node_count()` and `edge_count()` after synthesis
- Exposed the unified synthesis pipeline to Python via `rflux.compile_netlist(...)` with a real Rust-backed `SynthesisReport`.

## Deferred Beyond Phase 1

- richer technology mapping driven by characterized SFQ library data instead of the current minimal built-in library.
- richer global path balancing strategy beyond current longest-path sink balancing.
- Python `Circuit` still lacks full IR inspection/edit APIs beyond minimal node/edge operations.

## Quaigh Alignment Validation (Proxy Set)

The original Quaigh dependency has been removed from `rflux-synth`, so parity is currently tracked through behavior-oriented proxy cases in the synth unit tests.

Current validated coverage:

- absorption-law redundancy elimination (`a & (a | b) = a`, `a | (a & b) = a`)
  - `internal_boolean_optimization_eliminates_and_absorption_redundancy`
  - `internal_boolean_optimization_eliminates_or_absorption_redundancy`
- deep commutative cone canonicalization and sharing
  - `internal_boolean_optimization_deduplicates_deep_commutative_cones`
- non-commutative mux data-order semantics are preserved
  - `quaigh_alignment_keeps_mux_data_order_semantics`
- xor sharing obeys configuration toggle (`infer_xor_mux`)
  - `quaigh_alignment_respects_xor_sharing_toggle`

Test locations:

- `crates/synth/src/lib.rs` unit tests (see test names above).

Execution status on this workspace:

- `uv run cargo test -p rflux-synth` passes with all current synth tests green.
- focused runs for the three newest alignment tests also pass.

Remaining gap to full Quaigh parity evidence:

- no upstream Quaigh public test corpus is currently imported into this repo.
- once an upstream test source is available, add direct fixture import and a side-by-side result matrix (gate count delta, preserved node kinds, and rewrite legality).

Detailed tracking now lives in `docs/quaigh-alignment.md`.
