# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased] ? Engineering quality initiatives

### Added

#### Infrastructure
- clippy.toml: workspace-level clippy configuration (cognitive-complexity, arg thresholds)
- ust-toolchain.toml: pinned stable Rust toolchain with clippy + rustfmt components
- deny.toml: cargo-deny configuration (license, advisory, duplicate-crate checks)
- pyrightconfig.json: Python type checking configuration (basic mode)
- .github/dependabot.yml: automated dependency update tracking (Cargo, pip, GHA)
- CONTRIBUTING.md: contributor guide covering PR process, conventions, CI gates
- CHANGELOG.md: this file ? project changelog

#### CI pipeline
- cargo fmt --check: hard gate on every push
- cargo clippy --workspace: soft gate (continue-on-error)
- uff check python/: Python lint check
- cargo audit + cargo deny: security and dependency checks (optional dispatch)

#### Documentation
- API documentation comments (///) added to 5 core crates:
  - flux-ir: all public types and methods
  - flux-tech: all public types and methods
  - flux-io: all public types
  - flux-timing: all public types
  - flux-flow: all public types
- docs/ slimmed from 55 to 16 substantive files; 39 process/template/dated docs
  moved to docs/archive/

#### Performance
- Criterion benchmark skeleton for flux-synth boolean equivalence checking:
  - synth/boolean_equivalence/and: ~5 ?s
  - synth/boolean_equivalence/chain/4: ~10 ?s
  - synth/boolean_equivalence/chain/8: ~15.5 ?s
  - synth/boolean_equivalence/chain/16: ~23.7 ?s

#### Python package
- maturin build verified: produces 2.1 MB installable wheel
- All 155 Python tests pass (21 skipped ? external tool dependencies)

### Fixed

#### Clippy warnings
- Automated fixes: 153 clippy suggestions applied via cargo clippy --fix --workspace
- Manual fixes: 8 #[must_use] attributes, 3 doc comment formatting issues, 1
  map-or pattern rewrite
- E0271 type inference blocker resolved: route crate map().unwrap_or() replaced
  with llow + let binding pattern
- Route strict-f64 comparison warnings suppressed (EDA coordinates are exact values)
- Total warnings reduced from ~500 to ~202 (lib-only; -58%)

#### CI YAML
- Restored corrupted pull_request: trigger line (broken by prior edit attempts)
- Removed orphaned step definitions left in workflow_dispatch section
- Restructured checks job to include 5 lint/audit steps

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

[Unreleased]: https://github.com/your-org/rflux/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/your-org/rflux/releases/tag/v0.1.0
