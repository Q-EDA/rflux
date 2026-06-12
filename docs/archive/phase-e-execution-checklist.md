# Phase E Execution Checklist (CLI-first integration hardening)

Purpose: execute the frontend-integration capability plan without degrading CLI behavior or introducing service dependency.

## 1. Scope and hard boundaries

- Keep repository positioning as engine-only (Rust crate / Python API / CLI).
- Do not add in-repo REST service or always-on daemon as a required runtime path.
- Treat CLI compatibility as release-blocking.

## 2. Week 1 checklist (E1 kickoff)

- [ ] Publish engine integration contract doc (inputs/outputs, progress events, error/diagnostics schema).
- [ ] Add or update CLI compatibility snapshot checks for command surface and JSON surface.
- [ ] Define default output behavior for progress events (silent by default for script-safe mode).
- [ ] Add baseline performance capture command set for core CLI paths.
- [ ] Register E-01..E-05 blockers in weekly report.

## 3. Week 2 checklist (E1 close)

- [ ] Add CI anchor for CLI output compatibility regression.
- [ ] Add CI anchor for CLI performance baseline comparison and threshold policy.
- [ ] Add release-review checklist item for non-service boundary verification.
- [ ] Add failure-evidence smoke checks (error code + diagnostics bundle consistency).

Suggested E-03 baseline command:

- `uv run python python/scripts/capture_cli_perf_baseline.py --output target/cli-perf/cli_perf_baseline.current.json --iterations 3 --warmup 1`
- Optional regression check against prior baseline:
	- `uv run python python/scripts/capture_cli_perf_baseline.py --output target/cli-perf/cli_perf_baseline.current.json --previous-baseline target/cli-perf/cli_perf_baseline.previous.json --max-regression-ratio 0.2 --fail-on-regression`

Suggested CI trigger (workflow_dispatch) for strict E-03 gate:

- Enable input `run_cli_perf_regression_gate=true`.
- Set `cli_perf_previous_baseline` to a repo-relative JSON baseline path.
- Optional tuning: `cli_perf_max_regression_ratio`, `cli_perf_iterations`, `cli_perf_warmup`.

Suggested CI trigger (workflow_dispatch) for baseline refresh (candidate only):

- Enable input `run_cli_perf_baseline_refresh=true`.
- Do not auto-update approved baseline in CI; promote only after reviewed candidate artifact.

## 4. Week 3-4 checklist (E2)

- [ ] Implement structured progress event model for long-running flows.
- [ ] Implement cancellation/timeout control points in long-running loops.
- [ ] Keep default CLI output unchanged when integration features are disabled.
- [ ] Add focused tests covering enabled/disabled integration paths.

## 5. Week 5-6 checklist (E3)

- [ ] Enforce three CLI guard gates in release candidate flow: compatibility, output, performance.
- [ ] Verify E-01..E-05 blockers all pass in candidate release review.
- [ ] Record waiver owner and closure date for any temporary exception.

## 6. Evidence checklist

- Full plan: `docs/full-alignment-plan.md`
- Scorecard: `docs/alignment-scorecard.md`
- Weekly template: `docs/alignment-scorecard-weekly-template.md`
- Release policy: `docs/release-policy.md`
- Diagnostics policy: `docs/diagnostics.md`
- CLI performance baseline playbook: `docs/cli-performance-baseline-promotion-playbook.md`

## 7. Exit criteria

Phase E can be marked complete only when all conditions are true:

1. E-01..E-05 blockers are consistently pass for at least one full candidate release cycle.
2. No mandatory service dependency is introduced for existing CLI workflows.
3. Frontend integration capability is available as optional engine-level contracts.
