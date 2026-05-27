# rflux Interface Inventory

## 1. Purpose

This document records the user-visible interfaces that currently form an external contract in the repository, with emphasis on frontend capability boundaries, default behavior, and compatibility surfaces.

It is a current-state inventory, not a promise that every interface is fully mature or feature-complete.

## 2. Public Interface Surfaces

### 2.1 CLI commands

Current top-level `rflux-cli` commands:

- `pdk-minimal`
- `lint-input`
- `compile-netlist`
- `compile-layout`
- `analyze-timing`
- `verify-layout`
- `simulate-file`
- `solve-dimacs`
- `check-equivalence`

### 2.2 Python package surface

The top-level package `python/rflux/__init__.py` remains backward-compatible and still re-exports the main public API through `__all__`.

The runtime surface is now also structured into importable submodules with real implementation ownership:

- `rflux.flow`: compile, planning, layout, AC-bias, and library-aware flow APIs
- `rflux.timing`: timing reports, timing constraints, and timing analysis entry points
- `rflux.sim`: simulation reports and text/file simulation entry points
- `rflux.verify`: equivalence and layout verification entry points
- `rflux.pdk`: `Pdk` and cell-library metadata types

Top-level imports such as `rflux.compile_layout` remain supported as compatibility re-exports from `python/rflux/__init__.py`.

Stub files are provided alongside the runtime modules:

- `python/rflux/__init__.pyi`
- `python/rflux/flow.pyi`
- `python/rflux/timing.pyi`
- `python/rflux/sim.pyi`
- `python/rflux/verify.pyi`
- `python/rflux/pdk.pyi`

### 2.3 File and schema interfaces

Current external file contracts include:

- IR JSON read/write
- PDK JSON read/write
- bench subset netlist import
- LEF/DEF import and export helpers
- CLI JSON reports
- Python `Circuit.to_json()` / `from_json()`
- Python `Pdk.to_json()` / `from_json()`

## 3. Frontend Capability Snapshot

### 3.1 IR JSON

- Reader: `rflux_io::read_ir_json`
- Writer: `rflux_io::write_ir_json`
- Schema mode: versioned envelope is the primary contract
- Compatibility: legacy raw JSON is still accepted on read for repository compatibility
- Hierarchy preservation: none
- Source mapping: none
- Error location: JSON parser failures now surface line and column details when available

### 3.2 bench subset

- Reader: `rflux_io::read_bench_netlist`
- Writer: none
- Supported scope: flat combinational bench subset used by current tests and flow integration
- Hierarchy preservation: none
- Source mapping: no persistent source map, but parse diagnostics now report source line numbers
- Error location: malformed declarations, unsupported gate forms, duplicate definitions, and dependency issues are surfaced with line-aware diagnostics when the failing line is known

### 3.3 LEF/DEF

- Readers: `read_lef`, `read_def`, `read_lef_to_chip`, `read_lef_def_to_chip`
- Writers: `write_def_from_chip`, `write_lef_from_chip`
- Position in product: physical import/export utility surface, not yet a complete signoff-grade roundtrip frontend
- Current limitation: roundtrip completeness is improving, but not all source fidelity and semantic preservation guarantees are established yet

### 3.4 Missing or limited frontend features

The following are not yet mature repository-wide frontend guarantees:

- Verilog subset frontend
- BLIF subset frontend
- hierarchy-preserving import
- persistent source-map export across frontend formats
- full lint schema introspection for every input family
- complete LEF/DEF roundtrip fidelity guarantees

## 4. Default Behavior and Reporting

### 4.1 CLI defaults

- `pdk-minimal --name` defaults to `minimal-sfq`
- most flow commands fall back to `Pdk::minimal("minimal-sfq")` when `--pdk` is omitted
- `verify-layout` and `simulate-file` default `--mode` to `auto`
- `check-equivalence` defaults `--kind` to `combinational`
- `compile-layout`, `analyze-timing`, and `verify-layout` currently start from `FlowConfig::default()` when no explicit override is provided

### 4.2 `lint-input` contract

`lint-input` is the canonical frontend inspection entry point for repository-supported input formats.

Current success reports include:

- `schema_format`
- `input_schema_version`
- `legacy_compatibility_used`
- `schema_contract`: explicit contract metadata for the accepted schema mode
- `frontend_summary`: reader identity plus frontend capability snapshot for the input family
- `netlist_summary` for netlist-bearing inputs, including node/edge counts and structural kind counts

Current failure reports route parse errors through typed CLI diagnostics. For malformed JSON and bench parsing failures, the detail text now includes line-aware location data when available.

## 5. Python Runtime Availability Rules

The repository still supports importing `rflux` without a compiled extension for development ergonomics, but not all high-level APIs are available in that state.

Current expectations:

- `core_status()` is the authoritative probe for extension availability and import diagnostics
- simple model/container APIs remain importable for lightweight tests and tooling
- high-level flow, timing, verification, and most compilation-backed entry points fail explicitly when the required core implementation is unavailable
- the new `rflux.flow`, `rflux.timing`, `rflux.sim`, `rflux.verify`, and `rflux.pdk` modules do not introduce new fallback semantics; the top-level package re-exports their behavior for compatibility

## 6. Compatibility Policy Snapshot

- top-level Python imports remain supported for backward compatibility
- structured submodules are the primary ownership boundary and the preferred import surface
- versioned envelope JSON remains the primary schema contract for repository-owned JSON formats
- legacy raw JSON acceptance is compatibility behavior, not the preferred long-term write format

## 7. Current Maturity Labels

- `stable`: IR JSON envelope, PDK JSON envelope, basic CLI contract reporting, top-level Python compatibility exports
- `limited`: bench subset import, `lint-input` frontend summaries, LEF/DEF utility import-export, Python structured submodule split
- `experimental`: broader HDL frontend coverage, hierarchy-preserving import, durable source-map fidelity, signoff-grade LEF/DEF roundtrip completeness

## 8. Near-Term Follow-up

1. Extend `lint-input` to cover richer capability/schema summaries for additional frontend families.
2. Add frontend contracts for Verilog or BLIF only when the supported subset and diagnostics policy are explicit.
3. Tighten LEF/DEF roundtrip coverage with fidelity-focused regression tests before upgrading maturity level.