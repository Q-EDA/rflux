# Contributing to rflux

Thank you for your interest in contributing to rflux, the Rust-first SFQ EDA toolkit.

This document describes the development workflow, code conventions, and PR process.

---

## Quick start

`ash
# Prerequisites
#   - Rust toolchain (installed via rustup)
#   - uv (Python package manager, https://docs.astral.sh/uv/)
#   - Python >= 3.12

# Clone and set up
git clone https://github.com/Q-EDA/rflux
cd rflux
uv sync
cargo check --workspace

# Build Python extension (optional)
uv run maturin develop -m crates/py/Cargo.toml
`

---

## Repository structure

`
rflux/
??? crates/          # 14 Rust crates (rflux-ir, rflux-sim, rflux-cli, ...)
??? python/          # Python bindings, scripts, tests
?   ??? rflux/       # Pure Python facade + PyO3 extension
?   ??? scripts/     # Batch processing, benchmarks, CI helpers
?   ??? tests/       # pytest test suite
??? docs/            # Architecture and design documentation
?   ??? archive/     # Archived process/policy documents
??? .github/         # CI workflows, issue templates
`

See [docs/project-design.md](docs/project-design.md) for the full crate map.

---

## Before you start: read the rules

### Python dependencies ? uv only

This repository uses **uv** for ALL Python dependency management. Do not use pip install, poetry, or hand-maintained equirements.txt.

- Dependencies go in pyproject.toml under [project] or [dependency-groups]
- Lock file: uv.lock (committed)
- Run Python tools: uv run pytest, uv run maturin develop, etc.

See [AGENTS.md](AGENTS.md) for detailed rules.

### Rust conventions

- Workspace-level clippy.toml and ust-toolchain.toml define lint thresholds and toolchain version
- Run cargo fmt before committing
- Run cargo clippy --workspace to check for warnings
- Add #[must_use] to all pub fn returning owned values
- Avoid .unwrap() in production code; prefer ?, expect(), or proper error propagation
- Use 	hiserror for error types

---

## Pull request process

1. **Create an issue** first for significant changes (new features, API breaks, refactoring)
2. **Branch naming**: codex/<short-description> (or your own prefix)
3. **Before opening a PR**, ensure:
   - cargo check --workspace passes
   - cargo test --workspace passes
   - cargo fmt --check passes
   - cargo clippy --workspace has no new warnings
   - Python tests pass: uv run pytest python/tests/
   - If you changed Rust crate public API, update .pyi stubs in python/rflux/
4. **PR title**: conventional commits style (eat:, ix:, chore:, docs:, efactor:)
5. **PR description**: link to the issue, describe what changed and why

### CI gates

The CI pipeline (.github/workflows/ci.yml) runs these checks on every push:

| Gate | Enforcement |
|------|-------------|
| cargo fmt --check | Hard ? fails the build |
| cargo clippy | Soft ? warnings allowed (for now) |
| cargo test --workspace | Hard |
| uff check python/ | Soft |
| Python tests (uv run pytest) | Hard |
| cargo audit | Soft (optional dispatch) |
| cargo deny | Soft (optional dispatch) |

---

## Code review guidelines

- Reviewers check for correctness, test coverage, and adherence to conventions
- All review comments should be addressed before merging
- Prefer small, focused PRs over large monolithic ones
- Benchmark regressions should be documented in the PR description

---

## Testing

- Rust unit tests: inline #[cfg(test)] mod tests blocks
- Rust integration tests: crates/*/tests/ directory
- Python tests: python/tests/test_*.py
- Performance benchmarks: crates/synth/benches/equivalence_bench.rs (via criterion)
- To run benchmarks: cargo bench -p rflux-synth

### Test categories

| Layer | Test framework | Command |
|-------|---------------|---------|
| Rust lib | #[test] | cargo test --workspace |
| Python | pytest | uv run pytest python/tests/ |
| Benchmarks | criterion | cargo bench -p rflux-synth |

---

## Documentation

- Rust public API: add /// doc comments to all public types and methods
- Architecture docs: edit docs/project-design.md for structural changes
- Python API: update .pyi stubs in python/rflux/ when Rust API changes

To preview Rust docs:
`ash
cargo doc --no-deps --open
`

---

## Getting help

- Open an issue for bugs, feature requests, or questions
- For urgent matters, contact the maintainers through the issue tracker

Thank you for contributing!
