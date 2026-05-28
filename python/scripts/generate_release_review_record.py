from __future__ import annotations

import argparse
import json
from datetime import date
from pathlib import Path


def _load_json_if_exists(path: Path) -> dict[str, object] | None:
    if not path.is_file():
        return None
    payload = json.loads(path.read_text(encoding="utf-8"))
    return payload if isinstance(payload, dict) else None


def _path_or_pending(path: Path) -> str:
    return path.as_posix() if path.exists() else "pending"


def _release_outcome(bundle_check: dict[str, object] | None) -> tuple[str, str]:
    if not isinstance(bundle_check, dict):
        return ("conditional", "release bundle checker output missing")

    ready = bool(bundle_check.get("release_bundle_ready", False))
    if ready:
        return ("conditional", "none from release bundle checker")

    blockers: list[str] = []

    missing_root = bundle_check.get("missing_root_files")
    if isinstance(missing_root, list) and missing_root:
        blockers.append("missing root files: " + ", ".join(str(item) for item in missing_root))

    if not bool(bundle_check.get("manifest_kind_ok", False)):
        blockers.append("manifest kind is not release-candidate-artifacts")

    missing_commands = bundle_check.get("missing_manifest_commands")
    if isinstance(missing_commands, list) and missing_commands:
        blockers.append("missing manifest commands: " + ", ".join(str(item) for item in missing_commands))

    cli_binaries = bundle_check.get("cli_binaries")
    if not isinstance(cli_binaries, list) or not cli_binaries:
        blockers.append("no CLI binary found in artifact bundle")

    wheel_files = bundle_check.get("wheel_files")
    if not isinstance(wheel_files, list) or not wheel_files:
        blockers.append("no wheel artifact found in artifact bundle")

    if not blockers:
        blockers.append("release bundle checker reports not ready")
    return ("no-go", "; ".join(blockers))


def build_release_review_record(
    *,
    review_date: str,
    candidate_commit: str,
    candidate_branch: str,
    reviewer: str,
    target_platform: str,
    change_scope: str,
    release_artifact_dir: Path,
    release_bundle_check_json: Path,
    week3_output_root: Path,
    output_path: Path,
) -> str:
    manifest_path = release_artifact_dir / "manifest.json"
    readme_path = release_artifact_dir / "README.txt"

    bundle_check = _load_json_if_exists(release_bundle_check_json)
    decision, blocking_issues = _release_outcome(bundle_check)

    release_bundle_ready = "pending"
    if isinstance(bundle_check, dict):
        release_bundle_ready = "yes" if bool(bundle_check.get("release_bundle_ready", False)) else "no"

    content = f"""# Release Review Record - {review_date}

Use this record to capture go/no-go decisions for candidate releases.

## 1. Candidate identity

```md
Candidate commit: {candidate_commit or 'pending'}
Candidate branch: {candidate_branch or 'pending'}
Review date: {review_date}
Reviewer: {reviewer or 'pending'}
Target platform: {target_platform or 'pending'}
Change scope: {change_scope or 'pending'}
```

## 2. Validation commands actually run

```md
Rust validation command: pending
Python validation command: pending
Release artifact command: uv run python python/scripts/prepare_release_artifacts.py --output-dir target/release-artifacts
Release bundle checker command: uv run python python/scripts/check_release_artifact_bundle.py --artifact-dir target/release-artifacts --json-output target/release-artifacts/release_bundle_check.json --require-ready
CLI contract command: pending
Python API contract command: pending
Report schema contract command: pending
Week3 one-command baseline command: pending
```

## 3. Evidence artifacts

```md
Release artifact directory: {_path_or_pending(release_artifact_dir)}
Release artifact manifest: {_path_or_pending(manifest_path)}
Release artifact README: {_path_or_pending(readme_path)}
Release bundle checker JSON: {_path_or_pending(release_bundle_check_json)}
Release bundle ready: {release_bundle_ready}
CLI contract baseline + diff summary: pending
Python API baseline + diff summary: pending
Report schema baseline + diff summary: pending
Week3 pipeline output root: {_path_or_pending(week3_output_root)}
Week3 review manifest: {_path_or_pending(week3_output_root / 'review' / 'manifest.json')}
Week3 validation report: {_path_or_pending(week3_output_root / 'review' / 'quality_summary.validation.json')}
Week3 summary markdown: {_path_or_pending(week3_output_root / 'review' / 'quality_summary.current.md')}
```

## 4. Compatibility decision

```md
CLI compatibility risk: pending
Python API compatibility risk: pending
Report schema compatibility risk: pending
Default behavior risk: release bundle checker ready={release_bundle_ready}
```

## 5. Sign-off

```md
Packaging DRI: pending
QA reviewer: pending
Documentation reviewer: pending
Release policy update required: pending
Support matrix update required: pending
Known limitations update required: pending
```

## 6. Final outcome

```md
Decision: {decision}
Blocking issues: {blocking_issues}
Follow-up owner: pending
Follow-up due date: pending
```

## 7. Reference checklist docs

- [release-artifact-readiness-checklist.md](./release-artifact-readiness-checklist.md)
- [sim-release-readiness-checklist.md](./sim-release-readiness-checklist.md)
- [release-policy.md](./release-policy.md)
"""

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(content, encoding="utf-8")
    return content


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate a prefilled release review record from bundle-check evidence.")
    parser.add_argument("--date", type=str, default=date.today().isoformat(), help="Review record date (YYYY-MM-DD).")
    parser.add_argument("--candidate-commit", type=str, default="", help="Candidate commit SHA or label.")
    parser.add_argument("--candidate-branch", type=str, default="", help="Candidate branch name.")
    parser.add_argument("--reviewer", type=str, default="Core maintainers", help="Reviewer label.")
    parser.add_argument("--target-platform", type=str, default="", help="Target platform descriptor.")
    parser.add_argument("--change-scope", type=str, default="", help="Scope summary for this candidate.")
    parser.add_argument(
        "--release-artifact-dir",
        type=Path,
        default=Path("target/release-artifacts"),
        help="Directory containing release artifact bundle files.",
    )
    parser.add_argument(
        "--release-bundle-check-json",
        type=Path,
        default=Path("target/release-artifacts/release_bundle_check.json"),
        help="Path to release bundle checker JSON output.",
    )
    parser.add_argument(
        "--week3-output-root",
        type=Path,
        default=Path("target/week3-quality-pipeline"),
        help="Week3 pipeline output root for evidence references.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output markdown path. Defaults to docs/release-review-record-<date>.md.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    release_artifact_dir = (
        args.release_artifact_dir
        if args.release_artifact_dir.is_absolute()
        else (repo_root / args.release_artifact_dir)
    )
    release_bundle_check_json = (
        args.release_bundle_check_json
        if args.release_bundle_check_json.is_absolute()
        else (repo_root / args.release_bundle_check_json)
    )
    week3_output_root = args.week3_output_root if args.week3_output_root.is_absolute() else (repo_root / args.week3_output_root)

    output_path = args.output
    if output_path is None:
        output_path = repo_root / "docs" / f"release-review-record-{args.date}.md"
    elif not output_path.is_absolute():
        output_path = repo_root / output_path

    build_release_review_record(
        review_date=args.date,
        candidate_commit=args.candidate_commit,
        candidate_branch=args.candidate_branch,
        reviewer=args.reviewer,
        target_platform=args.target_platform,
        change_scope=args.change_scope,
        release_artifact_dir=release_artifact_dir,
        release_bundle_check_json=release_bundle_check_json,
        week3_output_root=week3_output_root,
        output_path=output_path,
    )


if __name__ == "__main__":
    main()
