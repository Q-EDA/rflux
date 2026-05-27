# Release Artifact Readiness Checklist

Use this checklist for any candidate change that intends to produce or review CLI / Python release artifacts, especially when touching `crates/cli`, `crates/py`, `pyproject.toml`, packaging scripts, or release workflow wiring.

The goal is to turn candidate artifact generation into a release decision record rather than an ad-hoc build snapshot.

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
- [ ] Preserve the exact build command lines used for the candidate bundle.

Recommended commands:

```bash
uv run cargo test --workspace
uv run pytest
uv run python python/scripts/prepare_release_artifacts.py --output-dir target/release-artifacts
```

The candidate artifact helper also has an explicit CI smoke anchor:

- `uv run pytest python/tests/test_prepare_release_artifacts.py -q`

## 3. Evidence package

- [ ] Attach or archive `target/release-artifacts/bin/` contents.
- [ ] Attach or archive `target/release-artifacts/wheels/` contents.
- [ ] Attach or archive `target/release-artifacts/manifest.json`.
- [ ] Attach or archive `target/release-artifacts/README.txt`.
- [ ] Record copied build-input snapshots (`README.md`, `Cargo.toml`, `pyproject.toml`, `uv.lock`).

Minimum evidence record:

```md
Artifact directory:
CLI binary path:
Wheel paths:
Manifest path:
README path:
Build input snapshots:
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