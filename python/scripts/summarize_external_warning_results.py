from __future__ import annotations

import argparse
import json
from pathlib import Path


def _load_contracts(path: Path) -> dict[str, dict[str, object]]:
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


def _load_results(result_dir: Path) -> dict[str, dict[str, object]]:
    results: dict[str, dict[str, object]] = {}
    for json_path in sorted(result_dir.glob("*.warning.json")):
        deck_name = json_path.name.replace(".warning.json", "")
        results[deck_name] = json.loads(json_path.read_text(encoding="utf-8"))
    return results


def build_summary_payload(
    contracts: dict[str, dict[str, object]],
    results: dict[str, dict[str, object]],
) -> dict[str, object]:
    decks: list[dict[str, object]] = []
    failures = 0
    for deck_name, config in sorted(contracts.items()):
        result = results.get(deck_name)
        if result is None:
            decks.append(
                {
                    "deck": deck_name,
                    "category": str(config["category"]),
                    "rationale": str(config["rationale"]),
                    "expected_warnings": list(config.get("expected_warnings", [])),
                    "forbidden_generated_deck_tokens": list(config.get("forbidden_generated_deck_tokens", [])),
                    "actual_warnings": [],
                    "present_forbidden_generated_deck_tokens": [],
                    "missing_expected_warnings": list(config.get("expected_warnings", [])),
                    "unexpected_warnings": [],
                    "backend": None,
                    "external_status_code": None,
                    "summary": "MISSING",
                }
            )
            failures += 1
            continue

        status = str(result.get("summary", "FAIL"))
        if status != "PASS":
            failures += 1
        decks.append(
            {
                "deck": deck_name,
                "category": str(config["category"]),
                "rationale": str(config["rationale"]),
                "expected_warnings": list(config.get("expected_warnings", [])),
                "forbidden_generated_deck_tokens": list(config.get("forbidden_generated_deck_tokens", [])),
                "actual_warnings": [str(entry) for entry in result.get("actual_warnings", [])],
                "present_forbidden_generated_deck_tokens": [
                    str(entry) for entry in result.get("present_forbidden_generated_deck_tokens", [])
                ],
                "missing_expected_warnings": [
                    str(entry) for entry in result.get("missing_expected_warnings", [])
                ],
                "unexpected_warnings": [str(entry) for entry in result.get("unexpected_warnings", [])],
                "backend": result.get("backend"),
                "external_status_code": result.get("external_status_code"),
                "summary": status,
            }
        )

    categories: dict[str, dict[str, object]] = {}
    for deck in decks:
        category_name = str(deck["category"])
        category = categories.setdefault(
            category_name,
            {
                "category": category_name,
                "deck_count": 0,
                "failures": 0,
                "failing_decks": [],
                "observed_warnings": [],
            },
        )
        category["deck_count"] = int(category["deck_count"]) + 1
        if str(deck["summary"]) != "PASS":
            category["failures"] = int(category["failures"]) + 1
            failing_decks = list(category["failing_decks"])
            failing_decks.append(str(deck["deck"]))
            category["failing_decks"] = failing_decks
        observed = set(category["observed_warnings"])
        observed.update(str(entry) for entry in deck.get("actual_warnings", []))
        category["observed_warnings"] = sorted(observed)

    return {
        "failures": failures,
        "decks": decks,
        "categories": [categories[name] for name in sorted(categories)],
    }


def validate_summary_payload(
    payload: dict[str, object],
    contracts: dict[str, dict[str, object]],
) -> list[str]:
    errors: list[str] = []
    decks = payload.get("decks")
    if not isinstance(decks, list):
        return ["summary payload is missing a decks list"]

    expected_decks = list(sorted(contracts))
    actual_decks = [str(deck.get("deck")) for deck in decks if isinstance(deck, dict)]
    if actual_decks != expected_decks:
        errors.append(f"summary deck set/order mismatch: expected {expected_decks}, got {actual_decks}")

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
        contract = contracts.get(deck_name)
        if contract is None:
            errors.append(f"summary contains unexpected deck entry: {deck_name}")
            continue
        if str(deck.get("category", "")).strip() != str(contract.get("category", "")).strip():
            errors.append(f"summary category mismatch for {deck_name}")
        if str(deck.get("rationale", "")).strip() != str(contract.get("rationale", "")).strip():
            errors.append(f"summary rationale mismatch for {deck_name}")
        if [str(entry) for entry in deck.get("expected_warnings", [])] != [
            str(entry) for entry in contract.get("expected_warnings", [])
        ]:
            errors.append(f"summary expected_warnings mismatch for {deck_name}")
        if [str(entry) for entry in deck.get("forbidden_generated_deck_tokens", [])] != [
            str(entry) for entry in contract.get("forbidden_generated_deck_tokens", [])
        ]:
            errors.append(f"summary forbidden_generated_deck_tokens mismatch for {deck_name}")

    categories = payload.get("categories")
    if not isinstance(categories, list):
        errors.append("summary payload is missing a categories list")
        return errors

    expected_category_names = sorted({str(config["category"]) for config in contracts.values()})
    actual_category_names = [str(entry.get("category")) for entry in categories if isinstance(entry, dict)]
    if actual_category_names != expected_category_names:
        errors.append(
            f"summary category set/order mismatch: expected {expected_category_names}, got {actual_category_names}"
        )
    return errors


def build_markdown_report(
    contracts: dict[str, dict[str, object]],
    results: dict[str, dict[str, object]],
) -> tuple[str, int]:
    payload = build_summary_payload(contracts, results)
    lines = [
        "# External Warning Summary",
        "",
        "| Deck | Category | Summary | Missing Expected | Unexpected | Forbidden Tokens Present | Actual Warnings |",
        "|------|----------|---------|------------------|------------|--------------------------|-----------------|",
    ]
    for deck in payload["decks"]:
        missing = ", ".join(deck["missing_expected_warnings"]) or "-"
        unexpected = ", ".join(deck["unexpected_warnings"]) or "-"
        forbidden_present = ", ".join(deck["present_forbidden_generated_deck_tokens"]) or "-"
        actual = ", ".join(deck["actual_warnings"]) or "-"
        lines.append(
            f"| {deck['deck']} | {deck['category']} | {deck['summary']} | {missing} | {unexpected} | {forbidden_present} | {actual} |"
        )

    lines.extend([
        "",
        "## Category Summary",
        "",
        "| Category | Deck Count | Failures | Failing Decks | Observed Warnings |",
        "|----------|------------|----------|---------------|-------------------|",
    ])
    for category in payload["categories"]:
        failing_decks = ", ".join(category["failing_decks"]) or "-"
        observed_warnings = ", ".join(category["observed_warnings"]) or "-"
        lines.append(
            f"| {category['category']} | {int(category['deck_count'])} | {int(category['failures'])} | {failing_decks} | {observed_warnings} |"
        )

    lines.extend(["", "## Contract Rationale", ""])
    for deck in payload["decks"]:
        lines.append(f"- {deck['deck']}: [{deck['category']}] {deck['rationale'] or 'no rationale recorded'}")

    lines.append("")
    lines.append(f"failures={payload['failures']}")
    return "\n".join(lines) + "\n", int(payload["failures"])


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Summarize external warning contract JSON results against the warning manifest.",
    )
    parser.add_argument(
        "--contracts",
        type=Path,
        default=Path("python/tests/benchmarks/phase6/external_warning_contracts.json"),
        help="Warning contract JSON file path",
    )
    parser.add_argument(
        "--result-dir",
        type=Path,
        default=Path("target/external-warning-results"),
        help="Directory containing *.warning.json outputs",
    )
    parser.add_argument("--markdown-output", type=Path, default=None, help="Optional markdown output path")
    parser.add_argument("--json-output", type=Path, default=None, help="Optional JSON summary output path")
    parser.add_argument(
        "--validate-pass",
        action="store_true",
        help="Fail unless the generated summary payload is internally consistent and all decks passed.",
    )
    args = parser.parse_args()

    contracts = _load_contracts(args.contracts)
    results = _load_results(args.result_dir)
    payload = build_summary_payload(contracts, results)
    markdown_report, failures = build_markdown_report(contracts, results)

    if args.markdown_output is not None:
        args.markdown_output.write_text(markdown_report, encoding="utf-8")
    else:
        print(markdown_report, end="")

    if args.json_output is not None:
        args.json_output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    if args.validate_pass:
        errors = validate_summary_payload(payload, contracts)
        if failures != 0:
            errors.append(f"external warning summary contains {failures} failing deck(s)")
        if errors:
            raise SystemExit("\n".join(errors))

    if failures != 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()