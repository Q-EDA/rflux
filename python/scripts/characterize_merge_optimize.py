# Characterize a compound cell, merge its library artifact into a PDK,
# and run library-aware design optimization on a consumer circuit.
from __future__ import annotations

import json

import rflux


def main() -> None:
    char_circuit = rflux.Circuit("compound")
    source = char_circuit.add_node("port", "source")
    gate = char_circuit.add_node("cell", "gate")
    sink = char_circuit.add_node("port", "sink")
    char_circuit.connect(source, 0, gate, 0)
    char_circuit.connect(gate, 0, sink, 0)

    char_report = rflux.characterize_compound_cell(char_circuit, cell_name="macro_buf")

    merged_json = rflux.merge_characterized_library([char_report.generated_library_json])

    consumer = rflux.Circuit("consumer")
    consumer_source = consumer.add_node("port", "consumer_source")
    macro_buf = consumer.add_node("macro", "macro_buf")
    consumer_sink = consumer.add_node("dff", "consumer_sink")
    consumer.connect(consumer_source, 0, macro_buf, 0)
    consumer.connect(macro_buf, 0, consumer_sink, 0)

    timing = rflux.analyze_timing(
        consumer,
        characterized_library_entries=[char_report.generated_library_json],
    )

    design = rflux.optimize_design_with_characterized_library(
        consumer,
        [char_report.generated_library_json],
    )

    summary = {
        "characterization": {
            "cell_name": char_report.cell_name,
            "derived_intrinsic_delay_ps": char_report.derived_intrinsic_delay_ps,
            "generated_library_json_bytes": len(char_report.generated_library_json),
        },
        "library_merge": {
            "merged_json_bytes": len(merged_json),
        },
        "timing": {
            "critical_path_delay_ps": timing.critical_path_delay_ps,
            "analyzed_timing_arcs": timing.analyzed_timing_arcs,
            "worst_setup_slack_ps": timing.worst_setup_slack_ps,
        },
        "design_optimization": {
            "design_optimization_score": design.design_optimization_score,
            "optimization_applied": design.ac_bias.optimization_applied,
            "baseline_worst_pessimistic_setup_slack_ps": design.baseline_statistical.worst_pessimistic_setup_slack_ps,
            "optimized_worst_pessimistic_setup_slack_ps": design.optimized_statistical.worst_pessimistic_setup_slack_ps,
            "baseline_placement_halo_scale": design.baseline_placement_halo_scale,
            "optimized_placement_halo_scale": design.optimized_placement_halo_scale,
            "baseline_cell_delay_sigma_ratio": design.baseline_cell_delay_sigma_ratio,
            "optimized_cell_delay_sigma_ratio": design.optimized_cell_delay_sigma_ratio,
        },
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
