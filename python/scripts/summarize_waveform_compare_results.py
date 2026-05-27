from __future__ import annotations

import argparse
import json
from pathlib import Path


def _load_thresholds(path: Path) -> dict[str, dict[str, object]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    thresholds: dict[str, dict[str, object]] = {}
    for deck_name, config in payload.items():
        thresholds[deck_name] = {
            "max_abs_threshold_v": float(config["max_abs_threshold_v"]),
            "category": str(config.get("category", "unspecified")),
            "rationale": str(config.get("rationale", "")),
        }
    return thresholds


def _load_results(result_dir: Path) -> dict[str, dict]:
    results: dict[str, dict] = {}
    for json_path in sorted(result_dir.glob("*.compare.json")):
        deck_name = json_path.name.replace(".compare.json", "").removesuffix(".cir")
        results[deck_name] = json.loads(json_path.read_text(encoding="utf-8"))
    return results


def _load_previous_summary(path: Path | None) -> dict[str, object] | None:
    if path is None:
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def build_summary_payload(
    thresholds: dict[str, dict[str, object]],
    results: dict[str, dict],
    previous_summary: dict[str, object] | None = None,
) -> dict[str, object]:
    decks: list[dict[str, object]] = []
    failures = 0
    for deck_name, config in sorted(thresholds.items()):
        threshold = float(config["max_abs_threshold_v"])
        key = deck_name.removesuffix(".cir")
        result = results.get(key)
        if result is None:
            decks.append(
                {
                    "deck": deck_name,
                    "threshold_v": threshold,
                    "category": str(config["category"]),
                    "rationale": str(config["rationale"]),
                    "worst_max_abs_v": None,
                    "summary": "MISSING",
                    "failing_nodes": [],
                }
            )
            failures += 1
            continue

        worst = float(result.get("worst_max_abs_v", 0.0))
        status = str(result.get("summary", "PASS"))
        if status == "PASS" and worst > threshold:
            status = "FAIL"
        failing_nodes = [str(node) for node in result.get("failing_nodes", [])]
        top_worst_nodes = [
            {
                "node": str(entry.get("node")),
                "max_abs_v": float(entry.get("max_abs_v", 0.0)),
                "rms_v": float(entry.get("rms_v", 0.0)),
            }
            for entry in result.get("top_worst_nodes", [])
            if isinstance(entry, dict)
        ]
        if status != "PASS":
            failures += 1
        decks.append(
            {
                "deck": deck_name,
                "threshold_v": threshold,
                "category": str(config["category"]),
                "rationale": str(config["rationale"]),
                "worst_max_abs_v": worst,
                "summary": status,
                "failing_nodes": failing_nodes,
                "top_worst_nodes": top_worst_nodes,
            }
        )

    categories: dict[str, dict[str, object]] = {}
    for deck in decks:
        category = str(deck["category"])
        entry = categories.setdefault(
            category,
            {
                "category": category,
                "deck_count": 0,
                "failures": 0,
                "worst_max_abs_v": 0.0,
                "worst_deck": None,
                "top_hotspots": [],
            },
        )
        entry["deck_count"] = int(entry["deck_count"]) + 1
        deck_summary = str(deck["summary"])
        deck_worst = deck["worst_max_abs_v"]
        if deck_summary != "PASS":
            entry["failures"] = int(entry["failures"]) + 1
        if deck_worst is not None and float(deck_worst) >= float(entry["worst_max_abs_v"]):
            entry["worst_max_abs_v"] = float(deck_worst)
            entry["worst_deck"] = str(deck["deck"])
        hotspots = list(entry["top_hotspots"])
        for node in deck.get("top_worst_nodes", []):
            hotspots.append(
                {
                    "deck": str(deck["deck"]),
                    "node": str(node.get("node")),
                    "max_abs_v": float(node.get("max_abs_v", 0.0)),
                    "rms_v": float(node.get("rms_v", 0.0)),
                }
            )
        hotspots.sort(
            key=lambda item: (-float(item["max_abs_v"]), -float(item["rms_v"]), str(item["deck"]), str(item["node"]))
        )
        entry["top_hotspots"] = hotspots[:3]

    payload = {
        "failures": failures,
        "decks": decks,
        "categories": [categories[name] for name in sorted(categories)],
    }
    if previous_summary is not None:
        payload["history_diff"] = build_history_diff(payload, previous_summary)
    return payload


def build_history_diff(
    current_payload: dict[str, object],
    previous_payload: dict[str, object],
) -> dict[str, object]:
    current_decks = {
        str(deck.get("deck")): deck
        for deck in current_payload.get("decks", [])
        if isinstance(deck, dict)
    }
    previous_decks = {
        str(deck.get("deck")): deck
        for deck in previous_payload.get("decks", [])
        if isinstance(deck, dict)
    }
    deck_changes: list[dict[str, object]] = []
    for deck_name in sorted(current_decks):
        current = current_decks[deck_name]
        previous = previous_decks.get(deck_name)
        current_worst = current.get("worst_max_abs_v")
        previous_worst = previous.get("worst_max_abs_v") if isinstance(previous, dict) else None
        current_summary = str(current.get("summary"))
        previous_summary_value = str(previous.get("summary")) if isinstance(previous, dict) else "NEW"
        delta = None
        if current_worst is not None and previous_worst is not None:
            delta = float(current_worst) - float(previous_worst)
        if previous is None or delta not in (None, 0.0) or current_summary != previous_summary_value:
            deck_changes.append(
                {
                    "deck": deck_name,
                    "current_summary": current_summary,
                    "previous_summary": previous_summary_value,
                    "current_worst_max_abs_v": current_worst,
                    "previous_worst_max_abs_v": previous_worst,
                    "worst_max_abs_v_delta": delta,
                }
            )

    current_categories = {
        str(category.get("category")): category
        for category in current_payload.get("categories", [])
        if isinstance(category, dict)
    }
    previous_categories = {
        str(category.get("category")): category
        for category in previous_payload.get("categories", [])
        if isinstance(category, dict)
    }
    category_changes: list[dict[str, object]] = []
    for category_name in sorted(current_categories):
        current = current_categories[category_name]
        previous = previous_categories.get(category_name)
        current_worst = current.get("worst_max_abs_v")
        previous_worst = previous.get("worst_max_abs_v") if isinstance(previous, dict) else None
        current_failures = int(current.get("failures", 0))
        previous_failures = int(previous.get("failures", 0)) if isinstance(previous, dict) else 0
        delta = None
        if current_worst is not None and previous_worst is not None:
            delta = float(current_worst) - float(previous_worst)
        if previous is None or delta not in (None, 0.0) or current_failures != previous_failures:
            category_changes.append(
                {
                    "category": category_name,
                    "current_failures": current_failures,
                    "previous_failures": previous_failures,
                    "current_worst_max_abs_v": current_worst,
                    "previous_worst_max_abs_v": previous_worst,
                    "worst_max_abs_v_delta": delta,
                }
            )

    return {
        "previous_failures": int(previous_payload.get("failures", 0)),
        "current_failures": int(current_payload.get("failures", 0)),
        "failure_delta": int(current_payload.get("failures", 0)) - int(previous_payload.get("failures", 0)),
        "deck_changes": deck_changes,
        "category_changes": category_changes,
    }


def _summary_rank(summary: str) -> int:
    normalized = summary.strip().upper()
    if normalized == "PASS":
        return 0
    if normalized == "FAIL":
        return 1
    if normalized == "MISSING":
        return 2
    return 3


def validate_no_regression(
    payload: dict[str, object],
    previous_payload: dict[str, object],
    tolerance_v: float,
) -> list[str]:
    errors: list[str] = []

    current_decks = {
        str(deck.get("deck")): deck
        for deck in payload.get("decks", [])
        if isinstance(deck, dict)
    }
    previous_decks = {
        str(deck.get("deck")): deck
        for deck in previous_payload.get("decks", [])
        if isinstance(deck, dict)
    }
    for deck_name, current in current_decks.items():
        previous = previous_decks.get(deck_name)
        if previous is None:
            continue
        current_summary = str(current.get("summary", "PASS"))
        previous_summary = str(previous.get("summary", "PASS"))
        if _summary_rank(current_summary) > _summary_rank(previous_summary):
            errors.append(
                f"deck regression for {deck_name}: summary worsened {previous_summary} -> {current_summary}"
            )

        current_worst = current.get("worst_max_abs_v")
        previous_worst = previous.get("worst_max_abs_v")
        if current_worst is not None and previous_worst is not None:
            delta = float(current_worst) - float(previous_worst)
            if delta > tolerance_v:
                errors.append(
                    f"deck regression for {deck_name}: worst_max_abs_v delta {delta:+.6e} exceeds tolerance {tolerance_v:.6e}"
                )

    current_categories = {
        str(category.get("category")): category
        for category in payload.get("categories", [])
        if isinstance(category, dict)
    }
    previous_categories = {
        str(category.get("category")): category
        for category in previous_payload.get("categories", [])
        if isinstance(category, dict)
    }
    for category_name, current in current_categories.items():
        previous = previous_categories.get(category_name)
        if previous is None:
            continue
        current_failures = int(current.get("failures", 0))
        previous_failures = int(previous.get("failures", 0))
        if current_failures > previous_failures:
            errors.append(
                f"category regression for {category_name}: failures increased {previous_failures} -> {current_failures}"
            )

        current_worst = current.get("worst_max_abs_v")
        previous_worst = previous.get("worst_max_abs_v")
        if current_worst is not None and previous_worst is not None:
            delta = float(current_worst) - float(previous_worst)
            if delta > tolerance_v:
                errors.append(
                    f"category regression for {category_name}: worst_max_abs_v delta {delta:+.6e} exceeds tolerance {tolerance_v:.6e}"
                )

    return errors


def validate_summary_payload(
    payload: dict[str, object],
    thresholds: dict[str, dict[str, object]],
) -> list[str]:
    errors: list[str] = []
    decks = payload.get("decks")
    if not isinstance(decks, list):
        return ["summary payload is missing a decks list"]

    expected_decks = list(sorted(thresholds))
    actual_decks = [str(deck.get("deck")) for deck in decks if isinstance(deck, dict)]
    if actual_decks != expected_decks:
        errors.append(
            f"summary deck set/order mismatch: expected {expected_decks}, got {actual_decks}"
        )

    expected_failures = sum(
        1 for deck in decks if isinstance(deck, dict) and str(deck.get("summary")) != "PASS"
    )
    if int(payload.get("failures", -1)) != expected_failures:
        errors.append(
            f"summary failures count mismatch: expected {expected_failures}, got {payload.get('failures')}"
        )

    for deck in decks:
        if not isinstance(deck, dict):
            errors.append("summary contains a non-object deck entry")
            continue
        deck_name = str(deck.get("deck"))
        threshold_config = thresholds.get(deck_name)
        if threshold_config is None:
            errors.append(f"summary contains unexpected deck entry: {deck_name}")
            continue
        if str(deck.get("category", "")).strip() != str(threshold_config.get("category", "")).strip():
            errors.append(f"summary category mismatch for {deck_name}")
        if str(deck.get("rationale", "")).strip() != str(threshold_config.get("rationale", "")).strip():
            errors.append(f"summary rationale mismatch for {deck_name}")

    categories = payload.get("categories")
    if not isinstance(categories, list):
        errors.append("summary payload is missing a categories list")
        return errors

    expected_category_names = sorted({str(config["category"]) for config in thresholds.values()})
    actual_category_names = [str(entry.get("category")) for entry in categories if isinstance(entry, dict)]
    if actual_category_names != expected_category_names:
        errors.append(
            f"summary category set/order mismatch: expected {expected_category_names}, got {actual_category_names}"
        )

    for category in categories:
        if not isinstance(category, dict):
            errors.append("summary contains a non-object category entry")
            continue
        if not isinstance(category.get("top_hotspots", []), list):
            errors.append(f"summary category hotspot list is invalid for {category.get('category')}")

    history_diff = payload.get("history_diff")
    if history_diff is not None:
        if not isinstance(history_diff, dict):
            errors.append("summary history_diff must be an object")
        else:
            if not isinstance(history_diff.get("deck_changes", []), list):
                errors.append("summary history_diff deck_changes must be a list")
            if not isinstance(history_diff.get("category_changes", []), list):
                errors.append("summary history_diff category_changes must be a list")

    return errors


def build_markdown_report(
    thresholds: dict[str, dict[str, object]],
    results: dict[str, dict],
    previous_summary: dict[str, object] | None = None,
) -> tuple[str, int]:
    payload = build_summary_payload(thresholds, results, previous_summary)
    lines: list[str] = []
    lines.append("# Waveform Compare Summary")
    lines.append("")
    lines.append("| Deck | Category | Threshold (V) | Worst Max Abs (V) | Summary | Details |")
    lines.append("|------|----------|---------------|-------------------|---------|---------|")

    for deck in payload["decks"]:
        deck_name = str(deck["deck"])
        category = str(deck["category"])
        threshold = float(deck["threshold_v"])
        status = str(deck["summary"])
        worst = deck["worst_max_abs_v"]
        failing_nodes = [str(node) for node in deck["failing_nodes"]]
        if status == "MISSING":
            detail = "missing compare result"
            lines.append(f"| {deck_name} | {category} | {threshold:.6e} | n/a | MISSING | {detail} |")
            continue

        detail = "-"
        if failing_nodes:
            detail = ", ".join(failing_nodes)
        elif deck["top_worst_nodes"]:
            detail = ", ".join(
                f"{entry['node']}:{float(entry['max_abs_v']):.3e}"
                for entry in deck["top_worst_nodes"][:2]
            )
        lines.append(
            f"| {deck_name} | {category} | {threshold:.6e} | {float(worst):.6e} | {status} | {detail} |"
        )

    lines.append("")
    lines.append("## Category Summary")
    lines.append("")
    lines.append("| Category | Deck Count | Failures | Worst Max Abs (V) | Worst Deck | Hotspots |")
    lines.append("|----------|------------|----------|-------------------|------------|----------|")
    for category in payload["categories"]:
        worst_deck = category["worst_deck"] or "-"
        hotspots = "-"
        if category["top_hotspots"]:
            hotspots = ", ".join(
                f"{entry['deck']}::{entry['node']}:{float(entry['max_abs_v']):.3e}"
                for entry in category["top_hotspots"][:2]
            )
        lines.append(
            f"| {category['category']} | {int(category['deck_count'])} | {int(category['failures'])} | {float(category['worst_max_abs_v']):.6e} | {worst_deck} | {hotspots} |"
        )

    lines.append("")
    lines.append("## Threshold Rationale")
    lines.append("")
    for deck in payload["decks"]:
        lines.append(
            f"- {deck['deck']}: [{deck['category']}] {deck['rationale'] or 'no rationale recorded'}"
        )

    history_diff = payload.get("history_diff")
    if isinstance(history_diff, dict):
        lines.append("")
        lines.append("## History Diff")
        lines.append("")
        lines.append(
            f"failures: {int(history_diff['current_failures'])} (delta {int(history_diff['failure_delta']):+d} vs previous {int(history_diff['previous_failures'])})"
        )
        if history_diff["deck_changes"]:
            lines.append("")
            lines.append("| Deck | Summary Change | Worst Max Abs Delta (V) |")
            lines.append("|------|----------------|-------------------------|")
            for entry in history_diff["deck_changes"]:
                delta_text = "n/a"
                if entry["worst_max_abs_v_delta"] is not None:
                    delta_text = f"{float(entry['worst_max_abs_v_delta']):+.6e}"
                lines.append(
                    f"| {entry['deck']} | {entry['previous_summary']} -> {entry['current_summary']} | {delta_text} |"
                )
        if history_diff["category_changes"]:
            lines.append("")
            lines.append("| Category | Failure Delta | Worst Max Abs Delta (V) |")
            lines.append("|----------|---------------|-------------------------|")
            for entry in history_diff["category_changes"]:
                delta_text = "n/a"
                if entry["worst_max_abs_v_delta"] is not None:
                    delta_text = f"{float(entry['worst_max_abs_v_delta']):+.6e}"
                lines.append(
                    f"| {entry['category']} | {int(entry['current_failures']) - int(entry['previous_failures']):+d} | {delta_text} |"
                )
        if not history_diff["deck_changes"] and not history_diff["category_changes"]:
            lines.append("")
            lines.append("No deck/category changes relative to previous summary.")

    lines.append("")
    lines.append(f"failures={payload['failures']}")
    return "\n".join(lines) + "\n", int(payload["failures"])


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
    parser.add_argument(
        "--json-output",
        type=Path,
        default=None,
        help="Optional JSON summary output path",
    )
    parser.add_argument(
        "--previous-summary-json",
        type=Path,
        default=None,
        help="Optional prior summary JSON path used to compute deck/category drift in the current report",
    )
    parser.add_argument(
        "--validate-pass",
        action="store_true",
        help="Fail unless the generated summary payload is internally consistent and all decks passed.",
    )
    parser.add_argument(
        "--validate-no-regression",
        action="store_true",
        help="Fail when the current summary is worse than --previous-summary-json by summary status or worst_max_abs_v beyond tolerance.",
    )
    parser.add_argument(
        "--regression-tolerance-v",
        type=float,
        default=0.0,
        help="Allowed positive worst_max_abs_v drift during --validate-no-regression checks.",
    )
    args = parser.parse_args()

    thresholds = _load_thresholds(args.thresholds)
    results = _load_results(args.result_dir)
    previous_summary = _load_previous_summary(args.previous_summary_json)
    payload = build_summary_payload(thresholds, results, previous_summary)
    report, failures = build_markdown_report(thresholds, results, previous_summary)
    print(report, end="")

    if args.markdown_output is not None:
        args.markdown_output.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_output.write_text(report, encoding="utf-8")

    if args.json_output is not None:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(json.dumps(payload, indent=2), encoding="utf-8")

    validation_errors = validate_summary_payload(payload, thresholds)
    if validation_errors:
        for error in validation_errors:
            print(f"validation_error={error}")
        raise SystemExit(1)

    if args.validate_pass and failures > 0:
        print("validation_error=summary contains failing or missing decks")
        raise SystemExit(1)

    if args.validate_no_regression:
        if previous_summary is None:
            print("validation_error=no previous summary JSON provided for no-regression validation")
            raise SystemExit(1)
        regression_errors = validate_no_regression(payload, previous_summary, args.regression_tolerance_v)
        if regression_errors:
            for error in regression_errors:
                print(f"validation_error={error}")
            raise SystemExit(1)

    if failures > 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
