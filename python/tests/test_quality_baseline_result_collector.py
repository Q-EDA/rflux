from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "collect_quality_baseline_results.py"
    spec = importlib.util.spec_from_file_location("collect_quality_baseline_results", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_quality_results = _module.build_quality_results
extract_verify_metrics = _module.extract_verify_metrics
assert_payloads_match = _module.assert_payloads_match


def test_extract_verify_metrics_counts_counterexamples_for_non_equivalent_result() -> None:
    metrics = extract_verify_metrics(
        {
            "equivalent": False,
            "output_mismatches": [{"pin": "out"}, {"pin": "aux"}],
        }
    )

    assert metrics["equivalence_pass_rate"] == 0.0
    assert metrics["counterexample_count"] == 2.0


def test_collector_rebuilds_week3_golden_quality_results() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    input_dir = repo_root / "python" / "tests" / "benchmarks" / "week3" / "inputs"
    expected = json.loads(
        (repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_results.golden.json").read_text(
            encoding="utf-8"
        )
    )

    payload = build_quality_results(
        timing_report=json.loads((input_dir / "timing_report.golden.json").read_text(encoding="utf-8")),
        verify_report=json.loads((input_dir / "verify_report.golden.json").read_text(encoding="utf-8")),
        sim_summary=json.loads((input_dir / "sim_summary.golden.json").read_text(encoding="utf-8")),
    )

    assert_payloads_match(expected, payload)
