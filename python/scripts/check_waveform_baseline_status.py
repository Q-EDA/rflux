from __future__ import annotations

import argparse
import json
from pathlib import Path


def check_waveform_baseline_status(*, repo_root: Path, benchmark_dir: Path, platform_key: str) -> dict[str, object]:
    normalized_platform = platform_key.strip().lower()
    if not normalized_platform:
        raise ValueError("platform key must not be empty")

    resolved_benchmark_dir = benchmark_dir if benchmark_dir.is_absolute() else (repo_root / benchmark_dir)
    baseline_json = resolved_benchmark_dir / f"waveform_compare_summary.{normalized_platform}-approved-baseline.json"
    baseline_md = resolved_benchmark_dir / f"waveform_compare_summary.{normalized_platform}-approved-baseline.md"

    status: dict[str, object] = {
        "platform": normalized_platform,
        "benchmark_dir": resolved_benchmark_dir.as_posix(),
        "baseline_json": baseline_json.as_posix(),
        "baseline_md": baseline_md.as_posix(),
        "json_exists": baseline_json.is_file(),
        "md_exists": baseline_md.is_file(),
        "summary_failures": None,
        "ready": False,
        "reason": "",
    }

    if not baseline_json.is_file():
        status["reason"] = "missing baseline json"
        return status

    if not baseline_md.is_file():
        status["reason"] = "missing baseline markdown"
        return status

    payload = json.loads(baseline_json.read_text(encoding="utf-8"))
    failures = int(payload.get("failures", 0)) if isinstance(payload, dict) else None
    status["summary_failures"] = failures
    if failures is None:
        status["reason"] = "invalid baseline json payload"
        return status

    if failures > 0:
        status["reason"] = "baseline json contains failures"
        return status

    status["ready"] = True
    status["reason"] = "baseline ready"
    return status


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Check whether a platform waveform approved baseline is present and ready for strict no-regression gating.",
    )
    parser.add_argument(
        "--platform",
        type=str,
        default="linux",
        help="Platform key, for example linux or windows.",
    )
    parser.add_argument(
        "--benchmark-dir",
        type=str,
        default="python/tests/benchmarks/phase6",
        help="Repo-relative or absolute benchmark directory containing approved baselines.",
    )
    parser.add_argument(
        "--json-output",
        type=str,
        default="",
        help="Optional path to write a machine-readable status JSON.",
    )
    parser.add_argument(
        "--require-ready",
        action="store_true",
        help="Exit non-zero when baseline is not ready.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    status = check_waveform_baseline_status(
        repo_root=repo_root,
        benchmark_dir=Path(args.benchmark_dir),
        platform_key=args.platform,
    )

    if args.json_output.strip():
        output_path = Path(args.json_output)
        if not output_path.is_absolute():
            output_path = (repo_root / output_path).resolve()
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(status, indent=2) + "\n", encoding="utf-8")

    print(f"baseline_platform={status['platform']}")
    print(f"baseline_ready={status['ready']}")
    print(f"baseline_reason={status['reason']}")

    if args.require_ready and not bool(status["ready"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
