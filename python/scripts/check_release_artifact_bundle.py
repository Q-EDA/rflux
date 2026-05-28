from __future__ import annotations

import argparse
import json
from pathlib import Path


REQUIRED_ROOT_FILES = [
    "manifest.json",
    "README.txt",
    "README.md",
    "Cargo.toml",
    "pyproject.toml",
    "uv.lock",
]

REQUIRED_COMMAND_NAMES = [
    "build_cli_release",
    "build_python_wheel",
]


def _load_json_if_exists(path: Path) -> dict[str, object] | None:
    if not path.is_file():
        return None
    payload = json.loads(path.read_text(encoding="utf-8"))
    return payload if isinstance(payload, dict) else None


def check_release_artifact_bundle(*, artifact_dir: Path) -> dict[str, object]:
    missing_root_files = [
        (artifact_dir / name).as_posix()
        for name in REQUIRED_ROOT_FILES
        if not (artifact_dir / name).is_file()
    ]

    bin_dir = artifact_dir / "bin"
    wheels_dir = artifact_dir / "wheels"
    cli_binaries = [path.as_posix() for path in sorted(bin_dir.glob("*")) if path.is_file()] if bin_dir.is_dir() else []
    wheel_files = [path.as_posix() for path in sorted(wheels_dir.glob("*.whl")) if path.is_file()] if wheels_dir.is_dir() else []

    manifest_payload = _load_json_if_exists(artifact_dir / "manifest.json")
    manifest_kind = str(manifest_payload.get("kind", "")) if isinstance(manifest_payload, dict) else ""
    manifest_kind_ok = manifest_kind == "release-candidate-artifacts"

    command_names: list[str] = []
    if isinstance(manifest_payload, dict):
        raw_inventory = manifest_payload.get("command_inventory")
        if isinstance(raw_inventory, list):
            for entry in raw_inventory:
                if isinstance(entry, dict) and "name" in entry:
                    command_names.append(str(entry["name"]))

    missing_commands = [name for name in REQUIRED_COMMAND_NAMES if name not in command_names]

    release_bundle_ready = bool(
        artifact_dir.is_dir()
        and not missing_root_files
        and bool(cli_binaries)
        and bool(wheel_files)
        and manifest_kind_ok
        and not missing_commands
    )

    return {
        "artifact_dir": artifact_dir.as_posix(),
        "missing_root_files": missing_root_files,
        "cli_binaries": cli_binaries,
        "wheel_files": wheel_files,
        "manifest_kind": manifest_kind,
        "manifest_kind_ok": manifest_kind_ok,
        "manifest_command_names": command_names,
        "missing_manifest_commands": missing_commands,
        "release_bundle_ready": release_bundle_ready,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Check whether a release artifact bundle is complete and ready for review.",
    )
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        default=Path("target/release-artifacts"),
        help="Directory containing release artifact bundle files.",
    )
    parser.add_argument(
        "--json-output",
        type=Path,
        default=Path(""),
        help="Optional path to write a machine-readable result JSON.",
    )
    parser.add_argument(
        "--require-ready",
        action="store_true",
        help="Exit non-zero when release_bundle_ready is false.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    artifact_dir = args.artifact_dir if args.artifact_dir.is_absolute() else (repo_root / args.artifact_dir)
    report = check_release_artifact_bundle(artifact_dir=artifact_dir)

    if args.json_output.as_posix():
        output_path = args.json_output if args.json_output.is_absolute() else (repo_root / args.json_output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print(f"release_bundle_ready={report['release_bundle_ready']}")
    print(f"missing_root_files={len(report['missing_root_files'])}")
    print(f"cli_binary_count={len(report['cli_binaries'])}")
    print(f"wheel_count={len(report['wheel_files'])}")

    if args.require_ready and not bool(report["release_bundle_ready"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
