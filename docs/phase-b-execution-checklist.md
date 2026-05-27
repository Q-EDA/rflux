# Phase B Execution Checklist (Simulation Alignment)

## 1. Goal

Close J-04 by turning Linux waveform path from "optional bootstrap" into stable same-platform no-regression enforcement with repo-tracked approved baseline.

## 2. Current status

- Windows baseline: ready and enforced.
- Linux gate path: implemented (`waveform-compare-gate-linux-optional`).
- Linux baseline readiness: currently not ready (missing approved baseline files).

Precheck commands (run before dispatching Linux gate):

```bash
uv run python python/scripts/check_waveform_baseline_status.py --platform windows --json-output target/waveform-baseline-status/windows.local.json
uv run python python/scripts/check_waveform_baseline_status.py --platform linux --json-output target/waveform-baseline-status/linux.local.json
```

Expected precheck state before J-04 closure:

- Windows: `baseline_ready=True`
- Linux: `baseline_ready=True` (if `False`, complete step 1-5 first)

## 3. Execution steps

Before running steps, create a run record from:

- `docs/phase-b-run-record-template.md`

Optional automation (recommended):

```bash
uv run python python/scripts/generate_phase_b_run_record.py \
  --date 2026-05-28 \
  --artifact-dir target/waveform-compare-linux \
  --linux-status-json target/waveform-baseline-status/linux.local.json \
  --phase-b-artifact-check-json target/waveform-compare-linux/phase-b-artifact-check.json \
  --output docs/phase-b-run-record-2026-05-28.md
```

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

Optional automation (recommended):

```bash
uv run python python/scripts/check_phase_b_artifact_bundle.py \
  --artifact-dir target/waveform-compare-linux \
  --linux-status-json target/waveform-baseline-status/linux.local.json \
  --json-output target/waveform-compare-linux/phase-b-artifact-check.json \
  --require-ready
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

7. Archive run record

```md
Store completed run record in docs/ with date suffix, for example:
  docs/phase-b-run-record-2026-05-28.md
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
