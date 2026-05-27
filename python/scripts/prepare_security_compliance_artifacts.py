from __future__ import annotations

import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Callable


README_TEXT = """rust-dependency-inventory.json
  Cargo metadata inventory for the current Rust workspace.

rust-licenses.json
  Machine-readable Rust license inventory produced by cargo-license.

cargo-audit.txt
  Plain-text cargo-audit report for the current lockfile.

python-dependency-inventory.json
  Machine-readable Python dependency inventory derived from uv.lock.

python-license-inventory.json
  Machine-readable Python license inventory derived from wheel metadata.

pyproject.toml / uv.lock
  Primary Python dependency review inputs copied into this artifact bundle.

manifest.json
  Machine-readable artifact metadata including file roles, hashes, command inventory,
  Python version, and GitHub Actions run context.
"""


CommandRunner = Callable[[list[str], Path], subprocess.CompletedProcess[str]]


def run_checked(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(command, cwd=str(cwd), capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr + "\n" + completed.stdout)
    return completed


def _write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _build_manifest(
    output_dir: Path,
    commands: list[dict[str, object]],
    github_context: dict[str, str],
) -> dict[str, object]:
    artifact_files = [
        "rust-dependency-inventory.json",
        "rust-licenses.json",
        "cargo-audit.txt",
        "python-dependency-inventory.json",
        "python-license-inventory.json",
        "pyproject.toml",
        "uv.lock",
    ]
    file_inventory = []
    for file_name in artifact_files:
        path = output_dir / file_name
        file_inventory.append(
            {
                "name": file_name,
                "relative_path": path.relative_to(output_dir).as_posix(),
                "size_bytes": path.stat().st_size,
                "sha256": _sha256(path),
            }
        )

    return {
        "kind": "security-compliance-artifacts",
        "schema_version": 1,
        "github_actions_context": github_context,
        "python_version": platform.python_version(),
        "artifact_files": file_inventory,
        "command_inventory": commands,
        "review_contract": {
            "rust_dependency_inventory": "Cargo metadata inventory for the current Rust workspace.",
            "rust_licenses": "Machine-readable Rust license inventory from cargo-license.",
            "cargo_audit": "Plain-text cargo-audit report for the current lockfile.",
            "python_dependency_inventory": "Machine-readable Python dependency inventory derived from uv.lock.",
            "python_license_inventory": "Machine-readable Python license inventory derived from wheel metadata.",
            "dependency_review_inputs": "Copied pyproject.toml and uv.lock used as Python dependency review inputs.",
        },
    }


def prepare_security_compliance_artifacts(
    *,
    repo_root: Path,
    output_dir: Path,
    github_context: dict[str, str],
    command_runner: CommandRunner = run_checked,
) -> dict[str, object]:
    output_dir.mkdir(parents=True, exist_ok=True)
    python_dependency_script = repo_root / "python" / "scripts" / "export_python_dependency_inventory.py"
    python_license_script = repo_root / "python" / "scripts" / "export_python_license_inventory.py"

    command_specs = [
        {
            "name": "rust_dependency_inventory",
            "command": ["cargo", "metadata", "--format-version", "1"],
            "output": output_dir / "rust-dependency-inventory.json",
        },
        {
            "name": "rust_licenses",
            "command": ["cargo", "license", "--json"],
            "output": output_dir / "rust-licenses.json",
        },
        {
            "name": "python_dependency_inventory",
            "command": [sys.executable, str(python_dependency_script)],
            "output": output_dir / "python-dependency-inventory.json",
        },
        {
            "name": "python_license_inventory",
            "command": [sys.executable, str(python_license_script)],
            "output": output_dir / "python-license-inventory.json",
        },
        {
            "name": "cargo_audit",
            "command": ["cargo", "audit"],
            "output": output_dir / "cargo-audit.txt",
        },
    ]

    command_inventory: list[dict[str, object]] = []
    for spec in command_specs:
        completed = command_runner(spec["command"], repo_root)
        _write_text(spec["output"], completed.stdout)
        command_inventory.append(
            {
                "name": spec["name"],
                "command": [str(part) for part in spec["command"]],
                "output": spec["output"].name,
            }
        )

    shutil.copy2(repo_root / "pyproject.toml", output_dir / "pyproject.toml")
    shutil.copy2(repo_root / "uv.lock", output_dir / "uv.lock")
    _write_text(output_dir / "README.txt", README_TEXT)

    manifest = _build_manifest(output_dir, command_inventory, github_context)
    _write_text(output_dir / "manifest.json", json.dumps(manifest, indent=2))
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Prepare security and compliance review artifacts for CI.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("target/compliance"),
        help="Directory where compliance artifacts are written.",
    )
    parser.add_argument("--github-workflow", type=str, default="")
    parser.add_argument("--github-job", type=str, default="security-compliance-optional")
    parser.add_argument("--github-event-name", type=str, default="")
    parser.add_argument("--github-run-id", type=str, default="")
    parser.add_argument("--github-run-attempt", type=str, default="")
    parser.add_argument("--github-sha", type=str, default="")
    parser.add_argument("--github-ref-name", type=str, default="")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    prepare_security_compliance_artifacts(
        repo_root=repo_root,
        output_dir=args.output_dir if args.output_dir.is_absolute() else (repo_root / args.output_dir),
        github_context={
            "workflow": args.github_workflow,
            "job": args.github_job,
            "event_name": args.github_event_name,
            "run_id": args.github_run_id,
            "run_attempt": args.github_run_attempt,
            "sha": args.github_sha,
            "ref_name": args.github_ref_name,
        },
    )


if __name__ == "__main__":
    main()