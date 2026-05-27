from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path

import rflux


def normalize_waveform_column_name(name: str) -> str:
    normalized = name.strip()
    if normalized.startswith(("P(", "p(", "V(", "v(")) and normalized.endswith(")"):
        normalized = normalized[2:-1]
    return normalized.strip().lower()


def read_waveform_csv(path: Path) -> tuple[list[float], dict[str, list[float]]]:
    with path.open("r", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        if reader.fieldnames is None:
            raise ValueError(f"waveform file missing header row: {path}")

        if "time_ps" in reader.fieldnames:
            time_field = "time_ps"
            time_scale = 1.0
        elif "time" in reader.fieldnames:
            time_field = "time"
            time_scale = 1.0e12
        else:
            raise ValueError(f"waveform file missing time_ps column: {path}")

        time_ps: list[float] = []
        trace_fields = [name for name in reader.fieldnames if name != time_field]
        traces: dict[str, list[float]] = {
            normalize_waveform_column_name(name): [] for name in trace_fields
        }
        for row in reader:
            time_ps.append(float(row[time_field]) * time_scale)
            for name in trace_fields:
                traces[normalize_waveform_column_name(name)].append(float(row[name]))
    return time_ps, traces


def interpolate_trace(time_ps: list[float], values: list[float], target_time_ps: float) -> float:
    if not time_ps:
        return 0.0
    if target_time_ps <= time_ps[0]:
        return values[0]
    if target_time_ps >= time_ps[-1]:
        return values[-1]

    lo = 0
    hi = len(time_ps) - 1
    while lo + 1 < hi:
        mid = (lo + hi) // 2
        if time_ps[mid] <= target_time_ps:
            lo = mid
        else:
            hi = mid

    left_t = time_ps[lo]
    right_t = time_ps[hi]
    left_v = values[lo]
    right_v = values[hi]
    span = max(right_t - left_t, 1.0e-18)
    alpha = (target_time_ps - left_t) / span
    return left_v + alpha * (right_v - left_v)


def compare_waveforms(
    internal_time: list[float],
    internal_traces: dict[str, list[float]],
    external_time: list[float],
    external_traces: dict[str, list[float]],
) -> dict[str, tuple[float, float]]:
    common_nodes = sorted(set(internal_traces).intersection(external_traces))
    if not common_nodes:
        raise ValueError("no common waveform columns between internal and external outputs")

    metrics: dict[str, tuple[float, float]] = {}
    for node in common_nodes:
        abs_errors: list[float] = []
        for time_point in internal_time:
            left = interpolate_trace(internal_time, internal_traces[node], time_point)
            right = interpolate_trace(external_time, external_traces[node], time_point)
            abs_errors.append(abs(left - right))
        max_abs = max(abs_errors) if abs_errors else 0.0
        rms = math.sqrt(sum(err * err for err in abs_errors) / max(len(abs_errors), 1))
        metrics[node] = (max_abs, rms)
    return metrics


def rank_nodes_by_error(
    metrics: dict[str, tuple[float, float]],
) -> list[dict[str, float | str]]:
    ranked = [
        {
            "node": node,
            "max_abs_v": max_abs,
            "rms_v": rms,
        }
        for node, (max_abs, rms) in metrics.items()
    ]
    ranked.sort(key=lambda entry: (-float(entry["max_abs_v"]), -float(entry["rms_v"]), str(entry["node"])))
    return ranked


def summarize_metrics(
    metrics: dict[str, tuple[float, float]],
    max_abs_threshold: float,
) -> dict[str, object]:
    ranked_nodes = rank_nodes_by_error(metrics)
    worst = 0.0
    failing_nodes: list[str] = []
    for entry in ranked_nodes:
        node = str(entry["node"])
        max_abs = float(entry["max_abs_v"])
        worst = max(worst, max_abs)
        if max_abs > max_abs_threshold:
            failing_nodes.append(node)

    status = "PASS" if not failing_nodes else "FAIL"
    return {
        "summary": status,
        "worst_max_abs_v": worst,
        "threshold": max_abs_threshold,
        "failing_nodes": sorted(failing_nodes),
        "top_worst_nodes": ranked_nodes[:3],
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Compare internal transient and external JoSIM waveform CSV outputs.",
    )
    parser.add_argument("deck", type=Path, help="Path to a SPICE/JoSIM deck file")
    parser.add_argument(
        "--josim-command",
        type=str,
        default="josim",
        help="External simulator command used with simulation_mode=external_josim",
    )
    parser.add_argument(
        "--max-abs-threshold",
        type=float,
        default=0.05,
        help="Maximum absolute error threshold for pass/fail summary (volts)",
    )
    parser.add_argument(
        "--json-output",
        type=Path,
        default=None,
        help="Optional path to write structured comparison results as JSON",
    )
    args = parser.parse_args()

    deck_path = args.deck.resolve()
    if not deck_path.is_file():
        raise FileNotFoundError(f"deck file not found: {deck_path}")

    internal = rflux.simulate_file(str(deck_path), simulation_mode="internal_transient")
    external = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=args.josim_command,
    )

    if internal.waveform_path is None:
        raise RuntimeError("internal_transient did not produce waveform_path")
    if external.waveform_path is None:
        raise RuntimeError("external_josim did not produce waveform_path")

    internal_time, internal_traces = read_waveform_csv(Path(internal.waveform_path))
    external_time, external_traces = read_waveform_csv(Path(external.waveform_path))

    metrics = compare_waveforms(internal_time, internal_traces, external_time, external_traces)
    summary = summarize_metrics(metrics, args.max_abs_threshold)
    print(f"deck: {deck_path}")
    print(f"internal waveform: {internal.waveform_path}")
    print(f"external waveform: {external.waveform_path}")
    print("node,max_abs_v,rms_v")
    for entry in summary["top_worst_nodes"] if len(metrics) <= 3 else rank_nodes_by_error(metrics):
        print(f"{entry['node']},{float(entry['max_abs_v']):.6e},{float(entry['rms_v']):.6e}")

    print(
        f"summary={summary['summary']},"
        f"worst_max_abs_v={float(summary['worst_max_abs_v']):.6e},"
        f"threshold={float(summary['threshold']):.6e}"
    )
    if summary["failing_nodes"]:
        print("failing_nodes=" + ",".join(summary["failing_nodes"]))
    if summary["top_worst_nodes"]:
        print(
            "top_worst_nodes="
            + ",".join(f"{entry['node']}:{float(entry['max_abs_v']):.6e}" for entry in summary["top_worst_nodes"])
        )

    if args.json_output is not None:
        payload = {
            "deck": str(deck_path),
            "internal_waveform": str(internal.waveform_path),
            "external_waveform": str(external.waveform_path),
            "threshold": summary["threshold"],
            "worst_max_abs_v": summary["worst_max_abs_v"],
            "summary": summary["summary"],
            "failing_nodes": summary["failing_nodes"],
            "top_worst_nodes": summary["top_worst_nodes"],
            "nodes": {
                node: {"max_abs_v": max_abs, "rms_v": rms}
                for node, (max_abs, rms) in metrics.items()
            },
        }
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(json.dumps(payload, indent=2), encoding="utf-8")

    if summary["summary"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
