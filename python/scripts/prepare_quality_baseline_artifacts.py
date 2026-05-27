from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import platform
import shutil
from pathlib import Path


def _load_script_module(script_name: str):
    script_path = Path(__file__).resolve().parent / script_name
    spec = importlib.util.spec_from_file_location(script_name.replace(".py", ""), script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load helper script module: {script_name}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_summary_module = _load_script_module("summarize_quality_baseline_results.py")

load_thresholds = _summary_module.load_thresholds
load_results = _summary_module.load_results
build_summary_payload = _summary_module.build_summary_payload
build_markdown_report = _summary_module.build_markdown_report
validate_summary_payload = _summary_module.validate_summary_payload
validate_no_regression = _summary_module.validate_no_regression


README_TEXT = """quality_summary.current.json
  Current quality baseline summary JSON for timing/verify/sim metrics.

quality_summary.approved-baseline.json
  Optional previously approved baseline summary JSON staged for history diff / no-regression review.

quality_summary.candidate-baseline.json
  Candidate summary JSON to archive as the next approved baseline after review.

quality_summary.current.md
  Human-readable review summary with suite tables and optional History Diff section.

quality_summary.validation.json
  Validation result including summary validation errors and optional no-regression errors.

manifest.json
  Machine-readable artifact metadata with file roles, threshold source/hash,
  no-regression settings, summary overview, and GitHub Actions run context.
"""


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _write_json(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def _summary_overview(payload: dict[str, object]) -> dict[str, object]:
    suites = [suite for suite in payload.get("suites", []) if isinstance(suite, dict)]
    return {
        "suite_count": len(suites),
        "failure_count": int(payload.get("failures", 0)),
        "failed_suites": [str(suite.get("suite")) for suite in suites if int(suite.get("failures", 0)) > 0],
    }


def _path_label(repo_root: Path, path: Path) -> str:
    try:
        return path.relative_to(repo_root).as_posix()
    except ValueError:
        return path.as_posix()


def prepare_quality_baseline_artifacts(
    *,
    repo_root: Path,
    thresholds_path: Path,
    results_json_path: Path,
    artifact_dir: Path,
    previous_summary_json: Path | None,
    validate_no_regression_flag: bool,
    regression_tolerance: float,
    github_context: dict[str, str],
) -> dict[str, object]:
    resolved_thresholds = thresholds_path if thresholds_path.is_absolute() else (repo_root / thresholds_path)
    resolved_results = results_json_path if results_json_path.is_absolute() else (repo_root / results_json_path)
    resolved_artifact_dir = artifact_dir if artifact_dir.is_absolute() else (repo_root / artifact_dir)
    resolved_previous = (
        None
        if previous_summary_json is None
        else (previous_summary_json if previous_summary_json.is_absolute() else (repo_root / previous_summary_json))
    )

    resolved_artifact_dir.mkdir(parents=True, exist_ok=True)

    thresholds = load_thresholds(resolved_thresholds)
    results = load_results(resolved_results)
    previous_summary = None
    if resolved_previous is not None and resolved_previous.exists():
        previous_summary = json.loads(resolved_previous.read_text(encoding="utf-8"))

    payload = build_summary_payload(thresholds, results, previous_summary)
    summary_errors = validate_summary_payload(payload, thresholds)
    regression_errors: list[str] = []
    if validate_no_regression_flag and previous_summary is not None:
        regression_errors = validate_no_regression(payload, previous_summary, regression_tolerance)

    current_json_path = resolved_artifact_dir / "quality_summary.current.json"
    current_md_path = resolved_artifact_dir / "quality_summary.current.md"
    approved_baseline_path = resolved_artifact_dir / "quality_summary.approved-baseline.json"
    candidate_baseline_path = resolved_artifact_dir / "quality_summary.candidate-baseline.json"
    validation_json_path = resolved_artifact_dir / "quality_summary.validation.json"

    _write_json(current_json_path, payload)
    current_md_path.write_text(build_markdown_report(payload), encoding="utf-8")
    shutil.copy2(current_json_path, candidate_baseline_path)
    if resolved_previous is not None and resolved_previous.exists():
        shutil.copy2(resolved_previous, approved_baseline_path)

    validation_payload = {
        "kind": "quality-baseline-validation",
        "schema_version": 1,
        "summary_validation_errors": summary_errors,
        "no_regression_validation_enabled": validate_no_regression_flag,
        "no_regression_errors": regression_errors,
        "all_checks_passed": not summary_errors and not regression_errors and int(payload.get("failures", 0)) == 0,
    }
    _write_json(validation_json_path, validation_payload)

    (resolved_artifact_dir / "README.txt").write_text(README_TEXT, encoding="utf-8")

    manifest = {
        "kind": "quality-baseline-artifacts",
        "schema_version": 1,
        "github_actions_context": github_context,
        "summary_role_files": {
            "current_json": "quality_summary.current.json",
            "current_markdown": "quality_summary.current.md",
            "approved_baseline_json": "quality_summary.approved-baseline.json",
            "candidate_baseline_json": "quality_summary.candidate-baseline.json",
            "validation_json": "quality_summary.validation.json",
        },
        "threshold_manifest": _path_label(repo_root, resolved_thresholds),
        "threshold_manifest_sha256": _sha256(resolved_thresholds),
        "results_source": _path_label(repo_root, resolved_results),
        "results_source_sha256": _sha256(resolved_results),
        "baseline_source": None if resolved_previous is None else _path_label(repo_root, resolved_previous),
        "validate_no_regression": validate_no_regression_flag,
        "regression_tolerance": regression_tolerance,
        "python_version": platform.python_version(),
        "summary_overview": _summary_overview(payload),
        "validation_overview": {
            "summary_validation_error_count": len(summary_errors),
            "no_regression_error_count": len(regression_errors),
            "all_checks_passed": validation_payload["all_checks_passed"],
        },
    }
    _write_json(resolved_artifact_dir / "manifest.json", manifest)
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description="Prepare review artifacts for Week 3 quality baseline summary.")
    parser.add_argument(
        "--thresholds",
        type=Path,
        default=Path("python/tests/benchmarks/week3/quality_thresholds.json"),
    )
    parser.add_argument(
        "--results-json",
        type=Path,
        default=Path("python/tests/benchmarks/week3/quality_results.golden.json"),
    )
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        default=Path("target/week3-quality-review"),
    )
    parser.add_argument(
        "--previous-summary-json",
        type=Path,
        default=Path("python/tests/benchmarks/week3/quality_summary.approved-baseline.json"),
    )
    parser.add_argument("--validate-no-regression", action="store_true")
    parser.add_argument("--regression-tolerance", type=float, default=0.0)
    parser.add_argument("--github-workflow", type=str, default="")
    parser.add_argument("--github-job", type=str, default="")
    parser.add_argument("--github-event-name", type=str, default="")
    parser.add_argument("--github-run-id", type=str, default="")
    parser.add_argument("--github-run-attempt", type=str, default="")
    parser.add_argument("--github-sha", type=str, default="")
    parser.add_argument("--github-ref-name", type=str, default="")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    prepare_quality_baseline_artifacts(
        repo_root=repo_root,
        thresholds_path=args.thresholds,
        results_json_path=args.results_json,
        artifact_dir=args.artifact_dir,
        previous_summary_json=args.previous_summary_json,
        validate_no_regression_flag=args.validate_no_regression,
        regression_tolerance=args.regression_tolerance,
        github_context={
            "workflow": args.github_workflow,
            "job": args.github_job,
            "event_name": args.github_event_name,
            "run_id": args.github_run_id,
            "run_attempt": args.github_run_attempt,
            "sha": args.github_sha,
            "ref_name": args.github_ref_name,
        },
    )


if __name__ == "__main__":
    main()
