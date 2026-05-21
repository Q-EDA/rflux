from __future__ import annotations

import argparse
import json
from pathlib import Path


def _load_thresholds(path: Path) -> dict[str, float]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    thresholds: dict[str, float] = {}
    for deck_name, config in payload.items():
        thresholds[deck_name] = float(config["max_abs_threshold_v"])
    return thresholds


def _load_results(result_dir: Path) -> dict[str, dict]:
    results: dict[str, dict] = {}
    for json_path in sorted(result_dir.glob("*.compare.json")):
        deck_name = json_path.name.replace(".compare.json", "")
        results[deck_name] = json.loads(json_path.read_text(encoding="utf-8"))
    return results


def build_markdown_report(
    thresholds: dict[str, float],
    results: dict[str, dict],
) -> tuple[str, int]:
    lines: list[str] = []
    lines.append("# Waveform Compare Summary")
    lines.append("")
    lines.append("| Deck | Threshold (V) | Worst Max Abs (V) | Summary |")
    lines.append("|------|---------------|-------------------|---------|")

    failures = 0
    for deck_name, threshold in sorted(thresholds.items()):
        key = deck_name.removesuffix(".cir")
        result = results.get(key)
        if result is None:
            lines.append(f"| {deck_name} | {threshold:.6e} | n/a | MISSING |")
            failures += 1
            continue

        worst = float(result.get("worst_max_abs_v", 0.0))
        status = "PASS" if worst <= threshold else "FAIL"
        if status != "PASS":
            failures += 1
        lines.append(f"| {deck_name} | {threshold:.6e} | {worst:.6e} | {status} |")

    lines.append("")
    lines.append(f"failures={failures}")
    return "\n".join(lines) + "\n", failures


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Summarize waveform compare JSON results against per-deck thresholds.",
    )
    parser.add_argument(
        "--thresholds",
        type=Path,
        default=Path("python/tests/benchmarks/phase6/waveform_thresholds.json"),
        help="Threshold JSON file path",
    )
    parser.add_argument(
        "--result-dir",
        type=Path,
        default=Path("python/tests/benchmarks/phase6"),
        help="Directory containing *.compare.json outputs",
    )
    parser.add_argument(
        "--markdown-output",
        type=Path,
        default=None,
        help="Optional markdown output path",
    )
    args = parser.parse_args()

    thresholds = _load_thresholds(args.thresholds)
    results = _load_results(args.result_dir)
    report, failures = build_markdown_report(thresholds, results)
    print(report, end="")

    if args.markdown_output is not None:
        args.markdown_output.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_output.write_text(report, encoding="utf-8")

    if failures > 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
