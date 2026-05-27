# Linux Waveform Baseline Promotion Playbook

## 1. Purpose

This playbook closes J-04 by promoting a Linux same-platform approved baseline and keeping Linux waveform no-regression checks strict afterward.

## 2. Preconditions

- A Linux-available JoSIM command is ready (for example `josim` on PATH or an absolute binary path).
- Workflow `CI` can be started with `workflow_dispatch` inputs.
- Current branch includes `waveform-compare-gate-linux-optional` job.

## 3. Step A: Run Linux waveform gate (bootstrap or strict)

In GitHub Actions, run workflow `CI` with:

- `run_waveform_compare_linux=true`
- `josim_command_linux=<linux-josim-command>`
- Optional `previous_summary_json_linux=<repo-relative-baseline-json>`
- Optional `validate_no_regression_linux=true|false` (default is `true`)
- Optional `regression_tolerance_v_linux=<float>`

Notes:

- Linux gate now defaults to strict no-regression.
- If no explicit baseline and no repo-tracked Linux approved baseline exist yet, the gate auto-downgrades no-regression for that run and prints a notice.

Expected artifact from the run:

- `waveform-compare-results-linux`

## 4. Step B: Promote Linux approved baseline

After a reviewed green run, download/extract artifacts so the candidate files are available locally, then run:

```bash
uv run python python/scripts/promote_waveform_approved_baseline.py \
  --platform linux \
  --candidate-json target/waveform-compare-linux/waveform_compare_summary.candidate-baseline.json \
  --candidate-md target/waveform-compare-linux/waveform_compare_summary.candidate-baseline.md
```

Expected updated repo files:

- `python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.json`
- `python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.md`

## 5. Step C: Verify strict Linux no-regression path

Run workflow `CI` again with:

- `run_waveform_compare_linux=true`
- `josim_command_linux=<linux-josim-command>`
- `validate_no_regression_linux=true`

Expected behavior:

- Gate runs strict no-regression against the promoted Linux baseline.
- No auto-downgrade notice appears.

## 6. Review checklist linkage

- Record baseline decision in `docs/sim-release-readiness-checklist.md`.
- Update scorecard status for J-04 in the weekly report.
- If baseline changed, summarize rationale in release review notes.
