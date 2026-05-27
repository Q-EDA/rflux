from __future__ import annotations

import importlib.util
from pathlib import Path


def _load_runner_module():
    from pathlib import Path

    script_path = Path(__file__).resolve().parents[1] / "scripts" / "run_waveform_compare_manifest.py"
    spec = importlib.util.spec_from_file_location("run_waveform_compare_manifest", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_runner_module = _load_runner_module()
resolve_previous_summary_json = _runner_module.resolve_previous_summary_json
select_thresholds = _runner_module.select_thresholds


def test_select_thresholds_filters_by_deck_and_preserves_manifest_order() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "jj", "rationale": "a"},
        "b.cir": {"max_abs_threshold_v": 0.2, "category": "delay", "rationale": "b"},
        "c.cir": {"max_abs_threshold_v": 0.3, "category": "jj", "rationale": "c"},
    }

    selected = select_thresholds(thresholds, ["c.cir", "a.cir"], None)

    assert list(selected) == ["a.cir", "c.cir"]


def test_select_thresholds_filters_by_category() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "jj", "rationale": "a"},
        "b.cir": {"max_abs_threshold_v": 0.2, "category": "delay", "rationale": "b"},
        "c.cir": {"max_abs_threshold_v": 0.3, "category": "jj", "rationale": "c"},
    }

    selected = select_thresholds(thresholds, None, ["jj"])

    assert list(selected) == ["a.cir", "c.cir"]


def test_select_thresholds_uses_intersection_when_both_filters_are_present() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "jj", "rationale": "a"},
        "b.cir": {"max_abs_threshold_v": 0.2, "category": "delay", "rationale": "b"},
        "c.cir": {"max_abs_threshold_v": 0.3, "category": "jj", "rationale": "c"},
    }

    selected = select_thresholds(thresholds, ["a.cir", "b.cir"], ["jj"])

    assert list(selected) == ["a.cir"]


def test_select_thresholds_reports_unknown_deck_filter() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "jj", "rationale": "a"},
    }

    try:
        select_thresholds(thresholds, ["missing.cir"], None)
    except ValueError as exc:
        assert "unknown deck filters" in str(exc)
    else:
        raise AssertionError("expected ValueError for unknown deck filter")


def test_select_thresholds_reports_empty_selection() -> None:
    thresholds = {
        "a.cir": {"max_abs_threshold_v": 0.1, "category": "jj", "rationale": "a"},
    }

    try:
        select_thresholds(thresholds, None, ["delay"])
    except ValueError as exc:
        assert "no decks selected" in str(exc)
    else:
        raise AssertionError("expected ValueError for empty selection")


def test_resolve_previous_summary_json_prefers_explicit_path(tmp_path: Path) -> None:
    explicit = tmp_path / "explicit.json"
    explicit.write_text("{}", encoding="utf-8")
    auto = tmp_path / "waveform_compare_summary.approved-baseline.json"
    auto.write_text("{}", encoding="utf-8")
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir()
    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.json").write_text("{}", encoding="utf-8")

    resolved = resolve_previous_summary_json(explicit, tmp_path, benchmark_dir, "linux")

    assert resolved == explicit


def test_resolve_previous_summary_json_uses_local_approved_baseline_when_present(tmp_path: Path) -> None:
    auto = tmp_path / "waveform_compare_summary.approved-baseline.json"
    auto.write_text("{}", encoding="utf-8")
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir()
    (benchmark_dir / "waveform_compare_summary.linux-approved-baseline.json").write_text("{}", encoding="utf-8")

    resolved = resolve_previous_summary_json(None, tmp_path, benchmark_dir, "linux")

    assert resolved == auto


def test_resolve_previous_summary_json_uses_repo_tracked_platform_baseline_when_present(tmp_path: Path) -> None:
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir()
    repo_baseline = benchmark_dir / "waveform_compare_summary.linux-approved-baseline.json"
    repo_baseline.write_text("{}", encoding="utf-8")

    resolved = resolve_previous_summary_json(None, tmp_path, benchmark_dir, "linux")

    assert resolved == repo_baseline


def test_resolve_previous_summary_json_returns_none_when_no_baseline_exists(tmp_path: Path) -> None:
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir()

    resolved = resolve_previous_summary_json(None, tmp_path, benchmark_dir, "linux")

    assert resolved is None