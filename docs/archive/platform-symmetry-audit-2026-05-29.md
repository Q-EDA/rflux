# Platform Symmetry Audit - 2026-05-29

Purpose: provide a machine-reviewable Week 2 symmetry snapshot for Ubuntu and Windows quality gates.

## 1. Scope and source of truth

- Workflow source: `.github/workflows/ci.yml`
- Policy anchors: `docs/support-matrix.md`, `docs/release-policy.md`
- Audit date: 2026-05-29

## 2. Platform gate matrix snapshot

| Area | Ubuntu (`checks`) | Windows (`core-smoke-windows`) | Symmetry status | Note |
|---|---|---|---|---|
| Rust workspace regression | `cargo test --workspace` | no full workspace run in core smoke | partial | Windows currently keeps minimal smoke for runtime and CI cost control. |
| Python regression | `uv run pytest` | `python/tests/test_basic.py` narrow subset | partial | Functional symmetry exists on core chain, not on full suite depth. |
| CLI core chain | covered in `checks` and dedicated anchors | dedicated 3-command smoke anchors | aligned (core) | Same core chain is present on both platforms. |
| Contract gates (CLI/Python/report) | explicit `--check` gates | not in core smoke | partial | Covered on Ubuntu by default; Windows path remains minimal. |
| Week3 quality baseline gate | explicit in `checks` | not in core smoke | partial | Release-quality gate currently Ubuntu-first. |
| Waveform compare gate | default Windows gate job | no default Linux/Windows parity in core-smoke job itself | partial | Windows has default waveform gate; Linux remains optional path. |
| Release helper smokes | explicit in `checks` | not in core smoke | partial | Current release helper validation is Ubuntu-first. |

## 3. Ten-command drift spot check

Rule used: each sampled command should have explicit CI anchor and be documented in release/support docs when relevant.

| ID | Command | CI anchor present | Doc anchor present | Drift result |
|---|---|---|---|---|
| D-01 | `uv run python python/scripts/export_python_api_surface.py --check` | yes | yes (`docs/release-policy.md`) | aligned |
| D-02 | `uv run python python/scripts/export_cli_command_surface.py --check` | yes | yes (`docs/release-policy.md`) | aligned |
| D-03 | `uv run python python/scripts/export_report_schema_surface.py --check` | yes | yes (`docs/release-policy.md`) | aligned |
| D-04 | `uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0` | yes | yes (`docs/release-policy.md`) | aligned |
| D-05 | `uv run pytest python/tests/test_prepare_release_artifacts.py -q` | yes | yes (`docs/release-policy.md`) | aligned |
| D-06 | `uv run pytest python/tests/test_check_release_artifact_bundle.py -q` | yes | indirect (`release-artifact-readiness-checklist`) | aligned |
| D-07 | `uv run pytest python/tests/test_generate_release_review_record.py -q` | yes | indirect (`release-review-record-template`) | aligned |
| D-08 | `uv run pytest python/tests/test_generate_release_notes.py -q` | yes | indirect (`release-notes-template`) | aligned |
| D-09 | `cargo test -p rflux-cli run_lint_input_reports_versioned_ir_contract -- --nocapture` | yes | yes (`docs/release-policy.md`, `docs/support-matrix.md`) | aligned |
| D-10 | `cargo test -p rflux-sat --test dimacs_end_to_end -- --nocapture` | yes | yes (`docs/support-matrix.md`) | aligned |

## 4. Symmetry gaps to close (non-Linux-blocked)

1. Promote selected contract and release-helper smokes into Windows extended lane (optional or scheduled) to improve platform symmetry.
2. Keep `core-smoke-windows` as minimal chain, and add a second Windows lane for Week3/release helper parity evidence.
3. Continue weekly ten-command drift check and append updates in scorecard weekly reports.

## 5. Linux-blocked item kept out of this audit

- Linux waveform approved baseline promotion and strict default no-regression closure (J-04).

This remains tracked by `docs/phase-b-run-record-2026-05-29.md` and is intentionally excluded from non-Linux symmetry closure assertions.
