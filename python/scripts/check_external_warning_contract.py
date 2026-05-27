from __future__ import annotations

import argparse
import json
from pathlib import Path

import rflux


WARNING_PREFIX = "external_josim_translation_warning:"


def extract_warning_markers(external_result: str | None) -> list[str]:
    if not external_result:
        return []

    warnings = {
        token.strip()
        for token in str(external_result).split(";")
        if token.strip().startswith(WARNING_PREFIX)
    }
    return sorted(warnings)


def evaluate_warning_contract(
    expected_warnings: list[str],
    actual_warnings: list[str],
    forbidden_generated_deck_tokens: list[str],
    generated_deck_text: str | None,
    backend: str,
    external_status_code: int | None,
) -> dict[str, object]:
    expected = list(dict.fromkeys(str(entry) for entry in expected_warnings))
    actual = list(dict.fromkeys(str(entry) for entry in actual_warnings))
    actual_set = set(actual)
    expected_set = set(expected)
    missing_expected_warnings = [entry for entry in expected if entry not in actual_set]
    unexpected_warnings = [entry for entry in actual if entry not in expected_set]
    forbidden_tokens = list(dict.fromkeys(str(entry) for entry in forbidden_generated_deck_tokens))
    lowered_generated_deck = (generated_deck_text or "").lower()
    present_forbidden_generated_deck_tokens = [
        entry for entry in forbidden_tokens if entry.lower() in lowered_generated_deck
    ]

    failure_reasons: list[str] = []
    if backend != "external_completed":
        failure_reasons.append(f"backend={backend}")
    if external_status_code != 0:
        failure_reasons.append(f"external_status_code={external_status_code}")
    if missing_expected_warnings:
        failure_reasons.append("missing_expected_warnings")
    if unexpected_warnings:
        failure_reasons.append("unexpected_warnings")
    if present_forbidden_generated_deck_tokens:
        failure_reasons.append("forbidden_generated_deck_tokens_present")

    return {
        "summary": "PASS" if not failure_reasons else "FAIL",
        "missing_expected_warnings": missing_expected_warnings,
        "unexpected_warnings": unexpected_warnings,
        "present_forbidden_generated_deck_tokens": present_forbidden_generated_deck_tokens,
        "failure_reasons": failure_reasons,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run an external_josim warning contract against a single benchmark deck.",
    )
    parser.add_argument("deck", type=Path, help="Path to the benchmark deck file")
    parser.add_argument(
        "--josim-command",
        type=str,
        default="josim",
        help="External simulator command used for external_josim runs.",
    )
    parser.add_argument(
        "--expected-warning",
        action="append",
        default=None,
        help="Expected external_josim_translation_warning:* marker. May be repeated.",
    )
    parser.add_argument(
        "--forbidden-generated-deck-token",
        action="append",
        default=None,
        help="Token that must not appear in the generated external deck. May be repeated.",
    )
    parser.add_argument(
        "--json-output",
        type=Path,
        default=None,
        help="Optional JSON output path.",
    )
    args = parser.parse_args()

    report = rflux.simulate_file(
        str(args.deck),
        simulation_mode="external_josim",
        external_command=args.josim_command,
    )
    actual_warnings = extract_warning_markers(report.external_result)
    generated_deck_text = None
    if report.generated_deck_path:
        generated_deck_text = Path(report.generated_deck_path).read_text(encoding="utf-8")
    evaluation = evaluate_warning_contract(
        list(args.expected_warning or []),
        actual_warnings,
        list(args.forbidden_generated_deck_token or []),
        generated_deck_text,
        str(report.backend),
        report.external_status_code,
    )
    payload = {
        "deck": args.deck.name,
        "summary": evaluation["summary"],
        "backend": str(report.backend),
        "external_status_code": report.external_status_code,
        "expected_warnings": list(args.expected_warning or []),
        "actual_warnings": actual_warnings,
        "forbidden_generated_deck_tokens": list(args.forbidden_generated_deck_token or []),
        "present_forbidden_generated_deck_tokens": evaluation["present_forbidden_generated_deck_tokens"],
        "missing_expected_warnings": evaluation["missing_expected_warnings"],
        "unexpected_warnings": evaluation["unexpected_warnings"],
        "failure_reasons": evaluation["failure_reasons"],
        "generated_deck_path": report.generated_deck_path,
        "waveform_path": report.waveform_path,
        "external_result": report.external_result,
    }

    text = json.dumps(payload, indent=2)
    if args.json_output is not None:
        args.json_output.write_text(text + "\n", encoding="utf-8")
    print(text)

    if payload["summary"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()