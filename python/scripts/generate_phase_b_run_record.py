from __future__ import annotations

import argparse
import json
from datetime import date
from pathlib import Path


def _load_json_if_exists(path: Path) -> dict[str, object] | None:
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def _path_or_pending(path: Path) -> str:
    return path.as_posix() if path.is_file() else "pending"


def _job_result(validation_payload: dict[str, object] | None, current_payload: dict[str, object] | None) -> str:
    if isinstance(validation_payload, dict):
        overview = validation_payload.get("validation_overview")
        if isinstance(overview, dict):
            return "pass" if bool(overview.get("all_decks_passed", False)) else "fail"
    if isinstance(current_payload, dict):
        failures = int(current_payload.get("failures", 0))
        return "pass" if failures == 0 else "fail"
    return "pending"


def _no_regression_path(manifest: dict[str, object] | None) -> str:
    if not isinstance(manifest, dict):
        return "pending"
    enabled = manifest.get("validate_no_regression")
    if enabled is True:
        return "strict"
    if enabled is False:
        return "fallback"
    return "pending"


def _readiness_fields(linux_status: dict[str, object] | None) -> tuple[str, str]:
    if not isinstance(linux_status, dict):
        return ("pending", "pending")

    ready_raw = linux_status.get("baseline_ready")
    if ready_raw is None:
        ready_raw = linux_status.get("ready")

    if ready_raw is None:
        readiness_result = "pending"
    else:
        readiness_result = "pass" if bool(ready_raw) else "fail"

    reason = str(linux_status.get("baseline_reason", "") or linux_status.get("reason", "")).strip()
    if not reason:
        reason = "unknown" if readiness_result != "pending" else "pending"

    return (readiness_result, reason)


def _bool_to_result(value: object) -> str:
    if value is True:
        return "pass"
    if value is False:
        return "fail"
    return "pending"


def _phase_b_precheck_fields(phase_b_check: dict[str, object] | None) -> tuple[str, str, str, str]:
    if not isinstance(phase_b_check, dict):
        return ("pending", "pending", "pending", "pending")

    artifact_bundle_ready = _bool_to_result(phase_b_check.get("artifact_bundle_ready"))
    candidate_promotable = _bool_to_result(phase_b_check.get("candidate_promotable"))
    promotion_ready = _bool_to_result(phase_b_check.get("phase_b_promotion_ready"))

    missing_files_raw = phase_b_check.get("missing_files")
    if isinstance(missing_files_raw, list):
        missing = [str(item) for item in missing_files_raw if str(item).strip()]
        missing_files = "none" if not missing else ", ".join(missing)
    else:
        missing_files = "pending"

    return (artifact_bundle_ready, candidate_promotable, promotion_ready, missing_files)


def build_phase_b_run_record(
    *,
    record_date: str,
    operator: str,
    branch_commit: str,
    workflow_run_url: str,
    artifact_dir: Path,
    linux_status_json: Path,
    phase_b_artifact_check_json: Path,
    output_path: Path,
) -> str:
    manifest_path = artifact_dir / "manifest.json"
    current_json_path = artifact_dir / "waveform_compare_summary.current.json"
    candidate_json_path = artifact_dir / "waveform_compare_summary.candidate-baseline.json"
    candidate_md_path = artifact_dir / "waveform_compare_summary.candidate-baseline.md"
    validation_json_path = artifact_dir / "waveform_compare_summary.validation.json"
    linux_status_path = linux_status_json
    phase_b_artifact_check_path = phase_b_artifact_check_json

    manifest = _load_json_if_exists(manifest_path)
    current_payload = _load_json_if_exists(current_json_path)
    validation_payload = _load_json_if_exists(validation_json_path)
    linux_status = _load_json_if_exists(linux_status_path)
    phase_b_artifact_check = _load_json_if_exists(phase_b_artifact_check_path)

    josim_command = ""
    validate_no_regression = "pending"
    previous_summary_json = ""
    regression_tolerance = ""
    if isinstance(manifest, dict):
        josim_command = str(manifest.get("josim_command", ""))
        validate_no_regression = str(manifest.get("validate_no_regression", "pending")).lower()
        regression_tolerance = str(manifest.get("regression_tolerance_v", ""))
        if (artifact_dir / "waveform_compare_summary.approved-baseline.json").is_file():
            previous_summary_json = (artifact_dir / "waveform_compare_summary.approved-baseline.json").as_posix()

    gate_result = _job_result(validation_payload, current_payload)
    no_regression_path = _no_regression_path(manifest)
    fallback_notice = "yes" if no_regression_path == "fallback" else ("no" if no_regression_path == "strict" else "pending")

    readiness_result, readiness_reason = _readiness_fields(linux_status)
    artifact_bundle_result, candidate_promotable_result, promotion_ready_result, missing_files = _phase_b_precheck_fields(
        phase_b_artifact_check
    )

    j04_status = "PASS" if gate_result == "pass" and readiness_result == "pass" else "FAIL"
    if promotion_ready_result == "fail":
        j04_status = "FAIL"

    content = f"""# Phase B Run Record - {record_date}

Use this record to track Linux waveform gate execution and J-04 closure evidence.

## 1. Run metadata

```md
Date: {record_date}
Operator: {operator}
Branch/commit: {branch_commit or 'pending'}
Workflow run URL: {workflow_run_url or 'pending'}
```

## 2. Workflow dispatch inputs

```md
run_waveform_compare_linux=true
josim_command_linux={josim_command}
validate_no_regression_linux={validate_no_regression}
previous_summary_json_linux={previous_summary_json}
regression_tolerance_v_linux={regression_tolerance}
```

## 3. Generated artifacts

```md
Artifact bundle: {artifact_dir.as_posix()}
current summary json: {_path_or_pending(current_json_path)}
candidate baseline json: {_path_or_pending(candidate_json_path)}
validation json: {_path_or_pending(validation_json_path)}
manifest json: {_path_or_pending(manifest_path)}
linux baseline status json: {_path_or_pending(linux_status_path)}
phase-b artifact check json: {_path_or_pending(phase_b_artifact_check_path)}
```

## 4. Gate outcome

```md
Workflow job result: {gate_result}
No-regression path used: {no_regression_path}
Fallback notice observed: {fallback_notice}
Failure reason (if any):
```

## 5. Baseline promotion

```md
Promotion command executed: no
Precheck command:
uv run python python/scripts/check_phase_b_artifact_bundle.py --artifact-dir target/waveform-compare-linux --linux-status-json target/waveform-baseline-status/linux.local.json --json-output target/waveform-compare-linux/phase-b-artifact-check.json --require-ready

precheck result: {promotion_ready_result}
artifact bundle ready: {artifact_bundle_result}
candidate promotable: {candidate_promotable_result}
missing files: {missing_files}

Command:
uv run python python/scripts/promote_waveform_approved_baseline.py --platform linux --candidate-json {_path_or_pending(candidate_json_path)} --candidate-md {_path_or_pending(candidate_md_path)}

promoted linux baseline json path: python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.json
promoted linux baseline md path: python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.md
```

## 6. Baseline readiness check

```md
Command:
uv run python python/scripts/check_waveform_baseline_status.py --platform linux --require-ready --json-output target/waveform-compare-linux/linux-baseline-status.json

Result: {readiness_result}
Reason: {readiness_reason}
status json path: {_path_or_pending(linux_status_path)}
```

## 7. Scorecard update

```md
Weekly report updated: no
Report file: docs/alignment-scorecard-weekly-2026-05-28.md
J-04 status after this run: {j04_status}
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
"""

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(content, encoding="utf-8")
    return content


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate a prefilled Phase B run record from Linux waveform artifacts.")
    parser.add_argument("--date", type=str, default=date.today().isoformat(), help="Run record date (YYYY-MM-DD).")
    parser.add_argument("--operator", type=str, default="Core maintainers", help="Run operator label.")
    parser.add_argument("--branch-commit", type=str, default="", help="Branch/commit descriptor.")
    parser.add_argument("--workflow-run-url", type=str, default="", help="Workflow run URL.")
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        default=Path("target/waveform-compare-linux"),
        help="Directory containing Linux waveform artifact files.",
    )
    parser.add_argument(
        "--linux-status-json",
        type=Path,
        default=Path("target/waveform-baseline-status/linux.local.json"),
        help="Path to Linux baseline status JSON used for readiness fields.",
    )
    parser.add_argument(
        "--phase-b-artifact-check-json",
        type=Path,
        default=Path("target/waveform-compare-linux/phase-b-artifact-check.json"),
        help="Path to Phase B artifact checker JSON used for promotion precheck fields.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output markdown path. Defaults to docs/phase-b-run-record-<date>.md.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    artifact_dir = args.artifact_dir if args.artifact_dir.is_absolute() else (repo_root / args.artifact_dir)
    linux_status_json = (
        args.linux_status_json if args.linux_status_json.is_absolute() else (repo_root / args.linux_status_json)
    )
    phase_b_artifact_check_json = (
        args.phase_b_artifact_check_json
        if args.phase_b_artifact_check_json.is_absolute()
        else (repo_root / args.phase_b_artifact_check_json)
    )
    output_path = args.output
    if output_path is None:
        output_path = repo_root / "docs" / f"phase-b-run-record-{args.date}.md"
    elif not output_path.is_absolute():
        output_path = repo_root / output_path

    build_phase_b_run_record(
        record_date=args.date,
        operator=args.operator,
        branch_commit=args.branch_commit,
        workflow_run_url=args.workflow_run_url,
        artifact_dir=artifact_dir,
        linux_status_json=linux_status_json,
        phase_b_artifact_check_json=phase_b_artifact_check_json,
        output_path=output_path,
    )


if __name__ == "__main__":
    main()
