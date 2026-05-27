from __future__ import annotations

import argparse
import difflib
import json
from pathlib import Path


def _load_json(path: Path) -> dict[str, object]:
    return json.loads(path.read_text(encoding="utf-8"))


def extract_timing_metrics(report: dict[str, object]) -> dict[str, float]:
    return {
        "worst_setup_slack_ps": float(report.get("worst_setup_slack_ps", 0.0)),
        "worst_hold_slack_ps": float(report.get("worst_hold_slack_ps", 0.0)),
    }


def _counterexample_count(report: dict[str, object], equivalent: bool) -> float:
    if equivalent:
        return 0.0

    total = 0
    for key in ["output_mismatches", "state_transition_mismatches", "mismatches", "counterexamples"]:
        value = report.get(key)
        if isinstance(value, list):
            total += len(value)

    # Keep a non-zero count when the report says non-equivalent but details are omitted.
    return float(total if total > 0 else 1)


def extract_verify_metrics(report: dict[str, object]) -> dict[str, float]:
    equivalent = bool(report.get("equivalent", False))
    return {
        "equivalence_pass_rate": 1.0 if equivalent else 0.0,
        "counterexample_count": _counterexample_count(report, equivalent),
    }


def extract_sim_metrics(summary: dict[str, object]) -> dict[str, float]:
    decks = summary.get("decks", [])
    worst = 0.0
    failing_decks = 0.0

    if isinstance(decks, list) and decks:
        for deck in decks:
            if not isinstance(deck, dict):
                continue
            deck_worst = deck.get("worst_max_abs_v")
            if deck_worst is not None:
                worst = max(worst, float(deck_worst))
            if str(deck.get("summary", "PASS")) != "PASS":
                failing_decks += 1.0
    else:
        worst = float(summary.get("worst_max_abs_v", 0.0))
        failing_decks = float(summary.get("failures", 0.0))

    return {
        "worst_max_abs_v": worst,
        "failing_deck_count": failing_decks,
    }


def build_quality_results(
    *,
    timing_report: dict[str, object],
    verify_report: dict[str, object],
    sim_summary: dict[str, object],
) -> dict[str, object]:
    return {
        "schema_version": 1,
        "kind": "quality-baseline-results",
        "suites": {
            "timing": extract_timing_metrics(timing_report),
            "verify": extract_verify_metrics(verify_report),
            "sim": extract_sim_metrics(sim_summary),
        },
    }


def _canonical_json(payload: dict[str, object]) -> str:
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def assert_payloads_match(expected: dict[str, object], actual: dict[str, object]) -> None:
    if expected == actual:
        return

    expected_text = _canonical_json(expected).splitlines(keepends=True)
    actual_text = _canonical_json(actual).splitlines(keepends=True)
    diff = "".join(
        difflib.unified_diff(
            expected_text,
            actual_text,
            fromfile="expected",
            tofile="actual",
        )
    )
    raise ValueError(f"quality results mismatch:\n{diff}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Collect timing/verify/sim metrics into a quality-baseline-results JSON payload.",
    )
    parser.add_argument("--timing-report", type=Path, required=True)
    parser.add_argument("--verify-report", type=Path, required=True)
    parser.add_argument("--sim-summary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check-against", type=Path, default=None)
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]

    timing_path = args.timing_report if args.timing_report.is_absolute() else (repo_root / args.timing_report)
    verify_path = args.verify_report if args.verify_report.is_absolute() else (repo_root / args.verify_report)
    sim_path = args.sim_summary if args.sim_summary.is_absolute() else (repo_root / args.sim_summary)
    output_path = args.output if args.output.is_absolute() else (repo_root / args.output)
    check_path = None if args.check_against is None else (args.check_against if args.check_against.is_absolute() else (repo_root / args.check_against))

    payload = build_quality_results(
        timing_report=_load_json(timing_path),
        verify_report=_load_json(verify_path),
        sim_summary=_load_json(sim_path),
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    if check_path is not None:
        expected = _load_json(check_path)
        try:
            assert_payloads_match(expected, payload)
        except ValueError as exc:
            raise SystemExit(str(exc)) from exc


if __name__ == "__main__":
    main()
