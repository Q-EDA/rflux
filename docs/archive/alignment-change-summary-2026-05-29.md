# Alignment Change Summary - 2026-05-29

Purpose: group this round of alignment updates into review-ready change sets.

## 1. Plan and scorecard updates

- Updated full alignment plan with reassessment ranges and a 4-week acceleration plan.
- Added immediate execution evidence and explicit Linux blocker statement.
- Added weekly scorecard update for 2026-05-29.
- Updated scorecard latest-weekly-report pointer.

Files:

- `docs/full-alignment-plan.md`
- `docs/alignment-scorecard-weekly-2026-05-29.md`
- `docs/alignment-scorecard.md`

## 2. Run records and parallel-progress records

- Added Phase B run record for 2026-05-29 with machine precheck outcomes.
- Added non-Linux progress record for Week 2/3 parallel lane.
- Added platform symmetry audit with a 10-command drift spot check.

Files:

- `docs/phase-b-run-record-2026-05-29.md`
- `docs/non-linux-progress-2026-05-29.md`
- `docs/platform-symmetry-audit-2026-05-29.md`

## 3. Known-limitations sync

- Synced simulation limitation wording with latest parity scope and six-coefficient CPR subset wording.
- Added explicit 2026-05-29 Linux baseline blocker evidence note.

Files:

- `docs/known-limitations.md`

## 4. Contract baseline updates from local gates

- Local contract gate execution refreshed API and report-schema contract baselines.
- Follow-up contract tests passed, confirming baseline consistency.

Files:

- `python/tests/contracts/python_api_surface.json`
- `python/tests/contracts/report_schema_surface.json`

Validation evidence:

- `uv run pytest python/tests/test_python_api_surface_contract.py -q` (3 passed)
- `uv run pytest python/tests/test_report_schema_surface_contract.py -q` (3 passed)

## 5. Current blocker and next action

- Blocker: no Linux environment for `josim_command_linux`, so J-04 Linux baseline promotion cannot be completed in this lane.
- Next action when Linux is available: run workflow_dispatch Linux waveform gate, promote baseline, rerun strict no-regression, and update scorecard J-04 from FAIL to PASS.
