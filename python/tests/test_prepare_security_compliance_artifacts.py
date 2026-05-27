from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "prepare_security_compliance_artifacts.py"
    spec = importlib.util.spec_from_file_location("prepare_security_compliance_artifacts", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
prepare_security_compliance_artifacts = _module.prepare_security_compliance_artifacts


class _Completed:
    def __init__(self, stdout: str):
        self.stdout = stdout
        self.returncode = 0


def test_prepare_security_compliance_artifacts_writes_expected_bundle(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    (repo_root / "python" / "scripts").mkdir(parents=True)
    (repo_root / "pyproject.toml").write_text("[project]\nname='rflux'\n", encoding="utf-8")
    (repo_root / "uv.lock").write_text("version = 1\n", encoding="utf-8")

    outputs = {
        "cargo metadata --format-version 1": '{"packages": []}\n',
        "cargo license --json": '[{"name":"rflux","license":"MIT OR Apache-2.0"}]\n',
        "cargo audit": "0 vulnerabilities found\n",
    }

    def fake_runner(command: list[str], cwd: Path):
        key = " ".join(command)
        if command[0].endswith("python") or command[0].endswith("python.exe"):
            if command[-1].endswith("export_python_dependency_inventory.py"):
                return _Completed('{"tool":"uv","package_count":1}\n')
            if command[-1].endswith("export_python_license_inventory.py"):
                return _Completed('{"metadata_source":"wheel-metadata","package_count":1}\n')
        return _Completed(outputs[key])

    output_dir = repo_root / "target" / "compliance"
    manifest = prepare_security_compliance_artifacts(
        repo_root=repo_root,
        output_dir=output_dir,
        github_context={
            "workflow": "ci",
            "job": "security-compliance-optional",
            "event_name": "workflow_dispatch",
            "run_id": "1",
            "run_attempt": "1",
            "sha": "deadbeef",
            "ref_name": "main",
        },
        command_runner=fake_runner,
    )

    assert (output_dir / "rust-dependency-inventory.json").exists()
    assert (output_dir / "rust-licenses.json").exists()
    assert (output_dir / "cargo-audit.txt").exists()
    assert (output_dir / "python-dependency-inventory.json").exists()
    assert (output_dir / "python-license-inventory.json").exists()
    assert (output_dir / "pyproject.toml").exists()
    assert (output_dir / "uv.lock").exists()
    assert (output_dir / "README.txt").exists()
    assert (output_dir / "manifest.json").exists()

    manifest_payload = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["kind"] == "security-compliance-artifacts"
    assert manifest_payload["github_actions_context"]["job"] == "security-compliance-optional"
    assert {entry["name"] for entry in manifest_payload["artifact_files"]} == {
        "rust-dependency-inventory.json",
        "rust-licenses.json",
        "cargo-audit.txt",
        "python-dependency-inventory.json",
        "python-license-inventory.json",
        "pyproject.toml",
        "uv.lock",
    }
    assert [entry["name"] for entry in manifest_payload["command_inventory"]] == [
        "rust_dependency_inventory",
        "rust_licenses",
        "python_dependency_inventory",
        "python_license_inventory",
        "cargo_audit",
    ]