from __future__ import annotations

import argparse
import json
import platform
import statistics
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


@dataclass(frozen=True)
class PerfCase:
    name: str
    command: list[str]


CommandRunner = Callable[[list[str], Path], subprocess.CompletedProcess[str]]


DEFAULT_CASES = [
    PerfCase(name="cli_help", command=["uv", "run", "cargo", "run", "-p", "rflux-cli", "--", "--help"]),
    PerfCase(
        name="lint_input_help",
        command=["uv", "run", "cargo", "run", "-p", "rflux-cli", "--", "lint-input", "--help"],
    ),
    PerfCase(
        name="run_with_diagnostics_help",
        command=[
            "uv",
            "run",
            "cargo",
            "run",
            "-p",
            "rflux-cli",
            "--",
            "run-with-diagnostics",
            "--help",
        ],
    ),
]


def validate_baseline_payload(
    payload: dict[str, object],
    *,
    expected_case_names: list[str] | None = None,
    expected_platform_prefix: str | None = None,
) -> None:
    if payload.get("kind") != "cli_perf_baseline":
        raise ValueError("baseline payload kind must be cli_perf_baseline")
    if payload.get("schema_version") != 1:
        raise ValueError("baseline payload schema_version must be 1")

    cases = payload.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("baseline payload cases must be a non-empty list")

    case_count = payload.get("case_count")
    if not isinstance(case_count, int) or case_count != len(cases):
        raise ValueError("baseline payload case_count must match cases length")

    seen_names: list[str] = []
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError("baseline payload cases must contain objects")

        name = case.get("name")
        if not isinstance(name, str) or not name:
            raise ValueError("baseline payload case name must be a non-empty string")
        seen_names.append(name)

        command = case.get("command")
        if not isinstance(command, list) or not command or not all(isinstance(part, str) for part in command):
            raise ValueError(f"baseline payload case {name} command must be a non-empty string list")

        summary = case.get("summary")
        if not isinstance(summary, dict):
            raise ValueError(f"baseline payload case {name} summary must be an object")
        for key in ("min_ms", "max_ms", "mean_ms", "median_ms", "p95_ms"):
            value = summary.get(key)
            if not isinstance(value, (int, float)):
                raise ValueError(f"baseline payload case {name} summary.{key} must be numeric")

    if len(set(seen_names)) != len(seen_names):
        raise ValueError("baseline payload case names must be unique")

    if expected_case_names is not None:
        if seen_names != expected_case_names:
            raise ValueError(
                "baseline payload case names do not match expected order: "
                + ", ".join(expected_case_names)
            )

    if expected_platform_prefix is not None:
        platform_value = payload.get("platform")
        if not isinstance(platform_value, str) or not platform_value.startswith(expected_platform_prefix):
            raise ValueError(
                "baseline payload platform must start with: " + expected_platform_prefix
            )


def run_checked(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(command, cwd=str(cwd), capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(
            "command failed:\n"
            + " ".join(command)
            + "\n\nstdout:\n"
            + completed.stdout
            + "\n\nstderr:\n"
            + completed.stderr
        )
    return completed


def _percentile(sorted_values: list[float], ratio: float) -> float:
    if not sorted_values:
        return 0.0
    if len(sorted_values) == 1:
        return sorted_values[0]
    index = (len(sorted_values) - 1) * ratio
    low = int(index)
    high = min(low + 1, len(sorted_values) - 1)
    fraction = index - low
    return sorted_values[low] * (1.0 - fraction) + sorted_values[high] * fraction


def _measure_case(
    *,
    case: PerfCase,
    repo_root: Path,
    iterations: int,
    warmup: int,
    command_runner: CommandRunner,
) -> dict[str, object]:
    if iterations <= 0:
        raise ValueError("iterations must be >= 1")
    if warmup < 0:
        raise ValueError("warmup must be >= 0")

    for _ in range(warmup):
        command_runner(case.command, repo_root)

    samples_ms: list[float] = []
    for _ in range(iterations):
        started = time.perf_counter()
        command_runner(case.command, repo_root)
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        samples_ms.append(elapsed_ms)

    sorted_samples = sorted(samples_ms)
    return {
        "name": case.name,
        "command": case.command,
        "iterations": iterations,
        "warmup": warmup,
        "samples_ms": [round(value, 3) for value in samples_ms],
        "summary": {
            "min_ms": round(min(samples_ms), 3),
            "max_ms": round(max(samples_ms), 3),
            "mean_ms": round(statistics.fmean(samples_ms), 3),
            "median_ms": round(statistics.median(samples_ms), 3),
            "p95_ms": round(_percentile(sorted_samples, 0.95), 3),
        },
    }


def _index_cases(payload: dict[str, object]) -> dict[str, dict[str, object]]:
    cases = payload.get("cases")
    if not isinstance(cases, list):
        return {}
    result: dict[str, dict[str, object]] = {}
    for item in cases:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        if isinstance(name, str):
            result[name] = item
    return result


def _summary_value(case_payload: dict[str, object], key: str) -> float | None:
    summary = case_payload.get("summary")
    if not isinstance(summary, dict):
        return None
    value = summary.get(key)
    if isinstance(value, (int, float)):
        return float(value)
    return None


def compare_with_previous(
    *,
    current_payload: dict[str, object],
    previous_payload: dict[str, object],
    max_regression_ratio: float,
) -> dict[str, object]:
    if max_regression_ratio < 0.0:
        raise ValueError("max_regression_ratio must be >= 0")

    current_cases = _index_cases(current_payload)
    previous_cases = _index_cases(previous_payload)
    comparisons: list[dict[str, object]] = []
    regression_count = 0

    for name, current_case in current_cases.items():
        previous_case = previous_cases.get(name)
        if previous_case is None:
            comparisons.append(
                {
                    "name": name,
                    "status": "missing_previous_case",
                    "previous_median_ms": None,
                    "current_median_ms": _summary_value(current_case, "median_ms"),
                    "regression_ratio": None,
                }
            )
            continue

        previous_median = _summary_value(previous_case, "median_ms")
        current_median = _summary_value(current_case, "median_ms")
        if previous_median is None or current_median is None or previous_median <= 0.0:
            comparisons.append(
                {
                    "name": name,
                    "status": "insufficient_data",
                    "previous_median_ms": previous_median,
                    "current_median_ms": current_median,
                    "regression_ratio": None,
                }
            )
            continue

        ratio = current_median / previous_median
        status = "pass"
        if ratio > (1.0 + max_regression_ratio):
            status = "regressed"
            regression_count += 1

        comparisons.append(
            {
                "name": name,
                "status": status,
                "previous_median_ms": round(previous_median, 3),
                "current_median_ms": round(current_median, 3),
                "regression_ratio": round(ratio, 4),
            }
        )

    return {
        "max_regression_ratio": max_regression_ratio,
        "comparison_count": len(comparisons),
        "regression_count": regression_count,
        "comparisons": comparisons,
    }


def capture_cli_perf_baseline(
    *,
    repo_root: Path,
    output_path: Path,
    iterations: int,
    warmup: int,
    command_runner: CommandRunner = run_checked,
    cases: list[PerfCase] | None = None,
    previous_baseline_path: Path | None = None,
    max_regression_ratio: float = 0.2,
    expected_previous_platform_prefix: str | None = None,
) -> dict[str, object]:
    active_cases = cases if cases is not None else DEFAULT_CASES
    expected_case_names = [case.name for case in active_cases]
    case_payloads = [
        _measure_case(
            case=case,
            repo_root=repo_root,
            iterations=iterations,
            warmup=warmup,
            command_runner=command_runner,
        )
        for case in active_cases
    ]

    payload: dict[str, object] = {
        "kind": "cli_perf_baseline",
        "schema_version": 1,
        "platform": platform.platform(),
        "python_version": platform.python_version(),
        "iterations": iterations,
        "warmup": warmup,
        "case_count": len(case_payloads),
        "cases": case_payloads,
    }
    validate_baseline_payload(payload, expected_case_names=expected_case_names)

    if previous_baseline_path is not None:
        previous_payload = json.loads(previous_baseline_path.read_text(encoding="utf-8"))
        validate_baseline_payload(
            previous_payload,
            expected_case_names=expected_case_names,
            expected_platform_prefix=expected_previous_platform_prefix,
        )
        payload["regression_check"] = compare_with_previous(
            current_payload=payload,
            previous_payload=previous_payload,
            max_regression_ratio=max_regression_ratio,
        )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return payload


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Capture a minimal CLI performance baseline for core command paths.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("target/cli-perf/cli_perf_baseline.json"),
        help="Path to write the baseline JSON.",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=3,
        help="Measured iterations per case.",
    )
    parser.add_argument(
        "--warmup",
        type=int,
        default=1,
        help="Warmup iterations per case before measurements.",
    )
    parser.add_argument(
        "--previous-baseline",
        type=Path,
        default=None,
        help="Optional previous baseline JSON for regression comparison.",
    )
    parser.add_argument(
        "--max-regression-ratio",
        type=float,
        default=0.2,
        help="Maximum allowed median regression ratio (0.2 = 20%%).",
    )
    parser.add_argument(
        "--fail-on-regression",
        action="store_true",
        help="Exit with non-zero status when regression check detects regressions.",
    )
    parser.add_argument(
        "--expected-previous-platform-prefix",
        type=str,
        default=None,
        help="Optional required prefix for previous baseline platform field (for example: linux).",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    output_path = args.output if args.output.is_absolute() else (repo_root / args.output)
    previous = None
    if args.previous_baseline is not None:
        previous = (
            args.previous_baseline
            if args.previous_baseline.is_absolute()
            else (repo_root / args.previous_baseline)
        )

    payload = capture_cli_perf_baseline(
        repo_root=repo_root,
        output_path=output_path,
        iterations=args.iterations,
        warmup=args.warmup,
        previous_baseline_path=previous,
        max_regression_ratio=args.max_regression_ratio,
        expected_previous_platform_prefix=args.expected_previous_platform_prefix,
    )

    regression_check = payload.get("regression_check")
    if args.fail_on_regression and isinstance(regression_check, dict):
        if int(regression_check.get("regression_count", 0)) > 0:
            raise SystemExit(2)


if __name__ == "__main__":
    main()
