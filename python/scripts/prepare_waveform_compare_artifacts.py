from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
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


_summary_module = _load_script_module("summarize_waveform_compare_results.py")
_manifest_runner_module = _load_script_module("run_waveform_compare_manifest.py")

_load_thresholds = _summary_module._load_thresholds
_load_results = _summary_module._load_results
_load_previous_summary = _summary_module._load_previous_summary
build_markdown_report = _summary_module.build_markdown_report
build_summary_payload = _summary_module.build_summary_payload
validate_no_regression = _summary_module.validate_no_regression
validate_summary_payload = _summary_module.validate_summary_payload
resolve_previous_summary_json = _manifest_runner_module.resolve_previous_summary_json


README_TEXT = """waveform_compare_summary.current.json
  Current run summary JSON for this optional waveform compare job.

waveform_compare_summary.approved-baseline.json
  Optional previously approved baseline summary JSON staged from workflow input.
  When present, the current summary is generated with --previous-summary-json so the History Diff section is populated automatically.

waveform_compare_summary.candidate-baseline.json
  Same payload, named for archival as the next approved baseline.
  Download this file from a reviewed green run and pass it to:
    python/scripts/summarize_waveform_compare_results.py --previous-summary-json <that-file>
  on a later run to generate a History Diff report.

manifest.json
  Machine-readable artifact metadata including current/candidate-baseline file roles,
  threshold manifest path and SHA-256, josim command, Python version,
  optional baseline_platform auto-resolution,
  a summary_overview quick-look object with pass/fail/missing deck triage,
  a category_overview quick-look list for bucket-level triage,
  a hotspot_overview quick-look object for deck/category hotspot triage,
  a history_diff_overview quick-look object when a previous summary is present,
  a validation_overview quick-look object plus validation_json semantics,
  optional no-regression validation settings,
  and GitHub Actions run context.
"""


def _resolve_repo_path(repo_root: Path, raw_path: str) -> Path | None:
    normalized = raw_path.strip()
    if not normalized:
        return None
    candidate = Path(normalized)
    if candidate.is_absolute():
        return candidate
    return (repo_root / candidate).resolve()


def _write_json(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _summary_overview(payload: dict[str, object]) -> dict[str, object]:
    decks = [deck for deck in payload.get("decks", []) if isinstance(deck, dict)]
    categories = [category for category in payload.get("categories", []) if isinstance(category, dict)]
    worst = max(
        (deck for deck in decks if deck.get("worst_max_abs_v") is not None),
        key=lambda deck: float(deck.get("worst_max_abs_v", 0.0)),
        default=None,
    )
    passing = [str(deck.get("deck")) for deck in decks if str(deck.get("summary")) == "PASS"]
    failing = [str(deck.get("deck")) for deck in decks if str(deck.get("summary")) == "FAIL"]
    missing = [str(deck.get("deck")) for deck in decks if str(deck.get("summary")) == "MISSING"]
    return {
        "deck_count": len(decks),
        "category_count": len(categories),
        "passing_deck_count": len(passing),
        "failure_count": int(payload.get("failures", 0)),
        "failing_decks": failing,
        "missing_decks": missing,
        "worst_deck": None if worst is None else str(worst.get("deck")),
        "worst_max_abs_v": None if worst is None else float(worst.get("worst_max_abs_v", 0.0)),
        "failing_categories": [
            str(category.get("category"))
            for category in categories
            if int(category.get("failures", 0)) > 0
        ],
    }


def _category_overview(payload: dict[str, object]) -> list[dict[str, object]]:
    categories = [category for category in payload.get("categories", []) if isinstance(category, dict)]
    return [
        {
            "category": str(category.get("category")),
            "deck_count": int(category.get("deck_count", 0)),
            "failure_count": int(category.get("failures", 0)),
            "worst_deck": None if category.get("worst_deck") is None else str(category.get("worst_deck")),
            "worst_max_abs_v": float(category.get("worst_max_abs_v", 0.0)),
        }
        for category in categories
    ]


def _hotspot_overview(payload: dict[str, object]) -> dict[str, object]:
    decks = [deck for deck in payload.get("decks", []) if isinstance(deck, dict)]
    categories = [category for category in payload.get("categories", []) if isinstance(category, dict)]
    deck_hotspots: list[dict[str, object]] = []
    for deck in decks:
        for node in deck.get("top_worst_nodes", []):
            if not isinstance(node, dict):
                continue
            deck_hotspots.append(
                {
                    "deck": str(deck.get("deck")),
                    "node": str(node.get("node")),
                    "max_abs_v": float(node.get("max_abs_v", 0.0)),
                    "rms_v": float(node.get("rms_v", 0.0)),
                }
            )
    deck_hotspots.sort(key=lambda entry: (-float(entry["max_abs_v"]), -float(entry["rms_v"]), str(entry["deck"]), str(entry["node"])))

    category_hotspots: list[dict[str, object]] = []
    for category in categories:
        for node in category.get("top_hotspots", []):
            if not isinstance(node, dict):
                continue
            category_hotspots.append(
                {
                    "category": str(category.get("category")),
                    "deck": str(node.get("deck")),
                    "node": str(node.get("node")),
                    "max_abs_v": float(node.get("max_abs_v", 0.0)),
                    "rms_v": float(node.get("rms_v", 0.0)),
                }
            )
    category_hotspots.sort(
        key=lambda entry: (
            -float(entry["max_abs_v"]),
            -float(entry["rms_v"]),
            str(entry["category"]),
            str(entry["deck"]),
            str(entry["node"]),
        )
    )
    return {
        "top_deck_hotspots": deck_hotspots[:3],
        "top_category_hotspots": category_hotspots[:3],
    }


def _history_diff_overview(payload: dict[str, object]) -> dict[str, object]:
    history = payload.get("history_diff")
    result: dict[str, object] = {"has_history_diff": False}
    if not isinstance(history, dict):
        return result

    def _change_key(entry: dict[str, object]) -> float:
        delta = entry.get("worst_max_abs_v_delta")
        return abs(float(delta)) if delta is not None else -1.0

    deck_changes = [entry for entry in history.get("deck_changes", []) if isinstance(entry, dict)]
    category_changes = [entry for entry in history.get("category_changes", []) if isinstance(entry, dict)]
    top_deck = max(deck_changes, key=_change_key, default=None)
    top_category = max(category_changes, key=_change_key, default=None)
    return {
        "has_history_diff": True,
        "failure_delta": int(history.get("failure_delta", 0)),
        "deck_change_count": len(deck_changes),
        "category_change_count": len(category_changes),
        "top_deck_change": None
        if top_deck is None
        else {
            "deck": str(top_deck.get("deck")),
            "current_summary": str(top_deck.get("current_summary")),
            "previous_summary": str(top_deck.get("previous_summary")),
            "worst_max_abs_v_delta": None
            if top_deck.get("worst_max_abs_v_delta") is None
            else float(top_deck.get("worst_max_abs_v_delta")),
        },
        "top_category_change": None
        if top_category is None
        else {
            "category": str(top_category.get("category")),
            "failure_delta": int(top_category.get("current_failures", 0)) - int(top_category.get("previous_failures", 0)),
            "worst_max_abs_v_delta": None
            if top_category.get("worst_max_abs_v_delta") is None
            else float(top_category.get("worst_max_abs_v_delta")),
        },
    }


def _validation_overview(payload: dict[str, object]) -> dict[str, object]:
    decks = [deck for deck in payload.get("decks", []) if isinstance(deck, dict)]
    categories = [category for category in payload.get("categories", []) if isinstance(category, dict)]
    failures = int(payload.get("failures", 0))
    return {
        "validated_deck_count": len(decks),
        "validated_category_count": len(categories),
        "validated_failure_count": failures,
        "all_decks_passed": failures == 0,
    }


def _build_manifest(
    payload: dict[str, object],
    validation_payload: dict[str, object],
    thresholds_path: Path,
    josim_command: str,
    baseline_platform: str,
    validate_no_regression_flag: bool,
    regression_tolerance_v: float,
    github_context: dict[str, str],
) -> dict[str, object]:
    return {
        "kind": "waveform-compare-artifacts",
        "schema_version": 8,
        "github_actions_context": github_context,
        "summary_role_files": {
            "current_markdown": "waveform_compare_summary.current.md",
            "current_json": "waveform_compare_summary.current.json",
            "approved_baseline_json": "waveform_compare_summary.approved-baseline.json",
            "candidate_baseline_markdown": "waveform_compare_summary.candidate-baseline.md",
            "candidate_baseline_json": "waveform_compare_summary.candidate-baseline.json",
            "validation_json": "waveform_compare_summary.validation.json",
        },
        "threshold_manifest": thresholds_path.as_posix(),
        "threshold_manifest_sha256": _sha256(thresholds_path),
        "josim_command": josim_command,
        "baseline_platform": baseline_platform,
        "validate_no_regression": validate_no_regression_flag,
        "regression_tolerance_v": regression_tolerance_v,
        "python_version": platform.python_version(),
        "summary_overview": _summary_overview(payload),
        "category_overview": _category_overview(payload),
        "hotspot_overview": _hotspot_overview(payload),
        "history_diff_overview": _history_diff_overview(payload),
        "validation_overview": _validation_overview(validation_payload),
        "validation_contract": {
            "validation_json": "waveform_compare_summary.validation.json",
            "schema_matches_current_summary": True,
            "generated_with_validate_pass": True,
            "present_only_when_zero_failures": True,
            "intended_use": "Machine-readable proof that the summary payload matched the threshold manifest and contained no failing or missing decks.",
        },
        "review_contract": {
            "current_json": "Current run summary JSON for this optional waveform compare job.",
            "approved_baseline_json": "Optional previous approved baseline summary JSON staged from workflow input and used to populate history_diff when present.",
            "candidate_baseline_json": "Archive this file from an approved green run and reuse it as --previous-summary-json on a later run.",
            "validation_json": "Validation-only summary payload produced with --validate-pass.",
        },
    }


def prepare_waveform_compare_artifacts(
    *,
    repo_root: Path,
    thresholds_path: Path,
    benchmark_dir: Path,
    result_dir: Path,
    artifact_dir: Path,
    previous_summary_json: Path | None,
    baseline_platform: str,
    validate_no_regression_flag: bool,
    regression_tolerance_v: float,
    josim_command: str,
    github_context: dict[str, str],
) -> dict[str, object]:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    staged_baseline_path = artifact_dir / "waveform_compare_summary.approved-baseline.json"
    resolved_previous_summary = resolve_previous_summary_json(
        previous_summary_json,
        artifact_dir,
        benchmark_dir,
        baseline_platform,
    )
    if resolved_previous_summary is not None and resolved_previous_summary.resolve() != staged_baseline_path.resolve():
        shutil.copy2(resolved_previous_summary, staged_baseline_path)
        resolved_previous_summary = staged_baseline_path

    previous_payload = _load_previous_summary(resolved_previous_summary)
    thresholds = _load_thresholds(thresholds_path)
    results = _load_results(result_dir)
    current_payload = build_summary_payload(thresholds, results, previous_payload)
    current_report, failures = build_markdown_report(thresholds, results, previous_payload)

    current_markdown_path = artifact_dir / "waveform_compare_summary.current.md"
    current_json_path = artifact_dir / "waveform_compare_summary.current.json"
    candidate_markdown_path = artifact_dir / "waveform_compare_summary.candidate-baseline.md"
    candidate_json_path = artifact_dir / "waveform_compare_summary.candidate-baseline.json"
    validation_json_path = artifact_dir / "waveform_compare_summary.validation.json"
    manifest_path = artifact_dir / "manifest.json"
    readme_path = artifact_dir / "README.txt"

    current_markdown_path.write_text(current_report, encoding="utf-8")
    _write_json(current_json_path, current_payload)

    validation_errors = validate_summary_payload(current_payload, thresholds)
    if validation_errors:
        for error in validation_errors:
            print(f"validation_error={error}")
        raise SystemExit(1)

    if validate_no_regression_flag:
        if previous_payload is None:
            print("validation_error=no previous summary JSON provided for no-regression validation")
            raise SystemExit(1)
        regression_errors = validate_no_regression(current_payload, previous_payload, regression_tolerance_v)
        if regression_errors:
            for error in regression_errors:
                print(f"validation_error={error}")
            raise SystemExit(1)

    if failures > 0:
        raise SystemExit(1)

    shutil.copy2(current_markdown_path, candidate_markdown_path)
    shutil.copy2(current_json_path, candidate_json_path)
    validation_payload = build_summary_payload(thresholds, results, previous_payload)
    _write_json(validation_json_path, validation_payload)
    readme_path.write_text(README_TEXT, encoding="utf-8")

    manifest_payload = _build_manifest(
        current_payload,
        validation_payload,
        thresholds_path,
        josim_command,
        baseline_platform,
        validate_no_regression_flag,
        regression_tolerance_v,
        github_context,
    )
    _write_json(manifest_path, manifest_payload)
    return manifest_payload


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Prepare waveform compare summary, validation, baseline, and manifest artifacts for CI review.",
    )
    parser.add_argument(
        "--thresholds",
        type=str,
        default="python/tests/benchmarks/phase6/waveform_thresholds.json",
        help="Repo-relative or absolute threshold JSON file path.",
    )
    parser.add_argument(
        "--benchmark-dir",
        type=str,
        default="python/tests/benchmarks/phase6",
        help="Repo-relative or absolute benchmark directory used for repo-tracked approved baselines.",
    )
    parser.add_argument(
        "--result-dir",
        type=str,
        default="python/tests/benchmarks/phase6",
        help="Repo-relative or absolute directory containing *.compare.json outputs.",
    )
    parser.add_argument(
        "--artifact-dir",
        type=str,
        default="target/waveform-compare",
        help="Repo-relative or absolute directory where review artifacts are written.",
    )
    parser.add_argument(
        "--previous-summary-json",
        type=str,
        default="",
        help="Optional repo-relative or absolute approved baseline summary JSON path.",
    )
    parser.add_argument(
        "--baseline-platform",
        type=str,
        default="",
        help="Optional platform key used to auto-resolve repo-tracked approved baselines.",
    )
    parser.add_argument(
        "--validate-no-regression",
        action="store_true",
        help="Fail when current results regress relative to the resolved approved baseline.",
    )
    parser.add_argument(
        "--regression-tolerance-v",
        type=float,
        default=0.0,
        help="Allowed positive worst_max_abs_v drift during no-regression validation.",
    )
    parser.add_argument(
        "--josim-command",
        type=str,
        default=os.environ.get("RFLOW_JOSIM_COMMAND", "josim"),
        help="External simulator command captured in the generated artifact manifest.",
    )
    parser.add_argument("--github-workflow", type=str, default=os.environ.get("GITHUB_WORKFLOW", ""))
    parser.add_argument("--github-job", type=str, default="waveform-compare-optional")
    parser.add_argument("--github-event-name", type=str, default=os.environ.get("GITHUB_EVENT_NAME", ""))
    parser.add_argument("--github-run-id", type=str, default=os.environ.get("GITHUB_RUN_ID", ""))
    parser.add_argument("--github-run-attempt", type=str, default=os.environ.get("GITHUB_RUN_ATTEMPT", ""))
    parser.add_argument("--github-sha", type=str, default=os.environ.get("GITHUB_SHA", ""))
    parser.add_argument("--github-ref-name", type=str, default=os.environ.get("GITHUB_REF_NAME", ""))
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    thresholds_path = _resolve_repo_path(repo_root, args.thresholds)
    benchmark_dir = _resolve_repo_path(repo_root, args.benchmark_dir)
    result_dir = _resolve_repo_path(repo_root, args.result_dir)
    artifact_dir = _resolve_repo_path(repo_root, args.artifact_dir)
    previous_summary_json = _resolve_repo_path(repo_root, args.previous_summary_json)
    if thresholds_path is None or benchmark_dir is None or result_dir is None or artifact_dir is None:
        raise SystemExit("required path argument resolved to empty value")

    prepare_waveform_compare_artifacts(
        repo_root=repo_root,
        thresholds_path=thresholds_path,
        benchmark_dir=benchmark_dir,
        result_dir=result_dir,
        artifact_dir=artifact_dir,
        previous_summary_json=previous_summary_json,
        baseline_platform=args.baseline_platform,
        validate_no_regression_flag=args.validate_no_regression,
        regression_tolerance_v=args.regression_tolerance_v,
        josim_command=args.josim_command,
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