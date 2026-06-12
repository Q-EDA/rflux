from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "capture_cli_perf_baseline.py"
    spec = importlib.util.spec_from_file_location("capture_cli_perf_baseline", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


_module = _load_module()
capture_cli_perf_baseline = _module.capture_cli_perf_baseline
compare_with_previous = _module.compare_with_previous
PerfCase = _module.PerfCase
DEFAULT_CASES = _module.DEFAULT_CASES
validate_baseline_payload = _module.validate_baseline_payload


class _Completed:
    def __init__(self) -> None:
        self.stdout = ""
        self.stderr = ""
        self.returncode = 0


def test_capture_cli_perf_baseline_writes_payload(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    repo_root.mkdir(parents=True)

    seen_commands: list[list[str]] = []

    def fake_runner(command: list[str], cwd: Path):
        assert cwd == repo_root
        seen_commands.append(command)
        return _Completed()

    output_path = repo_root / "target" / "cli-perf" / "baseline.json"
    payload = capture_cli_perf_baseline(
        repo_root=repo_root,
        output_path=output_path,
        iterations=2,
        warmup=1,
        command_runner=fake_runner,
        cases=[
            PerfCase(name="case_a", command=["echo", "a"]),
            PerfCase(name="case_b", command=["echo", "b"]),
        ],
    )

    assert output_path.exists()
    assert payload["kind"] == "cli_perf_baseline"
    assert payload["schema_version"] == 1
    assert payload["case_count"] == 2
    assert len(payload["cases"]) == 2

    rendered = json.loads(output_path.read_text(encoding="utf-8"))
    assert rendered["case_count"] == 2
    assert rendered["cases"][0]["name"] == "case_a"
    assert rendered["cases"][1]["name"] == "case_b"

    # warmup(1) + iterations(2) for each of two cases
    assert len(seen_commands) == 6


def test_compare_with_previous_detects_regression() -> None:
    previous = {
        "cases": [
            {"name": "alpha", "summary": {"median_ms": 10.0}},
            {"name": "beta", "summary": {"median_ms": 20.0}},
        ]
    }
    current = {
        "cases": [
            {"name": "alpha", "summary": {"median_ms": 16.0}},
            {"name": "beta", "summary": {"median_ms": 18.0}},
        ]
    }

    comparison = compare_with_previous(
        current_payload=current,
        previous_payload=previous,
        max_regression_ratio=0.3,
    )

    assert comparison["comparison_count"] == 2
    assert comparison["regression_count"] == 1
    statuses = {row["name"]: row["status"] for row in comparison["comparisons"]}
    assert statuses["alpha"] == "regressed"
    assert statuses["beta"] == "pass"


def test_validate_baseline_payload_accepts_repo_approved_baseline() -> None:
    baseline_dir = Path(__file__).resolve().parents[1] / "tests" / "benchmarks" / "phasee"
    baseline_paths = sorted(baseline_dir.glob("cli_perf_baseline.*-approved-baseline.json"))

    assert [path.name for path in baseline_paths] == [
        "cli_perf_baseline.linux-approved-baseline.json",
        "cli_perf_baseline.windows-approved-baseline.json",
    ]

    for baseline_path in baseline_paths:
        payload = json.loads(baseline_path.read_text(encoding="utf-8"))
        validate_baseline_payload(
            payload,
            expected_case_names=[case.name for case in DEFAULT_CASES],
        )


def test_validate_baseline_payload_rejects_wrong_case_order() -> None:
    payload = {
        "kind": "cli_perf_baseline",
        "schema_version": 1,
        "case_count": 2,
        "cases": [
            {
                "name": "beta",
                "command": ["echo", "beta"],
                "summary": {
                    "min_ms": 1.0,
                    "max_ms": 1.0,
                    "mean_ms": 1.0,
                    "median_ms": 1.0,
                    "p95_ms": 1.0,
                },
            },
            {
                "name": "alpha",
                "command": ["echo", "alpha"],
                "summary": {
                    "min_ms": 1.0,
                    "max_ms": 1.0,
                    "mean_ms": 1.0,
                    "median_ms": 1.0,
                    "p95_ms": 1.0,
                },
            },
        ],
    }

    try:
        validate_baseline_payload(payload, expected_case_names=["alpha", "beta"])
    except ValueError as exc:
        assert "case names do not match expected order" in str(exc)
    else:
        raise AssertionError("expected validate_baseline_payload to reject wrong case order")


def test_validate_baseline_payload_rejects_platform_prefix_mismatch() -> None:
    payload = {
        "kind": "cli_perf_baseline",
        "schema_version": 1,
        "platform": "windows-latest",
        "case_count": 1,
        "cases": [
            {
                "name": "alpha",
                "command": ["echo", "alpha"],
                "summary": {
                    "min_ms": 1.0,
                    "max_ms": 1.0,
                    "mean_ms": 1.0,
                    "median_ms": 1.0,
                    "p95_ms": 1.0,
                },
            }
        ],
    }

    try:
        validate_baseline_payload(payload, expected_platform_prefix="linux")
    except ValueError as exc:
        assert "platform must start with" in str(exc)
    else:
        raise AssertionError("expected validate_baseline_payload to reject platform prefix mismatch")
