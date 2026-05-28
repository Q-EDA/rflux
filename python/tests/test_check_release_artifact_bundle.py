from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "check_release_artifact_bundle.py"
    spec = importlib.util.spec_from_file_location("check_release_artifact_bundle", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
check_release_artifact_bundle = _module.check_release_artifact_bundle


def _write_release_bundle_root_files(artifact_dir: Path) -> None:
    (artifact_dir / "manifest.json").write_text(
        json.dumps(
            {
                "kind": "release-candidate-artifacts",
                "command_inventory": [
                    {"name": "build_cli_release"},
                    {"name": "build_python_wheel"},
                ],
            }
        ),
        encoding="utf-8",
    )
    (artifact_dir / "README.txt").write_text("bundle\n", encoding="utf-8")
    (artifact_dir / "README.md").write_text("# rflux\n", encoding="utf-8")
    (artifact_dir / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
    (artifact_dir / "pyproject.toml").write_text("[project]\nname='rflux'\nversion='0.1.0'\n", encoding="utf-8")
    (artifact_dir / "uv.lock").write_text("version = 1\n", encoding="utf-8")


def test_check_release_artifact_bundle_reports_ready(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "release-artifacts"
    (artifact_dir / "bin").mkdir(parents=True)
    (artifact_dir / "wheels").mkdir(parents=True)
    _write_release_bundle_root_files(artifact_dir)

    (artifact_dir / "bin" / "rflux.exe").write_text("binary", encoding="utf-8")
    (artifact_dir / "wheels" / "rflux-0.1.0-py3-none-any.whl").write_text("wheel", encoding="utf-8")

    result = check_release_artifact_bundle(artifact_dir=artifact_dir)

    assert result["release_bundle_ready"] is True
    assert result["missing_root_files"] == []
    assert result["manifest_kind_ok"] is True
    assert result["missing_manifest_commands"] == []
    assert len(result["cli_binaries"]) == 1
    assert len(result["wheel_files"]) == 1


def test_check_release_artifact_bundle_detects_missing_wheel_and_manifest_mismatch(tmp_path: Path) -> None:
    artifact_dir = tmp_path / "release-artifacts"
    (artifact_dir / "bin").mkdir(parents=True)
    (artifact_dir / "wheels").mkdir(parents=True)
    _write_release_bundle_root_files(artifact_dir)

    (artifact_dir / "bin" / "rflux").write_text("binary", encoding="utf-8")
    (artifact_dir / "manifest.json").write_text(
        json.dumps(
            {
                "kind": "unexpected-kind",
                "command_inventory": [{"name": "build_cli_release"}],
            }
        ),
        encoding="utf-8",
    )

    result = check_release_artifact_bundle(artifact_dir=artifact_dir)

    assert result["release_bundle_ready"] is False
    assert result["manifest_kind_ok"] is False
    assert result["missing_manifest_commands"] == ["build_python_wheel"]
    assert result["wheel_files"] == []
