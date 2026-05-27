from __future__ import annotations

import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Callable


README_TEXT = """bin/
  Candidate CLI binary built from `cargo build -p rflux-cli --release`.

wheels/
  Candidate Python wheel(s) built from `maturin build` for the current runner environment.

README.md / Cargo.toml / pyproject.toml / uv.lock
  Build-input snapshots copied into this artifact bundle for release review and rollback context.

manifest.json
  Machine-readable artifact metadata including file hashes, build commands, version, platform, and GitHub Actions run context.
"""


CommandRunner = Callable[[list[str], Path], subprocess.CompletedProcess[str]]


def run_checked(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(command, cwd=str(cwd), capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr + "\n" + completed.stdout)
    return completed


def os_name_is_windows() -> bool:
    return platform.system().lower().startswith("windows")


def _write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _load_project_version(pyproject_path: Path) -> str:
    payload = tomllib.loads(pyproject_path.read_text(encoding="utf-8"))
    return str(payload["project"]["version"])


def _built_cli_binary_path(repo_root: Path) -> Path:
    suffix = ".exe" if os_name_is_windows() else ""
    return repo_root / "target" / "release" / f"rflux{suffix}"


def _build_manifest(
    *,
    output_dir: Path,
    version: str,
    command_inventory: list[dict[str, object]],
    github_context: dict[str, str],
) -> dict[str, object]:
    file_inventory: list[dict[str, object]] = []
    for path in sorted(output_dir.rglob("*")):
        if not path.is_file() or path.name == "manifest.json":
            continue
        file_inventory.append(
            {
                "relative_path": path.relative_to(output_dir).as_posix(),
                "size_bytes": path.stat().st_size,
                "sha256": _sha256(path),
            }
        )

    return {
        "kind": "release-candidate-artifacts",
        "schema_version": 1,
        "version": version,
        "platform": platform.platform(),
        "python_version": platform.python_version(),
        "github_actions_context": github_context,
        "command_inventory": command_inventory,
        "file_inventory": file_inventory,
        "review_contract": {
            "cli_binary": "Candidate CLI binary built from the rflux-cli release profile.",
            "python_wheels": "Candidate Python wheel artifacts built by maturin for the current runner environment.",
            "build_inputs": "README.md, Cargo.toml, pyproject.toml, and uv.lock copied into the bundle for release review and rollback context.",
        },
    }


def prepare_release_artifacts(
    *,
    repo_root: Path,
    output_dir: Path,
    github_context: dict[str, str],
    command_runner: CommandRunner = run_checked,
) -> dict[str, object]:
    version = _load_project_version(repo_root / "pyproject.toml")
    output_dir.mkdir(parents=True, exist_ok=True)
    wheels_dir = output_dir / "wheels"
    bin_dir = output_dir / "bin"
    wheels_dir.mkdir(parents=True, exist_ok=True)
    bin_dir.mkdir(parents=True, exist_ok=True)

    command_specs = [
        {
            "name": "build_cli_release",
            "command": ["cargo", "build", "-p", "rflux-cli", "--release"],
        },
        {
            "name": "build_python_wheel",
            "command": [
                sys.executable,
                "-m",
                "maturin",
                "build",
                "-m",
                "crates/py/Cargo.toml",
                "--interpreter",
                sys.executable,
                "--out",
                str(wheels_dir),
            ],
        },
    ]

    command_inventory: list[dict[str, object]] = []
    for spec in command_specs:
        command_runner(spec["command"], repo_root)
        command_inventory.append(
            {
                "name": spec["name"],
                "command": [str(part) for part in spec["command"]],
            }
        )

    built_binary = _built_cli_binary_path(repo_root)
    if not built_binary.is_file():
        raise RuntimeError(f"expected CLI binary was not produced: {built_binary}")
    shutil.copy2(built_binary, bin_dir / built_binary.name)

    wheel_files = sorted(wheels_dir.glob("*.whl"))
    if not wheel_files:
        raise RuntimeError(f"no wheel artifacts were produced under {wheels_dir}")

    for file_name in ["README.md", "Cargo.toml", "pyproject.toml", "uv.lock"]:
        shutil.copy2(repo_root / file_name, output_dir / file_name)

    _write_text(output_dir / "README.txt", README_TEXT)
    manifest = _build_manifest(
        output_dir=output_dir,
        version=version,
        command_inventory=command_inventory,
        github_context=github_context,
    )
    _write_text(output_dir / "manifest.json", json.dumps(manifest, indent=2))
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build and stage candidate CLI/Python release artifacts for review.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("target/release-artifacts"),
        help="Directory where candidate release artifacts are written.",
    )
    parser.add_argument("--github-workflow", type=str, default="")
    parser.add_argument("--github-job", type=str, default="release-artifacts-optional")
    parser.add_argument("--github-event-name", type=str, default="")
    parser.add_argument("--github-run-id", type=str, default="")
    parser.add_argument("--github-run-attempt", type=str, default="")
    parser.add_argument("--github-sha", type=str, default="")
    parser.add_argument("--github-ref-name", type=str, default="")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    prepare_release_artifacts(
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
