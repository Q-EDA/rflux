# rflux Alignment Scorecard (Phase A v1)

## 0. Stage status

- Phase A: completed (2026-05-28)
- Phase B: in progress (J-04 closure)
- Phase B execution checklist: `docs/phase-b-execution-checklist.md`
- Phase B run record template: `docs/phase-b-run-record-template.md`
- Phase E: in progress (CLI-first integration hardening)
- Phase E execution checklist: `docs/phase-e-execution-checklist.md`

## 1. Purpose

This scorecard turns alignment progress into machine-checkable items.

Scoring rule:

- Each item has a weight.
- `must` items are release blockers for alignment maturity.
- `should` items are maturity accelerators.
- Total score = sum of passed item weights (target 100).

Gate rule:

- Any failed `must` item means alignment gate is not satisfied, even if total score is high.

## 2. Domain weights

| Domain | Weight |
|---|---:|
| Yosys-aligned core flow | 25 |
| Quaigh-aligned optimization | 20 |
| JoSIM-aligned simulation and correlation | 30 |
| Productization and release governance | 25 |

## 3. Scorecard items

| ID | Domain | Level | Weight | Check definition | Code anchor | Test anchor | CI anchor | Doc anchor |
|---|---|---|---:|---|---|---|---|---|
| Y-01 | Yosys core | must | 10 | Core CLI chain (lint-input, compile-netlist, check-equivalence) remains executable on Windows smoke path. | `crates/cli/src/main.rs` | `run_lint_input_reports_versioned_ir_contract`, `run_check_equivalence_accepts_checked_in_sequential_bench_fixtures` | `core-smoke-windows` | `docs/yosys-alignment.md` |
| Y-02 | Yosys core | must | 8 | DIMACS end-to-end SAT flow remains stable. | `crates/sat/src/lib.rs` | `crates/sat/tests/dimacs_end_to_end.rs` | `checks / Generate SAT and synth metric artifacts` | `docs/yosys-alignment.md` |
| Y-03 | Yosys core | should | 7 | Command-line surface contract remains stable under `--check`. | `python/scripts/export_cli_command_surface.py` | `python/tests/test_cli_command_surface_contract.py` | `checks / CLI command surface contract gate` | `docs/release-policy.md` |
| Q-01 | Quaigh alignment | must | 10 | Fixture-level Quaigh alignment regression remains green. | `crates/synth/src/lib.rs` | `crates/synth/tests/quaigh_alignment_fixtures.rs` | `checks / Generate SAT and synth metric artifacts` | `docs/quaigh-alignment.md` |
| Q-02 | Quaigh alignment | must | 6 | Classic end-to-end boolean optimization examples remain equivalent and stable. | `crates/synth/src/lib.rs` | `crates/synth/tests/end_to_end_classic_examples.rs` | `checks / Generate SAT and synth metric artifacts` | `docs/quaigh-alignment.md` |
| Q-03 | Quaigh alignment | should | 4 | Quaigh bench converter utility remains usable from Python toolchain. | `python/scripts/quaigh_bench_converter.py` | `python/tests/test_quaigh_bench_converter.py` | `checks / Quaigh converter smoke` | `docs/quaigh-alignment.md` |
| J-01 | JoSIM alignment | must | 10 | Windows waveform compare gate executes manifest path with `--validate-pass`. | `python/scripts/run_waveform_compare_manifest.py` | `python/tests/test_waveform_compare_manifest_runner.py` | `waveform-compare-gate` | `docs/josim-parity.md` |
| J-02 | JoSIM alignment | must | 8 | External warning manifest flow stays valid (including empty-contract path). | `python/scripts/run_external_warning_manifest.py` | `python/tests/test_external_warning_manifest_runner.py` | `checks / External warning helper smoke` and `waveform-compare-gate` | `docs/josim-parity.md` |
| J-03 | JoSIM alignment | must | 7 | Waveform summary helper and no-regression context remain stable. | `python/scripts/summarize_waveform_compare_results.py` | `python/tests/test_waveform_compare_summary_utils.py` | `checks / Waveform summary helper smoke` | `docs/josim-parity.md` |
| J-04 | JoSIM alignment | should | 5 | Linux same-platform approved baseline is present and Linux no-regression gate is enabled by default. | `.github/workflows/ci.yml`, `python/scripts/promote_waveform_approved_baseline.py`, `python/scripts/check_waveform_baseline_status.py`, `python/scripts/check_phase_b_artifact_bundle.py` | `python/tests/test_promote_waveform_approved_baseline.py`, `python/tests/test_check_waveform_baseline_status.py`, `python/tests/test_check_phase_b_artifact_bundle.py` | `checks / Waveform baseline promotion helper smoke`, `checks / Waveform baseline readiness helper smoke`, `checks / Phase B artifact bundle helper smoke`, `checks / Upload waveform baseline status snapshot`, `waveform-compare-gate-linux-optional` | `docs/full-alignment-plan.md` |
| P-01 | Productization | must | 8 | Python API surface contract remains stable under `--check`. | `python/scripts/export_python_api_surface.py` | `python/tests/test_python_api_surface_contract.py` | `checks / Python API surface contract gate` | `docs/release-policy.md` |
| P-02 | Productization | must | 6 | Report schema surface contract remains stable under `--check`. | `python/scripts/export_report_schema_surface.py` | `python/tests/test_report_schema_surface_contract.py` | `checks / Report schema surface contract gate` | `docs/release-policy.md` |
| P-03 | Productization | must | 6 | Week3 one-command quality baseline pipeline remains executable with no-regression check. | `python/scripts/generate_week3_golden_results.py` | `python/tests/test_generate_week3_golden_results.py` | `checks / Quality baseline artifact prep smoke` | `docs/full-alignment-plan.md` |
| P-04 | Productization | should | 5 | Candidate release artifact helper remains green for review bundles. | `python/scripts/prepare_release_artifacts.py`, `python/scripts/check_release_artifact_bundle.py`, `python/scripts/generate_release_review_record.py`, `python/scripts/generate_release_notes.py` | `python/tests/test_prepare_release_artifacts.py`, `python/tests/test_check_release_artifact_bundle.py`, `python/tests/test_generate_release_review_record.py`, `python/tests/test_generate_release_notes.py` | `checks / Release artifact helper smoke`, `checks / Release artifact bundle checker smoke`, `checks / Release review record generator smoke`, `checks / Release notes generator smoke` | `docs/release-artifact-readiness-checklist.md`, `docs/release-review-record-template.md`, `docs/release-notes-template.md` |

## 3.1 Phase E CLI-first blocker set (must-pass, non-scored)

The following blockers are mandatory for frontend-integration changes and are tracked in weekly reports. They are non-scored for the current `0-100` score model but release-blocking when failed.

| ID | Blocker | Check definition | Suggested evidence anchor |
|---|---|---|---|
| E-01 | CLI compatibility blocker | Existing command names, core flags, exit codes, and machine-readable JSON fields remain backward-compatible unless explicitly versioned. | Contract `--check` gates + focused CLI regression jobs |
| E-02 | CLI output blocker | Default CLI mode does not emit new progress noise to stdout that breaks script consumption. | Golden output diff or focused stdout/stderr regression tests |
| E-03 | CLI performance blocker | Core command-path runtime does not exceed agreed regression threshold without waiver. | `uv run python python/scripts/capture_cli_perf_baseline.py --previous-baseline ... --fail-on-regression` artifact + threshold report |
| E-04 | Non-service boundary blocker | No change introduces a mandatory always-on service dependency for existing CLI usage. | Release review checklist + docs consistency checks |
| E-05 | Failure evidence blocker | Failure paths emit stable error code and diagnostics bundle evidence. | Diagnostics smoke + error-code mapping regression tests |

## 4. Current baseline snapshot (2026-05-28)

| Domain | Estimated pass score |
|---|---:|
| Yosys-aligned core flow | 22 / 25 |
| Quaigh-aligned optimization | 20 / 20 |
| JoSIM-aligned simulation and correlation | 25 / 30 |
| Productization and release governance | 25 / 25 |
| Total | 92 / 100 |

Interpretation:

- `must` items are mostly covered by current CI.
- Remaining largest gap is `J-04` (Linux same-platform waveform baseline + default no-regression gate).

## 5. Weekly update protocol

1. For each changed item, update status evidence in PR description with item ID.
2. If a `must` item is intentionally waived, document reason and owner in release review record.
3. Recompute domain and total scores at least once per week.

Weekly report template:

- `docs/alignment-scorecard-weekly-template.md`

Latest weekly report:

- `docs/alignment-scorecard-weekly-2026-05-29.md`
