from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path

import rflux


def read_waveform_csv(path: Path) -> tuple[list[float], dict[str, list[float]]]:
    with path.open("r", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        if reader.fieldnames is None or "time_ps" not in reader.fieldnames:
            raise ValueError(f"waveform file missing time_ps column: {path}")

        time_ps: list[float] = []
        traces: dict[str, list[float]] = {
            name: [] for name in reader.fieldnames if name != "time_ps"
        }
        for row in reader:
            time_ps.append(float(row["time_ps"]))
            for name in traces:
                traces[name].append(float(row[name]))
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
    print(f"deck: {deck_path}")
    print(f"internal waveform: {internal.waveform_path}")
    print(f"external waveform: {external.waveform_path}")
    print("node,max_abs_v,rms_v")
    worst = 0.0
    for node, (max_abs, rms) in metrics.items():
        worst = max(worst, max_abs)
        print(f"{node},{max_abs:.6e},{rms:.6e}")

    status = "PASS" if worst <= args.max_abs_threshold else "FAIL"
    print(f"summary={status},worst_max_abs_v={worst:.6e},threshold={args.max_abs_threshold:.6e}")

    if args.json_output is not None:
        payload = {
            "deck": str(deck_path),
            "internal_waveform": str(internal.waveform_path),
            "external_waveform": str(external.waveform_path),
            "threshold": args.max_abs_threshold,
            "worst_max_abs_v": worst,
            "summary": status,
            "nodes": {
                node: {"max_abs_v": max_abs, "rms_v": rms}
                for node, (max_abs, rms) in metrics.items()
            },
        }
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(json.dumps(payload, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
