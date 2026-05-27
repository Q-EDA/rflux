from __future__ import annotations

import importlib.util
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "summarize_quality_baseline_results.py"
    spec = importlib.util.spec_from_file_location("summarize_quality_baseline_results", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_summary_payload = _module.build_summary_payload
build_markdown_report = _module.build_markdown_report
validate_summary_payload = _module.validate_summary_payload
validate_no_regression = _module.validate_no_regression


def test_build_summary_payload_reports_failures_and_missing_metrics() -> None:
    thresholds = {
        "timing": {
            "worst_setup_slack_ps": {"min": 0.0, "max": None, "rationale": "timing"},
            "worst_hold_slack_ps": {"min": 0.0, "max": None, "rationale": "hold"},
        }
    }
    results = {
        "timing": {
            "worst_setup_slack_ps": -0.5,
        }
    }

    payload = build_summary_payload(thresholds, results)

    assert payload["failures"] == 2
    suite = payload["suites"][0]
    metrics = {entry["metric"]: entry for entry in suite["metrics"]}
    assert metrics["worst_setup_slack_ps"]["status"] == "FAIL"
    assert metrics["worst_hold_slack_ps"]["status"] == "MISSING"


def test_build_markdown_report_includes_history_diff() -> None:
    thresholds = {
        "sim": {
            "worst_max_abs_v": {"min": None, "max": 0.1, "rationale": "sim"},
        }
    }
    current = {"sim": {"worst_max_abs_v": 0.09}}
    previous_summary = {
        "failures": 1,
        "suites": [
            {
                "suite": "sim",
                "metrics": [
                    {
                        "metric": "worst_max_abs_v",
                        "value": 0.2,
                        "status": "FAIL",
                    }
                ],
            }
        ],
    }

    payload = build_summary_payload(thresholds, current, previous_summary)
    markdown = build_markdown_report(payload)

    assert "# Quality Baseline Summary" in markdown
    assert "## History Diff" in markdown
    assert "FAIL -> PASS" in markdown


def test_validate_summary_payload_reports_suite_coverage_mismatch() -> None:
    thresholds = {
        "timing": {
            "worst_setup_slack_ps": {"min": 0.0, "max": None, "rationale": "timing"},
        },
        "verify": {
            "equivalence_pass_rate": {"min": 1.0, "max": None, "rationale": "verify"},
        },
    }
    payload = {
        "suites": [
            {
                "suite": "timing",
                "metrics": [],
            }
        ]
    }

    errors = validate_summary_payload(payload, thresholds)

    assert errors
    assert "suite coverage mismatch" in errors[0]


def test_validate_no_regression_reports_worsened_status_and_value_drift() -> None:
    payload = {
        "suites": [
            {
                "suite": "sim",
                "metrics": [
                    {
                        "metric": "worst_max_abs_v",
                        "status": "FAIL",
                        "value": 0.003,
                        "min": None,
                        "max": 0.01,
                    }
                ],
            }
        ]
    }
    previous = {
        "suites": [
            {
                "suite": "sim",
                "metrics": [
                    {
                        "metric": "worst_max_abs_v",
                        "status": "PASS",
                        "value": 0.001,
                    }
                ],
            }
        ]
    }

    errors = validate_no_regression(payload, previous, tolerance=0.0)

    assert errors
    assert "status worsened PASS -> FAIL" in "\n".join(errors)


def test_validate_no_regression_allows_small_drift_with_tolerance() -> None:
    payload = {
        "suites": [
            {
                "suite": "timing",
                "metrics": [
                    {
                        "metric": "worst_setup_slack_ps",
                        "status": "PASS",
                        "value": 10.0,
                        "min": 0.0,
                        "max": None,
                    }
                ],
            }
        ]
    }
    previous = {
        "suites": [
            {
                "suite": "timing",
                "metrics": [
                    {
                        "metric": "worst_setup_slack_ps",
                        "status": "PASS",
                        "value": 10.4,
                    }
                ],
            }
        ]
    }

    no_errors = validate_no_regression(payload, previous, tolerance=0.5)
    with_errors = validate_no_regression(payload, previous, tolerance=0.1)

    assert no_errors == []
    assert with_errors
