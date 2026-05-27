from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "check_waveform_baseline_status.py"
    spec = importlib.util.spec_from_file_location("check_waveform_baseline_status", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
check_waveform_baseline_status = _module.check_waveform_baseline_status


def test_check_waveform_baseline_status_reports_ready_when_files_exist_and_failures_zero(tmp_path: Path) -> None:
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir(parents=True)
    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.json").write_text(
        json.dumps({"failures": 0}),
        encoding="utf-8",
    )
    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.md").write_text("ok\n", encoding="utf-8")

    status = check_waveform_baseline_status(
        repo_root=tmp_path,
        benchmark_dir=benchmark_dir,
        platform_key="linux",
    )

    assert status["ready"] is True
    assert status["reason"] == "baseline ready"


def test_check_waveform_baseline_status_reports_not_ready_when_markdown_missing(tmp_path: Path) -> None:
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir(parents=True)
    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.json").write_text(
        json.dumps({"failures": 0}),
        encoding="utf-8",
    )

    status = check_waveform_baseline_status(
        repo_root=tmp_path,
        benchmark_dir=benchmark_dir,
        platform_key="linux",
    )

    assert status["ready"] is False
    assert status["reason"] == "missing baseline markdown"


def test_check_waveform_baseline_status_reports_not_ready_when_failures_positive(tmp_path: Path) -> None:
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir(parents=True)
    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.json").write_text(
        json.dumps({"failures": 2}),
        encoding="utf-8",
    )
    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.md").write_text("bad\n", encoding="utf-8")

    status = check_waveform_baseline_status(
        repo_root=tmp_path,
        benchmark_dir=benchmark_dir,
        platform_key="linux",
    )

    assert status["ready"] is False
    assert status["reason"] == "baseline json contains failures"
