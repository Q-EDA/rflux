# Release Artifact Readiness Checklist

Use this checklist for any candidate change that intends to produce or review CLI / Python release artifacts, especially when touching `crates/cli`, `crates/py`, `pyproject.toml`, packaging scripts, or release workflow wiring.

The goal is to turn candidate artifact generation into a release decision record rather than an ad-hoc build snapshot.

For release note content and final go/no-go consolidation, pair this checklist with [release-notes-template.md](./release-notes-template.md) and [release-review-record-template.md](./release-review-record-template.md).

## 1. Candidate identity

- [ ] Record candidate commit, branch, date, and operator.
- [ ] Record target runner platform for the candidate artifact bundle.
- [ ] Record whether the candidate change affects CLI packaging, Python wheel packaging, release workflow plumbing, or only documentation.

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
- [ ] Run the candidate artifact build path.
- [ ] Run CLI command surface contract check (`export_cli_command_surface.py --check`) when the candidate touches `crates/cli` command/arg surface.
- [ ] Run Python API public contract check (`export_python_api_surface.py --check`) when the candidate touches `python/rflux` public symbols.
- [ ] Run report schema surface contract check (`export_report_schema_surface.py --check`) when the candidate touches CLI report JSON or artifact manifest schemas.
- [ ] Run Week 3 quality baseline one-command check (`generate_week3_golden_results.py --validate-pass --validate-no-regression`) when the candidate touches week3 timing/verify/sim baseline inputs, thresholds, or summary logic.
- [ ] Preserve the exact build command lines used for the candidate bundle.

Recommended commands:

```bash
uv run cargo test --workspace
uv run pytest
uv run python python/scripts/prepare_release_artifacts.py --output-dir target/release-artifacts
uv run python python/scripts/check_release_artifact_bundle.py --artifact-dir target/release-artifacts --json-output target/release-artifacts/release_bundle_check.json --require-ready
uv run python python/scripts/generate_release_review_record.py --date 2026-05-28 --candidate-commit <sha> --candidate-branch <branch> --target-platform <runner-platform> --change-scope "candidate release review" --release-artifact-dir target/release-artifacts --release-bundle-check-json target/release-artifacts/release_bundle_check.json --week3-output-root target/week3-quality-pipeline --output docs/release-review-record-2026-05-28.md
uv run python python/scripts/export_cli_command_surface.py --check
uv run python python/scripts/export_python_api_surface.py --check
uv run python python/scripts/export_report_schema_surface.py --check
uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0
```

The candidate artifact helper also has an explicit CI smoke anchor:

- `uv run pytest python/tests/test_prepare_release_artifacts.py -q`
- `uv run pytest python/tests/test_check_release_artifact_bundle.py -q`
- `uv run pytest python/tests/test_generate_release_review_record.py -q`
- `uv run pytest python/tests/test_cli_command_surface_contract.py -q`
- `uv run pytest python/tests/test_python_api_surface_contract.py -q`
- `uv run pytest python/tests/test_report_schema_surface_contract.py -q`
- `uv run pytest python/tests/test_generate_week3_golden_results.py -q`
- `uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0`

## 3. Evidence package

- [ ] Attach or archive `target/release-artifacts/bin/` contents.
- [ ] Attach or archive `target/release-artifacts/wheels/` contents.
- [ ] Attach or archive `target/release-artifacts/manifest.json`.
- [ ] Attach or archive `target/release-artifacts/README.txt`.
- [ ] Record copied build-input snapshots (`README.md`, `Cargo.toml`, `pyproject.toml`, `uv.lock`).
- [ ] When `crates/cli` command/arg surface changed, attach or archive `python/tests/contracts/cli_command_surface.json` and record contract diff summary.
- [ ] When `python/rflux` public surface changed, attach or archive `python/tests/contracts/python_api_surface.json` and record contract diff summary.
- [ ] When report JSON/manifest kind-schema surface changed, attach or archive `python/tests/contracts/report_schema_surface.json` and record contract diff summary.
- [ ] When Week 3 baseline workflow changed, attach or archive `target/week3-quality-pipeline/review/manifest.json`, `target/week3-quality-pipeline/review/quality_summary.validation.json`, and `target/week3-quality-pipeline/review/quality_summary.current.md`.

Minimum evidence record:

```md
Artifact directory:
CLI binary path:
Wheel paths:
Manifest path:
README path:
Build input snapshots:
CLI command surface baseline path:
CLI command surface diff summary:
Python API surface baseline path:
Python API diff summary:
Report schema surface baseline path:
Report schema surface diff summary:
Week3 pipeline manifest path:
Week3 pipeline validation path:
Week3 pipeline summary markdown path:
```

## 4. Compatibility and installability review

- [ ] Confirm the CLI binary name and platform match the intended review scope.
- [ ] Confirm the wheel filename matches the intended Python / platform target.
- [ ] Record whether the candidate bundle is for internal review only or ready for external delivery discussion.
- [ ] Record any platform limitations, missing matrices, or install caveats.

Decision record:

```md
CLI artifact valid: yes/no
Wheel artifact valid: yes/no
Internal review only: yes/no
External delivery approved: yes/no
Platform caveats:
Install caveats:
```

## 5. Sign-off path

- [ ] Name the CLI / Python / productization DRI.
- [ ] Name the QA / benchmark reviewer.
- [ ] Name the documentation / support reviewer when packaging behavior or user-facing install guidance changed.
- [ ] Link any release-policy, support-matrix, or known-limitations updates required by the change.

Sign-off record:

```md
Packaging DRI:
QA reviewer:
Documentation reviewer:
Release policy updated:
Support matrix updated:
Known limitations updated:
Release notes required: yes/no
```

## 6. Go / no-go outcome

- [ ] `go` only if candidate artifacts, manifest, and named reviewers are present.
- [ ] `no-go` if candidate bundle is incomplete, build inputs are missing, or installability caveats are unexplained.
- [ ] If blocked only by platform-matrix gaps, mark as conditional and create the follow-up explicitly.

Final record:

```md
Decision: go / conditional / no-go
Blocking issues:
Follow-up owner:
Follow-up due date:
```

## 7. Current repository-specific notes

- Current candidate artifact generation is manual-review oriented via the `release-artifacts-optional` workflow job.
- The resulting wheel and CLI binary bundles are current-runner artifacts, not a formal public release channel.
- Support-matrix language for precompiled wheel / CLI distribution remains `experimental` until platform matrices, install validation, and public release handling are formalized.