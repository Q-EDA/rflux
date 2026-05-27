from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "export_cli_command_surface.py"
    spec = importlib.util.spec_from_file_location("export_cli_command_surface", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_surface_payload = _module.build_surface_payload
assert_surfaces_match = _module.assert_surfaces_match


def test_build_surface_payload_contains_compile_netlist_options() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    payload = build_surface_payload(repo_root / "crates" / "cli" / "src" / "main.rs")

    assert payload["kind"] == "cli_command_surface"
    assert payload["schema_version"] == 1

    commands = {entry["command"]: entry for entry in payload["commands"]}
    compile_netlist = commands["compile-netlist"]
    option_names = [option["long"] for option in compile_netlist["options"]]
    assert "input" in option_names
    assert "input-format" in option_names
    assert "netlist-output" in option_names


def test_assert_surfaces_match_reports_diff() -> None:
    expected = {
        "schema_version": 1,
        "kind": "cli_command_surface",
        "source": "crates/cli/src/main.rs",
        "command_count": 1,
        "commands": [
            {
                "command": "compile-netlist",
                "variant": "CompileNetlist",
                "args_struct": "CompileNetlistArgs",
                "option_count": 1,
                "options": [{"long": "input", "required": True}],
            }
        ],
    }
    actual = {
        "schema_version": 1,
        "kind": "cli_command_surface",
        "source": "crates/cli/src/main.rs",
        "command_count": 1,
        "commands": [
            {
                "command": "compile-netlist",
                "variant": "CompileNetlist",
                "args_struct": "CompileNetlistArgs",
                "option_count": 1,
                "options": [{"long": "output", "required": False}],
            }
        ],
    }

    try:
        assert_surfaces_match(expected, actual)
    except ValueError as exc:
        message = str(exc)
        assert "CLI command surface mismatch" in message
        assert '"long": "input"' in message
        assert '"long": "output"' in message
    else:
        raise AssertionError("expected ValueError for contract mismatch")


def test_surface_payload_matches_repo_contract_file() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    baseline_path = repo_root / "python" / "tests" / "contracts" / "cli_command_surface.json"
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    current = build_surface_payload(repo_root / "crates" / "cli" / "src" / "main.rs")

    assert_surfaces_match(baseline, current)
