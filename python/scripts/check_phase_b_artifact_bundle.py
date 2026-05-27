from __future__ import annotations

import argparse
import json
from pathlib import Path


REQUIRED_ARTIFACT_FILES = [
    "waveform_compare_summary.current.json",
    "waveform_compare_summary.candidate-baseline.json",
    "waveform_compare_summary.candidate-baseline.md",
    "waveform_compare_summary.validation.json",
    "manifest.json",
]


def _load_json_if_exists(path: Path) -> dict[str, object] | None:
    if not path.is_file():
        return None
    payload = json.loads(path.read_text(encoding="utf-8"))
    return payload if isinstance(payload, dict) else None


def _linux_ready(status_payload: dict[str, object] | None) -> bool:
    if not isinstance(status_payload, dict):
        return False
    ready = status_payload.get("baseline_ready")
    if ready is None:
        ready = status_payload.get("ready")
    return bool(ready)


def check_phase_b_artifact_bundle(*, artifact_dir: Path, linux_status_json: Path | None) -> dict[str, object]:
    required_paths = [artifact_dir / name for name in REQUIRED_ARTIFACT_FILES]
    missing_files = [path.as_posix() for path in required_paths if not path.is_file()]

    candidate_payload = _load_json_if_exists(artifact_dir / "waveform_compare_summary.candidate-baseline.json")
    manifest_payload = _load_json_if_exists(artifact_dir / "manifest.json")

    candidate_failures = None
    if isinstance(candidate_payload, dict):
        candidate_failures = int(candidate_payload.get("failures", 0))

    linux_status_payload = None
    linux_status_path = None
    if linux_status_json is not None:
        linux_status_path = linux_status_json.as_posix()
        linux_status_payload = _load_json_if_exists(linux_status_json)

    linux_status_ready = _linux_ready(linux_status_payload) if linux_status_json is not None else None

    report: dict[str, object] = {
        "artifact_dir": artifact_dir.as_posix(),
        "required_files": [path.as_posix() for path in required_paths],
        "missing_files": missing_files,
        "candidate_failures": candidate_failures,
        "candidate_promotable": bool(candidate_failures == 0),
        "manifest_validate_no_regression": None
        if not isinstance(manifest_payload, dict)
        else bool(manifest_payload.get("validate_no_regression", False)),
        "manifest_baseline_platform": None
        if not isinstance(manifest_payload, dict)
        else str(manifest_payload.get("baseline_platform", "")),
        "linux_status_json": linux_status_path,
        "linux_status_ready": linux_status_ready,
        "artifact_bundle_ready": len(missing_files) == 0,
    }

    report["phase_b_promotion_ready"] = bool(
        report["artifact_bundle_ready"]
        and report["candidate_promotable"]
        and (linux_status_ready is not False)
    )
    return report


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Check whether a Phase B Linux waveform artifact bundle is complete and promotable.",
    )
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        default=Path("target/waveform-compare-linux"),
        help="Directory containing waveform compare artifact files.",
    )
    parser.add_argument(
        "--linux-status-json",
        type=Path,
        default=Path(""),
        help="Optional linux baseline status JSON path from check_waveform_baseline_status.py.",
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
        help="Exit non-zero when phase_b_promotion_ready is false.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    artifact_dir = args.artifact_dir if args.artifact_dir.is_absolute() else (repo_root / args.artifact_dir)

    linux_status_json = None
    if args.linux_status_json.as_posix():
        linux_status_json = (
            args.linux_status_json if args.linux_status_json.is_absolute() else (repo_root / args.linux_status_json)
        )

    report = check_phase_b_artifact_bundle(
        artifact_dir=artifact_dir,
        linux_status_json=linux_status_json,
    )

    if args.json_output.as_posix():
        output_path = args.json_output if args.json_output.is_absolute() else (repo_root / args.json_output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print(f"artifact_bundle_ready={report['artifact_bundle_ready']}")
    print(f"candidate_promotable={report['candidate_promotable']}")
    print(f"phase_b_promotion_ready={report['phase_b_promotion_ready']}")

    if args.require_ready and not bool(report["phase_b_promotion_ready"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
