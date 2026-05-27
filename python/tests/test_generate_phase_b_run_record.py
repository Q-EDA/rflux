from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "generate_phase_b_run_record.py"
    spec = importlib.util.spec_from_file_location("generate_phase_b_run_record", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_phase_b_run_record = _module.build_phase_b_run_record


def test_build_phase_b_run_record_prefills_from_artifact_bundle(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "waveform-compare-linux"
    artifact_dir.mkdir(parents=True)

    (artifact_dir / "manifest.json").write_text(
        json.dumps(
            {
                "josim_command": "josim",
                "validate_no_regression": True,
                "regression_tolerance_v": 0.0,
            }
        ),
        encoding="utf-8",
    )
    (artifact_dir / "waveform_compare_summary.current.json").write_text(
        json.dumps({"failures": 0}),
        encoding="utf-8",
    )
    (artifact_dir / "waveform_compare_summary.validation.json").write_text(
        json.dumps({"validation_overview": {"all_decks_passed": True}}),
        encoding="utf-8",
    )
    (artifact_dir / "waveform_compare_summary.candidate-baseline.json").write_text("{}", encoding="utf-8")
    (artifact_dir / "waveform_compare_summary.candidate-baseline.md").write_text("# candidate\n", encoding="utf-8")
    (artifact_dir / "waveform_compare_summary.approved-baseline.json").write_text("{}", encoding="utf-8")
    (artifact_dir / "linux-baseline-status.json").write_text(
        json.dumps({"baseline_ready": False, "baseline_reason": "missing baseline json"}),
        encoding="utf-8",
    )
    (artifact_dir / "phase-b-artifact-check.json").write_text(
        json.dumps(
            {
                "artifact_bundle_ready": True,
                "candidate_promotable": True,
                "phase_b_promotion_ready": True,
                "missing_files": [],
            }
        ),
        encoding="utf-8",
    )

    output_path = tmp_path / "phase-b-run-record-2026-05-28.md"
    content = build_phase_b_run_record(
        record_date="2026-05-28",
        operator="Core maintainers",
        branch_commit="main@abc123",
        workflow_run_url="https://example.test/run/1",
        artifact_dir=artifact_dir,
        linux_status_json=artifact_dir / "linux-baseline-status.json",
        phase_b_artifact_check_json=artifact_dir / "phase-b-artifact-check.json",
        output_path=output_path,
    )

    assert output_path.exists()
    assert "Workflow job result: pass" in content
    assert "No-regression path used: strict" in content
    assert "Fallback notice observed: no" in content
    assert "Result: fail" in content
    assert "Reason: missing baseline json" in content
    assert "precheck result: pass" in content
    assert "artifact bundle ready: pass" in content
    assert "candidate promotable: pass" in content
    assert "missing files: none" in content
    assert "J-04 status after this run: FAIL" in content
    assert "previous_summary_json_linux=" in content


def test_build_phase_b_run_record_supports_ready_reason_status_schema(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "waveform-compare-linux"
    artifact_dir.mkdir(parents=True)
    (artifact_dir / "waveform_compare_summary.current.json").write_text(
        json.dumps({"failures": 0}),
        encoding="utf-8",
    )
    (artifact_dir / "waveform_compare_summary.validation.json").write_text(
        json.dumps({"validation_overview": {"all_decks_passed": True}}),
        encoding="utf-8",
    )

    linux_status_path = tmp_path / "linux.local.json"
    linux_status_path.write_text(
        json.dumps({"ready": False, "reason": "missing baseline json"}),
        encoding="utf-8",
    )

    output_path = tmp_path / "phase-b-run-record-2026-05-28.md"
    content = build_phase_b_run_record(
        record_date="2026-05-28",
        operator="Core maintainers",
        branch_commit="main@abc123",
        workflow_run_url="https://example.test/run/1",
        artifact_dir=artifact_dir,
        linux_status_json=linux_status_path,
        phase_b_artifact_check_json=tmp_path / "missing-check.json",
        output_path=output_path,
    )

    assert "Result: fail" in content
    assert "Reason: missing baseline json" in content
    assert "precheck result: pending" in content


def test_build_phase_b_run_record_fails_j04_when_precheck_fails(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "waveform-compare-linux"
    artifact_dir.mkdir(parents=True)
    (artifact_dir / "waveform_compare_summary.current.json").write_text(
        json.dumps({"failures": 0}),
        encoding="utf-8",
    )
    (artifact_dir / "waveform_compare_summary.validation.json").write_text(
        json.dumps({"validation_overview": {"all_decks_passed": True}}),
        encoding="utf-8",
    )

    linux_status_path = tmp_path / "linux.local.json"
    linux_status_path.write_text(
        json.dumps({"baseline_ready": True, "baseline_reason": "baseline ready"}),
        encoding="utf-8",
    )
    phase_b_check_path = tmp_path / "phase-b-artifact-check.json"
    phase_b_check_path.write_text(
        json.dumps(
            {
                "artifact_bundle_ready": False,
                "candidate_promotable": False,
                "phase_b_promotion_ready": False,
                "missing_files": ["missing.json"],
            }
        ),
        encoding="utf-8",
    )

    output_path = tmp_path / "phase-b-run-record-2026-05-28.md"
    content = build_phase_b_run_record(
        record_date="2026-05-28",
        operator="Core maintainers",
        branch_commit="main@abc123",
        workflow_run_url="https://example.test/run/1",
        artifact_dir=artifact_dir,
        linux_status_json=linux_status_path,
        phase_b_artifact_check_json=phase_b_check_path,
        output_path=output_path,
    )

    assert "Workflow job result: pass" in content
    assert "Result: pass" in content
    assert "precheck result: fail" in content
    assert "missing files: missing.json" in content
    assert "J-04 status after this run: FAIL" in content
