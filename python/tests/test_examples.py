from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_DIR = REPO_ROOT / "python" / "scripts"


def run_example(script_name: str) -> dict[str, object]:
    completed = subprocess.run(
        [sys.executable, str(SCRIPTS_DIR / script_name)],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
    )
    assert completed.returncode == 0, completed.stderr + "\n" + completed.stdout
    return json.loads(completed.stdout)


def test_compile_analyze_example_runs_end_to_end() -> None:
    payload = run_example("example_compile_analyze.py")

    assert payload["design"] == "xor_pipeline"
    layout = payload["layout"]
    assert layout["placed_nodes"] >= 3
    assert layout["routed_nets"] >= 2
    assert layout["critical_path_delay_ps"] > 0.0

    timing = payload["timing"]
    assert timing["analyzed_timing_arcs"] >= 1
    assert timing["worst_setup_slack_ps"] <= 120.0

    ac_bias = payload["ac_bias"]
    assert ac_bias["baseline_score"] > 0.0
    assert ac_bias["optimized_score"] > 0.0


def test_equivalence_example_reports_match_and_mismatch() -> None:
    payload = run_example("example_equivalence_check.py")

    equivalent_case = payload["equivalent_case"]
    assert equivalent_case["equivalent"] is True
    assert equivalent_case["checked_outputs"] == ["out"]
    assert equivalent_case["counterexample_inputs"] == {}

    mismatch_case = payload["mismatch_case"]
    assert mismatch_case["equivalent"] is False
    assert mismatch_case["counterexample_inputs"] == {"a": True, "b": False}
    assert mismatch_case["counterexample_outputs"]["out"] == {"lhs": False, "rhs": True}


def test_internal_transient_example_reports_waveform_summary() -> None:
    payload = run_example("example_simulate_internal_transient.py")

    assert payload["backend"] == "internal_transient_completed"
    assert payload["simulated_events"] == 6
    assert payload["reported_worst_delay_ps"] == 0.001
    assert payload["waveform_path"]


def test_benchmark_file_example_reports_internal_transient_summary() -> None:
    payload = run_example("example_simulate_benchmark_file.py")

    assert payload["deck_path"].endswith("python\\tests\\benchmarks\\phase6\\t_delay_smoke.cir")
    assert payload["backend"] == "internal_transient_completed"
    assert payload["simulated_events"] > 0
    assert payload["external_result"] == "internal_transient_linear_rc"
    assert payload["waveform_path"]


def test_characterize_merge_optimize_example_reports_design_loop_summary() -> None:
    payload = run_example("characterize_merge_optimize.py")

    characterization = payload["characterization"]
    assert characterization["cell_name"] == "macro_buf"
    assert characterization["derived_intrinsic_delay_ps"] > 0.0
    assert characterization["generated_library_json_bytes"] > 0

    library_merge = payload["library_merge"]
    assert library_merge["merged_json_bytes"] > 0

    timing = payload["timing"]
    assert timing["critical_path_delay_ps"] > 0.0
    assert timing["analyzed_timing_arcs"] >= 1

    design = payload["design_optimization"]
    assert design["design_optimization_score"] > 0.0
    assert isinstance(design["optimization_applied"], bool)
    assert design["baseline_placement_halo_scale"] > 0.0
    assert design["optimized_placement_halo_scale"] > 0.0


def test_bench_cli_flow_example_reports_compile_and_equivalence_summary() -> None:
    payload = run_example("example_bench_cli_flow.py")

    assert payload["bench_fixture"].endswith("crates\\synth\\tests\\fixtures\\quaigh_alignment\\bench\\dedup_and_pair.bench")

    compile_report = payload["compile"]
    assert compile_report["schema_version"] == 1
    assert compile_report["node_count"] > 0
    assert compile_report["edge_count"] > 0
    assert compile_report["gate_count_before"] >= compile_report["gate_count_after"]
    assert compile_report["mapped_nodes"] > 0

    equivalence_report = payload["equivalence"]
    assert equivalence_report["schema_version"] == 1
    assert equivalence_report["kind"] == "combinational"
    assert equivalence_report["equivalent"] is True
    assert equivalence_report["checked_outputs"]
    assert equivalence_report["sat_recursive_calls"] > 0


def test_run_with_diagnostics_example_reports_bundle_and_report_summary() -> None:
    payload = run_example("example_run_with_diagnostics.py")

    assert payload["bench_fixture"].endswith("crates\\synth\\tests\\fixtures\\quaigh_alignment\\bench\\dedup_and_pair.bench")

    bundle = payload["bundle"]
    assert bundle["schema_version"] == 1
    assert bundle["kind"] == "diagnostics_bundle"
    assert bundle["invocation_command"] == "compile-netlist"
    assert bundle["status"] == "succeeded"
    assert bundle["captured_input_count"] >= 1
    assert bundle["captured_report_count"] >= 1
    assert bundle["events_recorded"] >= 3

    compile_report = payload["compile_report"]
    assert compile_report["schema_version"] == 1
    assert compile_report["kind"] == "compile_netlist"
    assert compile_report["node_count"] > 0
    assert compile_report["edge_count"] > 0


def test_equivalence_cli_replay_example_exports_and_replays() -> None:
    payload = run_example("example_equivalence_cli_replay.py")

    assert payload["fixture"].endswith("crates\\synth\\tests\\fixtures\\classic_examples\\classic_majority3.json")
    assert payload["check_ref"] == "output:maj"
    assert payload["equivalent"] is True
    assert payload["checked_outputs"] == ["maj"]

    dimacs_export = payload["dimacs_export"]
    assert dimacs_export["schema_version"] == 1
    assert dimacs_export["variables"] > 0
    assert dimacs_export["clauses"] > 0
    assert dimacs_export["path"].endswith("majority3.cnf")

    sidecar = payload["sidecar"]
    assert sidecar["schema_version"] == 1
    assert sidecar["check_count"] >= 1
    assert sidecar["first_check_ref"].startswith("output:")

    solve = payload["solve"]
    assert solve["satisfiable"] is False
    assert solve["unsat_core"]
    assert solve["equivalence_check"]["check_ref"] == "output:maj"


def test_equivalence_cli_counterexample_example_finds_sat_replay() -> None:
    payload = run_example("example_equivalence_cli_counterexample.py")

    assert payload["lhs_fixture"].endswith("crates\\synth\\tests\\fixtures\\classic_examples\\classic_majority3.json")
    assert payload["rhs_fixture"].endswith("majority3_mutated.json")
    assert payload["mutation"] == {
        "node": "or1",
        "logic_op_before": "Or",
        "logic_op_after": "Xor",
    }
    assert payload["check_ref"] == "output:maj"
    assert payload["equivalent"] is False

    counterexample_inputs = payload["counterexample_inputs"]
    assert set(counterexample_inputs) == {"a", "b", "c"}
    assert payload["counterexample_outputs"]["maj"]["lhs"] != payload["counterexample_outputs"]["maj"]["rhs"]

    dimacs_export = payload["dimacs_export"]
    assert dimacs_export["variables"] > 0
    assert dimacs_export["clauses"] > 0
    assert dimacs_export["path"].endswith("majority3_counterexample.cnf")

    sidecar = payload["sidecar"]
    assert sidecar["schema_version"] == 1
    assert sidecar["check_count"] >= 1

    solve = payload["solve"]
    assert solve["satisfiable"] is True
    assert solve["model"]
    assert solve["unsat_core"] is None
    assert solve["equivalence_check"]["check_ref"] == "output:maj"