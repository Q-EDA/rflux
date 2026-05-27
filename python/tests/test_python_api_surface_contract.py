from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "export_python_api_surface.py"
    spec = importlib.util.spec_from_file_location("export_python_api_surface", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_surface_payload = _module.build_surface_payload
assert_surfaces_match = _module.assert_surfaces_match
DEFAULT_MODULES = _module.DEFAULT_MODULES


def test_build_surface_payload_covers_default_modules() -> None:
    payload = build_surface_payload(DEFAULT_MODULES)

    assert payload["kind"] == "python_api_surface"
    assert payload["schema_version"] == 1
    modules = payload["modules"]
    assert list(modules) == DEFAULT_MODULES

    rflux_surface = modules["rflux"]
    assert "compile_layout" in rflux_surface["exported"]
    assert "flow" in rflux_surface["exported"]


def test_assert_surfaces_match_reports_diff() -> None:
    expected = {
        "schema_version": 1,
        "kind": "python_api_surface",
        "modules": {
            "rflux": {
                "module": "rflux",
                "exported_count": 1,
                "exported": ["foo"],
                "symbols": [{"name": "foo", "kind": "value", "type": "int"}],
            }
        },
    }
    actual = {
        "schema_version": 1,
        "kind": "python_api_surface",
        "modules": {
            "rflux": {
                "module": "rflux",
                "exported_count": 1,
                "exported": ["bar"],
                "symbols": [{"name": "bar", "kind": "value", "type": "int"}],
            }
        },
    }

    try:
        assert_surfaces_match(expected, actual)
    except ValueError as exc:
        message = str(exc)
        assert "python API surface mismatch" in message
        assert "-        \"foo\"" in message
        assert "+        \"bar\"" in message
    else:
        raise AssertionError("expected ValueError for contract mismatch")


def test_surface_payload_matches_repo_contract_file() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    baseline_path = repo_root / "python" / "tests" / "contracts" / "python_api_surface.json"
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    current = build_surface_payload(DEFAULT_MODULES)

    assert_surfaces_match(baseline, current)
