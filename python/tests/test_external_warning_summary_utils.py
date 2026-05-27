from __future__ import annotations

import importlib.util
from pathlib import Path


def _load_module(name: str, relative_script_path: str):
    script_path = Path(__file__).resolve().parents[1] / "scripts" / relative_script_path
    spec = importlib.util.spec_from_file_location(name, script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_check_module = _load_module("check_external_warning_contract", "check_external_warning_contract.py")
_summary_module = _load_module("summarize_external_warning_results", "summarize_external_warning_results.py")

extract_warning_markers = _check_module.extract_warning_markers
evaluate_warning_contract = _check_module.evaluate_warning_contract
build_summary_payload = _summary_module.build_summary_payload
validate_summary_payload = _summary_module.validate_summary_payload
build_markdown_report = _summary_module.build_markdown_report


def test_extract_warning_markers_filters_and_sorts_translation_notes() -> None:
    warnings = extract_warning_markers(
        "ok; external_josim_translation_warning:jj_b ; external_josim_translation_warning:jj_a ; ok"
    )

    assert warnings == [
        "external_josim_translation_warning:jj_a",
        "external_josim_translation_warning:jj_b",
    ]


def test_evaluate_warning_contract_reports_missing_and_unexpected_warnings() -> None:
    evaluation = evaluate_warning_contract(
        ["warn:a", "warn:b"],
        ["warn:b", "warn:c"],
        [],
        None,
        "external_completed",
        0,
    )

    assert evaluation["summary"] == "FAIL"
    assert evaluation["missing_expected_warnings"] == ["warn:a"]
    assert evaluation["unexpected_warnings"] == ["warn:c"]


def test_evaluate_warning_contract_reports_forbidden_generated_deck_tokens() -> None:
    evaluation = evaluate_warning_contract(
        ["warn:a"],
        ["warn:a"],
        ["icrit2="],
        ".model jjmod jj(icrit=0.5m icrit2=0.2m)",
        "external_completed",
        0,
    )

    assert evaluation["summary"] == "FAIL"
    assert evaluation["present_forbidden_generated_deck_tokens"] == ["icrit2="]


def test_build_summary_payload_preserves_contract_manifest_coverage() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["warn:a"], "forbidden_generated_deck_tokens": ["bad"]},
        "b.cir": {"category": "jj", "rationale": "b", "expected_warnings": ["warn:b"], "forbidden_generated_deck_tokens": []},
    }
    results = {
        "a.cir": {
            "summary": "PASS",
            "actual_warnings": ["warn:a"],
            "present_forbidden_generated_deck_tokens": [],
            "missing_expected_warnings": [],
            "unexpected_warnings": [],
            "backend": "external_completed",
            "external_status_code": 0,
        }
    }

    payload = build_summary_payload(contracts, results)

    assert payload["failures"] == 1
    assert [deck["deck"] for deck in payload["decks"]] == ["a.cir", "b.cir"]
    assert payload["decks"][1]["summary"] == "MISSING"
    assert payload["categories"][0]["failures"] == 1


def test_validate_summary_payload_reports_contract_mismatch() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["warn:a"], "forbidden_generated_deck_tokens": ["bad"]},
    }
    payload = {
        "failures": 0,
        "decks": [
            {
                "deck": "a.cir",
                "category": "delay",
                "rationale": "a",
                "expected_warnings": ["warn:a"],
                "forbidden_generated_deck_tokens": ["bad"],
                "summary": "PASS",
            }
        ],
        "categories": [{"category": "jj"}],
    }

    errors = validate_summary_payload(payload, contracts)

    assert "summary category mismatch for a.cir" in errors


def test_build_markdown_report_includes_warning_details() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["warn:a"], "forbidden_generated_deck_tokens": ["icrit2="]},
    }
    results = {
        "a.cir": {
            "summary": "FAIL",
            "actual_warnings": ["warn:b"],
            "present_forbidden_generated_deck_tokens": ["icrit2="],
            "missing_expected_warnings": ["warn:a"],
            "unexpected_warnings": ["warn:b"],
            "backend": "external_completed",
            "external_status_code": 0,
        }
    }

    markdown, failures = build_markdown_report(contracts, results)

    assert failures == 1
    assert "# External Warning Summary" in markdown
    assert "warn:a" in markdown
    assert "warn:b" in markdown
    assert "icrit2=" in markdown