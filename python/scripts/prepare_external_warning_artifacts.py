from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import platform
import shutil
from pathlib import Path


def _load_script_module(script_name: str):
    script_path = Path(__file__).resolve().parent / script_name
    spec = importlib.util.spec_from_file_location(script_name.replace(".py", ""), script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load helper script module: {script_name}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_summary_module = _load_script_module("summarize_external_warning_results.py")
_load_contracts = _summary_module._load_contracts
_load_results = _summary_module._load_results
build_summary_payload = _summary_module.build_summary_payload
validate_summary_payload = _summary_module.validate_summary_payload


README_TEXT = """external_warning_summary.current.md
  Current warning-contract review summary markdown for this optional JoSIM job.

external_warning_summary.current.json
  Current warning-contract review summary JSON for this optional JoSIM job.

selected_external_warning_contracts.json
  The exact warning-contract subset resolved for this run.

*.warning.json
  Per-deck warning-contract evaluation results produced by run_external_warning_manifest.py.

manifest.json
  Machine-readable artifact metadata including file roles, hashes, selected contract path,
  josim command, Python version, summary/category quick-look data, and GitHub Actions run context.
"""


def _resolve_repo_path(repo_root: Path, raw_path: str) -> Path | None:
    normalized = raw_path.strip()
    if not normalized:
        return None
    candidate = Path(normalized)
    if candidate.is_absolute():
        return candidate
    return (repo_root / candidate).resolve()


def _write_json(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _summary_overview(payload: dict[str, object]) -> dict[str, object]:
    decks = [deck for deck in payload.get("decks", []) if isinstance(deck, dict)]
    passing = [str(deck.get("deck")) for deck in decks if str(deck.get("summary")) == "PASS"]
    failing = [str(deck.get("deck")) for deck in decks if str(deck.get("summary")) == "FAIL"]
    missing = [str(deck.get("deck")) for deck in decks if str(deck.get("summary")) == "MISSING"]
    return {
        "deck_count": len(decks),
        "passing_deck_count": len(passing),
        "failure_count": int(payload.get("failures", 0)),
        "failing_decks": failing,
        "missing_decks": missing,
    }


def _category_overview(payload: dict[str, object]) -> list[dict[str, object]]:
    categories = [category for category in payload.get("categories", []) if isinstance(category, dict)]
    return [
        {
            "category": str(category.get("category")),
            "deck_count": int(category.get("deck_count", 0)),
            "failure_count": int(category.get("failures", 0)),
            "failing_decks": [str(entry) for entry in category.get("failing_decks", [])],
            "observed_warnings": [str(entry) for entry in category.get("observed_warnings", [])],
        }
        for category in categories
    ]


def _build_manifest(
    *,
    artifact_dir: Path,
    selected_contracts_path: Path,
    summary_payload: dict[str, object],
    josim_command: str,
    github_context: dict[str, str],
) -> dict[str, object]:
    artifact_files = [
        artifact_dir / "external_warning_summary.current.md",
        artifact_dir / "external_warning_summary.current.json",
        artifact_dir / "selected_external_warning_contracts.json",
    ]
    artifact_files.extend(sorted(artifact_dir.glob("*.warning.json")))
    file_inventory = [
        {
            "name": path.name,
            "relative_path": path.relative_to(artifact_dir).as_posix(),
            "size_bytes": path.stat().st_size,
            "sha256": _sha256(path),
        }
        for path in artifact_files
    ]

    return {
        "kind": "external-warning-artifacts",
        "schema_version": 1,
        "github_actions_context": github_context,
        "python_version": platform.python_version(),
        "josim_command": josim_command,
        "selected_contracts_file": selected_contracts_path.name,
        "artifact_files": file_inventory,
        "summary_overview": _summary_overview(summary_payload),
        "category_overview": _category_overview(summary_payload),
        "review_contract": {
            "current_markdown": "Human-readable summary for warning-contract review.",
            "current_json": "Machine-readable summary payload for warning-contract review.",
            "selected_contracts_json": "Exact warning-contract subset resolved for this run.",
            "per_deck_warning_json": "Per-deck warning-contract evaluation outputs used to build the summary.",
        },
    }


def prepare_external_warning_artifacts(
    *,
    repo_root: Path,
    result_dir: Path,
    artifact_dir: Path,
    josim_command: str,
    github_context: dict[str, str],
) -> dict[str, object]:
    artifact_dir.mkdir(parents=True, exist_ok=True)

    selected_contracts_path = result_dir / "selected_external_warning_contracts.json"
    summary_markdown_path = result_dir / "external_warning_summary.md"
    summary_json_path = result_dir / "external_warning_summary.json"
    if not selected_contracts_path.exists():
        raise SystemExit(f"missing selected contracts file: {selected_contracts_path}")
    if not summary_markdown_path.exists():
        raise SystemExit(f"missing summary markdown file: {summary_markdown_path}")
    if not summary_json_path.exists():
        raise SystemExit(f"missing summary JSON file: {summary_json_path}")

    contracts = _load_contracts(selected_contracts_path)
    results = _load_results(result_dir)
    summary_payload = build_summary_payload(contracts, results)
    validation_errors = validate_summary_payload(summary_payload, contracts)
    if validation_errors:
        raise SystemExit("\n".join(validation_errors))
    if int(summary_payload.get("failures", 0)) != 0:
        raise SystemExit(
            f"external warning summary contains {summary_payload.get('failures')} failing deck(s)"
        )

    copied_summary_markdown_path = artifact_dir / "external_warning_summary.current.md"
    copied_summary_json_path = artifact_dir / "external_warning_summary.current.json"
    copied_selected_contracts_path = artifact_dir / "selected_external_warning_contracts.json"
    shutil.copy2(summary_markdown_path, copied_summary_markdown_path)
    shutil.copy2(summary_json_path, copied_summary_json_path)
    shutil.copy2(selected_contracts_path, copied_selected_contracts_path)

    for warning_json_path in sorted(result_dir.glob("*.warning.json")):
        shutil.copy2(warning_json_path, artifact_dir / warning_json_path.name)

    (artifact_dir / "README.txt").write_text(README_TEXT, encoding="utf-8")
    manifest = _build_manifest(
        artifact_dir=artifact_dir,
        selected_contracts_path=copied_selected_contracts_path,
        summary_payload=summary_payload,
        josim_command=josim_command,
        github_context=github_context,
    )
    _write_json(artifact_dir / "manifest.json", manifest)
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Prepare external warning review artifacts for CI.",
    )
    parser.add_argument(
        "--result-dir",
        type=str,
        default="target/external-warning-manifest",
        help="Repo-relative or absolute directory containing warning-manifest outputs.",
    )
    parser.add_argument(
        "--artifact-dir",
        type=str,
        default="target/external-warning-review",
        help="Repo-relative or absolute directory where review artifacts are written.",
    )
    parser.add_argument("--josim-command", type=str, default="josim")
    parser.add_argument("--github-workflow", type=str, default="")
    parser.add_argument("--github-job", type=str, default="waveform-compare-optional")
    parser.add_argument("--github-event-name", type=str, default="")
    parser.add_argument("--github-run-id", type=str, default="")
    parser.add_argument("--github-run-attempt", type=str, default="")
    parser.add_argument("--github-sha", type=str, default="")
    parser.add_argument("--github-ref-name", type=str, default="")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    result_dir = _resolve_repo_path(repo_root, args.result_dir)
    artifact_dir = _resolve_repo_path(repo_root, args.artifact_dir)
    if result_dir is None or artifact_dir is None:
        raise SystemExit("required path argument resolved to empty value")

    prepare_external_warning_artifacts(
        repo_root=repo_root,
        result_dir=result_dir,
        artifact_dir=artifact_dir,
        josim_command=args.josim_command,
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