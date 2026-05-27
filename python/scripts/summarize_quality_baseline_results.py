from __future__ import annotations

import argparse
import json
from pathlib import Path


def load_thresholds(path: Path) -> dict[str, dict[str, dict[str, object]]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    suites = payload.get("suites", {})
    result: dict[str, dict[str, dict[str, object]]] = {}
    for suite_name, metrics in suites.items():
        suite_metrics: dict[str, dict[str, object]] = {}
        for metric_name, config in metrics.items():
            suite_metrics[str(metric_name)] = {
                "min": config.get("min"),
                "max": config.get("max"),
                "rationale": str(config.get("rationale", "")),
            }
        result[str(suite_name)] = suite_metrics
    return result


def load_results(path: Path) -> dict[str, dict[str, float]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    suites = payload.get("suites", {})
    result: dict[str, dict[str, float]] = {}
    for suite_name, metrics in suites.items():
        suite_metrics: dict[str, float] = {}
        for metric_name, value in metrics.items():
            suite_metrics[str(metric_name)] = float(value)
        result[str(suite_name)] = suite_metrics
    return result


def build_summary_payload(
    thresholds: dict[str, dict[str, dict[str, object]]],
    results: dict[str, dict[str, float]],
    previous_summary: dict[str, object] | None = None,
) -> dict[str, object]:
    suites_payload: list[dict[str, object]] = []
    failures = 0

    for suite_name in sorted(thresholds):
        metric_entries: list[dict[str, object]] = []
        suite_thresholds = thresholds[suite_name]
        suite_results = results.get(suite_name, {})

        for metric_name in sorted(suite_thresholds):
            config = suite_thresholds[metric_name]
            value = suite_results.get(metric_name)
            min_v = config.get("min")
            max_v = config.get("max")
            status = "PASS"
            reason = ""
            if value is None:
                status = "MISSING"
                reason = "missing metric value"
                failures += 1
            else:
                if min_v is not None and float(value) < float(min_v):
                    status = "FAIL"
                    reason = f"value {value:.6e} below min {float(min_v):.6e}"
                if max_v is not None and float(value) > float(max_v):
                    status = "FAIL"
                    reason = f"value {value:.6e} above max {float(max_v):.6e}"
                if status != "PASS":
                    failures += 1

            metric_entries.append(
                {
                    "metric": metric_name,
                    "value": value,
                    "min": min_v,
                    "max": max_v,
                    "status": status,
                    "reason": reason,
                    "rationale": str(config.get("rationale", "")),
                }
            )

        suites_payload.append(
            {
                "suite": suite_name,
                "metric_count": len(metric_entries),
                "failures": len([entry for entry in metric_entries if entry["status"] != "PASS"]),
                "metrics": metric_entries,
            }
        )

    payload: dict[str, object] = {
        "kind": "quality-baseline-summary",
        "schema_version": 1,
        "suite_count": len(suites_payload),
        "failures": failures,
        "suites": suites_payload,
    }
    if previous_summary is not None:
        payload["history_diff"] = build_history_diff(payload, previous_summary)
    return payload


def build_history_diff(current_payload: dict[str, object], previous_payload: dict[str, object]) -> dict[str, object]:
    previous_map: dict[tuple[str, str], dict[str, object]] = {}
    for suite in previous_payload.get("suites", []):
        if not isinstance(suite, dict):
            continue
        suite_name = str(suite.get("suite"))
        for metric in suite.get("metrics", []):
            if not isinstance(metric, dict):
                continue
            previous_map[(suite_name, str(metric.get("metric")))] = metric

    metric_changes: list[dict[str, object]] = []
    for suite in current_payload.get("suites", []):
        if not isinstance(suite, dict):
            continue
        suite_name = str(suite.get("suite"))
        for metric in suite.get("metrics", []):
            if not isinstance(metric, dict):
                continue
            metric_name = str(metric.get("metric"))
            previous_metric = previous_map.get((suite_name, metric_name))
            current_value = metric.get("value")
            previous_value = previous_metric.get("value") if isinstance(previous_metric, dict) else None
            current_status = str(metric.get("status"))
            previous_status = str(previous_metric.get("status")) if isinstance(previous_metric, dict) else "NEW"

            delta = None
            if current_value is not None and previous_value is not None:
                delta = float(current_value) - float(previous_value)

            if previous_metric is None or delta not in (None, 0.0) or current_status != previous_status:
                metric_changes.append(
                    {
                        "suite": suite_name,
                        "metric": metric_name,
                        "current_value": current_value,
                        "previous_value": previous_value,
                        "delta": delta,
                        "current_status": current_status,
                        "previous_status": previous_status,
                    }
                )

    return {
        "current_failures": int(current_payload.get("failures", 0)),
        "previous_failures": int(previous_payload.get("failures", 0)),
        "failure_delta": int(current_payload.get("failures", 0)) - int(previous_payload.get("failures", 0)),
        "metric_changes": metric_changes,
    }


def build_markdown_report(payload: dict[str, object]) -> str:
    lines = [
        "# Quality Baseline Summary",
        "",
        f"failures: {int(payload.get('failures', 0))}",
        "",
    ]
    for suite in payload.get("suites", []):
        if not isinstance(suite, dict):
            continue
        lines.append(f"## {suite.get('suite')}")
        lines.append("")
        lines.append("| metric | value | min | max | status | reason | rationale |")
        lines.append("|---|---:|---:|---:|---|---|---|")
        for metric in suite.get("metrics", []):
            if not isinstance(metric, dict):
                continue
            value = metric.get("value")
            min_v = metric.get("min")
            max_v = metric.get("max")
            value_text = "-" if value is None else f"{float(value):.6e}"
            min_text = "-" if min_v is None else f"{float(min_v):.6e}"
            max_text = "-" if max_v is None else f"{float(max_v):.6e}"
            lines.append(
                f"| {metric.get('metric')} | {value_text} | {min_text} | {max_text} | {metric.get('status')} | {metric.get('reason')} | {metric.get('rationale')} |"
            )
        lines.append("")

    history = payload.get("history_diff")
    if isinstance(history, dict):
        lines.extend(
            [
                "## History Diff",
                "",
                f"failures: {history.get('current_failures')} (delta {int(history.get('failure_delta', 0)):+d} vs previous {history.get('previous_failures')})",
                "",
            ]
        )
        changes = history.get("metric_changes", [])
        if changes:
            lines.append("| suite | metric | status | value delta |")
            lines.append("|---|---|---|---:|")
            for change in changes:
                if not isinstance(change, dict):
                    continue
                delta = change.get("delta")
                delta_text = "-" if delta is None else f"{float(delta):+.6e}"
                lines.append(
                    f"| {change.get('suite')} | {change.get('metric')} | {change.get('previous_status')} -> {change.get('current_status')} | {delta_text} |"
                )
            lines.append("")
    return "\n".join(lines) + "\n"


def validate_summary_payload(payload: dict[str, object], thresholds: dict[str, dict[str, dict[str, object]]]) -> list[str]:
    errors: list[str] = []
    suites = payload.get("suites", [])
    suite_names = [str(entry.get("suite")) for entry in suites if isinstance(entry, dict)]
    expected_names = sorted(thresholds)
    if suite_names != expected_names:
        errors.append(f"suite coverage mismatch: expected {expected_names}, got {suite_names}")
    return errors


def _summary_rank(summary: str) -> int:
    normalized = summary.strip().upper()
    if normalized == "PASS":
        return 0
    if normalized == "FAIL":
        return 1
    if normalized == "MISSING":
        return 2
    return 3


def validate_no_regression(
    payload: dict[str, object],
    previous_payload: dict[str, object],
    tolerance: float,
) -> list[str]:
    errors: list[str] = []

    previous_map: dict[tuple[str, str], dict[str, object]] = {}
    for suite in previous_payload.get("suites", []):
        if not isinstance(suite, dict):
            continue
        suite_name = str(suite.get("suite"))
        for metric in suite.get("metrics", []):
            if not isinstance(metric, dict):
                continue
            previous_map[(suite_name, str(metric.get("metric")))] = metric

    for suite in payload.get("suites", []):
        if not isinstance(suite, dict):
            continue
        suite_name = str(suite.get("suite"))
        for metric in suite.get("metrics", []):
            if not isinstance(metric, dict):
                continue
            metric_name = str(metric.get("metric"))
            previous_metric = previous_map.get((suite_name, metric_name))
            if not isinstance(previous_metric, dict):
                continue

            current_status = str(metric.get("status", "PASS"))
            previous_status = str(previous_metric.get("status", "PASS"))
            if _summary_rank(current_status) > _summary_rank(previous_status):
                errors.append(
                    f"metric regression for {suite_name}/{metric_name}: status worsened {previous_status} -> {current_status}"
                )

            current_value = metric.get("value")
            previous_value = previous_metric.get("value")
            if current_value is None or previous_value is None:
                continue

            current_value_f = float(current_value)
            previous_value_f = float(previous_value)
            min_v = metric.get("min")
            max_v = metric.get("max")

            if max_v is not None and current_value_f > previous_value_f + tolerance:
                errors.append(
                    f"metric regression for {suite_name}/{metric_name}: value increased {current_value_f:.6e} from {previous_value_f:.6e} beyond tolerance {tolerance:.6e}"
                )
            if min_v is not None and current_value_f < previous_value_f - tolerance:
                errors.append(
                    f"metric regression for {suite_name}/{metric_name}: value decreased {current_value_f:.6e} from {previous_value_f:.6e} beyond tolerance {tolerance:.6e}"
                )

    return errors


def main() -> None:
    parser = argparse.ArgumentParser(description="Summarize timing/verify/sim quality baseline results.")
    parser.add_argument(
        "--thresholds",
        type=Path,
        default=Path("python/tests/benchmarks/week3/quality_thresholds.json"),
    )
    parser.add_argument("--results-json", type=Path, required=True)
    parser.add_argument("--summary-json", type=Path, required=True)
    parser.add_argument("--summary-md", type=Path, required=True)
    parser.add_argument("--previous-summary-json", type=Path, default=None)
    parser.add_argument("--validate-pass", action="store_true")
    parser.add_argument("--validate-no-regression", action="store_true")
    parser.add_argument("--regression-tolerance", type=float, default=0.0)
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]

    thresholds_path = args.thresholds if args.thresholds.is_absolute() else (repo_root / args.thresholds)
    results_path = args.results_json if args.results_json.is_absolute() else (repo_root / args.results_json)
    summary_json_path = args.summary_json if args.summary_json.is_absolute() else (repo_root / args.summary_json)
    summary_md_path = args.summary_md if args.summary_md.is_absolute() else (repo_root / args.summary_md)
    previous_path = (
        None
        if args.previous_summary_json is None
        else (args.previous_summary_json if args.previous_summary_json.is_absolute() else (repo_root / args.previous_summary_json))
    )

    thresholds = load_thresholds(thresholds_path)
    results = load_results(results_path)
    previous_summary = None if previous_path is None else json.loads(previous_path.read_text(encoding="utf-8"))
    payload = build_summary_payload(thresholds, results, previous_summary)

    validation_errors = validate_summary_payload(payload, thresholds)
    if validation_errors:
        raise SystemExit("\n".join(validation_errors))

    summary_json_path.parent.mkdir(parents=True, exist_ok=True)
    summary_md_path.parent.mkdir(parents=True, exist_ok=True)
    summary_json_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    summary_md_path.write_text(build_markdown_report(payload), encoding="utf-8")

    if args.validate_pass and int(payload.get("failures", 0)) > 0:
        raise SystemExit(f"quality baseline check failed with {payload['failures']} failure(s)")

    if args.validate_no_regression:
        if previous_summary is None:
            raise SystemExit("--validate-no-regression requires --previous-summary-json")
        regression_errors = validate_no_regression(payload, previous_summary, args.regression_tolerance)
        if regression_errors:
            raise SystemExit("\n".join(regression_errors))


if __name__ == "__main__":
    main()
