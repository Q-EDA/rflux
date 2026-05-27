from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_summary_module():
    from pathlib import Path

    script_path = Path(__file__).resolve().parents[1] / "scripts" / "summarize_waveform_compare_results.py"
    spec = importlib.util.spec_from_file_location("summarize_waveform_compare_results", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_summary_module = _load_summary_module()
_load_results = _summary_module._load_results
build_summary_payload = _summary_module.build_summary_payload
build_markdown_report = _summary_module.build_markdown_report
validate_no_regression = _summary_module.validate_no_regression
validate_summary_payload = _summary_module.validate_summary_payload


def test_load_results_normalizes_cir_suffix_from_compare_json_names(tmp_path: Path) -> None:
    payload = {"summary": "PASS", "worst_max_abs_v": 0.01, "failing_nodes": []}
    (tmp_path / "a.cir.compare.json").write_text(json.dumps(payload), encoding="utf-8")

    results = _load_results(tmp_path)

    assert list(results) == ["a"]
    assert results["a"]["summary"] == "PASS"


def test_build_markdown_report_counts_missing_and_failures() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
        "b.cir": {"max_abs_threshold_v": 0.1, "category": "jj", "rationale": "nonlinear"},
        "c.cir": {"max_abs_threshold_v": 0.1, "category": "mutual", "rationale": "missing result"},
    }
    results = {
        "a": {"worst_max_abs_v": 0.05},
        "b": {"worst_max_abs_v": 0.2},
    }

    report, failures = build_markdown_report(thresholds, results)

    assert failures == 2
    assert "| a.cir |" in report
    assert "| b.cir |" in report
    assert "| c.cir |" in report
    assert "MISSING" in report
    assert "FAIL" in report
    assert "missing compare result" in report
    assert "Category Summary" in report
    assert "Threshold Rationale" in report
    assert "[jj] nonlinear" in report


def test_build_summary_payload_keeps_failing_node_details() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
        "b.cir": {"max_abs_threshold_v": 0.1, "category": "jj", "rationale": "nonlinear"},
    }
    results = {
        "a": {
            "summary": "PASS",
            "worst_max_abs_v": 0.05,
            "failing_nodes": [],
        },
        "b": {
            "summary": "FAIL",
            "worst_max_abs_v": 0.2,
            "failing_nodes": ["n1", "n2"],
            "top_worst_nodes": [{"node": "n2", "max_abs_v": 0.2, "rms_v": 0.1}],
        },
    }

    payload = build_summary_payload(thresholds, results)

    assert payload["failures"] == 1
    assert payload["decks"][1]["deck"] == "b.cir"
    assert payload["decks"][1]["summary"] == "FAIL"
    assert payload["decks"][1]["failing_nodes"] == ["n1", "n2"]
    assert payload["decks"][1]["category"] == "jj"
    assert payload["decks"][1]["rationale"] == "nonlinear"
    assert payload["decks"][1]["top_worst_nodes"][0]["node"] == "n2"
    assert payload["categories"][0]["category"] == "jj"
    assert payload["categories"][0]["failures"] == 1
    assert payload["categories"][0]["top_hotspots"][0]["node"] == "n2"


def test_build_markdown_report_lists_failing_nodes() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
    }
    results = {
        "a": {
            "summary": "FAIL",
            "worst_max_abs_v": 0.2,
            "failing_nodes": ["n1", "n2"],
        },
    }

    report, failures = build_markdown_report(thresholds, results)

    assert failures == 1
    assert "| a.cir | passive | 1.000000e-01 | 2.000000e-01 | FAIL | n1, n2 |" in report


def test_build_summary_payload_preserves_threshold_manifest_coverage() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
        "b.cir": {"max_abs_threshold_v": 0.2, "category": "delay", "rationale": "transport"},
    }
    results = {
        "a": {"summary": "PASS", "worst_max_abs_v": 0.05, "failing_nodes": []},
        "b": {"summary": "PASS", "worst_max_abs_v": 0.15, "failing_nodes": []},
    }

    payload = build_summary_payload(thresholds, results)

    assert [deck["deck"] for deck in payload["decks"]] == ["a.cir", "b.cir"]
    assert [deck["threshold_v"] for deck in payload["decks"]] == [0.1, 0.2]
    assert payload["failures"] == 0
    assert [category["category"] for category in payload["categories"]] == ["delay", "passive"]


def test_build_markdown_report_uses_top_worst_nodes_for_passing_decks() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
    }
    results = {
        "a": {
            "summary": "PASS",
            "worst_max_abs_v": 0.05,
            "failing_nodes": [],
            "top_worst_nodes": [
                {"node": "out", "max_abs_v": 0.05, "rms_v": 0.02},
                {"node": "tap", "max_abs_v": 0.04, "rms_v": 0.01},
            ],
        },
    }

    report, failures = build_markdown_report(thresholds, results)

    assert failures == 0
    assert "| a.cir | passive | 1.000000e-01 | 5.000000e-02 | PASS | out:5.000e-02, tap:4.000e-02 |" in report


def test_validate_summary_payload_accepts_consistent_all_pass_payload() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
    }
    payload = {
        "failures": 0,
        "decks": [
            {
                "deck": "a.cir",
                "threshold_v": 0.1,
                "category": "passive",
                "rationale": "sanity",
                "worst_max_abs_v": 0.05,
                "summary": "PASS",
                "failing_nodes": [],
            }
        ],
        "categories": [
            {
                "category": "passive",
                "deck_count": 1,
                "failures": 0,
                "worst_max_abs_v": 0.05,
                "worst_deck": "a.cir",
                "top_hotspots": [],
            }
        ],
    }

    assert validate_summary_payload(payload, thresholds) == []


def test_validate_summary_payload_reports_manifest_drift() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
        "b.cir": {"max_abs_threshold_v": 0.2, "category": "delay", "rationale": "transport"},
    }
    payload = {
        "failures": 0,
        "decks": [
            {
                "deck": "a.cir",
                "threshold_v": 0.1,
                "category": "wrong",
                "rationale": "sanity",
                "worst_max_abs_v": 0.05,
                "summary": "PASS",
                "failing_nodes": [],
            }
        ],
        "categories": [
            {
                "category": "wrong",
                "deck_count": 1,
                "failures": 0,
                "worst_max_abs_v": 0.05,
                "worst_deck": "a.cir",
                "top_hotspots": [],
            }
        ],
    }

    errors = validate_summary_payload(payload, thresholds)

    assert any("summary deck set/order mismatch" in error for error in errors)
    assert any("summary category mismatch for a.cir" in error for error in errors)
    assert any("summary category set/order mismatch" in error for error in errors)


def test_build_markdown_report_lists_category_rollup() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
        "b.cir": {"max_abs_threshold_v": 0.2, "category": "passive", "rationale": "transport"},
        "c.cir": {"max_abs_threshold_v": 0.2, "category": "jj", "rationale": "nonlinear"},
    }
    results = {
        "a": {
            "summary": "PASS",
            "worst_max_abs_v": 0.05,
            "failing_nodes": [],
            "top_worst_nodes": [{"node": "tap", "max_abs_v": 0.05, "rms_v": 0.02}],
        },
        "b": {
            "summary": "FAIL",
            "worst_max_abs_v": 0.15,
            "failing_nodes": ["out"],
            "top_worst_nodes": [{"node": "out", "max_abs_v": 0.15, "rms_v": 0.08}],
        },
        "c": {
            "summary": "PASS",
            "worst_max_abs_v": 0.12,
            "failing_nodes": [],
            "top_worst_nodes": [{"node": "jj", "max_abs_v": 0.12, "rms_v": 0.06}],
        },
    }

    report, failures = build_markdown_report(thresholds, results)

    assert failures == 1
    assert "| passive | 2 | 1 | 1.500000e-01 | b.cir | b.cir::out:1.500e-01, a.cir::tap:5.000e-02 |" in report
    assert "| jj | 1 | 0 | 1.200000e-01 | c.cir | c.cir::jj:1.200e-01 |" in report


def test_build_summary_payload_includes_history_diff_when_previous_summary_provided() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
        "b.cir": {"max_abs_threshold_v": 0.2, "category": "jj", "rationale": "nonlinear"},
    }
    results = {
        "a": {"summary": "PASS", "worst_max_abs_v": 0.05, "failing_nodes": []},
        "b": {"summary": "FAIL", "worst_max_abs_v": 0.22, "failing_nodes": ["out"]},
    }
    previous_summary = {
        "failures": 0,
        "decks": [
            {"deck": "a.cir", "summary": "PASS", "worst_max_abs_v": 0.04},
            {"deck": "b.cir", "summary": "PASS", "worst_max_abs_v": 0.18},
        ],
        "categories": [
            {"category": "jj", "failures": 0, "worst_max_abs_v": 0.18},
            {"category": "passive", "failures": 0, "worst_max_abs_v": 0.04},
        ],
    }

    payload = build_summary_payload(thresholds, results, previous_summary)

    assert payload["history_diff"]["failure_delta"] == 1
    assert payload["history_diff"]["deck_changes"][0]["deck"] == "a.cir"
    assert payload["history_diff"]["deck_changes"][1]["deck"] == "b.cir"
    assert payload["history_diff"]["deck_changes"][1]["previous_summary"] == "PASS"
    assert payload["history_diff"]["deck_changes"][1]["current_summary"] == "FAIL"
    assert payload["history_diff"]["category_changes"][0]["category"] == "jj"


def test_build_markdown_report_includes_history_diff_section() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "passive", "rationale": "sanity"},
    }
    results = {
        "a": {
            "summary": "FAIL",
            "worst_max_abs_v": 0.12,
            "failing_nodes": ["out"],
        },
    }
    previous_summary = {
        "failures": 0,
        "decks": [
            {"deck": "a.cir", "summary": "PASS", "worst_max_abs_v": 0.05},
        ],
        "categories": [
            {"category": "passive", "failures": 0, "worst_max_abs_v": 0.05},
        ],
    }

    report, failures = build_markdown_report(thresholds, results, previous_summary)

    assert failures == 1
    assert "## History Diff" in report
    assert "failures: 1 (delta +1 vs previous 0)" in report
    assert "| a.cir | PASS -> FAIL | +7.000000e-02 |" in report
    assert "| passive | +1 | +7.000000e-02 |" in report


def test_validate_no_regression_accepts_equal_or_improved_payload() -> None:
    previous_summary = {
        "decks": [
            {"deck": "a.cir", "summary": "PASS", "worst_max_abs_v": 0.05},
        ],
        "categories": [
            {"category": "passive", "failures": 0, "worst_max_abs_v": 0.05},
        ],
    }
    current_summary = {
        "decks": [
            {"deck": "a.cir", "summary": "PASS", "worst_max_abs_v": 0.04},
        ],
        "categories": [
            {"category": "passive", "failures": 0, "worst_max_abs_v": 0.04},
        ],
    }

    assert validate_no_regression(current_summary, previous_summary, 0.0) == []


def test_validate_no_regression_reports_worsened_deck_delta_and_summary() -> None:
    previous_summary = {
        "decks": [
            {"deck": "a.cir", "summary": "PASS", "worst_max_abs_v": 0.05},
        ],
        "categories": [
            {"category": "passive", "failures": 0, "worst_max_abs_v": 0.05},
        ],
    }
    current_summary = {
        "decks": [
            {"deck": "a.cir", "summary": "FAIL", "worst_max_abs_v": 0.08},
        ],
        "categories": [
            {"category": "passive", "failures": 1, "worst_max_abs_v": 0.08},
        ],
    }

    errors = validate_no_regression(current_summary, previous_summary, 0.01)

    assert any("summary worsened PASS -> FAIL" in error for error in errors)
    assert any("deck regression for a.cir: worst_max_abs_v delta +3.000000e-02 exceeds tolerance 1.000000e-02" in error for error in errors)
    assert any("category regression for passive: failures increased 0 -> 1" in error for error in errors)
