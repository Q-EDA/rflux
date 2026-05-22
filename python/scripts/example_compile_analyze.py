# Demonstrate an end-to-end Python flow from circuit construction to layout, timing, and AC-bias reports.
from __future__ import annotations

import json

import rflux


def build_demo_circuit() -> tuple[rflux.Circuit, int]:
    circuit = rflux.Circuit("xor_pipeline")
    src_a = circuit.add_node("port", "a")
    src_b = circuit.add_node("port", "b")
    xor0 = circuit.add_node("cell", "xor0", logic_op="xor")
    out = circuit.add_node("port", "out")

    circuit.connect(src_a, 0, xor0, 0)
    circuit.connect(src_b, 0, xor0, 1)
    circuit.connect(xor0, 0, out, 0)
    return circuit, xor0


def main() -> None:
    circuit, xor0 = build_demo_circuit()
    layout = rflux.compile_layout(
        circuit,
        timing_constraints=[rflux.NodeTimingConstraint(node=xor0, required_ps=120.0)],
    )
    timing = rflux.analyze_timing(
        circuit,
        timing_constraints=[rflux.NodeTimingConstraint(node=xor0, required_ps=120.0)],
    )
    ac_bias = rflux.optimize_ac_bias(circuit)

    summary = {
        "design": circuit.name,
        "layout": {
            "placed_nodes": layout.placed_nodes,
            "routed_nets": layout.routed_nets,
            "critical_path_delay_ps": layout.critical_path_delay_ps,
            "worst_setup_slack_ps": layout.worst_setup_slack_ps,
        },
        "timing": {
            "analyzed_timing_arcs": timing.analyzed_timing_arcs,
            "critical_path_delay_ps": timing.critical_path_delay_ps,
            "worst_setup_slack_ps": timing.worst_setup_slack_ps,
        },
        "ac_bias": {
            "baseline_score": ac_bias.baseline.optimization_score,
            "optimized_score": ac_bias.optimized.optimization_score,
            "optimization_applied": ac_bias.optimization_applied,
        },
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()