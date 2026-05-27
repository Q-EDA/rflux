from __future__ import annotations

import importlib.util
import json
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "promote_waveform_approved_baseline.py"
    spec = importlib.util.spec_from_file_location("promote_waveform_approved_baseline", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
promote_waveform_approved_baseline = _module.promote_waveform_approved_baseline


def test_promote_waveform_baseline_writes_platform_targets(tmp_path: Path) -> None:
    repo_root = tmp_path
    benchmark_dir = repo_root / "python" / "tests" / "benchmarks" / "phase6"
    benchmark_dir.mkdir(parents=True)
    candidate_dir = repo_root / "target" / "waveform-compare"
    candidate_dir.mkdir(parents=True)

    candidate_json = candidate_dir / "waveform_compare_summary.candidate-baseline.json"
    candidate_json.write_text(
        json.dumps({"failures": 0, "decks": [], "categories": []}),
        encoding="utf-8",
    )
    candidate_md = candidate_dir / "waveform_compare_summary.candidate-baseline.md"
    candidate_md.write_text("# candidate\n", encoding="utf-8")

    promoted = promote_waveform_approved_baseline(
        repo_root=repo_root,
        platform_key="LiNuX",
        benchmark_dir=benchmark_dir,
        candidate_json=candidate_json,
        candidate_md=candidate_md,
    )

    assert promoted["json"].name == "waveform_compare_summary.linux-approved-baseline.json"
    assert promoted["md"].name == "waveform_compare_summary.linux-approved-baseline.md"
    assert promoted["json"].is_file()
    assert promoted["md"].is_file()


def test_promote_waveform_baseline_rejects_failing_candidate(tmp_path: Path) -> None:
    benchmark_dir = tmp_path / "benchmarks"
    benchmark_dir.mkdir(parents=True)
    candidate_json = tmp_path / "candidate.json"
    candidate_json.write_text(json.dumps({"failures": 2}), encoding="utf-8")
    candidate_md = tmp_path / "candidate.md"
    candidate_md.write_text("# candidate\n", encoding="utf-8")

    try:
        promote_waveform_approved_baseline(
            repo_root=tmp_path,
            platform_key="linux",
            benchmark_dir=benchmark_dir,
            candidate_json=candidate_json,
            candidate_md=candidate_md,
        )
    except ValueError as exc:
        assert "contains failures" in str(exc)
    else:
        raise AssertionError("expected ValueError for failing candidate summary")
