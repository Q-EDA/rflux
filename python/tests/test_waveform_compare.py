from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest


def test_waveform_compare_script_against_external_josim_when_available() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    if josim_override:
        josim = josim_override
    else:
        josim = shutil.which("josim")
    if josim is None:
        pytest.skip("josim command not found; skipping external waveform comparison")

    repo_root = Path(__file__).resolve().parents[2]
    script_path = repo_root / "python" / "scripts" / "compare_internal_external_waveforms.py"
    benchmark_dir = repo_root / "python" / "tests" / "benchmarks" / "phase6"
    thresholds_path = benchmark_dir / "waveform_thresholds.json"

    thresholds = json.loads(thresholds_path.read_text(encoding="utf-8"))

    for deck_name, config in thresholds.items():
        deck_path = benchmark_dir / deck_name
        threshold = float(config["max_abs_threshold_v"])
        json_output_path = benchmark_dir / f"{deck_name}.compare.json"
        if json_output_path.exists():
            json_output_path.unlink()

        completed = subprocess.run(
            [
                sys.executable,
                str(script_path),
                str(deck_path),
                "--josim-command",
                josim,
                "--max-abs-threshold",
                str(threshold),
                "--json-output",
                str(json_output_path),
            ],
            cwd=str(repo_root),
            capture_output=True,
            text=True,
        )

        assert completed.returncode == 0, completed.stderr + "\n" + completed.stdout
        assert "summary=PASS" in completed.stdout, completed.stdout
        payload = json.loads(json_output_path.read_text(encoding="utf-8"))
        assert payload["summary"] == "PASS"
        assert float(payload["threshold"]) == threshold
        assert float(payload["worst_max_abs_v"]) <= threshold
