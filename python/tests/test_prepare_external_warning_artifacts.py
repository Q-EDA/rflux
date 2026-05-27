from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "prepare_external_warning_artifacts.py"
    spec = importlib.util.spec_from_file_location("prepare_external_warning_artifacts", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
prepare_external_warning_artifacts = _module.prepare_external_warning_artifacts


def test_prepare_external_warning_artifacts_writes_expected_bundle(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    result_dir = repo_root / "target" / "external-warning-manifest"
    artifact_dir = repo_root / "target" / "external-warning-review"
    result_dir.mkdir(parents=True)

    contracts = {
        "jj_second_harmonic_warning_smoke.cir": {
            "category": "josephson-junction",
            "rationale": "warning-only unsupported second harmonic without primary icrit",
            "expected_warnings": [
                "external_josim_translation_warning:jj_second_harmonic_instance_unsupported"
            ],
            "forbidden_generated_deck_tokens": ["icrit2="],
        }
    }
    (result_dir / "selected_external_warning_contracts.json").write_text(
        json.dumps(contracts, indent=2) + "\n",
        encoding="utf-8",
    )
    (result_dir / "jj_second_harmonic_warning_smoke.cir.warning.json").write_text(
        json.dumps(
            {
                "deck": "jj_second_harmonic_warning_smoke.cir",
                "summary": "PASS",
                "backend": "external_completed",
                "external_status_code": 0,
                "expected_warnings": contracts["jj_second_harmonic_warning_smoke.cir"]["expected_warnings"],
                "actual_warnings": contracts["jj_second_harmonic_warning_smoke.cir"]["expected_warnings"],
                "forbidden_generated_deck_tokens": contracts["jj_second_harmonic_warning_smoke.cir"]["forbidden_generated_deck_tokens"],
                "present_forbidden_generated_deck_tokens": [],
                "missing_expected_warnings": [],
                "unexpected_warnings": [],
            },
            indent=2,
        ) + "\n",
        encoding="utf-8",
    )
    summary_payload = {
        "failures": 0,
        "decks": [
            {
                "deck": "jj_second_harmonic_warning_smoke.cir",
                "category": "josephson-junction",
                "rationale": "warning-only unsupported second harmonic without primary icrit",
                "expected_warnings": contracts["jj_second_harmonic_warning_smoke.cir"]["expected_warnings"],
                "forbidden_generated_deck_tokens": contracts["jj_second_harmonic_warning_smoke.cir"]["forbidden_generated_deck_tokens"],
                "actual_warnings": contracts["jj_second_harmonic_warning_smoke.cir"]["expected_warnings"],
                "present_forbidden_generated_deck_tokens": [],
                "missing_expected_warnings": [],
                "unexpected_warnings": [],
                "backend": "external_completed",
                "external_status_code": 0,
                "summary": "PASS",
            }
        ],
        "categories": [
            {
                "category": "josephson-junction",
                "deck_count": 1,
                "failures": 0,
                "failing_decks": [],
                "observed_warnings": contracts["jj_second_harmonic_warning_smoke.cir"]["expected_warnings"],
            }
        ],
    }
    (result_dir / "external_warning_summary.json").write_text(
        json.dumps(summary_payload, indent=2) + "\n",
        encoding="utf-8",
    )
    (result_dir / "external_warning_summary.md").write_text("# External Warning Summary\n", encoding="utf-8")

    manifest = prepare_external_warning_artifacts(
        repo_root=repo_root,
        result_dir=result_dir,
        artifact_dir=artifact_dir,
        josim_command="josim",
        github_context={
            "workflow": "ci",
            "job": "waveform-compare-optional",
            "event_name": "workflow_dispatch",
            "run_id": "1",
            "run_attempt": "1",
            "sha": "deadbeef",
            "ref_name": "main",
        },
    )

    assert (artifact_dir / "external_warning_summary.current.md").exists()
    assert (artifact_dir / "external_warning_summary.current.json").exists()
    assert (artifact_dir / "selected_external_warning_contracts.json").exists()
    assert (artifact_dir / "jj_second_harmonic_warning_smoke.cir.warning.json").exists()
    assert (artifact_dir / "README.txt").exists()
    assert (artifact_dir / "manifest.json").exists()

    manifest_payload = json.loads((artifact_dir / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["kind"] == "external-warning-artifacts"
    assert manifest_payload["summary_overview"]["deck_count"] == 1
    assert manifest_payload["summary_overview"]["failure_count"] == 0
    assert manifest_payload["github_actions_context"]["job"] == "waveform-compare-optional"
    assert {entry["name"] for entry in manifest_payload["artifact_files"]} == {
        "external_warning_summary.current.md",
        "external_warning_summary.current.json",
        "selected_external_warning_contracts.json",
        "jj_second_harmonic_warning_smoke.cir.warning.json",
    }