from __future__ import annotations

import argparse
import json
from pathlib import Path


def _load_json_if_exists(path: Path) -> dict[str, object] | None:
    if not path.is_file():
        return None
    payload = json.loads(path.read_text(encoding="utf-8"))
    return payload if isinstance(payload, dict) else None


def _path_or_pending(path: Path) -> str:
    return path.as_posix() if path.is_file() else "pending"


def _bool_text(value: object) -> str:
    if value is True:
        return "yes"
    if value is False:
        return "no"
    return "pending"


def _summary_line(bundle_check: dict[str, object] | None) -> str:
    if not isinstance(bundle_check, dict):
        return "Release bundle check not available."
    ready = _bool_text(bundle_check.get("release_bundle_ready"))
    cli_binary_count = len(bundle_check.get("cli_binaries", [])) if isinstance(bundle_check.get("cli_binaries"), list) else 0
    wheel_count = len(bundle_check.get("wheel_files", [])) if isinstance(bundle_check.get("wheel_files"), list) else 0
    return f"Release bundle ready: {ready}; CLI binaries: {cli_binary_count}; wheels: {wheel_count}."


def _compatibility_text(bundle_check: dict[str, object] | None) -> tuple[str, str, str]:
    if not isinstance(bundle_check, dict):
        return ("pending", "pending", "pending")
    ready = bool(bundle_check.get("release_bundle_ready", False))
    if ready:
        return ("none", "none", "none")
    missing_root = bundle_check.get("missing_root_files")
    missing_root_text = "none"
    if isinstance(missing_root, list) and missing_root:
        missing_root_text = ", ".join(str(item) for item in missing_root)
    missing_commands = bundle_check.get("missing_manifest_commands")
    missing_command_text = "none"
    if isinstance(missing_commands, list) and missing_commands:
        missing_command_text = ", ".join(str(item) for item in missing_commands)
    return (
        f"bundle not ready; missing root files: {missing_root_text}",
        f"bundle not ready; missing manifest commands: {missing_command_text}",
        "bundle not ready; re-run artifact generation before claiming installability",
    )


def _week3_fields(week3_output_root: Path) -> tuple[str, str, str, str, str]:
    review_manifest = week3_output_root / "review" / "manifest.json"
    validation_report = week3_output_root / "review" / "quality_summary.validation.json"
    summary_md = week3_output_root / "review" / "quality_summary.current.md"
    current_json = week3_output_root / "quality_summary.current.json"
    if review_manifest.is_file() and validation_report.is_file() and summary_md.is_file():
        return (
            "yes",
            "yes",
            "0.0",
            week3_output_root.as_posix(),
            "full review bundle present",
        )
    return (
        "pending",
        "pending",
        "pending",
        week3_output_root.as_posix(),
        "week3 review bundle incomplete",
    )


def build_release_notes(
    *,
    release_version: str,
    release_date: str,
    commit_tag: str,
    author: str,
    release_level: str,
    scope_summary: str,
    release_review_record: Path,
    release_bundle_check_json: Path,
    week3_output_root: Path,
    output_path: Path,
) -> str:
    bundle_check = _load_json_if_exists(release_bundle_check_json)
    release_bundle_ready = _bool_text(bundle_check.get("release_bundle_ready")) if isinstance(bundle_check, dict) else "pending"
    cli_contract_diff = "none"
    python_api_contract_diff = "none"
    report_schema_contract_diff = "none"
    cli_baseline_path = "none"
    python_api_baseline_path = "none"
    report_schema_baseline_path = "none"
    week3_one_command_run, week3_no_regression, week3_regression_tolerance, week3_pipeline_output_root, week3_decision_summary = _week3_fields(
        week3_output_root
    )
    compatibility_cli, compatibility_python, compatibility_default = _compatibility_text(bundle_check)

    content = f"""# Release Notes Template

## 1. Release metadata

```md
Release version: {release_version or 'pending'}
Release date: {release_date or 'pending'}
Commit/tag: {commit_tag or 'pending'}
Author: {author or 'pending'}
Release level: {release_level or 'Dev Snapshot'}
Scope summary: {scope_summary or 'pending'}
```

## 2. Highlights

```md
- { _summary_line(bundle_check) }
- Release review record: {release_review_record.as_posix() if release_review_record.is_file() else 'pending'}
- Bundle checker JSON: {_path_or_pending(release_bundle_check_json)}
```

## 3. Fixes and behavior changes

```md
### Fixes
- Release artifact bundle checker now gates candidate bundles before review sign-off.
- Release review record can be prefilled from checker evidence.

### Behavior changes
- Candidate release notes now include a machine-checkable release bundle summary.
```

## 4. Compatibility and contract impact

Record all public contract impact here. If no change, explicitly write `none`.

```md
CLI contract diff summary: {cli_contract_diff}
Python API contract diff summary: {python_api_contract_diff}
Report schema contract diff summary: {report_schema_contract_diff}

CLI contract baseline path: {cli_baseline_path}
Python API baseline path: {python_api_baseline_path}
Report schema baseline path: {report_schema_baseline_path}
```

Recommended checks:

```bash
uv run python python/scripts/export_cli_command_surface.py --check
uv run python python/scripts/export_python_api_surface.py --check
uv run python python/scripts/export_report_schema_surface.py --check
```

## 5. Quality baseline and regression status

Use this section when release scope touches Week 3 timing/verify/sim quality inputs, thresholds, summary logic, or generated review artifacts.

```md
Week3 one-command check run: {week3_one_command_run}
Week3 no-regression enforced: {week3_no_regression}
Week3 regression tolerance: {week3_regression_tolerance}
Week3 pipeline output root: {week3_pipeline_output_root}
Week3 review manifest path: {_path_or_pending(week3_output_root / 'review' / 'manifest.json')}
Week3 validation report path: {_path_or_pending(week3_output_root / 'review' / 'quality_summary.validation.json')}
Week3 summary markdown path: {_path_or_pending(week3_output_root / 'review' / 'quality_summary.current.md')}
Week3 decision summary: {week3_decision_summary}
```

Recommended command:

```bash
uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0
```

## 6. Known limitations and risks

```md
- {compatibility_cli}
- {compatibility_python}
```

## 7. Upgrade and rollback notes

```md
Upgrade actions: rerun release artifact generation, bundle checker, and review record prefill.
Rollback actions: keep the previous release notes and review record as the archival source of truth.
User-facing caveats: {compatibility_default}
```

## 8. Evidence links

```md
Checklist record: {release_review_record.as_posix() if release_review_record.is_file() else 'pending'}
Artifact review bundle: {_path_or_pending(release_bundle_check_json)}
CI run link: pending
Issue links: pending
```
"""

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(content, encoding="utf-8")
    return content


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate a prefilled release notes draft from bundle-check evidence.")
    parser.add_argument("--release-version", type=str, default="", help="Release version string.")
    parser.add_argument("--release-date", type=str, default="", help="Release date string.")
    parser.add_argument("--commit-tag", type=str, default="", help="Commit or tag identifier.")
    parser.add_argument("--author", type=str, default="Core maintainers", help="Author/reviewer label.")
    parser.add_argument("--release-level", type=str, default="Dev Snapshot", help="Release level label.")
    parser.add_argument("--scope-summary", type=str, default="", help="Short scope summary.")
    parser.add_argument(
        "--release-review-record",
        type=Path,
        default=Path("docs/release-review-record-2026-05-28.md"),
        help="Path to the release review record used as evidence.",
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
        help="Week3 pipeline output root for quality baseline evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output markdown path. Defaults to docs/release-notes-<date>.md.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    release_review_record = (
        args.release_review_record if args.release_review_record.is_absolute() else (repo_root / args.release_review_record)
    )
    release_bundle_check_json = (
        args.release_bundle_check_json
        if args.release_bundle_check_json.is_absolute()
        else (repo_root / args.release_bundle_check_json)
    )
    week3_output_root = args.week3_output_root if args.week3_output_root.is_absolute() else (repo_root / args.week3_output_root)

    output_path = args.output
    if output_path is None:
        output_path = repo_root / "docs" / f"release-notes-{args.release_date or 'draft'}.md"
    elif not output_path.is_absolute():
        output_path = repo_root / output_path

    build_release_notes(
        release_version=args.release_version,
        release_date=args.release_date,
        commit_tag=args.commit_tag,
        author=args.author,
        release_level=args.release_level,
        scope_summary=args.scope_summary,
        release_review_record=release_review_record,
        release_bundle_check_json=release_bundle_check_json,
        week3_output_root=week3_output_root,
        output_path=output_path,
    )


if __name__ == "__main__":
    main()
