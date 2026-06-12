# Phase B Run Record - 2026-05-29

Use this record to track Linux waveform gate execution and J-04 closure evidence.

## 1. Run metadata

```md
Date: 2026-05-29
Operator: Core maintainers
Branch/commit: main@working-tree
Workflow run URL: pending
```

## 2. Workflow dispatch inputs

```md
run_waveform_compare_linux=true
josim_command_linux=
validate_no_regression_linux=pending
previous_summary_json_linux=
regression_tolerance_v_linux=
```

## 3. Generated artifacts

```md
Artifact bundle: C:/Users/lilu/works/rflux/target/waveform-compare-linux
current summary json: pending
candidate baseline json: pending
validation json: pending
manifest json: pending
linux baseline status json: C:/Users/lilu/works/rflux/target/waveform-baseline-status/linux.2026-05-29.json
phase-b artifact check json: C:/Users/lilu/works/rflux/target/waveform-compare-linux/phase-b-artifact-check.2026-05-29.json
```

## 4. Gate outcome

```md
Workflow job result: pending
No-regression path used: pending
Fallback notice observed: pending
Failure reason (if any):
```

## 5. Baseline promotion

```md
Promotion command executed: no
Precheck command:
uv run python python/scripts/check_phase_b_artifact_bundle.py --artifact-dir target/waveform-compare-linux --linux-status-json target/waveform-baseline-status/linux.local.json --json-output target/waveform-compare-linux/phase-b-artifact-check.json --require-ready

precheck result: fail
artifact bundle ready: fail
candidate promotable: fail
missing files: C:/Users/lilu/works/rflux/target/waveform-compare-linux/waveform_compare_summary.current.json, C:/Users/lilu/works/rflux/target/waveform-compare-linux/waveform_compare_summary.candidate-baseline.json, C:/Users/lilu/works/rflux/target/waveform-compare-linux/waveform_compare_summary.candidate-baseline.md, C:/Users/lilu/works/rflux/target/waveform-compare-linux/waveform_compare_summary.validation.json, C:/Users/lilu/works/rflux/target/waveform-compare-linux/manifest.json

Command:
uv run python python/scripts/promote_waveform_approved_baseline.py --platform linux --candidate-json pending --candidate-md pending

promoted linux baseline json path: python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.json
promoted linux baseline md path: python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.md
```

## 6. Baseline readiness check

```md
Command:
uv run python python/scripts/check_waveform_baseline_status.py --platform linux --require-ready --json-output target/waveform-compare-linux/linux-baseline-status.json

Result: fail
Reason: missing baseline json
status json path: C:/Users/lilu/works/rflux/target/waveform-baseline-status/linux.2026-05-29.json
```

## 7. Scorecard update

```md
Weekly report updated: no
Report file: docs/alignment-scorecard-weekly-2026-05-28.md
J-04 status after this run: FAIL
Evidence links:
- docs/phase-b-execution-checklist.md
- docs/linux-waveform-baseline-promotion-playbook.md
```

## 8. Follow-up actions

```md
Action 1: Review artifact bundle and confirm candidate baseline is promotable.
Owner: Simulation maintainers
ETA: pending

Action 2: Promote linux-approved baseline and rerun strict no-regression verification.
Owner: Simulation maintainers
ETA: pending
```
