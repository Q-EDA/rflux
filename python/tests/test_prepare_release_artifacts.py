from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "prepare_release_artifacts.py"
    spec = importlib.util.spec_from_file_location("prepare_release_artifacts", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
prepare_release_artifacts = _module.prepare_release_artifacts
os_name_is_windows = _module.os_name_is_windows


class _Completed:
    def __init__(self) -> None:
        self.stdout = ""
        self.returncode = 0


def test_prepare_release_artifacts_writes_candidate_bundle(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    (repo_root / "target" / "release").mkdir(parents=True)
    (repo_root / "crates" / "py").mkdir(parents=True)
    (repo_root / "README.md").write_text("# rflux\n", encoding="utf-8")
    (repo_root / "Cargo.toml").write_text("[workspace]\nmembers=[]\n", encoding="utf-8")
    (repo_root / "pyproject.toml").write_text(
        "[project]\nname='rflux'\nversion='0.1.0'\n",
        encoding="utf-8",
    )
    (repo_root / "uv.lock").write_text("version = 1\n", encoding="utf-8")

    binary_name = "rflux.exe" if os_name_is_windows() else "rflux"

    def fake_runner(command: list[str], cwd: Path):
        assert cwd == repo_root
        if command[:5] == ["cargo", "build", "-p", "rflux-cli", "--release"]:
            (repo_root / "target" / "release" / binary_name).write_text("binary", encoding="utf-8")
            return _Completed()
        if len(command) >= 3 and command[0] == sys.executable and command[1] == "-m" and command[2] == "maturin":
            out_dir = Path(command[-1])
            out_dir.mkdir(parents=True, exist_ok=True)
            (out_dir / "rflux-0.1.0-py3-none-any.whl").write_text("wheel", encoding="utf-8")
            return _Completed()
        raise AssertionError(f"unexpected command: {command}")

    output_dir = repo_root / "target" / "release-artifacts"
    manifest = prepare_release_artifacts(
        repo_root=repo_root,
        output_dir=output_dir,
        github_context={
            "workflow": "ci",
            "job": "release-artifacts-optional",
            "event_name": "workflow_dispatch",
            "run_id": "1",
            "run_attempt": "1",
            "sha": "deadbeef",
            "ref_name": "main",
        },
        command_runner=fake_runner,
    )

    assert (output_dir / "bin" / binary_name).exists()
    assert (output_dir / "wheels" / "rflux-0.1.0-py3-none-any.whl").exists()
    assert (output_dir / "README.md").exists()
    assert (output_dir / "Cargo.toml").exists()
    assert (output_dir / "pyproject.toml").exists()
    assert (output_dir / "uv.lock").exists()
    assert (output_dir / "README.txt").exists()
    assert (output_dir / "manifest.json").exists()

    manifest_payload = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["kind"] == "release-candidate-artifacts"
    assert manifest_payload["version"] == "0.1.0"
    assert manifest_payload["github_actions_context"]["job"] == "release-artifacts-optional"
    command_names = [entry["name"] for entry in manifest_payload["command_inventory"]]
    assert command_names == ["build_cli_release", "build_python_wheel"]
    file_paths = {entry["relative_path"] for entry in manifest_payload["file_inventory"]}
    assert f"bin/{binary_name}" in file_paths
    assert "wheels/rflux-0.1.0-py3-none-any.whl" in file_paths
