from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_artifact_prep_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "prepare_waveform_compare_artifacts.py"
    spec = importlib.util.spec_from_file_location("prepare_waveform_compare_artifacts", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_artifact_module = _load_artifact_prep_module()
prepare_waveform_compare_artifacts = _artifact_module.prepare_waveform_compare_artifacts


def test_prepare_waveform_compare_artifacts_stages_baseline_and_writes_manifest(tmp_path: Path) -> None:
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir()
    result_dir = tmp_path / "results"
    result_dir.mkdir()
    artifact_dir = tmp_path / "artifacts"

    thresholds_path = benchmark_dir / "waveform_thresholds.json"
    thresholds_path.write_text(
        json.dumps(
            {
                "deck_a.cir": {
                    "max_abs_threshold_v": 0.1,
                    "category": "jj",
                    "rationale": "sanity",
                }
            }
        ),
        encoding="utf-8",
    )
    (result_dir / "deck_a.cir.compare.json").write_text(
        json.dumps(
            {
                "summary": "PASS",
                "worst_max_abs_v": 0.05,
                "failing_nodes": [],
                "top_worst_nodes": [{"node": "out", "max_abs_v": 0.05, "rms_v": 0.02}],
            }
        ),
        encoding="utf-8",
    )

    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.json").write_text(
        json.dumps(
            {
                "failures": 0,
                "decks": [
                    {
                        "deck": "deck_a.cir",
                        "threshold_v": 0.1,
                        "category": "jj",
                        "rationale": "sanity",
                        "worst_max_abs_v": 0.04,
                        "summary": "PASS",
                        "failing_nodes": [],
                        "top_worst_nodes": [{"node": "out", "max_abs_v": 0.04, "rms_v": 0.02}],
                    }
                ],
                "categories": [
                    {
                        "category": "jj",
                        "deck_count": 1,
                        "failures": 0,
                        "worst_max_abs_v": 0.04,
                        "worst_deck": "deck_a.cir",
                        "top_hotspots": [{"deck": "deck_a.cir", "node": "out", "max_abs_v": 0.04, "rms_v": 0.02}],
                    }
                ],
            }
        ),
        encoding="utf-8",
    )

    manifest = prepare_waveform_compare_artifacts(
        repo_root=tmp_path,
        thresholds_path=thresholds_path,
        benchmark_dir=benchmark_dir,
        result_dir=result_dir,
        artifact_dir=artifact_dir,
        previous_summary_json=None,
        baseline_platform="linux",
        validate_no_regression_flag=False,
        regression_tolerance_v=0.0,
        josim_command="josim",
        github_context={
            "workflow": "ci",
            "job": "waveform-compare-optional",
            "event_name": "workflow_dispatch",
            "run_id": "123",
            "run_attempt": "1",
            "sha": "deadbeef",
            "ref_name": "main",
        },
    )

    assert (artifact_dir / "waveform_compare_summary.approved-baseline.json").exists()
    assert (artifact_dir / "waveform_compare_summary.current.md").exists()
    assert (artifact_dir / "waveform_compare_summary.current.json").exists()
    assert (artifact_dir / "waveform_compare_summary.candidate-baseline.md").exists()
    assert (artifact_dir / "waveform_compare_summary.candidate-baseline.json").exists()
    assert (artifact_dir / "waveform_compare_summary.validation.json").exists()
    assert (artifact_dir / "manifest.json").exists()
    assert (artifact_dir / "README.txt").exists()

    current_payload = json.loads((artifact_dir / "waveform_compare_summary.current.json").read_text(encoding="utf-8"))
    manifest_payload = json.loads((artifact_dir / "manifest.json").read_text(encoding="utf-8"))

    assert current_payload["failures"] == 0
    assert current_payload["decks"][0]["deck"] == "deck_a.cir"
    assert manifest["threshold_manifest"] == thresholds_path.as_posix()
    assert manifest_payload["baseline_platform"] == "linux"
    assert manifest_payload["summary_overview"]["deck_count"] == 1
    assert manifest_payload["summary_overview"]["worst_deck"] == "deck_a.cir"
    assert manifest_payload["history_diff_overview"]["has_history_diff"] is True
    assert manifest_payload["validation_overview"]["all_decks_passed"] is True
    assert manifest_payload["github_actions_context"]["job"] == "waveform-compare-optional"