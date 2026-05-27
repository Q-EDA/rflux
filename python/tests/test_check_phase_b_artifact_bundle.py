from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "check_phase_b_artifact_bundle.py"
    spec = importlib.util.spec_from_file_location("check_phase_b_artifact_bundle", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
check_phase_b_artifact_bundle = _module.check_phase_b_artifact_bundle


def test_check_phase_b_artifact_bundle_ready(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "waveform-compare-linux"
    artifact_dir.mkdir(parents=True)

    (artifact_dir / "waveform_compare_summary.current.json").write_text("{}", encoding="utf-8")
    (artifact_dir / "waveform_compare_summary.candidate-baseline.json").write_text(
        json.dumps({"failures": 0}),
        encoding="utf-8",
    )
    (artifact_dir / "waveform_compare_summary.candidate-baseline.md").write_text("# ok\n", encoding="utf-8")
    (artifact_dir / "waveform_compare_summary.validation.json").write_text("{}", encoding="utf-8")
    (artifact_dir / "manifest.json").write_text(
        json.dumps({"validate_no_regression": True, "baseline_platform": "linux"}),
        encoding="utf-8",
    )

    linux_status_json = tmp_path / "linux.local.json"
    linux_status_json.write_text(
        json.dumps({"baseline_ready": True, "baseline_reason": "baseline ready"}),
        encoding="utf-8",
    )

    result = check_phase_b_artifact_bundle(
        artifact_dir=artifact_dir,
        linux_status_json=linux_status_json,
    )

    assert result["missing_files"] == []
    assert result["candidate_failures"] == 0
    assert result["candidate_promotable"] is True
    assert result["linux_status_ready"] is True
    assert result["artifact_bundle_ready"] is True
    assert result["phase_b_promotion_ready"] is True


def test_check_phase_b_artifact_bundle_missing_files_and_failures(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "waveform-compare-linux"
    artifact_dir.mkdir(parents=True)

    (artifact_dir / "waveform_compare_summary.candidate-baseline.json").write_text(
        json.dumps({"failures": 2}),
        encoding="utf-8",
    )

    result = check_phase_b_artifact_bundle(
        artifact_dir=artifact_dir,
        linux_status_json=None,
    )

    assert len(result["missing_files"]) == 4
    assert result["candidate_promotable"] is False
    assert result["artifact_bundle_ready"] is False
    assert result["phase_b_promotion_ready"] is False
