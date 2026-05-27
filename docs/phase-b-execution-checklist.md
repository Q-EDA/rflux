# Phase B Execution Checklist (Simulation Alignment)

## 1. Goal

Close J-04 by turning Linux waveform path from "optional bootstrap" into stable same-platform no-regression enforcement with repo-tracked approved baseline.

## 2. Current status

- Windows baseline: ready and enforced.
- Linux gate path: implemented (`waveform-compare-gate-linux-optional`).
- Linux baseline readiness: currently not ready (missing approved baseline files).

## 3. Execution steps

1. Run Linux waveform gate for candidate generation

```md
Workflow: CI (workflow_dispatch)
Inputs:
  run_waveform_compare_linux=true
  josim_command_linux=<linux-josim-command>
  validate_no_regression_linux=true
  previous_summary_json_linux=
```

2. Review artifact bundle

```md
Artifact: waveform-compare-results-linux
Required files:
  waveform_compare_summary.current.json
  waveform_compare_summary.candidate-baseline.json
  waveform_compare_summary.validation.json
  manifest.json
  linux-baseline-status.json
```

3. Promote Linux approved baseline (after reviewed green run)

```bash
uv run python python/scripts/promote_waveform_approved_baseline.py \
  --platform linux \
  --candidate-json target/waveform-compare-linux/waveform_compare_summary.candidate-baseline.json \
  --candidate-md target/waveform-compare-linux/waveform_compare_summary.candidate-baseline.md
```

4. Validate baseline readiness (must pass)

```bash
uv run python python/scripts/check_waveform_baseline_status.py \
  --platform linux \
  --require-ready \
  --json-output target/waveform-compare-linux/linux-baseline-status.json
```

5. Run strict Linux no-regression verification

```md
Workflow: CI (workflow_dispatch)
Inputs:
  run_waveform_compare_linux=true
  josim_command_linux=<linux-josim-command>
  validate_no_regression_linux=true
  previous_summary_json_linux=python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.json
```

6. Sync scorecard and weekly report

```md
Update:
  docs/alignment-scorecard-weekly-2026-05-28.md (or latest weekly report)
  J-04 status: FAIL -> PASS
```

## 4. Exit criteria

- Linux approved baseline files exist in repo:
  - `python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.json`
  - `python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.md`
- `check_waveform_baseline_status.py --platform linux --require-ready` exits 0.
- Linux waveform gate can run with strict no-regression and no fallback notice.
- Weekly scorecard marks J-04 as PASS with artifact evidence.

## 5. Risks and mitigations

- Risk: Linux JoSIM installation/path inconsistency.
  - Mitigation: standardize `josim_command_linux` in workflow dispatch notes.
- Risk: Cross-platform drift confusion.
  - Mitigation: enforce same-platform baseline only; never reuse Windows baseline for Linux gate.
- Risk: Artifact review ambiguity.
  - Mitigation: always require `manifest.json` and `linux-baseline-status.json` in review packet.
