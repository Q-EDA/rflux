from __future__ import annotations

import importlib.util
from pathlib import Path


def _load_runner_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "run_external_warning_manifest.py"
    spec = importlib.util.spec_from_file_location("run_external_warning_manifest", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_runner_module = _load_runner_module()
load_warning_contracts = _runner_module.load_warning_contracts
select_warning_contracts = _runner_module.select_warning_contracts


def test_load_warning_contracts_normalizes_expected_warnings(tmp_path: Path) -> None:
    manifest_path = tmp_path / "contracts.json"
    manifest_path.write_text(
        '{"demo.cir": {"category": "jj", "rationale": "demo", "expected_warnings": [1, "warn"]}}',
        encoding="utf-8",
    )

    contracts = load_warning_contracts(manifest_path)

    assert contracts == {
        "demo.cir": {
            "category": "jj",
            "rationale": "demo",
            "expected_warnings": ["1", "warn"],
            "forbidden_generated_deck_tokens": [],
        }
    }


def test_select_warning_contracts_filters_by_deck_and_preserves_manifest_order() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["wa"]},
        "b.cir": {"category": "delay", "rationale": "b", "expected_warnings": ["wb"]},
        "c.cir": {"category": "jj", "rationale": "c", "expected_warnings": ["wc"]},
    }

    selected = select_warning_contracts(contracts, ["c.cir", "a.cir"], None)

    assert list(selected) == ["a.cir", "c.cir"]


def test_select_warning_contracts_filters_by_category() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["wa"]},
        "b.cir": {"category": "delay", "rationale": "b", "expected_warnings": ["wb"]},
        "c.cir": {"category": "jj", "rationale": "c", "expected_warnings": ["wc"]},
    }

    selected = select_warning_contracts(contracts, None, ["jj"])

    assert list(selected) == ["a.cir", "c.cir"]


def test_select_warning_contracts_uses_intersection_when_both_filters_are_present() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["wa"]},
        "b.cir": {"category": "delay", "rationale": "b", "expected_warnings": ["wb"]},
        "c.cir": {"category": "jj", "rationale": "c", "expected_warnings": ["wc"]},
    }

    selected = select_warning_contracts(contracts, ["a.cir", "b.cir"], ["jj"])

    assert list(selected) == ["a.cir"]


def test_select_warning_contracts_reports_unknown_deck_filter() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["wa"]},
    }

    try:
        select_warning_contracts(contracts, ["missing.cir"], None)
    except ValueError as exc:
        assert "unknown deck filters" in str(exc)
    else:
        raise AssertionError("expected ValueError for unknown deck filter")


def test_select_warning_contracts_reports_empty_selection() -> None:
    contracts = {
        "a.cir": {"category": "jj", "rationale": "a", "expected_warnings": ["wa"]},
    }

    try:
        select_warning_contracts(contracts, None, ["delay"])
    except ValueError as exc:
        assert "no decks selected" in str(exc)
    else:
        raise AssertionError("expected ValueError for empty selection")


def test_select_warning_contracts_allows_empty_manifest_without_filters() -> None:
    selected = select_warning_contracts({}, None, None)

    assert selected == {}