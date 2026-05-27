from __future__ import annotations

import importlib.util
from pathlib import Path


def _load_compare_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "compare_internal_external_waveforms.py"
    spec = importlib.util.spec_from_file_location("compare_internal_external_waveforms", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_compare_module = _load_compare_module()
compare_waveforms = _compare_module.compare_waveforms
interpolate_trace = _compare_module.interpolate_trace
rank_nodes_by_error = _compare_module.rank_nodes_by_error
read_waveform_csv = _compare_module.read_waveform_csv
summarize_metrics = _compare_module.summarize_metrics


def test_interpolate_trace_linearly() -> None:
    time_ps = [0.0, 10.0]
    values = [0.0, 1.0]
    assert abs(interpolate_trace(time_ps, values, 5.0) - 0.5) < 1.0e-12


def test_read_waveform_csv_and_compare_waveforms(tmp_path: Path) -> None:
    internal_path = tmp_path / "internal.csv"
    external_path = tmp_path / "external.csv"

    internal_path.write_text(
        "time_ps,n1\n"
        "0,0.0\n"
        "10,1.0\n",
        encoding="utf-8",
    )
    external_path.write_text(
        "time_ps,n1\n"
        "0,0.0\n"
        "10,0.8\n",
        encoding="utf-8",
    )

    internal_time, internal_traces = read_waveform_csv(internal_path)
    external_time, external_traces = read_waveform_csv(external_path)

    metrics = compare_waveforms(internal_time, internal_traces, external_time, external_traces)
    max_abs, rms = metrics["n1"]
    assert abs(max_abs - 0.2) < 1.0e-12
    assert 0.0 < rms <= max_abs


def test_read_waveform_csv_accepts_josim_headers_and_normalizes_node_names(tmp_path: Path) -> None:
    external_path = tmp_path / "external.csv"
    external_path.write_text(
        "time,P(N1),P(OUT)\n"
        "0.0,0.0,0.0\n"
        "1.0e-12,0.8,0.5\n",
        encoding="utf-8",
    )

    time_ps, traces = read_waveform_csv(external_path)

    assert time_ps == [0.0, 1.0]
    assert traces == {"n1": [0.0, 0.8], "out": [0.0, 0.5]}


def test_summarize_metrics_reports_failures_by_node() -> None:
    summary = summarize_metrics(
        {
            "n1": (0.05, 0.02),
            "n2": (0.20, 0.10),
        },
        max_abs_threshold=0.10,
    )

    assert summary["summary"] == "FAIL"
    assert abs(float(summary["worst_max_abs_v"]) - 0.20) < 1.0e-12
    assert summary["failing_nodes"] == ["n2"]
    assert summary["top_worst_nodes"][0]["node"] == "n2"


def test_summarize_metrics_reports_pass_when_all_nodes_within_threshold() -> None:
    summary = summarize_metrics(
        {
            "n1": (0.05, 0.02),
            "n2": (0.08, 0.04),
        },
        max_abs_threshold=0.10,
    )

    assert summary["summary"] == "PASS"
    assert abs(float(summary["worst_max_abs_v"]) - 0.08) < 1.0e-12
    assert summary["failing_nodes"] == []
    assert summary["top_worst_nodes"][0]["node"] == "n2"


def test_rank_nodes_by_error_sorts_by_max_abs_then_rms() -> None:
    ranked = rank_nodes_by_error(
        {
            "n1": (0.1, 0.05),
            "n2": (0.2, 0.01),
            "n3": (0.2, 0.02),
        }
    )

    assert [entry["node"] for entry in ranked] == ["n3", "n2", "n1"]
