# Phase 5 Workflow: Characterize → Merge → Optimize

This guide walks through the Python end-to-end loop for library artifact feedback in rflux.

## Prerequisites

```bash
uv sync
uv run maturin develop
```

## 1. Characterize a compound cell

Build a candidate macro netlist and run characterization. The generated JSON includes cell timing, metadata, `delay_details`, and per-arc `arc_delays` when simulation provides endpoint refs.

```python
import rflux

char_circuit = rflux.Circuit("compound")
source = char_circuit.add_node("port", "source")
gate = char_circuit.add_node("cell", "gate")
sink = char_circuit.add_node("port", "sink")
char_circuit.connect(source, 0, gate, 0)
char_circuit.connect(gate, 0, sink, 0)

char_report = rflux.characterize_compound_cell(char_circuit, cell_name="macro_buf")
print(char_report.generated_library_json)
```

## 2. Merge into a PDK

Merge one or more characterization artifacts into a technology library snapshot.

```python
# Option A: merge JSON strings
merged_pdk_json = rflux.merge_characterized_library([char_report.generated_library_json])

# Option B: incremental Pdk helper
pdk = rflux.Pdk.minimal().merge_characterized_library_json(char_report.generated_library_json)
```

## 3. Consume the library in timing / layout

Pass characterized entries into flow APIs:

```python
consumer = rflux.Circuit("top")
# ... build consumer netlist with MacroCell named "macro_buf" ...

layout = rflux.compile_layout(
    consumer,
    characterized_library_entries=[char_report.generated_library_json],
)
timing = rflux.analyze_timing(
    consumer,
    characterized_library_entries=[char_report.generated_library_json],
)
```

STA uses name-indexed cell timing and per-pin `arc_delays` when present.

## 4. Library-aware optimization

### AC bias only

```python
ac_report = rflux.optimize_ac_bias_with_characterized_library(
    consumer,
    [char_report.generated_library_json],
)
```

### AC bias + SSTA + constraints (design loop)

Co-optimizes routing thresholds, placement macro halo scale, and SSTA sigma knobs:

```python
design_report = rflux.optimize_design_with_characterized_library(
    consumer,
    [char_report.generated_library_json],
)
print(design_report.design_optimization_score)
print(design_report.optimized_statistical.worst_pessimistic_setup_slack_ps)
print(design_report.optimized_placement_halo_scale)
print(design_report.optimized_cell_delay_sigma_ratio)
```

## Script and notebook

```bash
uv run python python/scripts/characterize_merge_optimize.py
```

Interactive walkthrough: [python/notebooks/phase5_characterize_merge_optimize.ipynb](../python/notebooks/phase5_characterize_merge_optimize.ipynb)

### Design optimization (AC bias + SSTA + constraints)

```python
design = rflux.optimize_design_with_characterized_library(
    consumer,
    [char_report.generated_library_json],
)
print(design.design_optimization_score)
print(design.optimized_statistical.worst_pessimistic_setup_slack_ps)
```

## Rust API equivalents

| Python | Rust (`rflux-flow` / `rflux-tech`) |
|--------|-------------------------------------|
| `merge_characterized_library` | `Pdk::merge_characterized_library_json_strings` |
| `Pdk.merge_characterized_library_json` | `Pdk::with_characterized_library_json` |
| `optimize_ac_bias_with_characterized_library` | `FlowRunner::optimize_ac_bias_with_characterized_library` |
| `optimize_design_with_characterized_library` | `FlowRunner::optimize_design_with_characterized_library` |
| arc-level STA delay | `Pdk::characterized_arc_delay_ps` via `rflux-timing::arc_components_ps` |
