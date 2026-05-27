# Simulation Release-Readiness Checklist

Use this checklist for any candidate change that touches `crates/sim`, external JoSIM integration, waveform compare scripts, or `python/tests/benchmarks/phase6/waveform_thresholds.json`.

The goal is to turn the phase-6 waveform compare workflow into a release decision record rather than an informal benchmark note.

## 1. Candidate identity

- [ ] Record the candidate commit, branch, date, and operator.
- [ ] Record whether the target release environment is Windows, Linux, or another platform.
- [ ] Record whether the candidate changes internal simulation behavior, external JoSIM translation behavior, compare/summary logic, or only documentation.

Suggested record block:

```md
Candidate commit:
Candidate branch:
Evaluation date:
Evaluator:
Target platform:
Change scope:
```

## 2. Required validation

- [ ] Run `uv run cargo test --workspace` or a narrower command plus justification.
- [ ] Run `uv run pytest` or a narrower command plus justification.
- [ ] Run the relevant waveform compare command for the affected deck/category/full manifest.
- [ ] Preserve the exact command lines used for correlation evidence.

Recommended commands:

```bash
uv run cargo test --workspace
uv run pytest
uv run python python/scripts/run_waveform_compare_manifest.py --josim-command <josim-command> --result-dir <result-dir> --validate-pass
```

The manifest-runner command family also has an explicit CI smoke anchor now instead of relying only on the broad Python suite:

- `uv run pytest python/tests/test_waveform_compare_manifest_runner.py -q`

When the evaluated change affects unsupported-warning contracts rather than numeric threshold parity, use the warning-manifest path as the adjacent review command family:

```bash
uv run python python/scripts/run_external_warning_manifest.py --josim-command <josim-command> --validate-pass --result-dir <result-dir>
uv run python python/scripts/prepare_external_warning_artifacts.py --result-dir <result-dir> --artifact-dir <artifact-dir> --josim-command <josim-command>
```

The warning-manifest and warning-summary helper paths also have explicit CI smoke anchors now:

- `uv run pytest python/tests/test_external_warning_manifest_runner.py -q`
- `uv run pytest python/tests/test_external_warning_summary_utils.py -q`
- `uv run pytest python/tests/test_prepare_external_warning_artifacts.py -q`

## 3. Evidence package

- [ ] Attach or archive `waveform_compare_summary.current.json`.
- [ ] Attach or archive `waveform_compare_summary.current.md`.
- [ ] Attach or archive `waveform_compare_summary.candidate-baseline.json`.
- [ ] When unsupported-warning contracts were in scope, attach or archive `external_warning_summary.current.json`.
- [ ] When unsupported-warning contracts were in scope, attach or archive `external_warning_summary.current.md`.
- [ ] When unsupported-warning contracts were in scope, attach or archive `target/external-warning-review/manifest.json`.
- [ ] Record the threshold manifest path used for the run.
- [ ] Record the warning-contract manifest path used for the run when unsupported-warning review was in scope.
- [ ] Record the external JoSIM command or explain why external correlation was not run.

Minimum evidence record:

```md
Result directory:
Threshold manifest:
Warning-contract manifest:
Current summary JSON:
Current summary MD:
Candidate baseline JSON:
Warning summary JSON:
Warning summary MD:
Warning review manifest:
JoSIM command:
```

## 4. Baseline decision

- [ ] Identify the prior approved baseline path, if one exists for the same platform.
- [ ] Prefer the repo-tracked naming convention `python/tests/benchmarks/phase6/waveform_compare_summary.<platform>-approved-baseline.json` when a stable same-platform baseline is promoted.
- [ ] If no same-platform approved baseline exists, record that strict no-regression is deferred and why.
- [ ] If a same-platform approved baseline exists, run no-regression validation with an explicit tolerance.
- [ ] Record whether the candidate is approved to replace the prior baseline.

Recommended no-regression command:

```bash
uv run python python/scripts/run_waveform_compare_manifest.py --josim-command <josim-command> --result-dir <result-dir> --previous-summary-json <approved-baseline.json> --validate-no-regression --regression-tolerance-v <tolerance>
```

Decision record:

```md
Approved baseline source:
Same-platform baseline exists: yes/no
Baseline platform key:
No-regression enforced: yes/no
Regression tolerance (V):
Outcome:
Reason if deferred:
Promote candidate baseline: yes/no
```

## 5. Sign-off path

- [ ] Name the sim/kernel DRI.
- [ ] Name the QA / benchmark reviewer.
- [ ] Name the productization/release reviewer when public behavior or release workflow changed.
- [ ] Link any parity-matrix or known-limitations updates required by the change.
- [ ] Link any warning-contract review bundle or explain why unsupported-warning scope was out of scope for this candidate.

Sign-off record:

```md
Sim DRI:
QA reviewer:
Productization reviewer:
Parity doc updated:
Known limitations updated:
Warning review bundle attached: yes/no
Release notes required: yes/no
```

## 6. Go / no-go outcome

- [ ] `go` only if the candidate has current summary artifacts, a clear baseline decision, and named reviewers.
- [ ] `no-go` if summary artifacts are missing, the threshold manifest does not match the evaluated scope, or regressions are unexplained.
- [ ] If blocked only by missing same-platform baseline, mark as conditional and create the baseline promotion follow-up explicitly.

Final record:

```md
Decision: go / conditional / no-go
Blocking issues:
Follow-up owner:
Follow-up due date:
```

## 7. Current repository-specific notes

- The tracked repo baseline today is Windows-local: `python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json`.
- The intended Linux promotion target is `python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.json`.
- The default CI quality gate now uses the Windows baseline on the same-platform `windows-latest` waveform-compare job.
- Do not reuse the Windows baseline as the strict no-regression source for a future Ubuntu/Linux gate.
- Before enabling a Linux default gate, capture and promote a Linux approved baseline on the runner class that will enforce it.