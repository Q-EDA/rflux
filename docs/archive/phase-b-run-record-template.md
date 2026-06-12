# Phase B Run Record Template

Use this template for each Linux waveform gate execution during Phase B.

## 1. Run metadata

```md
Date:
Operator:
Branch/commit:
Workflow run URL:
```

## 2. Workflow dispatch inputs

```md
run_waveform_compare_linux=true
josim_command_linux=
validate_no_regression_linux=
previous_summary_json_linux=
regression_tolerance_v_linux=
```

## 3. Generated artifacts

```md
Artifact bundle: waveform-compare-results-linux
current summary json:
candidate baseline json:
validation json:
manifest json:
linux baseline status json:
phase-b artifact check json:
```

## 4. Gate outcome

```md
Workflow job result: pass / fail
No-regression path used: strict / fallback
Fallback notice observed: yes / no
Failure reason (if any):
```

## 5. Baseline promotion

```md
Promotion command executed: yes / no
Precheck command:
uv run python python/scripts/check_phase_b_artifact_bundle.py --artifact-dir target/waveform-compare-linux --linux-status-json target/waveform-baseline-status/linux.local.json --json-output target/waveform-compare-linux/phase-b-artifact-check.json --require-ready

precheck result: pass / fail
artifact bundle ready: pass / fail
candidate promotable: pass / fail
missing files:

Command:
promoted linux baseline json path:
promoted linux baseline md path:
```

## 6. Baseline readiness check

```md
Command:
uv run python python/scripts/check_waveform_baseline_status.py --platform linux --require-ready --json-output target/waveform-compare-linux/linux-baseline-status.json

Result: pass / fail
status json path:
```

## 7. Scorecard update

```md
Weekly report updated: yes / no
Report file:
J-04 status after this run: FAIL / PASS
Evidence links:
```

## 8. Follow-up actions

```md
Action 1:
Owner:
ETA:

Action 2:
Owner:
ETA:
```
