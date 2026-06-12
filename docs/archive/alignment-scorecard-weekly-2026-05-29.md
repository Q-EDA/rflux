# Alignment Scorecard Weekly Report

## 1. Report metadata

```md
Week window: 2026-05-25 to 2026-05-31
Report date: 2026-05-29
Owner: Core maintainers
Git SHA (optional): main@working-tree
```

## 2. Domain score snapshot

| Domain | Planned max | Current pass score | Delta vs last week | Notes |
|---|---:|---:|---:|---|
| Yosys-aligned core flow | 25 | 22 | 0 | Core chain and SAT anchors remain stable in CI. |
| Quaigh-aligned optimization | 20 | 20 | 0 | Fixture and classic example anchors remain stable. |
| JoSIM-aligned simulation and correlation | 30 | 25 | 0 | Windows baseline is ready; Linux baseline is still missing in latest precheck. |
| Productization and release governance | 25 | 25 | 0 | Contract gates and Week3 baseline gates remain active. |
| Total | 100 | 92 | 0 | Must-item gate is pass; J-04 is still pending. |

## 3. MUST item status

Rule: any failed MUST item means alignment gate is not satisfied.

| Item ID | Status (PASS/FAIL/WAIVED) | Evidence command or job | Evidence link/path | Owner | ETA if FAIL |
|---|---|---|---|---|---|
| Y-01 | PASS | `core-smoke-windows` CLI minimal chain smoke | `.github/workflows/ci.yml` | Core maintainers | n/a |
| Y-02 | PASS | `cargo test -p rflux-sat --test dimacs_end_to_end -- --nocapture` | `.github/workflows/ci.yml` | Core maintainers | n/a |
| Q-01 | PASS | `cargo test -p rflux-synth --test quaigh_alignment_fixtures -- --nocapture` | `.github/workflows/ci.yml` | Synthesis maintainers | n/a |
| Q-02 | PASS | `cargo test -p rflux-synth --test end_to_end_classic_examples -- --nocapture` | `.github/workflows/ci.yml` | Synthesis maintainers | n/a |
| J-01 | PASS | `waveform-compare-gate` with `--validate-pass` | `.github/workflows/ci.yml` | Simulation maintainers | n/a |
| J-02 | PASS | external warning helper + gate manifest run | `.github/workflows/ci.yml` | Simulation maintainers | n/a |
| J-03 | PASS | waveform summary helper smoke | `.github/workflows/ci.yml` | Simulation maintainers | n/a |
| P-01 | PASS | `uv run python python/scripts/export_python_api_surface.py --check` | `.github/workflows/ci.yml` | Release/QA maintainers | n/a |
| P-02 | PASS | `uv run python python/scripts/export_report_schema_surface.py --check` | `.github/workflows/ci.yml` | Release/QA maintainers | n/a |
| P-03 | PASS | `uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0` | `.github/workflows/ci.yml` | Release/QA maintainers | n/a |

Note (2026-05-29 local lane): running contract gates locally refreshed contract baselines (`python/tests/contracts/python_api_surface.json`, `python/tests/contracts/report_schema_surface.json`); both follow-up contract tests passed.

## 4. SHOULD item status

| Item ID | Status (PASS/FAIL/WAIVED) | Evidence command or job | Evidence link/path | Owner | ETA if FAIL |
|---|---|---|---|---|---|
| Y-03 | PASS | CLI command surface contract gate | `.github/workflows/ci.yml` | Core maintainers | n/a |
| Q-03 | PASS | Quaigh converter smoke | `.github/workflows/ci.yml` | Synthesis maintainers | n/a |
| J-04 | FAIL | Linux baseline precheck + artifact precheck on 2026-05-29 (`check_waveform_baseline_status`, `check_phase_b_artifact_bundle`) | `target/waveform-baseline-status/linux.2026-05-29.json`, `target/waveform-compare-linux/phase-b-artifact-check.2026-05-29.json`, `docs/phase-b-run-record-2026-05-29.md` | Simulation maintainers | 2026-06-15 |
| P-04 | PASS | release artifact helper smoke + local non-Linux regression (`test_prepare_release_artifacts`, `test_check_release_artifact_bundle`, `test_generate_release_review_record`, `test_generate_release_notes`) | `.github/workflows/ci.yml`, `docs/non-linux-progress-2026-05-29.md` | Release/QA maintainers | n/a |

## 5. Gate decision

```md
Alignment gate result: pass
Blocking MUST items: none
Waivers approved by: none
```

## 6. Top risks and next actions

```md
Risk 1: Linux same-platform approved baseline is still missing, blocking J-04 closure.
Action 1: Run Linux workflow_dispatch gate with valid josim_command_linux and produce candidate baseline artifacts.

Risk 2: Artifact bundle completeness is currently false for Linux promotion precheck.
Action 2: Regenerate Linux waveform compare artifact bundle and rerun phase-b artifact checker with --require-ready.

Risk 3: Platform symmetry can drift if Linux gating remains manual too long.
Action 3: Promote Linux approved baseline and then keep strict no-regression path enabled as default behavior.

Risk 4: Linux blocking can stall other alignment items if not split into parallel tracks.
Action 4: Keep Week2/3 non-Linux tasks on a separate execution lane and archive evidence in docs/non-linux-progress-2026-05-29.md.
```

## 7. References

- Scorecard: `docs/alignment-scorecard.md`
- Full plan: `docs/full-alignment-plan.md`
- PR template: `.github/PULL_REQUEST_TEMPLATE.md`
- Phase B checklist: `docs/phase-b-execution-checklist.md`
- Phase B run record: `docs/phase-b-run-record-2026-05-29.md`
- Non-Linux parallel record: `docs/non-linux-progress-2026-05-29.md`
- Platform symmetry audit: `docs/platform-symmetry-audit-2026-05-29.md`
- Alignment change summary: `docs/alignment-change-summary-2026-05-29.md`
