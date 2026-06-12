# Non-Linux Progress Record - 2026-05-29

Purpose: track alignment work that can proceed without a Linux waveform environment.

## 1. Scope

This record covers Week 2/3 parallel tasks that do not require Linux JoSIM execution.

## 2. Contract and baseline gates

Executed commands:

- uv run python python/scripts/export_python_api_surface.py --check
- uv run python python/scripts/export_cli_command_surface.py --check
- uv run python python/scripts/export_report_schema_surface.py --check
- uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0

Result:

- All commands completed successfully (no error output).

Contract baseline refresh detected from local gates:

- python/tests/contracts/python_api_surface.json
- python/tests/contracts/report_schema_surface.json

Post-refresh validation:

- uv run pytest python/tests/test_python_api_surface_contract.py -q (3 passed)
- uv run pytest python/tests/test_report_schema_surface_contract.py -q (3 passed)

## 3. Release helper smoke tests

Executed commands:

- uv run pytest python/tests/test_prepare_release_artifacts.py -q
- uv run pytest python/tests/test_check_release_artifact_bundle.py -q
- uv run pytest python/tests/test_generate_release_review_record.py -q
- uv run pytest python/tests/test_generate_release_notes.py -q

Result summary:

- test_prepare_release_artifacts.py: 1 passed
- test_check_release_artifact_bundle.py: 2 passed
- test_generate_release_review_record.py: 2 passed
- test_generate_release_notes.py: 2 passed

## 4. Blocking item not covered by this record

The following item remains blocked by missing Linux environment:

- J-04 Linux same-platform approved baseline generation and promotion.

## 5. Next action when Linux environment is available

1. Run CI workflow_dispatch with run_waveform_compare_linux=true and a valid josim_command_linux.
2. Promote candidate baseline with promote_waveform_approved_baseline.py --platform linux.
3. Re-run strict no-regression Linux verification with previous_summary_json_linux set to the promoted Linux baseline.
