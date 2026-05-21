# Phase 0 Completion

Date: 2026-05-19

## Checklist

- [x] Workspace and crate interfaces are created.
- [x] Python uv baseline is initialized (`pyproject.toml`, `.python-version`, `uv.lock`).
- [x] PyO3 + maturin skeleton is ready in `crates/py` and `python/rflux`.
- [x] `rflux-ir` has a single-consumer ownership prototype.
- [x] `rflux-tech` defines PDK abstraction and PTL forbidden-length query.
- [x] `rflux-io` supports JSON IR/PDK read-write.
- [x] `rflux-io` supports LEF/DEF parsing/import and DEF export via `libreda-lefdef` + `libreda-db`.
- [x] RustSFQ/cmtrs boundary is decided for `rflux-hdl`.

## LEF/DEF Scope in Phase 0

`libreda-lefdef` currently provides robust LEF/DEF parsing and DEF writing.
LEF writing is not implemented upstream. In phase 0, `rflux-io` exposes:

- read LEF AST
- read DEF AST
- build DB from LEF
- import DEF into existing DB
- export DB to DEF

The LEF writer API in `rflux-io` returns an explicit unsupported error to avoid silent failures.

## Minimal DSL Boundary (RustSFQ/cmtrs Investigation Outcome)

`rflux-hdl` phase-0 boundary is intentionally thin:

- own the API surface for constructing SFQ-oriented netlists
- keep syntax lightweight (builder-style) before macro DSL
- output only `rflux-ir::Netlist`

Deferred to phase 1+:

- proc-macro syntax inspired by RustSFQ/cmtrs
- compile-time fanout diagnostics wired directly into DSL syntax
- SPICE-oriented emitters from HDL layer

## Validation Commands

- `cargo check --workspace`
- `cargo test --workspace`
- `uv sync --all-groups`
- `uv run pytest`
