from __future__ import annotations

import importlib.util


def _load_summary_module():
    from pathlib import Path

    script_path = Path(__file__).resolve().parents[1] / "scripts" / "summarize_waveform_compare_results.py"
    spec = importlib.util.spec_from_file_location("summarize_waveform_compare_results", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_summary_module = _load_summary_module()
build_markdown_report = _summary_module.build_markdown_report


def test_build_markdown_report_counts_missing_and_failures() -> None:
    thresholds = {
        "a.cir": 0.1,
        "b.cir": 0.1,
        "c.cir": 0.1,
    }
    results = {
        "a": {"worst_max_abs_v": 0.05},
        "b": {"worst_max_abs_v": 0.2},
    }

    report, failures = build_markdown_report(thresholds, results)

    assert failures == 2
    assert "| a.cir |" in report
    assert "| b.cir |" in report
    assert "| c.cir |" in report
    assert "MISSING" in report
    assert "FAIL" in report
