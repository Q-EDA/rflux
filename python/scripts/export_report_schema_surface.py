from __future__ import annotations

import argparse
import difflib
import json
import re
from pathlib import Path


DEFAULT_CLI_SOURCE = Path("crates/cli/src/main.rs")
DEFAULT_PYTHON_SCRIPTS_DIR = Path("python/scripts")
DEFAULT_CONTRACT = Path("python/tests/contracts/report_schema_surface.json")


def _extract_cli_schema_version(source_text: str) -> int | None:
    match = re.search(r"const\s+CLI_SCHEMA_VERSION\s*:\s*u64\s*=\s*(\d+)\s*;", source_text)
    return int(match.group(1)) if match else None


def _extract_cli_report_kinds(source_text: str) -> list[str]:
    kinds = set()

    for match in re.finditer(r'"kind"\s*:\s*"([a-z0-9_\-]+)"', source_text):
        kinds.add(match.group(1))

    for match in re.finditer(r'insert\("kind"\.to_string\(\),\s*json!\("([a-z0-9_\-]+)"\)\)', source_text):
        kinds.add(match.group(1))

    return sorted(kinds)


def _extract_python_manifest_surfaces(scripts_dir: Path) -> list[dict[str, object]]:
    surfaces: list[dict[str, object]] = []
    for script_path in sorted(scripts_dir.glob("*.py")):
        text = script_path.read_text(encoding="utf-8")
        kind_match = re.search(r'"kind"\s*:\s*"([a-z0-9_\-]+)"', text)
        schema_match = re.search(r'"schema_version"\s*:\s*(\d+)', text)
        if kind_match and schema_match:
            surfaces.append(
                {
                    "script": script_path.as_posix(),
                    "kind": kind_match.group(1),
                    "schema_version": int(schema_match.group(1)),
                }
            )
    return surfaces


def build_surface_payload(*, repo_root: Path, cli_source: Path, python_scripts_dir: Path) -> dict[str, object]:
    cli_source_path = cli_source if cli_source.is_absolute() else (repo_root / cli_source)
    python_scripts_path = (
        python_scripts_dir if python_scripts_dir.is_absolute() else (repo_root / python_scripts_dir)
    )

    cli_text = cli_source_path.read_text(encoding="utf-8")
    cli_schema_version = _extract_cli_schema_version(cli_text)
    cli_report_kinds = _extract_cli_report_kinds(cli_text)
    python_manifest_surfaces = _extract_python_manifest_surfaces(python_scripts_path)

    return {
        "schema_version": 1,
        "kind": "report_schema_surface",
        "cli": {
            "source": cli_source_path.relative_to(repo_root).as_posix(),
            "schema_version_constant": cli_schema_version,
            "report_kind_count": len(cli_report_kinds),
            "report_kinds": cli_report_kinds,
        },
        "python_artifact_manifests": {
            "scripts_dir": python_scripts_path.relative_to(repo_root).as_posix(),
            "manifest_count": len(python_manifest_surfaces),
            "manifests": python_manifest_surfaces,
        },
    }


def _canonical_json(payload: dict[str, object]) -> str:
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def assert_surfaces_match(expected: dict[str, object], actual: dict[str, object]) -> None:
    if expected == actual:
        return

    expected_text = _canonical_json(expected).splitlines(keepends=True)
    actual_text = _canonical_json(actual).splitlines(keepends=True)
    diff = "".join(
        difflib.unified_diff(
            expected_text,
            actual_text,
            fromfile="expected",
            tofile="actual",
        )
    )
    raise ValueError(f"report schema surface mismatch:\n{diff}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export and optionally validate a report schema surface contract.",
    )
    parser.add_argument(
        "--cli-source",
        type=Path,
        default=DEFAULT_CLI_SOURCE,
        help="Rust CLI source file to scan for report kinds.",
    )
    parser.add_argument(
        "--python-scripts-dir",
        type=Path,
        default=DEFAULT_PYTHON_SCRIPTS_DIR,
        help="Python scripts directory to scan for artifact manifest kinds.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_CONTRACT,
        help="Where to write the report schema surface JSON.",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=DEFAULT_CONTRACT,
        help="Baseline JSON used by --check.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Compare extracted surface to baseline and fail on mismatch.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    payload = build_surface_payload(
        repo_root=repo_root,
        cli_source=args.cli_source,
        python_scripts_dir=args.python_scripts_dir,
    )

    output_path = args.output if args.output.is_absolute() else (repo_root / args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(_canonical_json(payload), encoding="utf-8")

    if args.check:
        baseline_path = args.baseline if args.baseline.is_absolute() else (repo_root / args.baseline)
        if not baseline_path.exists():
            raise SystemExit(f"baseline file does not exist: {baseline_path}")
        baseline_payload = json.loads(baseline_path.read_text(encoding="utf-8"))
        try:
            assert_surfaces_match(baseline_payload, payload)
        except ValueError as exc:
            raise SystemExit(str(exc)) from exc


if __name__ == "__main__":
    main()
