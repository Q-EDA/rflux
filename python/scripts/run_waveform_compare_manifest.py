from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def load_thresholds(path: Path) -> dict[str, dict[str, object]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    return {
        str(deck_name): {
            "max_abs_threshold_v": float(config["max_abs_threshold_v"]),
            "category": str(config.get("category", "unspecified")),
            "rationale": str(config.get("rationale", "")),
        }
        for deck_name, config in payload.items()
    }


def select_thresholds(
    thresholds: dict[str, dict[str, object]],
    decks: list[str] | None,
    categories: list[str] | None,
) -> dict[str, dict[str, object]]:
    deck_filters = [deck.strip() for deck in (decks or []) if deck.strip()]
    category_filters = {category.strip() for category in (categories or []) if category.strip()}

    missing_decks = [deck for deck in deck_filters if deck not in thresholds]
    if missing_decks:
        raise ValueError(f"unknown deck filters: {', '.join(missing_decks)}")

    selected: dict[str, dict[str, object]] = {}
    for deck_name, config in thresholds.items():
        if deck_filters and deck_name not in deck_filters:
            continue
        if category_filters and str(config.get("category", "")) not in category_filters:
            continue
        selected[deck_name] = config

    if not selected:
        raise ValueError("no decks selected by the provided deck/category filters")
    return selected


def run_checked(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(command, cwd=str(cwd), capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr + "\n" + completed.stdout)
    return completed


def resolve_previous_summary_json(
    explicit_path: Path | None,
    result_dir: Path,
    benchmark_dir: Path,
    baseline_platform: str | None,
) -> Path | None:
    if explicit_path is not None:
        return explicit_path

    auto_baseline = result_dir / "waveform_compare_summary.approved-baseline.json"
    if auto_baseline.is_file():
        return auto_baseline

    normalized_platform = (baseline_platform or "").strip().lower()
    if normalized_platform:
        repo_baseline = benchmark_dir / f"waveform_compare_summary.{normalized_platform}-approved-baseline.json"
        if repo_baseline.is_file():
            return repo_baseline
    return None


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run waveform compare over a selected subset of the threshold manifest.",
    )
    parser.add_argument(
        "--thresholds",
        type=Path,
        default=Path("python/tests/benchmarks/phase6/waveform_thresholds.json"),
        help="Path to the waveform threshold manifest.",
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
        default=Path("target/waveform-compare-subset"),
        help="Directory where compare JSON and summary outputs will be written.",
    )
    parser.add_argument(
        "--deck",
        action="append",
        default=None,
        help="Specific deck name from the manifest to compare. May be repeated.",
    )
    parser.add_argument(
        "--category",
        action="append",
        default=None,
        help="Manifest category to compare. May be repeated. Combined with --deck as an intersection.",
    )
    parser.add_argument(
        "--josim-command",
        type=str,
        default=os.environ.get("RFLOW_JOSIM_COMMAND", "josim"),
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
        "--previous-summary-json",
        type=Path,
        default=None,
        help="Optional previous summary JSON used for history diff generation.",
    )
    parser.add_argument(
        "--baseline-platform",
        type=str,
        default="",
        help="Optional platform key used to auto-resolve repo-tracked baselines such as waveform_compare_summary.<platform>-approved-baseline.json.",
    )
    parser.add_argument(
        "--validate-pass",
        action="store_true",
        help="Also run summary validation after compare generation.",
    )
    parser.add_argument(
        "--validate-no-regression",
        action="store_true",
        help="Also fail when the current summary regresses relative to the resolved previous summary baseline.",
    )
    parser.add_argument(
        "--regression-tolerance-v",
        type=float,
        default=0.0,
        help="Allowed positive worst_max_abs_v drift during --validate-no-regression checks.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    compare_script = repo_root / "python" / "scripts" / "compare_internal_external_waveforms.py"
    summary_script = repo_root / "python" / "scripts" / "summarize_waveform_compare_results.py"
    thresholds_path = (repo_root / args.thresholds).resolve() if not args.thresholds.is_absolute() else args.thresholds
    benchmark_dir = (repo_root / args.benchmark_dir).resolve() if not args.benchmark_dir.is_absolute() else args.benchmark_dir
    result_dir = (repo_root / args.result_dir).resolve() if not args.result_dir.is_absolute() else args.result_dir
    result_dir.mkdir(parents=True, exist_ok=True)

    thresholds = load_thresholds(thresholds_path)
    selected_thresholds = select_thresholds(thresholds, args.deck, args.category)
    subset_thresholds_path = result_dir / "selected_waveform_thresholds.json"
    subset_thresholds_path.write_text(json.dumps(selected_thresholds, indent=2), encoding="utf-8")

    for deck_name, config in selected_thresholds.items():
        json_output = result_dir / f"{deck_name}.compare.json"
        run_checked(
            [
                sys.executable,
                str(compare_script),
                str((benchmark_dir / deck_name).resolve()),
                "--josim-command",
                args.josim_command,
                "--max-abs-threshold",
                str(float(config["max_abs_threshold_v"])),
                "--json-output",
                str(json_output),
            ],
            cwd=repo_root,
        )

    summary_markdown_output = args.summary_markdown_output or (result_dir / "waveform_compare_summary.md")
    summary_json_output = args.summary_json_output or (result_dir / "waveform_compare_summary.json")
    summary_command = [
        sys.executable,
        str(summary_script),
        "--result-dir",
        str(result_dir),
        "--thresholds",
        str(subset_thresholds_path),
        "--markdown-output",
        str(summary_markdown_output),
        "--json-output",
        str(summary_json_output),
    ]
    previous_summary_json = resolve_previous_summary_json(
        args.previous_summary_json,
        result_dir,
        benchmark_dir,
        args.baseline_platform,
    )
    if previous_summary_json is not None:
        summary_command.extend(["--previous-summary-json", str(previous_summary_json)])
    if args.validate_no_regression:
        summary_command.extend([
            "--validate-no-regression",
            "--regression-tolerance-v",
            str(args.regression_tolerance_v),
        ])
    run_checked(summary_command, cwd=repo_root)

    if args.validate_pass:
        validation_command = [
            sys.executable,
            str(summary_script),
            "--result-dir",
            str(result_dir),
            "--thresholds",
            str(subset_thresholds_path),
            "--json-output",
            str(result_dir / "waveform_compare_summary.validation.json"),
            "--validate-pass",
        ]
        if previous_summary_json is not None:
            validation_command.extend(["--previous-summary-json", str(previous_summary_json)])
        if args.validate_no_regression:
            validation_command.extend([
                "--validate-no-regression",
                "--regression-tolerance-v",
                str(args.regression_tolerance_v),
            ])
        run_checked(validation_command, cwd=repo_root)

    print(f"selected_decks={','.join(selected_thresholds)}")
    print(f"result_dir={result_dir}")
    print(f"summary_json={summary_json_output}")
    if previous_summary_json is not None:
        print(f"previous_summary_json={previous_summary_json}")


if __name__ == "__main__":
    main()