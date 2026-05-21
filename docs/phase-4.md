# Phase 4 - Advanced Features

Status: complete.

This phase starts by adding prototype analysis interfaces for higher-order SFQ concerns without forcing a full device-level solver into the core flow.

## Implemented so far

- Added a prototype statistical timing analysis path on top of deterministic STA.
  - `rflux-timing` now provides a lightweight SSTA model with path-accumulated setup sigma estimation, device-aware cell sigma sensitivity, route-aware wire sigma sensitivity, optional global-correlation sigma terms, clock-uncertainty sigma, cross-domain uncertainty sigma, crossing-kind categorized cross-domain sigma, and pessimistic setup/hold slack.
  - `rflux-flow` exposes `analyze_timing_statistical`.
  - PyO3 and the Python facade expose the same API and per-arc statistical summaries, including global-correlation, clock-uncertainty, cross-domain, and crossing-kind categorized uncertainty inputs.
- Added a prototype AC bias analysis path in `rflux-flow`.
  - `analyze_ac_bias` reports JTL carrier candidates, PTL coupling risk routes, estimated static power savings, area overhead ratio, frequency derate ratio, timing guardband, a coarse feasibility score, and a multi-objective optimization score.
  - `optimize_ac_bias` now adds a lightweight dual-parameter scan over PTL preference and routing detour margin, returning before/after AC bias reports selected by an explicit timing-aware multi-objective score.
  - PyO3 and the Python facade expose the same AC bias analysis and optimization summaries.
- Added a prototype compound-cell characterization path in `rflux-flow`.
  - `characterize_compound_cell` compiles a candidate compound cell, derives timing-library-ready delay/setup/hold values, and reuses the simulation hook to preserve generated deck and waveform metadata.
  - PyO3 and the Python facade expose the same characterization summary.
- Added prototype electro-thermal-mechanical and manufacturing-constraint analysis.
  - `analyze_advanced_constraints` reports estimated thermal load, mechanical stress score, JTL density, detour overhead ratio, PTL coupling ratio, manufacturing hotspots, and structured violations against configurable budgets.
  - PyO3 and the Python facade expose the same advanced constraint summary.
- Expanded `rflux-hdl` into a minimal Rust-native SFQ builder DSL.
  - The crate now supports ports, logic cells, macro cells, DFFs, splitters, and explicit connections as a Rust-first front-end to `rflux-ir`.

## Beyond Phase 4

- waveform-aware SFQ timing distributions and richer correlation modeling beyond the current device-aware and route-aware sigma prototype
- optimization loops that feed SSTA, AC bias, and advanced-constraint results back into placement/routing/synthesis beyond the current lightweight candidate scans
- richer proc-macro syntax and compile-time diagnostics on top of the current `rflux-hdl` builder DSL
- tighter coupling between compound-cell characterization and generated technology-library artifacts