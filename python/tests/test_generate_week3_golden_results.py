from __future__ import annotations

import importlib.util
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "generate_week3_golden_results.py"
    spec = importlib.util.spec_from_file_location("generate_week3_golden_results", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
run_pipeline = _module.run_pipeline


def test_run_pipeline_generates_all_week3_outputs(tmp_path: Path) -> None:
    repo_root = Path(__file__).resolve().parents[2]
    out_root = tmp_path / "out"

    result = run_pipeline(
        repo_root=repo_root,
        timing_report=repo_root / "python" / "tests" / "benchmarks" / "week3" / "inputs" / "timing_report.golden.json",
        verify_report=repo_root / "python" / "tests" / "benchmarks" / "week3" / "inputs" / "verify_report.golden.json",
        sim_summary=repo_root / "python" / "tests" / "benchmarks" / "week3" / "inputs" / "sim_summary.golden.json",
        thresholds=repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_thresholds.json",
        results_output=out_root / "quality_results.generated.json",
        summary_json=out_root / "quality_summary.current.json",
        summary_md=out_root / "quality_summary.current.md",
        artifact_dir=out_root / "review",
        previous_summary_json=repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_summary.approved-baseline.json",
        check_results_against=repo_root / "python" / "tests" / "benchmarks" / "week3" / "quality_results.golden.json",
        validate_pass=True,
        validate_no_regression_flag=True,
        regression_tolerance=0.0,
        github_context={
            "workflow": "ci",
            "job": "checks",
            "event_name": "push",
            "run_id": "1",
            "run_attempt": "1",
            "sha": "deadbeef",
            "ref_name": "main",
        },
    )

    assert (out_root / "quality_results.generated.json").exists()
    assert (out_root / "quality_summary.current.json").exists()
    assert (out_root / "quality_summary.current.md").exists()
    assert (out_root / "review" / "quality_summary.current.json").exists()
    assert (out_root / "review" / "quality_summary.validation.json").exists()
    assert (out_root / "review" / "manifest.json").exists()
    assert result["artifact_manifest"]["kind"] == "quality-baseline-artifacts"
