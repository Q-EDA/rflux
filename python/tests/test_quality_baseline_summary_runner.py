from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "summarize_quality_baseline_results.py"
    spec = importlib.util.spec_from_file_location("summarize_quality_baseline_results", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
load_thresholds = _module.load_thresholds
load_results = _module.load_results
build_summary_payload = _module.build_summary_payload
build_markdown_report = _module.build_markdown_report
validate_no_regression = _module.validate_no_regression


def test_week3_golden_results_pass_thresholds() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    thresholds = load_thresholds(repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_thresholds.json")
    results = load_results(repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_results.golden.json")

    payload = build_summary_payload(thresholds, results)

    assert payload["failures"] == 0
    assert payload["suite_count"] == 3


def test_runner_outputs_history_diff_with_previous_summary() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    thresholds = load_thresholds(repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_thresholds.json")
    results = load_results(repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_results.golden.json")
    previous = {
        "failures": 1,
        "suites": [
            {
                "suite": "sim",
                "metrics": [
                    {
                        "metric": "worst_max_abs_v",
                        "value": 0.003,
                        "status": "FAIL",
                    }
                ],
            }
        ],
    }

    payload = build_summary_payload(thresholds, results, previous)
    markdown = build_markdown_report(payload)

    assert payload["history_diff"]["failure_delta"] == -1
    assert "## History Diff" in markdown
    assert "FAIL -> PASS" in markdown


def test_week3_golden_results_no_regression_against_approved_baseline() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    thresholds = load_thresholds(repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_thresholds.json")
    results = load_results(repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_results.golden.json")
    previous_summary = json.loads(
        (repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_summary.approved-baseline.json").read_text(
            encoding="utf-8"
        )
    )

    payload = build_summary_payload(thresholds, results, previous_summary)
    errors = validate_no_regression(payload, previous_summary, tolerance=0.0)

    assert errors == []
