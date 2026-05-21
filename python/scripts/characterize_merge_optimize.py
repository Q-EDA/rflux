# Characterize a compound cell, merge its library artifact into a PDK,
# and run library-aware design optimization on a consumer circuit.
from __future__ import annotations

import rflux


def main() -> None:
    char_circuit = rflux.Circuit("compound")
    source = char_circuit.add_node("port", "source")
    gate = char_circuit.add_node("cell", "gate")
    sink = char_circuit.add_node("port", "sink")
    char_circuit.connect(source, 0, gate, 0)
    char_circuit.connect(gate, 0, sink, 0)

    char_report = rflux.characterize_compound_cell(char_circuit, cell_name="macro_buf")
    print(f"characterized cell: {char_report.cell_name}")
    print(f"intrinsic delay (ps): {char_report.derived_intrinsic_delay_ps:.2f}")
    print(f"library json bytes: {len(char_report.generated_library_json)}")
    assert "arc_delays" in char_report.generated_library_json or "metadata" in char_report.generated_library_json

    merged_json = rflux.merge_characterized_library([char_report.generated_library_json])
    print(f"merged pdk json bytes: {len(merged_json)}")

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
    print(f"consumer critical path (ps): {timing.critical_path_delay_ps:.2f}")

    design = rflux.optimize_design_with_characterized_library(
        consumer,
        [char_report.generated_library_json],
    )
    print(f"design optimization score: {design.design_optimization_score:.3f}")
    print(
        "baseline pessimistic setup slack (ps): "
        f"{design.baseline_statistical.worst_pessimistic_setup_slack_ps:.2f}"
    )
    print(
        "optimized pessimistic setup slack (ps): "
        f"{design.optimized_statistical.worst_pessimistic_setup_slack_ps:.2f}"
    )
    print(f"routing optimization applied: {design.ac_bias.optimization_applied}")
    print(f"placement halo scale: {design.baseline_placement_halo_scale:.2f} -> {design.optimized_placement_halo_scale:.2f}")
    print(
        "cell delay sigma ratio: "
        f"{design.baseline_cell_delay_sigma_ratio:.3f} -> {design.optimized_cell_delay_sigma_ratio:.3f}"
    )


if __name__ == "__main__":
    main()
