# Phase B Run Record - 2026-05-28

Use this record to track Linux waveform gate execution and J-04 closure evidence.

## 1. Run metadata

```md
Date: 2026-05-28
Operator: Core maintainers
Branch/commit: main@893cc20
Workflow run URL: pending
```

## 2. Workflow dispatch inputs

```md
run_waveform_compare_linux=true
josim_command_linux=
validate_no_regression_linux=true
previous_summary_json_linux=
regression_tolerance_v_linux=
```

## 3. Generated artifacts

```md
Artifact bundle: waveform-compare-results-linux
current summary json: pending
candidate baseline json: pending
validation json: pending
manifest json: pending
linux baseline status json: pending
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
Command:
uv run python python/scripts/promote_waveform_approved_baseline.py --platform linux --candidate-json <candidate-json> --candidate-md <candidate-md>

promoted linux baseline json path: pending
promoted linux baseline md path: pending
```

## 6. Baseline readiness check

```md
Command:
uv run python python/scripts/check_waveform_baseline_status.py --platform linux --require-ready --json-output target/waveform-compare-linux/linux-baseline-status.json

Precheck status (local): fail (missing baseline json)
status json path: target/waveform-baseline-status/linux.local.json
```

## 7. Scorecard update

```md
Weekly report updated: yes
Report file: docs/alignment-scorecard-weekly-2026-05-28.md
J-04 status after this run: FAIL
Evidence links:
- docs/phase-b-execution-checklist.md
- docs/linux-waveform-baseline-promotion-playbook.md
```

## 8. Follow-up actions

```md
Action 1: Run workflow_dispatch Linux waveform gate and capture artifact paths.
Owner: Simulation maintainers
ETA: 2026-05-30

Action 2: Promote linux-approved baseline and rerun strict no-regression verification.
Owner: Simulation maintainers
ETA: 2026-06-05
```
