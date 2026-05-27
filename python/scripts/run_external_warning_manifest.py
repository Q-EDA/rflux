from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def load_warning_contracts(path: Path) -> dict[str, dict[str, object]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    return {
        str(deck_name): {
            "category": str(config.get("category", "unspecified")),
            "rationale": str(config.get("rationale", "")),
            "expected_warnings": [str(entry) for entry in config.get("expected_warnings", [])],
            "forbidden_generated_deck_tokens": [
                str(entry) for entry in config.get("forbidden_generated_deck_tokens", [])
            ],
        }
        for deck_name, config in payload.items()
    }


def select_warning_contracts(
    contracts: dict[str, dict[str, object]],
    decks: list[str] | None,
    categories: list[str] | None,
) -> dict[str, dict[str, object]]:
    deck_filters = [deck.strip() for deck in (decks or []) if deck.strip()]
    category_filters = {category.strip() for category in (categories or []) if category.strip()}

    missing_decks = [deck for deck in deck_filters if deck not in contracts]
    if missing_decks:
        raise ValueError(f"unknown deck filters: {', '.join(missing_decks)}")

    selected: dict[str, dict[str, object]] = {}
    for deck_name, config in contracts.items():
        if deck_filters and deck_name not in deck_filters:
            continue
        if category_filters and str(config.get("category", "")) not in category_filters:
            continue
        selected[deck_name] = config

    if not contracts and not deck_filters and not category_filters:
        return {}
    if not selected:
        raise ValueError("no decks selected by the provided deck/category filters")
    return selected


def run_checked(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(command, cwd=str(cwd), capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr + "\n" + completed.stdout)
    return completed


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Resolve a selected subset of the external warning contract manifest.",
    )
    parser.add_argument(
        "--contracts",
        type=Path,
        default=Path("python/tests/benchmarks/phase6/external_warning_contracts.json"),
        help="Path to the external warning contract manifest.",
    )
    parser.add_argument(
        "--deck",
        action="append",
        default=None,
        help="Specific deck name from the manifest to select. May be repeated.",
    )
    parser.add_argument(
        "--category",
        action="append",
        default=None,
        help="Manifest category to select. May be repeated. Combined with --deck as an intersection.",
    )
    parser.add_argument(
        "--benchmark-dir",
        type=Path,
        default=Path("python/tests/benchmarks/phase6"),
        help="Directory containing benchmark deck files.",
    )
    parser.add_argument(
        "--result-dir",
        type=Path,
        default=Path("target/external-warning-results"),
        help="Directory where warning JSON and summary outputs will be written.",
    )
    parser.add_argument(
        "--josim-command",
        type=str,
        default="josim",
        help="External simulator command used for external_josim runs.",
    )
    parser.add_argument(
        "--summary-markdown-output",
        type=Path,
        default=None,
        help="Optional markdown summary output path. Defaults under --result-dir.",
    )
    parser.add_argument(
        "--summary-json-output",
        type=Path,
        default=None,
        help="Optional JSON summary output path. Defaults under --result-dir.",
    )
    parser.add_argument(
        "--validate-pass",
        action="store_true",
        help="Also run summary validation after warning generation.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    check_script = repo_root / "python" / "scripts" / "check_external_warning_contract.py"
    summary_script = repo_root / "python" / "scripts" / "summarize_external_warning_results.py"
    contracts_path = (repo_root / args.contracts).resolve() if not args.contracts.is_absolute() else args.contracts
    benchmark_dir = (repo_root / args.benchmark_dir).resolve() if not args.benchmark_dir.is_absolute() else args.benchmark_dir
    result_dir = (repo_root / args.result_dir).resolve() if not args.result_dir.is_absolute() else args.result_dir
    result_dir.mkdir(parents=True, exist_ok=True)
    contracts = load_warning_contracts(contracts_path)
    selected_contracts = select_warning_contracts(contracts, args.deck, args.category)

    selected_contracts_path = result_dir / "selected_external_warning_contracts.json"
    selected_contracts_path.write_text(json.dumps(selected_contracts, indent=2), encoding="utf-8")

    for deck_name, config in selected_contracts.items():
        json_output = result_dir / f"{deck_name}.warning.json"
        command = [
            sys.executable,
            str(check_script),
            str((benchmark_dir / deck_name).resolve()),
            "--josim-command",
            args.josim_command,
            "--json-output",
            str(json_output),
        ]
        for warning in config.get("expected_warnings", []):
            command.extend(["--expected-warning", str(warning)])
        for token in config.get("forbidden_generated_deck_tokens", []):
            command.extend(["--forbidden-generated-deck-token", str(token)])
        run_checked(command, cwd=repo_root)

    summary_markdown_output = args.summary_markdown_output or (result_dir / "external_warning_summary.md")
    summary_json_output = args.summary_json_output or (result_dir / "external_warning_summary.json")
    summary_command = [
        sys.executable,
        str(summary_script),
        "--contracts",
        str(selected_contracts_path),
        "--result-dir",
        str(result_dir),
        "--markdown-output",
        str(summary_markdown_output),
        "--json-output",
        str(summary_json_output),
    ]
    if args.validate_pass:
        summary_command.append("--validate-pass")
    run_checked(summary_command, cwd=repo_root)

    print(f"selected_decks={','.join(selected_contracts)}")
    print(f"result_dir={result_dir}")
    print(f"summary_json={summary_json_output}")


if __name__ == "__main__":
    main()