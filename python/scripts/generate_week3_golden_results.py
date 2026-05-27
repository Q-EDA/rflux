from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path


def _load_script_module(script_name: str):
    script_path = Path(__file__).resolve().parent / script_name
    spec = importlib.util.spec_from_file_location(script_name.replace(".py", ""), script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load helper script module: {script_name}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_collector_module = _load_script_module("collect_quality_baseline_results.py")
_summary_module = _load_script_module("summarize_quality_baseline_results.py")
_artifact_module = _load_script_module("prepare_quality_baseline_artifacts.py")

build_quality_results = _collector_module.build_quality_results
assert_payloads_match = _collector_module.assert_payloads_match

load_thresholds = _summary_module.load_thresholds
build_summary_payload = _summary_module.build_summary_payload
build_markdown_report = _summary_module.build_markdown_report
validate_summary_payload = _summary_module.validate_summary_payload
validate_no_regression = _summary_module.validate_no_regression

prepare_quality_baseline_artifacts = _artifact_module.prepare_quality_baseline_artifacts


def _load_json(path: Path) -> dict[str, object]:
    return json.loads(path.read_text(encoding="utf-8"))


def run_pipeline(
    *,
    repo_root: Path,
    timing_report: Path,
    verify_report: Path,
    sim_summary: Path,
    thresholds: Path,
    results_output: Path,
    summary_json: Path,
    summary_md: Path,
    artifact_dir: Path,
    previous_summary_json: Path | None,
    check_results_against: Path | None,
    validate_pass: bool,
    validate_no_regression_flag: bool,
    regression_tolerance: float,
    github_context: dict[str, str],
) -> dict[str, object]:
    timing_payload = _load_json(timing_report)
    verify_payload = _load_json(verify_report)
    sim_payload = _load_json(sim_summary)

    quality_results = build_quality_results(
        timing_report=timing_payload,
        verify_report=verify_payload,
        sim_summary=sim_payload,
    )
    results_output.parent.mkdir(parents=True, exist_ok=True)
    results_output.write_text(json.dumps(quality_results, indent=2) + "\n", encoding="utf-8")

    if check_results_against is not None:
        assert_payloads_match(_load_json(check_results_against), quality_results)

    thresholds_payload = load_thresholds(thresholds)
    previous_summary = None
    if previous_summary_json is not None and previous_summary_json.exists():
        previous_summary = _load_json(previous_summary_json)

    summary_payload = build_summary_payload(thresholds_payload, quality_results.get("suites", {}), previous_summary)
    summary_errors = validate_summary_payload(summary_payload, thresholds_payload)
    if summary_errors:
        raise RuntimeError("\n".join(summary_errors))

    if validate_pass and int(summary_payload.get("failures", 0)) > 0:
        raise RuntimeError(f"quality baseline check failed with {summary_payload['failures']} failure(s)")

    if validate_no_regression_flag:
        if previous_summary is None:
            raise RuntimeError("--validate-no-regression requires a valid previous summary file")
        regression_errors = validate_no_regression(summary_payload, previous_summary, regression_tolerance)
        if regression_errors:
            raise RuntimeError("\n".join(regression_errors))

    summary_json.parent.mkdir(parents=True, exist_ok=True)
    summary_md.parent.mkdir(parents=True, exist_ok=True)
    summary_json.write_text(json.dumps(summary_payload, indent=2) + "\n", encoding="utf-8")
    summary_md.write_text(build_markdown_report(summary_payload), encoding="utf-8")

    manifest = prepare_quality_baseline_artifacts(
        repo_root=repo_root,
        thresholds_path=thresholds,
        results_json_path=results_output,
        artifact_dir=artifact_dir,
        previous_summary_json=previous_summary_json,
        validate_no_regression_flag=validate_no_regression_flag,
        regression_tolerance=regression_tolerance,
        github_context=github_context,
    )

    return {
        "results_output": results_output.as_posix(),
        "summary_json": summary_json.as_posix(),
        "summary_md": summary_md.as_posix(),
        "artifact_dir": artifact_dir.as_posix(),
        "artifact_manifest": manifest,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate Week 3 quality baseline outputs with one command (collect + summarize + artifact bundle).",
    )
    parser.add_argument(
        "--timing-report",
        type=Path,
        default=Path("python/tests/benchmarks/week3/inputs/timing_report.golden.json"),
    )
    parser.add_argument(
        "--verify-report",
        type=Path,
        default=Path("python/tests/benchmarks/week3/inputs/verify_report.golden.json"),
    )
    parser.add_argument(
        "--sim-summary",
        type=Path,
        default=Path("python/tests/benchmarks/week3/inputs/sim_summary.golden.json"),
    )
    parser.add_argument(
        "--thresholds",
        type=Path,
        default=Path("python/tests/benchmarks/week3/quality_thresholds.json"),
    )
    parser.add_argument(
        "--results-output",
        type=Path,
        default=Path("target/week3-quality-pipeline/quality_results.generated.json"),
    )
    parser.add_argument(
        "--summary-json",
        type=Path,
        default=Path("target/week3-quality-pipeline/quality_summary.current.json"),
    )
    parser.add_argument(
        "--summary-md",
        type=Path,
        default=Path("target/week3-quality-pipeline/quality_summary.current.md"),
    )
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        default=Path("target/week3-quality-pipeline/review"),
    )
    parser.add_argument(
        "--previous-summary-json",
        type=Path,
        default=Path("python/tests/benchmarks/week3/quality_summary.approved-baseline.json"),
    )
    parser.add_argument(
        "--check-results-against",
        type=Path,
        default=Path("python/tests/benchmarks/week3/quality_results.golden.json"),
    )
    parser.add_argument("--validate-pass", action="store_true")
    parser.add_argument("--validate-no-regression", action="store_true")
    parser.add_argument("--regression-tolerance", type=float, default=0.0)
    parser.add_argument("--github-workflow", type=str, default="")
    parser.add_argument("--github-job", type=str, default="")
    parser.add_argument("--github-event-name", type=str, default="")
    parser.add_argument("--github-run-id", type=str, default="")
    parser.add_argument("--github-run-attempt", type=str, default="")
    parser.add_argument("--github-sha", type=str, default="")
    parser.add_argument("--github-ref-name", type=str, default="")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]

    def resolve(path: Path | None) -> Path | None:
        if path is None:
            return None
        return path if path.is_absolute() else (repo_root / path)

    run_pipeline(
        repo_root=repo_root,
        timing_report=resolve(args.timing_report),
        verify_report=resolve(args.verify_report),
        sim_summary=resolve(args.sim_summary),
        thresholds=resolve(args.thresholds),
        results_output=resolve(args.results_output),
        summary_json=resolve(args.summary_json),
        summary_md=resolve(args.summary_md),
        artifact_dir=resolve(args.artifact_dir),
        previous_summary_json=resolve(args.previous_summary_json),
        check_results_against=resolve(args.check_results_against),
        validate_pass=args.validate_pass,
        validate_no_regression_flag=args.validate_no_regression,
        regression_tolerance=args.regression_tolerance,
        github_context={
            "workflow": args.github_workflow,
            "job": args.github_job,
            "event_name": args.github_event_name,
            "run_id": args.github_run_id,
            "run_attempt": args.github_run_attempt,
            "sha": args.github_sha,
            "ref_name": args.github_ref_name,
        },
    )


if __name__ == "__main__":
    main()
