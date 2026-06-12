# Alignment Scorecard Weekly Report

## 1. Report metadata

```md
Week window: 2026-05-25 to 2026-05-31
Report date: 2026-05-28
Owner: Core maintainers
Git SHA (optional): a450c03
```

## 2. Domain score snapshot

| Domain | Planned max | Current pass score | Delta vs last week | Notes |
|---|---:|---:|---:|---|
| Yosys-aligned core flow | 25 | 22 | +22 | Core chain and SAT anchors are in CI; CLI surface gate remains stable. |
| Quaigh-aligned optimization | 20 | 20 | +20 | Fixture and classic example anchors are active in CI. |
| JoSIM-aligned simulation and correlation | 30 | 25 | +25 | Windows waveform and warning manifests are gated; Linux optional gate and strict-default fallback path are in place, but Linux approved baseline is still missing. |
| Productization and release governance | 25 | 25 | +25 | Contract gates and Week3 one-command baseline gate are active. |
| Total | 100 | 92 | +92 | Must-item gate is currently pass. |

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

## 4. SHOULD item status

| Item ID | Status (PASS/FAIL/WAIVED) | Evidence command or job | Evidence link/path | Owner | ETA if FAIL |
|---|---|---|---|---|---|
| Y-03 | PASS | CLI command surface contract gate | `.github/workflows/ci.yml` | Core maintainers | n/a |
| Q-03 | PASS | Quaigh converter smoke | `.github/workflows/ci.yml` | Synthesis maintainers | n/a |
| J-04 | FAIL | Linux waveform same-platform baseline + default no-regression gate; local precheck shows linux baseline missing while windows baseline ready (`uv run python python/scripts/check_waveform_baseline_status.py --platform linux --json-output target/waveform-baseline-status/linux.local.json`, `uv run python python/scripts/check_waveform_baseline_status.py --platform windows --json-output target/waveform-baseline-status/windows.local.json`) | `.github/workflows/ci.yml`, `docs/linux-waveform-baseline-promotion-playbook.md`, `target/waveform-baseline-status/linux.local.json`, `target/waveform-baseline-status/windows.local.json` | Simulation maintainers | 2026-06-15 |
| P-04 | PASS | release artifact helper smoke | `.github/workflows/ci.yml` | Release/QA maintainers | n/a |

## 5. Gate decision

```md
Alignment gate result: pass
Blocking MUST items: none
Waivers approved by: none
```

## 6. Top risks and next actions

```md
Risk 1: Linux waveform baseline missing keeps JoSIM domain below full score.
Action 1: Execute docs/linux-waveform-baseline-promotion-playbook.md and promote linux-approved baseline.

Risk 2: Cross-platform numeric drift may cause unstable thresholds.
Action 2: Keep strict same-platform baseline policy and publish drift rationale in parity docs.

Risk 3: Weekly scorecard updates may drift from CI evolution.
Action 3: Require scorecard item IDs in PR template and refresh this report every week.
```

## 7. References

- Scorecard: `docs/alignment-scorecard.md`
- Full plan: `docs/full-alignment-plan.md`
- PR template: `.github/PULL_REQUEST_TEMPLATE.md`
- Phase B checklist: `docs/phase-b-execution-checklist.md`
- Phase B run record: `docs/phase-b-run-record-2026-05-28.md`
