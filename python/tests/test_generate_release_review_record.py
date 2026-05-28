from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "generate_release_review_record.py"
    spec = importlib.util.spec_from_file_location("generate_release_review_record", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_release_review_record = _module.build_release_review_record


def test_build_release_review_record_prefills_from_ready_bundle_check(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "release-artifacts"
    artifact_dir.mkdir(parents=True)
    (artifact_dir / "manifest.json").write_text("{}", encoding="utf-8")
    (artifact_dir / "README.txt").write_text("bundle\n", encoding="utf-8")

    bundle_check_path = artifact_dir / "release_bundle_check.json"
    bundle_check_path.write_text(
        json.dumps({"release_bundle_ready": True}),
        encoding="utf-8",
    )

    output_path = tmp_path / "release-review-record-2026-05-28.md"
    content = build_release_review_record(
        review_date="2026-05-28",
        candidate_commit="abc123",
        candidate_branch="main",
        reviewer="Core maintainers",
        target_platform="ubuntu-latest",
        change_scope="release packaging updates",
        release_artifact_dir=artifact_dir,
        release_bundle_check_json=bundle_check_path,
        week3_output_root=tmp_path / "week3-quality-pipeline",
        output_path=output_path,
    )

    assert output_path.exists()
    assert "Candidate commit: abc123" in content
    assert "Release bundle ready: yes" in content
    assert "Decision: conditional" in content
    assert "Blocking issues: none from release bundle checker" in content


def test_build_release_review_record_reports_no_go_for_not_ready_bundle(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "release-artifacts"
    artifact_dir.mkdir(parents=True)

    bundle_check_path = artifact_dir / "release_bundle_check.json"
    bundle_check_path.write_text(
        json.dumps(
            {
                "release_bundle_ready": False,
                "missing_root_files": ["manifest.json"],
                "manifest_kind_ok": False,
                "missing_manifest_commands": ["build_python_wheel"],
                "cli_binaries": [],
                "wheel_files": [],
            }
        ),
        encoding="utf-8",
    )

    output_path = tmp_path / "release-review-record-2026-05-28.md"
    content = build_release_review_record(
        review_date="2026-05-28",
        candidate_commit="",
        candidate_branch="",
        reviewer="Core maintainers",
        target_platform="",
        change_scope="",
        release_artifact_dir=artifact_dir,
        release_bundle_check_json=bundle_check_path,
        week3_output_root=tmp_path / "week3-quality-pipeline",
        output_path=output_path,
    )

    assert "Decision: no-go" in content
    assert "missing root files: manifest.json" in content
    assert "manifest kind is not release-candidate-artifacts" in content
    assert "missing manifest commands: build_python_wheel" in content
    assert "no CLI binary found in artifact bundle" in content
    assert "no wheel artifact found in artifact bundle" in content
