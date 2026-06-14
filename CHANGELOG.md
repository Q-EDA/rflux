# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased] — Engineering quality, security, and release-readiness

### Added

#### Community health & licensing
- `LICENSE-MIT` and `LICENSE-APACHE`: full license texts to back the SPDX
  `MIT OR Apache-2.0` declaration (previously declared but absent from the tree).
- `SECURITY.md`: vulnerability reporting policy, supported-versions table,
  scope statement (including the external-command surface for JoSIM).
- `CODE_OF_CONDUCT.md`: Contributor Covenant 2.1.
- `.github/CODEOWNERS`: per-crate ownership routing for PR review.

#### CI pipeline
- `cargo audit` and `cargo deny check advisories bans licenses sources` are now
  hard gates on every push/PR (previously `continue-on-error`, which silently
  swallowed advisories and even masked their exit code via `| head -100`).
- New `coverage` job: `cargo-llvm-cov --workspace --exclude rflux-py` produces
  lcov and uploads to Codecov (informational, non-blocking).
- `codecov.yml`: project/patch status config and ignore rules.

#### Rust crates
- `rflux-route`: congestion-aware JTL/PTL routing.
- `rflux-flow`: `build_clock_tree` and `build_bias_grid` flow entry points and tests.
- Structured `RFLOW-*` error codes layered across all crate error types; CLI
  `classify_cli_error` now consumes crate-native `code()` / `suggestion()`.
- `rflux-py`: static registration guards in a new `#[cfg(test)]` module that
  catch duplicate or missing `#[pyfunction]` / `#[pyclass]` registrations
  (the `_core` entry-point body is refactored into a testable `register_module`
  helper).
- `rflux-verify`: verification scenario tests.
- `rflux-flow`: PDK flow-level regression tests.
- `rflux-hdl`: remaining `LogicOp` variant and error-path tests.
- Windows path handling for the simulation external-command allowlist.

#### Documentation
- `README.md`: badge row (CI / license / Rust / Python / codecov / deps.rs),
  table of contents, dedicated License section, and a note flagging the
  legacy encoding-corrupted body as a known pending rewrite.
- `CONTRIBUTING.md` / `CHANGELOG.md`: placeholder `your-org` URLs corrected to
  `Q-EDA/rflux`.

### Fixed
- `rflux-py` `_core` module: removed duplicate `build_clock_tree` /
  `build_bias_grid` registrations that silently overwrote the earlier binding.
- `python/tests/contracts/python_api_surface.json`: regenerated to include
  `build_clock_tree` and `build_bias_grid`, unblocking the surface-contract test.
- Cross-platform Windows path handling in the `rflux-sim` external-command
  allowlist.
- `rflux-flow` test compilation errors.

#### Clippy / formatting
- Workspace clippy: 153 automated fixes + 8 `#[must_use]` attributes; warning
  count reduced from ~500 to ~202 (lib-only).

### Removed
- Repo-root `_fix_ci.py` and `.github/workflows/ci.yml.head` / `ci.yml.tail`:
  leftover one-shot CI-debugging artifacts (the script hard-coded a
  `C:\Users\lilu\...` path and the `.head`/`.tail` files were split halves of
  `ci.yml`).

### Infrastructure
- `clippy.toml`: workspace-level clippy configuration (cognitive-complexity,
  argument-count thresholds).
- `rust-toolchain.toml`: stable toolchain with `clippy` + `rustfmt` components.
- `deny.toml`: cargo-deny configuration (license, advisory, duplicate-crate,
  source-registry checks).
- `pyrightconfig.json`: Python type checking (basic mode).
- `.github/dependabot.yml`: dependency update tracking (Cargo, pip, GHA).

---

## [0.1.0] ? Initial prototype

### Added
- 14-crate Rust workspace covering SFQ EDA flow (IR, synthesis, placement, routing,
  timing, simulation, verification, I/O, CLI, Python bindings)
- Python bindings via PyO3 + maturin
- uv-based Python dependency management
- CI pipeline with Rust tests, Python tests, waveform comparison, quality baselines
- Phase-based execution checklists and release readiness templates
- JoSIM waveform comparison infrastructure
- CLI performance baseline capture and regression gate

[Unreleased]: https://github.com/Q-EDA/rflux/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Q-EDA/rflux/releases/tag/v0.1.0
