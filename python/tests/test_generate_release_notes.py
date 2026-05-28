from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "generate_release_notes.py"
    spec = importlib.util.spec_from_file_location("generate_release_notes", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_release_notes = _module.build_release_notes


def test_build_release_notes_prefills_from_ready_bundle_and_review_record(tmp_path: Path) -> None:
    release_artifact_dir = tmp_path / "release-artifacts"
    release_artifact_dir.mkdir(parents=True)
    release_review_record = tmp_path / "docs" / "release-review-record-2026-05-28.md"
    release_review_record.parent.mkdir(parents=True)
    release_review_record.write_text("# review\n", encoding="utf-8")

    bundle_check_path = release_artifact_dir / "release_bundle_check.json"
    bundle_check_path.write_text(
        json.dumps({"release_bundle_ready": True}),
        encoding="utf-8",
    )

    week3_output_root = tmp_path / "week3-quality-pipeline"
    (week3_output_root / "review").mkdir(parents=True)
    (week3_output_root / "review" / "manifest.json").write_text("{}", encoding="utf-8")
    (week3_output_root / "review" / "quality_summary.validation.json").write_text("{}", encoding="utf-8")
    (week3_output_root / "review" / "quality_summary.current.md").write_text("# summary\n", encoding="utf-8")

    output_path = tmp_path / "release-notes-draft.md"
    content = build_release_notes(
        release_version="0.1.0",
        release_date="2026-05-28",
        commit_tag="abc123",
        author="Core maintainers",
        release_level="Dev Snapshot",
        scope_summary="release automation updates",
        release_review_record=release_review_record,
        release_bundle_check_json=bundle_check_path,
        week3_output_root=week3_output_root,
        output_path=output_path,
    )

    assert output_path.exists()
    assert "Release version: 0.1.0" in content
    assert "Release bundle ready: yes; CLI binaries: 0; wheels: 0." in content
    assert "CLI contract diff summary: none" in content
    assert "Week3 one-command check run: yes" in content
    assert "Checklist record: " in content


def test_build_release_notes_reports_missing_bundle_and_week3_pending(tmp_path: Path) -> None:
    release_artifact_dir = tmp_path / "release-artifacts"
    release_artifact_dir.mkdir(parents=True)
    bundle_check_path = release_artifact_dir / "release_bundle_check.json"
    bundle_check_path.write_text(
        json.dumps(
            {
                "release_bundle_ready": False,
                "missing_root_files": ["README.txt"],
                "manifest_kind_ok": False,
                "missing_manifest_commands": ["build_python_wheel"],
                "cli_binaries": [],
                "wheel_files": [],
            }
        ),
        encoding="utf-8",
    )

    output_path = tmp_path / "release-notes-draft.md"
    content = build_release_notes(
        release_version="",
        release_date="2026-05-28",
        commit_tag="",
        author="",
        release_level="Beta",
        scope_summary="",
        release_review_record=tmp_path / "missing-review.md",
        release_bundle_check_json=bundle_check_path,
        week3_output_root=tmp_path / "week3-quality-pipeline",
        output_path=output_path,
    )

    assert "Release bundle ready: no" in content
    assert "bundle not ready; missing root files: README.txt" in content
    assert "bundle not ready; missing manifest commands: build_python_wheel" in content
    assert "Week3 one-command check run: pending" in content
