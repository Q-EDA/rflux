# Phase 5 - Library and Feedback Integration

Status: **complete**.

This phase turns Phase 4 prototypes into reusable flow artifacts that can feed later optimization and technology modeling loops.

## Implemented

### Characterization → library artifacts

- `rflux-flow::characterize_compound_cell` emits a generated library artifact (cell + timing + metadata), not only a timing-library-ready summary.
- `rflux-tech`: `CharacterizedCellLibraryEntry`, `Pdk::with_characterized_library_json`, name-indexed timing overrides.
- `rflux-synth` / `rflux-timing` prefer exact cell-name matches before kind-level defaults.
- SSTA derives cell sigma from characterized metadata; advanced constraints reflect mapped-area changes.

### Multi-entry library assembly

- `CharacterizedCellLibraryBundle`, `Pdk::with_characterized_library_entries`, merge JSON helpers.
- `CharacterizationArtifactMetadata`: waveform path, simulated vs STA delay, calibration sigma, `delay_details`, `arc_delays`.
- Placement halo and routing detour / PTL preference scale from characterized cell area and pipeline depth in `compile_artifacts`.

### Simulation → arc delay mapping

- Endpoint refs (`from` / `to` in delay detail lines) map directly to `CharacterizationArcDelay`.
- **Name-heuristic matching** when refs are absent: detail names like `source_to_gate` match netlist edges by driver/sink tokens (order-independent).
- Positional zip remains as fallback for unmatched details.

### Library-aware optimization

- `optimize_ac_bias_with_characterized_library`: routing thresholds + constraints.
- `optimize_design_with_characterized_library`: joint search over
  - routing (`prefer_ptl_from_length_um`, `detour_margin_um`),
  - **placement** (`FlowConfig.placement_halo_scale` on library-aware macro halo),
  - **SSTA** (`StatisticalTimingConfig` candidates including calibration-aware sigma from merged library).
- Scoring: AC bias + pessimistic SSTA + advanced constraints (`design_optimization_score`).

### Python / PyO3

- `merge_characterized_library`, `optimize_design_with_characterized_library`, `LibraryAwareDesignOptimizationReport` (routing + placement + statistical tuning fields).
- `analyze_timing` / `analyze_timing_statistical` accept `characterized_library_entries`.
- Mixed install: `uv run maturin develop` (see root `pyproject.toml` `[tool.maturin]`).
- Script: `python/scripts/characterize_merge_optimize.py`
- Notebook: `python/notebooks/phase5_characterize_merge_optimize.ipynb`
- Workflow: [phase-5-workflow.md](./phase-5-workflow.md)

## Verification

```bash
cargo test -p rflux-tech -p rflux-flow -p rflux-timing
uv run maturin develop
uv run pytest
uv run python python/scripts/characterize_merge_optimize.py
```

The Phase 5 workflow script also has an explicit CI smoke anchor for its characterize/merge/optimize path:

- `uv run pytest python/tests/test_basic.py -k "merge_characterized_library_round_trip or optimize_design_with_characterized_library_workflow" -q`
