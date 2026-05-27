from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "export_report_schema_surface.py"
    spec = importlib.util.spec_from_file_location("export_report_schema_surface", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
build_surface_payload = _module.build_surface_payload
assert_surfaces_match = _module.assert_surfaces_match


def test_build_surface_payload_contains_core_report_kinds() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    payload = build_surface_payload(
        repo_root=repo_root,
        cli_source=Path("crates/cli/src/main.rs"),
        python_scripts_dir=Path("python/scripts"),
    )

    assert payload["kind"] == "report_schema_surface"
    assert payload["schema_version"] == 1
    assert payload["cli"]["schema_version_constant"] == 1

    cli_kinds = payload["cli"]["report_kinds"]
    assert "compile_layout" in cli_kinds
    assert "timing_analysis" in cli_kinds
    assert "diagnostics_bundle" in cli_kinds


def test_assert_surfaces_match_reports_diff() -> None:
    expected = {
        "schema_version": 1,
        "kind": "report_schema_surface",
        "cli": {
            "source": "crates/cli/src/main.rs",
            "schema_version_constant": 1,
            "report_kind_count": 1,
            "report_kinds": ["compile_layout"],
        },
        "python_artifact_manifests": {
            "scripts_dir": "python/scripts",
            "manifest_count": 1,
            "manifests": [
                {
                    "script": "python/scripts/prepare_release_artifacts.py",
                    "kind": "release-candidate-artifacts",
                    "schema_version": 1,
                }
            ],
        },
    }
    actual = {
        "schema_version": 1,
        "kind": "report_schema_surface",
        "cli": {
            "source": "crates/cli/src/main.rs",
            "schema_version_constant": 1,
            "report_kind_count": 1,
            "report_kinds": ["simulate_file"],
        },
        "python_artifact_manifests": {
            "scripts_dir": "python/scripts",
            "manifest_count": 1,
            "manifests": [
                {
                    "script": "python/scripts/prepare_release_artifacts.py",
                    "kind": "release-candidate-artifacts",
                    "schema_version": 1,
                }
            ],
        },
    }

    try:
        assert_surfaces_match(expected, actual)
    except ValueError as exc:
        message = str(exc)
        assert "report schema surface mismatch" in message
        assert '"compile_layout"' in message
        assert '"simulate_file"' in message
    else:
        raise AssertionError("expected ValueError for contract mismatch")


def test_surface_payload_matches_repo_contract_file() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    baseline_path = repo_root / "python" / "tests" / "contracts" / "report_schema_surface.json"
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    current = build_surface_payload(
        repo_root=repo_root,
        cli_source=Path("crates/cli/src/main.rs"),
        python_scripts_dir=Path("python/scripts"),
    )

    assert_surfaces_match(baseline, current)
