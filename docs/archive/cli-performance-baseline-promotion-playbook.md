# CLI Performance Baseline Promotion Playbook

## 1. Purpose

This playbook defines how to keep the Phase E E-03 CLI performance gate strict, reproducible, and reviewable.

## 2. Scope

- In scope: baseline capture for core CLI paths, regression threshold checks, approved-baseline promotion.
- Out of scope: adding service runtime dependencies or changing default CLI behavior.

## 3. Repo baseline files

Approved baseline naming convention:

- `python/tests/benchmarks/phasee/cli_perf_baseline.linux-approved-baseline.json`
- `python/tests/benchmarks/phasee/cli_perf_baseline.windows-approved-baseline.json`

Current CI workflow-dispatch gate runs on Ubuntu and defaults to this Linux baseline unless explicitly overridden.

The Windows baseline file is reserved for future Windows strict gating so the schema and naming stay symmetric before the Windows job is enabled.

## 4. Step A: Capture a candidate baseline

Run from repo root:

```bash
uv run python python/scripts/capture_cli_perf_baseline.py \
  --output target/cli-perf/cli_perf_baseline.candidate.json \
  --iterations 3 \
  --warmup 1
```

Optional strict check against current approved baseline:

```bash
uv run python python/scripts/capture_cli_perf_baseline.py \
  --output target/cli-perf/cli_perf_baseline.candidate.json \
  --previous-baseline python/tests/benchmarks/phasee/cli_perf_baseline.linux-approved-baseline.json \
  --expected-previous-platform-prefix linux \
  --iterations 3 \
  --warmup 1 \
  --max-regression-ratio 0.2 \
  --fail-on-regression
```

## 5. Step B: Review and promote baseline

Promotion policy:

- Promote only after a reviewed green run.
- Do not promote when regressions are unexplained.
- Include reason and threshold context in release review notes.

Promotion command:

```bash
copy target\cli-perf\cli_perf_baseline.candidate.json python\tests\benchmarks\phasee\cli_perf_baseline.linux-approved-baseline.json
```

## 6. Step C: Run strict CI gate

In GitHub Actions `CI` workflow (`workflow_dispatch`):

- Set `run_cli_perf_regression_gate=true`.
- Optionally override:
  - `cli_perf_previous_baseline`
  - `cli_perf_max_regression_ratio`
  - `cli_perf_iterations`
  - `cli_perf_warmup`

Expected artifact on success/failure:

- `cli-perf-regression-gate`

## 7. Review linkage

- Scorecard blocker definition: `docs/alignment-scorecard.md` (E-03).
- Execution checklist: `docs/phase-e-execution-checklist.md`.
- Full plan anchor: `docs/full-alignment-plan.md`.
