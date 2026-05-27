from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "prepare_quality_baseline_artifacts.py"
    spec = importlib.util.spec_from_file_location("prepare_quality_baseline_artifacts", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
prepare_quality_baseline_artifacts = _module.prepare_quality_baseline_artifacts


def test_prepare_quality_baseline_artifacts_writes_expected_bundle(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    benchmark_dir = repo_root / "python" / "tests" / "benchmarks" / "week3"
    benchmark_dir.mkdir(parents=True)

    thresholds_path = benchmark_dir / "quality_thresholds.json"
    thresholds_path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "kind": "quality-thresholds",
                "suites": {
                    "timing": {
                        "worst_setup_slack_ps": {
                            "min": 0.0,
                            "rationale": "timing",
                        }
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    results_path = benchmark_dir / "quality_results.golden.json"
    results_path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "kind": "quality-baseline-results",
                "suites": {
                    "timing": {
                        "worst_setup_slack_ps": 5.0,
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    previous_path = benchmark_dir / "quality_summary.approved-baseline.json"
    previous_path.write_text(
        json.dumps(
            {
                "kind": "quality-baseline-summary",
                "schema_version": 1,
                "failures": 0,
                "suites": [
                    {
                        "suite": "timing",
                        "metrics": [
                            {
                                "metric": "worst_setup_slack_ps",
                                "status": "PASS",
                                "value": 5.0,
                            }
                        ],
                    }
                ],
            }
        ),
        encoding="utf-8",
    )

    artifact_dir = repo_root / "target" / "week3-quality-review"
    manifest = prepare_quality_baseline_artifacts(
        repo_root=repo_root,
        thresholds_path=thresholds_path,
        results_json_path=results_path,
        artifact_dir=artifact_dir,
        previous_summary_json=previous_path,
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

    assert (artifact_dir / "quality_summary.current.json").exists()
    assert (artifact_dir / "quality_summary.current.md").exists()
    assert (artifact_dir / "quality_summary.approved-baseline.json").exists()
    assert (artifact_dir / "quality_summary.candidate-baseline.json").exists()
    assert (artifact_dir / "quality_summary.validation.json").exists()
    assert (artifact_dir / "manifest.json").exists()
    assert (artifact_dir / "README.txt").exists()

    manifest_payload = json.loads((artifact_dir / "manifest.json").read_text(encoding="utf-8"))
    validation_payload = json.loads((artifact_dir / "quality_summary.validation.json").read_text(encoding="utf-8"))

    assert manifest["kind"] == "quality-baseline-artifacts"
    assert manifest_payload["summary_overview"]["failure_count"] == 0
    assert manifest_payload["validation_overview"]["all_checks_passed"] is True
    assert manifest_payload["github_actions_context"]["job"] == "checks"
    assert validation_payload["all_checks_passed"] is True
