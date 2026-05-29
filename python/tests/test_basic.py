import json
import os
import shutil
import stat
import inspect
from pathlib import Path

import pytest
import rflux


def test_python_package_version():
    assert rflux.__version__ == "0.1.0"


def test_circuit_minimal_api():
    circuit = rflux.Circuit("demo")
    assert circuit is not None
    assert circuit.node_count() == 0
    assert circuit.edge_count() == 0


def test_circuit_node_and_edge_listing():
    circuit = rflux.Circuit("demo")
    src = circuit.add_node("cell", "src", logic_op="xor")
    sink = circuit.add_node("cell", "sink")
    circuit.connect(src, 0, sink, 0)

    nodes = circuit.nodes()
    edges = circuit.edges()

    assert len(nodes) == 2
    assert nodes[0][2] == "src"
    assert len(edges) == 1
    assert edges[0][0] == (0, 0)
    assert edges[0][1] == (1, 0)


def test_core_version_callable():
    version = rflux.core_version()
    assert isinstance(version, str)
    assert len(version) > 0


def test_core_available_probe_and_public_error_types():
    assert isinstance(rflux.core_available(), bool)
    assert issubclass(rflux.RfluxCoreUnavailableError, RuntimeError)
    assert issubclass(rflux.RfluxCoreUnavailableError, rflux.RfluxError)
    assert "auto" in rflux.SIMULATION_MODES
    assert rflux.is_supported_external_command("josim") is True
    assert rflux.is_supported_external_command("python") is False


def test_core_status_reports_extension_diagnostics():
    status = rflux.core_status()

    assert isinstance(status, rflux.CoreStatus)
    assert status.available is rflux.core_available()
    assert status.version == rflux.core_version()
    if status.available:
        assert status.extension_path is not None
        assert status.import_error is None


def test_public_facades_have_docstrings_and_typed_marker():
    public_functions = [
        name
        for name in rflux.__all__
        if inspect.isfunction(getattr(rflux, name))
    ]

    assert public_functions
    assert not [
        name
        for name in public_functions
        if not inspect.getdoc(getattr(rflux, name))
    ]
    assert Path(rflux.__file__).with_name("py.typed").exists()
    assert "simulate_text" in rflux.__all__
    assert "simulate_file" in rflux.__all__
    assert "SimulationReport" in rflux.__all__


def test_structured_submodules_reexport_top_level_api():
    assert "flow" in rflux.__all__
    assert "timing" in rflux.__all__
    assert "sim" in rflux.__all__
    assert "verify" in rflux.__all__
    assert "pdk" in rflux.__all__

    assert rflux.flow.compile is rflux.compile
    assert rflux.flow.compile_plan_report is rflux.compile_plan_report
    assert rflux.flow.compile_plan is rflux.compile_plan
    assert rflux.flow.compile_netlist is rflux.compile_netlist
    assert rflux.flow.compile_layout is rflux.compile_layout
    assert rflux.flow.analyze_ac_bias is rflux.analyze_ac_bias
    assert rflux.flow.optimize_ac_bias is rflux.optimize_ac_bias
    assert (
        rflux.flow.optimize_design_with_characterized_library
        is rflux.optimize_design_with_characterized_library
    )
    assert rflux.timing.analyze_advanced_constraints is rflux.analyze_advanced_constraints
    assert rflux.timing.analyze_timing is rflux.analyze_timing
    assert rflux.sim.simulate_text is rflux.simulate_text
    assert rflux.verify.check_equivalence is rflux.check_equivalence
    assert rflux.verify.verify_layout is rflux.verify_layout
    assert rflux.pdk.Pdk is rflux.Pdk

    package_dir = Path(rflux.__file__).parent
    assert (package_dir / "flow.pyi").exists()
    assert (package_dir / "timing.pyi").exists()
    assert (package_dir / "sim.pyi").exists()
    assert (package_dir / "verify.pyi").exists()
    assert (package_dir / "pdk.pyi").exists()
    assert (package_dir / "__init__.pyi").exists()


def test_compile_facade_returns_circuit():
    circuit = rflux.Circuit("demo")
    compiled = rflux.compile(circuit)
    assert compiled is circuit


def test_compile_facade_accepts_plan_objects():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")

    compiled = rflux.compile(
        circuit,
        rflux.CompilePlan(
            connections=[
                rflux.ConnectionSpec(
                    from_pin=rflux.PinRef(source, 0),
                    to_pin=rflux.PinRef(sink, 0),
                )
            ]
        ),
    )

    assert compiled is circuit
    assert circuit.edge_count() == 1


def test_compile_plan_report_requires_compiled_extension(monkeypatch):
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan()

    monkeypatch.setattr(rflux, "_core_compile_plan", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"requires the compiled rflux\._core extension"):
        rflux.compile_plan_report(circuit, plan)


def test_compile_netlist_requires_compiled_extension(monkeypatch):
    circuit = rflux.Circuit("demo")

    monkeypatch.setattr(rflux, "_core_compile_plan", None)
    monkeypatch.setattr(rflux, "_core_compile_netlist", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"requires the compiled rflux\._core extension"):
        rflux.compile_netlist(circuit)


def test_read_bench_text_requires_compiled_extension(monkeypatch):
    monkeypatch.setattr(rflux, "_core_read_bench_text", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"requires the compiled rflux\._core extension"):
        rflux.read_bench_text("INPUT(a)\nOUTPUT(y)\ny = BUF(a)\n")


def test_pdk_requires_compiled_extension(monkeypatch):
    monkeypatch.setattr(rflux, "_CorePdk", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"Pdk\.minimal\(\.\.\.\) requires the compiled rflux\._core extension"):
        rflux.Pdk.minimal()


def test_pdk_repr_and_constructor_validation():
    pdk = rflux.Pdk.minimal("demo-pdk")

    assert repr(pdk).startswith("Pdk(name='demo-pdk'")
    assert "cell_library=" in repr(pdk)
    with pytest.raises(ValueError, match="must not be None"):
        rflux.Pdk(None)


def test_simulate_text_requires_compiled_extension(monkeypatch):
    monkeypatch.setattr(rflux, "_core_simulate_text", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"simulate_text\(\.\.\.\) requires the compiled rflux\._core extension"):
        rflux.simulate_text(".tran 1p 2p\n")


def test_simulation_mode_validation_lists_allowed_modes():
    with pytest.raises(ValueError, match=r"expected one of: auto, event_only, external_josim, internal_transient"):
        rflux.simulate_text(".tran 1p 2p\n", simulation_mode="mystery")


def test_simulate_file_accepts_path_objects(tmp_path):
    deck = tmp_path / "smoke.cir"
    deck.write_text(".tran 1p 2p\n", encoding="utf-8")

    report = rflux.simulate_file(deck)

    assert isinstance(report, rflux.SimulationReport)
    assert report.backend == "event_only"
    assert report.requested_mode == "auto"
    assert report.josim_alignment_level == "event_only"
    assert report.josim_alignment_available is False
    assert report.josim_quality_passed is False
    assert report.josim_quality_status == "failed_external_alignment_missing"
    assert report.waveform_format is None
    assert report.diagnostic_code is None
    assert isinstance(report.josim_next_step, str)
    assert len(report.josim_next_step) > 0
    assert report.external_summary_contract is None


def test_compile_layout_requires_compiled_extension(monkeypatch):
    circuit = rflux.Circuit("demo")

    monkeypatch.setattr(rflux, "_core_compile_layout", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"requires the compiled rflux\._core extension"):
        rflux.compile_layout(circuit)


def test_analyze_timing_requires_compiled_extension(monkeypatch):
    circuit = rflux.Circuit("demo")

    monkeypatch.setattr(rflux, "_core_analyze_timing", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"requires the compiled rflux\._core extension"):
        rflux.analyze_timing(circuit)


def test_verify_layout_requires_compiled_extension(monkeypatch):
    circuit = rflux.Circuit("demo")

    monkeypatch.setattr(rflux, "_core_verify_layout", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"requires the compiled rflux\._core extension"):
        rflux.verify_layout(circuit)


def test_analyze_timing_statistical_requires_compiled_extension(monkeypatch):
    circuit = rflux.Circuit("demo")

    monkeypatch.setattr(rflux, "_core_analyze_timing_statistical", None)

    with pytest.raises(rflux.RfluxCoreUnavailableError, match=r"requires the compiled rflux\._core extension"):
        rflux.analyze_timing_statistical(circuit)


@pytest.mark.parametrize(
    ("binding_name", "api_name", "call"),
    [
        ("_core_analyze_ac_bias", "analyze_ac_bias", lambda circuit: rflux.analyze_ac_bias(circuit)),
        ("_core_optimize_ac_bias", "optimize_ac_bias", lambda circuit: rflux.optimize_ac_bias(circuit)),
        (
            "_core_characterize_compound_cell",
            "characterize_compound_cell",
            lambda circuit: rflux.characterize_compound_cell(circuit),
        ),
        (
            "_core_analyze_advanced_constraints",
            "analyze_advanced_constraints",
            lambda circuit: rflux.analyze_advanced_constraints(circuit),
        ),
        (
            "_core_optimize_ac_bias_with_characterized_library",
            "optimize_ac_bias_with_characterized_library",
            lambda circuit: rflux.optimize_ac_bias_with_characterized_library(circuit, []),
        ),
        (
            "_core_optimize_design_with_characterized_library",
            "optimize_design_with_characterized_library",
            lambda circuit: rflux.optimize_design_with_characterized_library(circuit, []),
        ),
    ],
)
def test_advanced_facades_require_matching_core_binding(monkeypatch, binding_name, api_name, call):
    circuit = rflux.Circuit("demo")
    monkeypatch.setattr(rflux, binding_name, None)

    with pytest.raises(
        rflux.RfluxCoreUnavailableError,
        match=rf"{api_name}\(\.\.\.\) requires the compiled rflux\._core extension",
    ):
        call(circuit)


def test_merge_characterized_library_requires_core_binding(monkeypatch):
    monkeypatch.setattr(rflux, "_core_merge_characterized_library", None)

    with pytest.raises(
        rflux.RfluxCoreUnavailableError,
        match=r"merge_characterized_library\(\.\.\.\) requires the compiled rflux\._core extension",
    ):
        rflux.merge_characterized_library([])


def test_compile_plan_facade_accepts_plan_objects():
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=1, port=0),
            )
        ],
        balance_strategy=rflux.BalanceStrategy.EXPLICIT,
        balancing_sources=[rflux.PinRef(node=0, port=0)],
    )

    compiled = rflux.compile_plan(circuit, plan)
    assert compiled is circuit
    assert plan.balance_strategy is rflux.BalanceStrategy.EXPLICIT


def test_compile_plan_supports_by_sink_level_strategy():
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=2, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=2, port=0),
                to_pin=rflux.PinRef(node=3, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=1, port=0),
                to_pin=rflux.PinRef(node=3, port=1),
            ),
        ],
        balance_strategy=rflux.BalanceStrategy.BY_SINK_LEVEL,
    )

    compiled = rflux.compile_plan(circuit, plan)
    assert compiled is circuit
    assert plan.balance_strategy is rflux.BalanceStrategy.BY_SINK_LEVEL


def test_compile_plan_report_returns_counts():
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=1, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=2, port=0),
            ),
        ]
    )

    report = rflux.compile_plan_report(circuit, plan)
    assert report.connections_applied == 2
    assert report.splitters_inserted == 1


def test_pdk_cell_library_entries_expose_effective_timing():
    pdk = rflux.Pdk.minimal("library-api")

    entries = pdk.cell_library_entries()
    by_name = {entry.name: entry for entry in entries}
    gate = by_name["sfq_gate"]

    assert pdk.cell_library_name == "minimal-sfq"
    assert pdk.cell_library_version == "0.1.0"
    assert pdk.cell_library_source == "rflux-minimal"
    assert pdk.cell_library_metadata() == rflux.CellLibraryMetadata(
        name="minimal-sfq",
        version="0.1.0",
        source="rflux-minimal",
    )
    assert pdk.cell_library_kinds() == [
        "generic_gate",
        "macro",
        "splitter",
        "dff",
        "jtl",
        "ptl",
        "port",
    ]
    summary = pdk.cell_library_summary()
    assert isinstance(summary, rflux.CellLibrarySummary)
    assert summary.cell_count == 7
    assert summary.kind_count == 7
    assert summary.kind_counts["generic_gate"] == 1
    assert summary.kind_counts["macro"] == 1
    assert summary.named_timing_count == 0
    assert summary.kind_timing_count == 7
    assert summary.missing_timing_count == 0
    assert summary.characterized_cell_count == 0
    assert summary.named_timing_cells == []
    assert summary.missing_timing_cells == []
    assert summary.characterized_cells == []
    assert len(entries) >= 7
    assert isinstance(gate, rflux.CellLibraryEntry)
    assert gate.kind == "generic_gate"
    assert gate.timing_source == "kind"
    assert gate.intrinsic_delay_ps == 8.0
    assert not gate.has_characterization_metadata
    assert pdk.cell_library_entry("sfq_gate") == gate
    assert pdk.cell_library_entry("missing") is None
    assert [entry.name for entry in pdk.cell_library_entries_by_kind("macro")] == ["sfq_macro"]
    assert [entry.name for entry in pdk.cell_library_entries_by_kind("Macro")] == ["sfq_macro"]


def test_pdk_cell_library_entries_reflect_characterized_cells():
    pdk = rflux.Pdk.minimal("library-api").merge_characterized_library_json(
        json.dumps(
            {
                "cell": {
                    "name": "compound_buf",
                    "kind": "Macro",
                    "area_um2": 52.0,
                    "pipeline_stages": 2,
                },
                "timing": {
                    "kind": "Macro",
                    "intrinsic_delay_ps": 17.5,
                    "setup_ps": 8.5,
                    "hold_ps": 5.5,
                },
                "metadata": {
                    "waveform_path": "compound.raw",
                    "simulated_delay_ps": 18.0,
                    "delay_calibration_sigma_ps": 0.5,
                },
            }
        )
    )

    entry = next(entry for entry in pdk.cell_library_entries() if entry.name == "compound_buf")

    assert entry.kind == "macro"
    assert entry.area_um2 == 52.0
    assert entry.pipeline_stages == 2
    assert entry.intrinsic_delay_ps == 17.5
    assert entry.timing_source == "named"
    assert entry.has_characterization_metadata
    summary = pdk.cell_library_summary()
    assert summary.kind_counts["macro"] == 2
    assert summary.named_timing_count == 1
    assert summary.characterized_cell_count == 1
    assert summary.missing_timing_count == 0
    assert summary.named_timing_cells == ["compound_buf"]
    assert summary.characterized_cells == ["compound_buf"]
    assert summary.missing_timing_cells == []


def test_pdk_timing_corner_api_exposes_active_overlay():
    pdk = rflux.Pdk.from_json(
        json.dumps(
            {
                "name": "corner-api",
                "metal_layers": 2,
                "ptl_forbidden_ranges": [],
                "cell_library": {
                    "name": "minimal-sfq",
                    "version": "0.1.0",
                    "source": "rflux-minimal",
                    "cells": [
                        {
                            "name": "sfq_gate",
                            "kind": "GenericGate",
                            "area_um2": 12.0,
                            "pipeline_stages": 1,
                        }
                    ],
                },
                "cell_timing": [
                    {
                        "kind": "GenericGate",
                        "intrinsic_delay_ps": 8.0,
                        "setup_ps": 5.0,
                        "hold_ps": 3.0,
                    }
                ],
                "named_cell_timing": [],
                "characterized_cell_metadata": [],
                "interconnect_timing": [],
                "active_timing_corner": "slow",
                "timing_corners": [
                    {
                        "name": "slow",
                        "process": "ss",
                        "voltage_v": 2.4,
                        "temperature_k": 4.2,
                        "cell_timing": [
                            {
                                "kind": "GenericGate",
                                "intrinsic_delay_ps": 24.0,
                                "setup_ps": 7.0,
                                "hold_ps": 4.0,
                            }
                        ],
                    }
                ],
            }
        )
    )

    assert pdk.active_timing_corner == "slow"
    assert pdk.timing_corner_names() == ["slow"]
    gate = pdk.cell_library_entry("sfq_gate")
    assert gate is not None
    assert gate.timing_source == "corner_kind"
    assert gate.intrinsic_delay_ps == 24.0
    changed = pdk.with_active_timing_corner("fast")
    assert changed.active_timing_corner == "fast"
    assert pdk.active_timing_corner == "slow"


def test_analyze_timing_corners_reports_multi_corner_signoff():
    circuit = rflux.Circuit("corner-signoff")
    circuit.add_node("port", "source")
    circuit.add_node("cell", "gate")
    circuit.add_node("dff", "sink")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=1, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=1, port=0),
                to_pin=rflux.PinRef(node=2, port=0),
            ),
        ]
    )
    pdk = rflux.Pdk.from_json(
        json.dumps(
            {
                "name": "corner-api",
                "metal_layers": 2,
                "ptl_forbidden_ranges": [],
                "cell_library": {
                    "name": "minimal-sfq",
                    "version": "0.1.0",
                    "source": "rflux-minimal",
                    "cells": [
                        {
                            "name": "sfq_gate",
                            "kind": "GenericGate",
                            "area_um2": 12.0,
                            "pipeline_stages": 1,
                        },
                        {
                            "name": "sfq_dff",
                            "kind": "Dff",
                            "area_um2": 18.0,
                            "pipeline_stages": 1,
                        },
                        {
                            "name": "sfq_port",
                            "kind": "Port",
                            "area_um2": 0.0,
                            "pipeline_stages": 0,
                        },
                    ],
                },
                "cell_timing": [
                    {
                        "kind": "GenericGate",
                        "intrinsic_delay_ps": 8.0,
                        "setup_ps": 5.0,
                        "hold_ps": 3.0,
                    },
                    {
                        "kind": "Dff",
                        "intrinsic_delay_ps": 10.0,
                        "setup_ps": 7.0,
                        "hold_ps": 4.0,
                    },
                    {
                        "kind": "Port",
                        "intrinsic_delay_ps": 0.0,
                        "setup_ps": 0.0,
                        "hold_ps": 0.0,
                    },
                ],
                "named_cell_timing": [],
                "characterized_cell_metadata": [],
                "interconnect_timing": [
                    {
                        "kind": "Jtl",
                        "points": [
                            {"length_um": 0.0, "delay_ps": 8.0},
                            {"length_um": 40.0, "delay_ps": 18.0},
                        ],
                    },
                    {
                        "kind": "Ptl",
                        "points": [
                            {"length_um": 0.0, "delay_ps": 4.0},
                            {"length_um": 80.0, "delay_ps": 12.0},
                        ],
                    },
                ],
                "active_timing_corner": "slow",
                "timing_corners": [
                    {
                        "name": "slow",
                        "cell_timing": [
                            {
                                "kind": "GenericGate",
                                "intrinsic_delay_ps": 28.0,
                                "setup_ps": 8.0,
                                "hold_ps": 4.0,
                            }
                        ],
                        "interconnect_timing": [
                            {
                                "kind": "Jtl",
                                "points": [
                                    {"length_um": 0.0, "delay_ps": 8.0},
                                    {"length_um": 40.0, "delay_ps": 24.0},
                                ],
                            }
                        ],
                    }
                ],
            }
        )
    )

    report = rflux.analyze_timing_corners(circuit, pdk, plan=plan)

    assert isinstance(report, rflux.MultiCornerTimingAnalysisReport)
    assert report.active_timing_corner == "slow"
    assert report.corner_count == 2
    assert [corner.corner_name for corner in report.corners] == ["default", "slow"]
    assert report.corners[0].is_default_corner
    assert report.corners[1].is_active_corner
    assert report.worst_setup_corner == "slow"
    assert report.worst_critical_path_corner == "slow"
    assert report.corners[1].critical_path_delay_ps > report.corners[0].critical_path_delay_ps


def test_compile_plan_report_tracks_level_balancing():
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=2, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=2, port=0),
                to_pin=rflux.PinRef(node=3, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=1, port=0),
                to_pin=rflux.PinRef(node=3, port=1),
            ),
        ],
        balance_strategy=rflux.BalanceStrategy.BY_SINK_LEVEL,
    )

    report = rflux.compile_plan_report(circuit, plan)
    assert report.connections_applied == 3
    assert report.balancing_dffs_inserted == 1


def test_circuit_holds_netlist_state_after_compile_plan():
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=1, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=2, port=0),
            ),
        ]
    )

    report = rflux.compile_plan_report(circuit, plan)
    assert report.splitters_inserted == 1
    assert circuit.node_count() == 4
    assert circuit.edge_count() == 3


def test_circuit_json_roundtrip():
    circuit = rflux.Circuit("demo")
    src = circuit.add_node("cell", "src")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(src, 0, sink, 0)

    payload = circuit.to_json()
    restored = rflux.Circuit.from_json(payload, name="restored")

    assert restored.node_count() == 2
    assert restored.edge_count() == 1
    assert restored.nodes()[1][1] in {"dff", "NodeKind.Dff", "Dff"}


def test_read_bench_text_returns_circuit():
    circuit = rflux.read_bench_text(
        "INPUT(a)\nINPUT(b)\nOUTPUT(y)\ny = XOR(a, b)\n",
        name="bench-demo",
    )

    assert circuit.name == "bench-demo"
    assert circuit.node_count() == 4
    assert circuit.edge_count() == 3
    assert circuit.nodes()[2][2] == "y"


def test_read_bench_file_returns_circuit(tmp_path):
    bench_path = tmp_path / "sample.logic"
    bench_path.write_text("INPUT(a)\nOUTPUT(y)\ny = BUF(a)\n", encoding="utf-8")

    circuit = rflux.read_bench_file(bench_path)

    assert circuit.node_count() == 3
    assert circuit.edge_count() == 2


def test_compile_netlist_returns_unified_summary():
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=2, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=1, port=0),
                to_pin=rflux.PinRef(node=2, port=1),
            ),
        ]
    )

    report = rflux.compile_netlist(circuit, plan)

    assert report.connections_applied == 2
    assert report.mapped_nodes == 3
    assert report.node_count == 3
    assert report.edge_count == 2
    assert report.bool_opt_compatible is True


def test_compile_layout_returns_physical_summary():
    circuit = rflux.Circuit("demo")
    plan = rflux.CompilePlan(
        connections=[
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=0, port=0),
                to_pin=rflux.PinRef(node=2, port=0),
            ),
            rflux.ConnectionSpec(
                from_pin=rflux.PinRef(node=1, port=0),
                to_pin=rflux.PinRef(node=2, port=1),
            ),
        ]
    )

    report = rflux.compile_layout(circuit, plan)

    assert report.connections_applied == 2
    assert report.placed_nodes == 3
    assert report.routed_nets == 2
    assert report.total_route_length_um > 0.0
    assert report.clock_phase_count >= 1
    assert report.analyzed_timing_arcs == 2
    assert report.critical_path_delay_ps > 0.0
    assert report.worst_setup_slack_ps <= 120.0
    assert isinstance(report.timing_closure, rflux.TimingClosureSummary)
    assert report.timing_closure.status == "closed"
    assert report.timing_closure.closed is True
    assert isinstance(report.timing_closure_loop, rflux.TimingClosureLoopReport)
    assert report.timing_closure_loop.status == "closed"
    assert report.timing_closure_loop.hold_fix_applied is report.hold_fix_applied
    assert isinstance(report.timing_closure_loop.route_delay_optimization_attempted, bool)
    assert isinstance(report.timing_closure_loop.route_delay_optimization_applied, bool)
    assert report.timing_closure_loop.final_hold_violations == report.final_hold_violations
    assert report.initial_hold_violations >= report.final_hold_violations
    assert isinstance(report.hold_fix_applied, bool)
    assert report.setup_violations >= 0
    assert report.initial_total_detour_overhead_um >= report.total_detour_overhead_um
    assert report.total_detour_overhead_um >= 0.0
    assert report.detoured_routes >= 0
    assert isinstance(report.detour_feedback_applied, bool)
    assert report.effective_prefer_ptl_from_length_um > 0.0
    assert report.effective_detour_margin_um >= 0.0
    assert report.flow_config_patch["kind"] == rflux.FLOW_CONFIG_KIND
    assert report.flow_config_patch["schema_version"] == rflux.FLOW_CONFIG_SCHEMA_VERSION
    assert report.flow_config_patch["metadata"]["source_command"] == "compile_layout"
    assert (
        report.flow_config_patch["payload"]["routing"]["prefer_ptl_from_length_um"]
        == report.effective_prefer_ptl_from_length_um
    )
    assert (
        report.flow_config_patch["payload"]["routing"]["detour_margin_um"]
        == report.effective_detour_margin_um
    )
    assert report.node_count == 3
    assert report.edge_count == 2


def test_compile_layout_can_apply_hold_fix_closure_loop():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("cell", "source")
    sink = circuit.add_node("dff", "tight_hold_dff")
    circuit.connect(source, 0, sink, 0)

    report = rflux.compile_layout(
        circuit,
        min_hold_jtl_length_um=60.0,
        characterized_library_entries=[
            json.dumps(
                {
                    "cell": {
                        "name": "tight_hold_dff",
                        "kind": "Dff",
                        "area_um2": 24.0,
                        "pipeline_stages": 1,
                    },
                    "timing": {
                        "kind": "Dff",
                        "intrinsic_delay_ps": 6.0,
                        "setup_ps": 12.0,
                        "hold_ps": 20.0,
                    },
                }
            )
        ],
    )

    assert report.timing_closure_loop.hold_fix_attempted is True
    assert report.timing_closure_loop.hold_fix_applied is True
    assert report.timing_closure_loop.initial_hold_violations > 0
    assert report.timing_closure_loop.final_hold_violations == 0
    assert report.timing_closure.status == "closed"
    assert report.flow_config_patch["metadata"]["hold_fix_applied"] is True
    assert report.flow_config_patch["payload"]["routing"]["min_hold_jtl_length_um"] == 60.0


def test_analyze_timing_returns_standalone_summary():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_timing(circuit)

    assert report.analyzed_timing_arcs == 1
    assert report.critical_path_delay_ps > 0.0
    assert report.setup_violations >= 0
    assert report.hold_violations >= 0
    assert report.closure.status == "closed"
    assert report.closure.closed is True


def test_compile_layout_reports_reduce_route_delay_candidate():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("cell", "source")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.compile_layout(
        circuit,
        fixed_nodes=[
            rflux.FixedNodePlacement(node=source, x_um=0.0, y_um=0.0),
            rflux.FixedNodePlacement(node=sink, x_um=120.0, y_um=0.0),
        ],
        timing_constraints=[rflux.NodeTimingConstraint(node=sink, required_ps=20.0)],
    )

    assert report.timing_closure.status == "open"
    assert report.timing_closure.reduce_route_delay_actions == 1
    assert report.timing_closure_loop.reduce_route_delay_candidate_available is True
    assert report.timing_closure_loop.route_delay_optimization_attempted is True
    assert report.timing_closure_loop.route_delay_optimization_applied is False
    assert report.effective_prefer_ptl_from_length_um == 60.0
    assert report.effective_detour_margin_um == 12.0
    assert report.flow_config_patch["metadata"]["timing_closure_status"] == "open"
    assert (
        report.flow_config_patch["payload"]["routing"]["prefer_ptl_from_length_um"]
        == report.effective_prefer_ptl_from_length_um
    )
    assert report.timing_closure_loop.recommended_route_mode == "jtl"
    assert report.timing_closure_loop.recommended_prefer_ptl_from_length_um == 121.0
    assert report.timing_closure_loop.estimated_route_length_um == 120.0
    assert report.timing_closure_loop.estimated_slack_deficit_ps is not None
    assert report.timing_closure_loop.estimated_slack_deficit_ps > 0.0
    assert report.timing_closure_loop.reduce_route_delay_candidate_attempted is True
    assert report.timing_closure_loop.reduce_route_delay_candidate_improved is False
    assert report.timing_closure_loop.candidate_setup_violations == report.setup_violations
    assert report.timing_closure_loop.candidate_hold_violations == report.final_hold_violations
    assert report.timing_closure_loop.candidate_route_mode == "jtl"
    assert report.timing_closure_loop.candidate_route_length_um == 120.0
    assert report.timing_closure_loop.candidate_worst_setup_slack_ps is not None


def test_analyze_timing_accepts_clock_domain_constraints():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_timing(
        circuit,
        timing_constraints=[rflux.NodeTimingConstraint(node=sink, clock_domain=1)],
        clock_domains=[rflux.ClockDomainConstraint(id=1, period_ps=20.0)],
    )

    assert report.analyzed_timing_arcs == 1
    assert report.setup_violations == 1
    assert report.worst_setup_slack_ps < 0.0
    assert report.closure.status == "open"
    assert report.closure.failing_checks == ["setup", "capture_window"]
    assert report.closure.action_count == 2
    assert len(report.closure.actions) == 2
    assert report.closure.actions[0].check == "setup"
    assert report.closure.actions[1].check == "capture_window"
    assert report.closure.primary_action == report.closure.actions[0]
    assert report.closure.actions[0].priority == 1
    assert (
        report.closure.actions[0].remediation_kind
        == "relax_constraint_or_improve_library_timing"
    )
    assert (
        report.closure.actions[1].remediation_kind
        == "adjust_sfq_phase_or_pulse_window"
    )
    assert report.closure.reduce_route_delay_actions == 0
    assert report.closure.relax_constraint_or_improve_library_timing_actions == 1
    assert report.closure.add_hold_padding_actions == 0
    assert report.closure.adjust_sfq_phase_or_pulse_window_actions == 1
    assert report.closure.actions[0].slack_ps < 0.0
    assert report.closure.actions[0].to_pin.node == sink


def test_analyze_timing_reports_top_closure_actions():
    circuit = rflux.Circuit("demo")
    sources = [circuit.add_node("port", f"source_{index}") for index in range(4)]
    sinks = [circuit.add_node("dff", f"sink_{index}") for index in range(4)]
    for index, (source, sink) in enumerate(zip(sources, sinks)):
        circuit.connect(source, index, sink, 0)

    report = rflux.analyze_timing(
        circuit,
        timing_constraints=[
            rflux.NodeTimingConstraint(node=sink, required_ps=18.0 + index)
            for index, sink in enumerate(sinks)
        ],
    )

    assert report.setup_violations == 4
    assert report.closure.status == "open"
    assert report.closure.failing_checks == ["setup"]
    assert report.closure.action_count == 3
    assert len(report.closure.actions) == 3
    assert all(action.check == "setup" for action in report.closure.actions)
    assert report.closure.primary_action == report.closure.actions[0]
    assert [action.to_pin for action in report.closure.actions] == [
        arc.to_pin
        for arc in sorted(
            (arc for arc in report.timing_arcs if arc.setup_slack_ps < 0.0),
            key=lambda arc: arc.setup_slack_ps,
        )[:3]
    ]


def test_analyze_timing_pin_constraint_overrides_node_constraint():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_timing(
        circuit,
        timing_constraints=[rflux.NodeTimingConstraint(node=sink, required_ps=120.0)],
        pin_timing_constraints=[
            rflux.PinTimingConstraint(
                pin=rflux.PinRef(node=sink, port=0),
                required_ps=20.0,
            )
        ],
    )

    assert report.analyzed_timing_arcs == 1
    assert report.setup_violations == 1
    assert report.worst_setup_slack_ps < 0.0


def test_analyze_timing_accepts_false_path_crossing_constraints():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_timing(
        circuit,
        timing_constraints=[
            rflux.NodeTimingConstraint(node=source, clock_domain=1),
            rflux.NodeTimingConstraint(node=sink, clock_domain=2),
        ],
        clock_domains=[
            rflux.ClockDomainConstraint(id=1, period_ps=10.0),
            rflux.ClockDomainConstraint(id=2, period_ps=10.0),
        ],
        crossing_constraints=[
            rflux.CrossingConstraint(from_domain=1, to_domain=2, kind="false_path")
        ],
    )

    assert report.analyzed_timing_arcs == 1
    assert report.false_path_arcs == 1
    assert report.setup_violations == 0
    assert len(report.timing_arcs) == 1
    assert report.timing_arcs[0].is_false_path is True
    assert report.timing_arcs[0].from_pin.node == source
    assert report.timing_arcs[0].to_pin.node == sink
    assert report.timing_arcs[0].route_mode == "jtl"
    assert report.timing_arcs[0].route_length_um == 40.0
    assert report.timing_arcs[0].from_domain == 1
    assert report.timing_arcs[0].to_domain == 2
    assert report.timing_arcs[0].launch_phase == 0
    assert report.timing_arcs[0].capture_phase == 0
    assert report.timing_arcs[0].launch_window_start_ps == 0.0
    assert report.timing_arcs[0].launch_window_end_ps == 4.0
    assert report.timing_arcs[0].capture_window_start_ps == 0.0
    assert report.timing_arcs[0].capture_window_end_ps == 4.0
    assert report.timing_arcs[0].arrival_phase_offset_ps == 8.0
    assert report.timing_arcs[0].capture_window_slack_ps == -4.0
    assert report.timing_arcs[0].capture_window_violation is False


def test_analyze_timing_closure_reports_capture_window_violation():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_timing(
        circuit,
        timing_constraints=[
            rflux.NodeTimingConstraint(
                node=sink,
                required_ps=120.0,
                clock_domain=1,
            ),
        ],
        clock_domains=[rflux.ClockDomainConstraint(id=1, period_ps=10.0)],
    )

    assert report.setup_violations == 0
    assert report.hold_violations == 0
    assert report.capture_window_violations == 1
    assert report.timing_arcs[0].capture_window_violation is True
    assert report.timing_arcs[0].arrival_phase_offset_ps == 8.0
    assert report.timing_arcs[0].capture_window_slack_ps == -4.0
    assert report.closure.status == "open"
    assert report.closure.closed is False
    assert report.closure.setup_closed is True
    assert report.closure.hold_closed is True
    assert report.closure.capture_window_closed is False
    assert report.closure.capture_window_violations == 1
    assert report.closure.failing_checks == ["capture_window"]
    assert report.closure.action_count == 1
    assert report.closure.actions[0].check == "capture_window"
    assert (
        report.closure.actions[0].remediation_kind
        == "adjust_sfq_phase_or_pulse_window"
    )
    assert report.closure.adjust_sfq_phase_or_pulse_window_actions == 1


def test_analyze_timing_statistical_returns_pessimistic_summary():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_timing_statistical(circuit)

    assert report.analyzed_timing_arcs == 1
    assert report.sigma_multiplier == 3.0
    assert len(report.timing_arcs) == 1
    assert report.timing_arcs[0].setup_sigma_ps > 0.0
    assert report.timing_arcs[0].pessimistic_setup_slack_ps < report.timing_arcs[0].setup_slack_ps


def test_analyze_timing_statistical_accepts_global_sigma_parameters():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    baseline = rflux.analyze_timing_statistical(circuit)
    correlated = rflux.analyze_timing_statistical(
        circuit,
        global_cell_delay_sigma_ratio=0.05,
        global_wire_delay_sigma_ratio=0.05,
    )

    assert correlated.timing_arcs[0].setup_sigma_ps > baseline.timing_arcs[0].setup_sigma_ps
    assert (
        correlated.timing_arcs[0].pessimistic_setup_slack_ps
        < baseline.timing_arcs[0].pessimistic_setup_slack_ps
    )


def test_analyze_timing_statistical_accepts_clock_uncertainty_sigma():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    baseline = rflux.analyze_timing_statistical(circuit)
    uncertain = rflux.analyze_timing_statistical(
        circuit,
        clock_uncertainty_sigma_ps=2.5,
    )

    assert uncertain.timing_arcs[0].setup_sigma_ps > baseline.timing_arcs[0].setup_sigma_ps
    assert uncertain.timing_arcs[0].hold_sigma_ps > baseline.timing_arcs[0].hold_sigma_ps
    assert (
        uncertain.timing_arcs[0].pessimistic_setup_slack_ps
        < baseline.timing_arcs[0].pessimistic_setup_slack_ps
    )


def test_analyze_timing_statistical_accepts_cross_domain_uncertainty_sigma():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    baseline = rflux.analyze_timing_statistical(
        circuit,
        clock_domains=[
            rflux.ClockDomainConstraint(1, 10.0),
            rflux.ClockDomainConstraint(2, 10.0),
        ],
        timing_constraints=[
            rflux.NodeTimingConstraint(node=source, clock_domain=1),
            rflux.NodeTimingConstraint(node=sink, clock_domain=2),
        ],
    )
    uncertain = rflux.analyze_timing_statistical(
        circuit,
        clock_domains=[
            rflux.ClockDomainConstraint(1, 10.0),
            rflux.ClockDomainConstraint(2, 10.0),
        ],
        timing_constraints=[
            rflux.NodeTimingConstraint(node=source, clock_domain=1),
            rflux.NodeTimingConstraint(node=sink, clock_domain=2),
        ],
        cross_domain_uncertainty_sigma_ps=1.5,
    )

    assert uncertain.timing_arcs[0].setup_sigma_ps > baseline.timing_arcs[0].setup_sigma_ps
    assert uncertain.timing_arcs[0].hold_sigma_ps > baseline.timing_arcs[0].hold_sigma_ps


def test_analyze_timing_statistical_accepts_multicycle_crossing_uncertainty_sigma():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    baseline = rflux.analyze_timing_statistical(
        circuit,
        clock_domains=[
            rflux.ClockDomainConstraint(1, 10.0),
            rflux.ClockDomainConstraint(2, 10.0),
        ],
        timing_constraints=[
            rflux.NodeTimingConstraint(node=source, clock_domain=1),
            rflux.NodeTimingConstraint(node=sink, clock_domain=2),
        ],
        crossing_constraints=[rflux.CrossingConstraint(1, 2, "multicycle", cycles=2)],
        cross_domain_uncertainty_sigma_ps=1.0,
    )
    categorized = rflux.analyze_timing_statistical(
        circuit,
        clock_domains=[
            rflux.ClockDomainConstraint(1, 10.0),
            rflux.ClockDomainConstraint(2, 10.0),
        ],
        timing_constraints=[
            rflux.NodeTimingConstraint(node=source, clock_domain=1),
            rflux.NodeTimingConstraint(node=sink, clock_domain=2),
        ],
        crossing_constraints=[rflux.CrossingConstraint(1, 2, "multicycle", cycles=2)],
        cross_domain_uncertainty_sigma_ps=1.0,
        multicycle_cross_domain_uncertainty_sigma_ps=2.0,
    )

    assert categorized.timing_arcs[0].setup_sigma_ps > baseline.timing_arcs[0].setup_sigma_ps


def test_optimize_ac_bias_returns_before_after_summary():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("cell", "source")
    sink = circuit.add_node("cell", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.optimize_ac_bias(
        circuit,
        fixed_nodes=[
            rflux.FixedNodePlacement(node=source, x_um=0.0, y_um=0.0),
            rflux.FixedNodePlacement(node=sink, x_um=120.0, y_um=0.0),
        ],
    )

    assert report.threshold_candidates_evaluated >= 1
    assert report.detour_margin_candidates_evaluated >= 1
    assert report.optimized.optimization_score >= report.baseline.optimization_score
    assert report.optimized.timing_guardband_score >= 0.0
    assert report.optimized.feasibility_score >= report.baseline.feasibility_score



def test_analyze_ac_bias_returns_feasibility_summary():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_ac_bias(circuit)

    assert report.routed_nets == 1
    assert report.jtl_carrier_candidates == 1
    assert report.estimated_static_power_savings_uw > 0.0
    assert report.worst_setup_slack_ps > 0.0
    assert report.worst_hold_slack_ps >= 0.0
    assert 0.0 <= report.timing_guardband_score <= 1.0
    assert 0.0 <= report.feasibility_score <= 1.0
    assert 0.0 <= report.optimization_score <= 1.0


def test_characterize_compound_cell_returns_timing_library_ready_summary():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    gate = circuit.add_node("cell", "gate")
    sink = circuit.add_node("port", "y")
    circuit.connect(source, 0, gate, 0)
    circuit.connect(gate, 0, sink, 0)

    report = rflux.characterize_compound_cell(circuit, cell_name="macro_buf")

    assert report.cell_name == "macro_buf"
    assert report.node_count >= 2
    assert report.derived_intrinsic_delay_ps > 0.0
    assert report.generated_cell_kind == "macro"
    assert report.generated_pipeline_stages >= 1
    assert "macro_buf" in report.generated_library_json
    assert "metadata" in report.generated_library_json
    assert "sta_derived_delay_ps" in report.generated_library_json
    assert report.generated_deck_lines > 0


def test_analyze_advanced_constraints_reports_budget_violations():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("cell", "source")
    sink = circuit.add_node("cell", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.analyze_advanced_constraints(
        circuit,
        fixed_nodes=[
            rflux.FixedNodePlacement(node=source, x_um=0.0, y_um=0.0),
            rflux.FixedNodePlacement(node=sink, x_um=120.0, y_um=0.0),
        ],
        blocked_regions=[rflux.BlockedRegion(min_x_um=40.0, max_x_um=60.0, min_y_um=-4.0, max_y_um=4.0)],
        max_estimated_thermal_load_uw=0.05,
        max_estimated_mechanical_stress_score=0.05,
        max_jtl_density_per_100um=0.05,
        max_detour_overhead_ratio=0.01,
        max_ptl_coupling_ratio=0.01,
    )

    assert report.violation_count >= 1
    assert report.manufacturing_hotspots > 0
    assert any(item.category == "thermal" for item in report.violations)


def test_verify_layout_reports_ptl_macro_boundary_checks():
    circuit = rflux.Circuit("demo")
    macro = circuit.add_node("macro", "macro")
    sink = circuit.add_node("cell", "sink")
    circuit.connect(macro, 0, sink, 0)

    report = rflux.verify_layout(
        circuit,
        None,
        fixed_nodes=[
            rflux.FixedNodePlacement(node=macro, x_um=0.0, y_um=0.0),
            rflux.FixedNodePlacement(node=sink, x_um=120.0, y_um=0.0),
        ],
    )

    assert report.checked_routes == 1
    assert report.checked_ptl_routes == 1
    assert report.ptl_macro_boundary_violations == 1
    assert report.simulation_backend == "event_only"
    assert report.requested_mode == "auto"
    assert report.josim_alignment_level == "event_only"
    assert report.josim_alignment_available is False
    assert report.josim_quality_passed is False
    assert report.josim_quality_status == "failed_external_alignment_missing"
    assert report.waveform_format is None
    assert report.diagnostic_code is None
    assert report.external_summary_contract is None
    assert isinstance(report.josim_next_step, str)
    assert len(report.josim_next_step) > 0
    assert report.simulated_events > 0


def test_verify_layout_reports_missing_external_simulator_with_deck_path():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    missing_simulator = Path.cwd() / "__missing_rflux_simulator__" / "josim"
    if os.name == "nt":
        missing_simulator = missing_simulator.with_suffix(".exe")

    report = rflux.verify_layout(
        circuit,
        external_command=str(missing_simulator),
    )

    assert report.simulation_backend == "external_unavailable"
    assert report.requested_mode == "auto"
    assert report.josim_alignment_level == "unavailable"
    assert report.josim_alignment_available is False
    assert report.josim_quality_passed is False
    assert report.josim_quality_status == "failed_external_alignment_missing"
    assert report.diagnostic_code == "external_command_spawn_failed"
    assert report.generated_deck_lines > 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is None
    assert report.reported_violations == 0
    assert report.external_result == "external_command_spawn_failed"


def test_verify_layout_event_only_mode_ignores_external_command():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.verify_layout(
        circuit,
        simulation_mode="event_only",
        external_command="__missing_rflux_simulator__",
    )

    assert report.simulation_backend == "event_only"
    assert report.generated_deck_path is None
    assert report.external_result is None


def test_verify_layout_internal_transient_mode_reports_unavailable():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.verify_layout(
        circuit,
        simulation_mode="internal_transient",
    )

    assert report.simulation_backend == "internal_transient_completed"
    assert report.requested_mode == "internal_transient"
    assert report.josim_alignment_level == "internal_transient"
    assert report.josim_alignment_available is False
    assert report.josim_quality_passed is False
    assert report.josim_quality_status == "failed_external_alignment_missing"
    assert report.waveform_format == "csv_v1"
    assert report.diagnostic_code is None
    assert report.external_summary_contract is None
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_parses_param_tran_and_returns_event_only_report():
    report = rflux.simulate_text(
        ".title demo\n"
        ".param tstep=0.5p tstop=20p\n"
        "R1 n1 0 50\n"
        ".tran tstep tstop\n"
        ".end\n",
        simulation_mode="event_only",
    )

    assert report.backend == "event_only"
    assert report.requested_mode == "event_only"
    assert report.josim_alignment_level == "event_only"
    assert report.josim_alignment_available is False
    assert report.josim_quality_passed is False
    assert report.josim_quality_status == "failed_external_alignment_missing"
    assert report.waveform_format is None
    assert report.diagnostic_code is None
    assert report.external_summary_contract is None
    assert report.simulated_events == 1
    assert report.generated_deck_lines == 5
    assert report.external_result is None


def test_simulate_file_resolves_include_and_returns_event_only_report(tmp_path):
    include_file = tmp_path / "defs.inc"
    include_file.write_text(
        ".param tstep=0.5p tstop=20p\n"
        "R1 n1 0 50\n",
        encoding="utf-8",
    )
    deck_file = tmp_path / "top.cir"
    deck_file.write_text(
        ".title demo\n"
        ".include \"defs.inc\"\n"
        ".tran tstep tstop\n"
        ".end\n",
        encoding="utf-8",
    )

    report = rflux.simulate_file(
        str(deck_file),
        simulation_mode="event_only",
    )

    assert report.backend == "event_only"
    assert report.simulated_events == 1
    assert report.generated_deck_lines >= 5
    assert report.external_result is None


def test_simulate_text_supports_subckt_param_override():
    report = rflux.simulate_text(
        ".subckt stage in out rval=50\n"
        "R1 in out rval\n"
        ".ends\n"
        "X1 n1 n2 stage rval=75\n"
        ".tran 1p 10p\n"
        ".end\n",
        simulation_mode="event_only",
    )

    assert report.backend == "event_only"
    assert report.simulated_events == 1
    assert report.external_result is None


def test_simulate_text_supports_subckt_params_marker_override():
    report = rflux.simulate_text(
        ".subckt stage in out params: rval=50\n"
        "R1 in out rval\n"
        ".ends\n"
        "X1 n1 n2 stage params: rval=75\n"
        ".tran 1p 10p\n"
        ".end\n",
        simulation_mode="event_only",
    )

    assert report.backend == "event_only"
    assert report.simulated_events == 1
    assert report.external_result is None


def test_simulate_text_internal_transient_completes_for_passive_source_only_deck():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 DC 1m\n"
        "R1 in out 50\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.requested_mode == "internal_transient"
    assert report.josim_alignment_level == "internal_transient"
    assert report.josim_alignment_available is False
    assert report.josim_quality_passed is False
    assert report.josim_quality_status == "failed_external_alignment_missing"
    assert report.waveform_format == "csv_v1"
    assert report.diagnostic_code is None
    assert report.external_summary_contract is None
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.reported_worst_delay_ps == 0.001
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_pulse_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p,6p)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 6
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_reports_measurement_details():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p,6p)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran out_peak max V(out)\n"
        ".measure tran out_rms rms V(out)\n"
        ".measure tran out_final final V(out)\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.delay_details == []
    assert len(report.measurement_details) == 3
    assert report.measurement_details[0].name == "out_peak"
    assert report.measurement_details[0].kind == "max"
    assert report.measurement_details[0].at_ref.node == "out"
    assert report.measurement_details[0].measured_value > 0.0
    assert report.measurement_details[1].name == "out_rms"
    assert report.measurement_details[1].kind == "rms"
    assert report.measurement_details[1].measured_value > 0.0
    assert report.measurement_details[2].name == "out_final"
    assert report.measurement_details[2].kind == "final"


def test_simulate_text_internal_transient_measurement_details_honor_time_windows():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 3p 1m 6p 0)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran early_final final V(out) FROM = 0p TO=3p\n"
        ".measure tran full_final final V(out)\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.measurement_details) == 2
    assert report.measurement_details[0].name == "early_final"
    assert report.measurement_details[0].kind == "final"
    assert report.measurement_details[1].name == "full_final"
    assert report.measurement_details[1].kind == "final"
    assert report.measurement_details[0].measured_value > report.measurement_details[1].measured_value


def test_simulate_text_internal_transient_measurement_details_support_differential_voltage():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 6p 1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran diff_final final V(in,out)\n"
        ".tran 0.5p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.measurement_details) == 1
    assert report.measurement_details[0].name == "diff_final"
    assert report.measurement_details[0].at_ref.node == "in,out"
    assert report.measurement_details[0].measured_value >= 0.0


def test_simulate_text_internal_transient_measurement_details_support_find_at():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 6p 1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran out_at find V(out) AT=2.5p\n"
        ".measure tran diff_at find V(in,out) AT=2.5p\n"
        ".tran 0.5p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.measurement_details) == 2
    assert report.measurement_details[0].name == "out_at"
    assert report.measurement_details[0].kind == "find"
    assert report.measurement_details[0].measured_value > 0.0
    assert report.measurement_details[1].name == "diff_at"
    assert report.measurement_details[1].at_ref.node == "in,out"
    assert report.measurement_details[1].measured_value >= 0.0


def test_simulate_text_internal_transient_measurement_details_support_find_when():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 6p 1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran out_when find V(out) WHEN V(in)=0.5m RISE=1\n"
        ".measure tran diff_when find V(in,out) WHEN V(in,out)=0.2m RISE=1\n"
        ".tran 0.5p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.measurement_details) == 2
    assert report.measurement_details[0].name == "out_when"
    assert report.measurement_details[0].kind == "find"
    assert report.measurement_details[0].measured_value >= 0.0
    assert report.measurement_details[1].name == "diff_when"
    assert report.measurement_details[1].at_ref.node == "in,out"
    assert report.measurement_details[1].measured_value >= 0.0


def test_simulate_text_internal_transient_reports_measurement_warnings():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 6p 1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran missing find V(out) WHEN V(in)=2m RISE=1\n"
        ".tran 0.5p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.measurement_details == []
    assert len(report.measurement_warnings) == 1
    assert report.measurement_warnings[0].name == "missing"
    assert report.measurement_warnings[0].kind == "find"
    assert report.measurement_warnings[0].reason == "measurement_crossing_not_found"
    assert report.measurement_warnings[0].at_ref.node == "in"


def test_simulate_text_internal_transient_reports_delay_measurement_details():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 8p 1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran rc_delay TRIG V(in)=0.5m RISE=1 TARG V(out)=0.25m RISE=1\n"
        ".tran 0.5p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.delay_details) == 1
    assert report.delay_details[0].name == "rc_delay"
    assert report.delay_details[0].from_ref.node == "in"
    assert report.delay_details[0].to_ref.node == "out"
    assert report.delay_details[0].delay_ps > 0.0
    assert report.reported_worst_delay_ps == report.delay_details[0].delay_ps


def test_simulate_text_internal_transient_delay_measurements_support_differential_voltage():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 8p 1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran diff_delay TRIG V(in,out) VAL=0.2m RISE=1 TARG V(out) VAL=0.25m RISE=1\n"
        ".tran 0.5p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.delay_details) == 1
    assert report.delay_details[0].name == "diff_delay"
    assert report.delay_details[0].from_ref.node == "in,out"
    assert report.delay_details[0].to_ref.node == "out"
    assert report.delay_details[0].delay_ps > 0.0


def test_simulate_text_internal_transient_delay_measurements_support_fall_and_td():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 3p 1m 4p 0 8p 0)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran fall_delay TRIG V(in) VAL=0.5m FALL=1 TARGET V(out) VAL=0.25m FALL=1 TD=2p\n"
        ".tran 0.5p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.delay_details) == 1
    assert report.delay_details[0].name == "fall_delay"
    assert report.delay_details[0].delay_ps > 0.0


def test_simulate_text_internal_transient_delay_measurements_support_last_crossing():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 1p 1m 2p 1m 3p 0 8p 0 9p 1m 14p 1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".measure tran last_rise_delay TRIG V(in) VAL=0.5m RISE=LAST TARG V(out) VAL=0.25m RISE=LAST\n"
        ".tran 0.5p 14p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert len(report.delay_details) == 1
    assert report.delay_details[0].name == "last_rise_delay"
    assert report.delay_details[0].delay_ps > 0.0


def test_simulate_text_internal_transient_supports_one_shot_pulse_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 8
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_finite_cycle_pulse_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p,4p,2)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 10p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 10
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_finite_cycle_pulse_with_ncycles_keyword():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p,4p,ncycles=2)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 10p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 10
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_finite_cycle_pulse_with_spaced_ncycles_keyword():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p,4p,ncycles = 2)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 10p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 10
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_pwl_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0,0,2p,1m,4p,0)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_whitespace_separated_pwl_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0 0 2p 1m 4p 0)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_repeated_time_pwl_step():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PWL(0,0,2p,0,2p,1m,4p,1m)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_exp_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 EXP(0,1m,1p,0.5p,4p,0.5p)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 6
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_keyword_exp_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 EXP(v1=0 v2=1m td1=1p tau1=0.5p td2=4p tau2=0.5p)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 6
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_keyword_pulse_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(v1 = 0 v2 = 1m td = 1p tr = 0.2p tf = 0.2p pw = 2p per = 4p ncycles = 2)\n"
        "R1 in out 10\n"
        "C1 out 0 1p\n"
        ".tran 0.5p 10p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 20
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_inductor():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 DC 1m\n"
        "L1 in out 1p\n"
        "R1 out 0 1\n"
        ".tran 1p 4p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 4
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_mutual_inductance():
    report = rflux.simulate_text(
        ".title demo\n"
        "K1 L1 L2 0.9\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\n"
        "L1 in out 1p\n"
        "R1 out 0 1\n"
        "L2 tap 0 1p\n"
        "R2 tap 0 1\n"
        ".tran 1p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 8
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_mutual_inductance_coupling_keyword():
    report = rflux.simulate_text(
        ".title demo\n"
        "K1 L1 L2 coupling = 0.9\n"
        "V1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\n"
        "L1 in out 1p\n"
        "R1 out 0 1\n"
        "L2 tap 0 1p\n"
        "R2 tap 0 1\n"
        ".tran 1p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 8
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_zero_delay_transmission_line_subset():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 DC 1m\n"
        "T1 in 0 out 0 50 0\n"
        "R1 out 0 50\n"
        ".tran 1p 4p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 4
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_finite_delay_transmission_line_subset():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 DC 1m\n"
        "T1 in 0 out 0 z0=50 td=3p\n"
        "R1 out 0 50\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 6
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_transmission_line_loss_parameter():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 DC 1m\n"
        "T1 in 0 out 0 z0=50 td=3p loss=0.3\n"
        "R1 out 0 50\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 6
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_spaced_t_assignments():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 DC 1m\n"
        "T1 in 0 out 0 z0 = 50 td = 3p\n"
        "R1 out 0 50\n"
        ".tran 1p 6p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 6
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_minimal_junction_subset():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\n"
        "R1 in n1 10\n"
        "J1 n1 0 icrit=0.5m rn=20 cj=0.5p\n"
        ".tran 1p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 8
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_junction_with_model_token_prefix():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\n"
        "R1 in n1 10\n"
        "J1 n1 0 jjmod icrit=0.5m rn=20 cj=0.5p\n"
        ".tran 1p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 8
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_junction_model_keyword_reference():
    report = rflux.simulate_text(
        ".title demo\n"
        ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n"
        "V1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\n"
        "R1 in n1 10\n"
        "J1 n1 0 model=jjmod\n"
        ".tran 1p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 8
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_spaced_junction_assignments():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\n"
        "R1 in n1 10\n"
        "J1 n1 0 icrit = 0.5m rn = 20 cj = 0.5p\n"
        ".tran 1p 8p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 8
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_file_internal_transient_phase6_benchmark_smoke():
    benchmark_dir = os.path.join(os.path.dirname(__file__), "benchmarks", "phase6")
    thresholds_path = os.path.join(benchmark_dir, "waveform_thresholds.json")
    threshold_payload = json.loads(open(thresholds_path, "r", encoding="utf-8").read())
    deck_paths = [os.path.join(benchmark_dir, deck_name) for deck_name in sorted(threshold_payload)]

    for deck_path in deck_paths:
        report = rflux.simulate_file(deck_path, simulation_mode="internal_transient")
        assert report.backend == "internal_transient_completed"
        assert report.simulated_events > 0
        assert report.external_result == "internal_transient_linear_rc"


def test_simulate_text_internal_transient_accepts_dc_source_with_ac_tail():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 DC 1m AC 0\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_sin_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 SIN(0,1m,100g)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_sin_phase_and_tran_uic():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 SIN(0,1m,100g,0,90)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p 1p 1p uic\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_sin_damping_and_phase():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 SIN(0,1m,100g,0,300g,90)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_keyword_sin_source():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 SIN(vo=0 va=1m freq=100g td=0 theta=300g phi=90)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_mixed_case_function_source_name():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 sIn(0 1m 100g 0 90)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_space_before_function_source_paren():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 SIN (0 1m 100g 0 90)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_supports_whitespace_separated_sin_phase():
    report = rflux.simulate_text(
        ".title demo\n"
        "V1 in 0 SIN(0 1m 100g 0 90)\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 5p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 5
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_applies_ic_with_uic():
    report = rflux.simulate_text(
        ".title demo\n"
        ".ic V(out)=1m\n"
        "R1 out 0 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 2p uic\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 2
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_accepts_nodeset_startup_hint():
    report = rflux.simulate_text(
        ".title demo\n"
        ".nodeset V(out)=1m\n"
        "R1 out 0 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 2p\n"
        ".end\n",
        simulation_mode="internal_transient",
    )

    assert report.backend == "internal_transient_completed"
    assert report.simulated_events == 2
    assert report.external_result == "internal_transient_linear_rc"
    assert report.waveform_path is not None


def test_simulate_text_internal_transient_noise_is_reproducible_with_same_seed() -> None:
    deck = (
        ".title demo\n"
        ".option seed=42 tnoise=1m\n"
        "V1 in 0 DC 1\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 3p\n"
        ".end\n"
    )

    first = rflux.simulate_text(deck, simulation_mode="internal_transient")
    second = rflux.simulate_text(deck, simulation_mode="internal_transient")

    assert first.backend == "internal_transient_completed"
    assert second.backend == "internal_transient_completed"
    assert first.external_result == "internal_transient_linear_rc;seed=42"
    assert second.external_result == "internal_transient_linear_rc;seed=42"
    assert first.waveform_path is not None
    assert second.waveform_path is not None
    assert Path(first.waveform_path).read_text(encoding="utf-8") == Path(
        second.waveform_path
    ).read_text(encoding="utf-8")


def test_simulate_text_internal_transient_noise_differs_for_different_seed() -> None:
    deck_a = (
        ".title demo\n"
        ".option seed=100 tnoise=1m\n"
        "V1 in 0 DC 1\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 3p\n"
        ".end\n"
    )
    deck_b = (
        ".title demo\n"
        ".option seed=101 tnoise=1m\n"
        "V1 in 0 DC 1\n"
        "R1 in out 1\n"
        "C1 out 0 1p\n"
        ".tran 1p 3p\n"
        ".end\n"
    )

    report_a = rflux.simulate_text(deck_a, simulation_mode="internal_transient")
    report_b = rflux.simulate_text(deck_b, simulation_mode="internal_transient")

    assert report_a.backend == "internal_transient_completed"
    assert report_b.backend == "internal_transient_completed"
    assert report_a.external_result == "internal_transient_linear_rc;seed=100"
    assert report_b.external_result == "internal_transient_linear_rc;seed=101"
    assert report_a.waveform_path is not None
    assert report_b.waveform_path is not None
    assert Path(report_a.waveform_path).read_text(encoding="utf-8") != Path(
        report_b.waveform_path
    ).read_text(encoding="utf-8")


def test_simulate_file_external_josim_preserves_pi_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external pi benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_pi_model_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    if report.external_result is not None:
        assert "external_josim_translation_warning:jj_pi_model_unsupported" not in report.external_result
        assert "external_josim_translation_warning:jj_pi_instance_unsupported" not in report.external_result
        assert "external_josim_translation_warning:jj_model_override_unsupported" not in report.external_result


def test_simulate_file_external_josim_preserves_modelname_keyword_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external modelname benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_modelname_keyword_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    if report.external_result is not None:
        assert "external_josim_translation_warning:jj_model_override_unsupported" not in report.external_result


def test_simulate_file_external_josim_preserves_second_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external second-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_second_harmonic_model_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={1," in generated_deck.lower()
    if report.external_result is not None:
        assert (
            "external_josim_translation_warning:jj_second_harmonic_model_unsupported"
            not in report.external_result
        )
        assert (
            "external_josim_translation_warning:jj_second_harmonic_instance_unsupported"
            not in report.external_result
        )
        assert "external_josim_translation_warning:jj_model_override_unsupported" not in report.external_result


def test_simulate_file_external_josim_preserves_second_harmonic_model_override_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external second-harmonic model-override benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_second_harmonic_model_override_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert ".model rflux_auto_j1 jj(" in generated_deck.lower()
    assert "rn=25" in generated_deck.lower()
    assert "cpr={1," in generated_deck.lower()
    if report.external_result is not None:
        assert (
            "external_josim_translation_warning:jj_second_harmonic_model_unsupported"
            not in report.external_result
        )
        assert (
            "external_josim_translation_warning:jj_second_harmonic_instance_unsupported"
            not in report.external_result
        )
        assert "external_josim_translation_warning:jj_model_override_unsupported" not in report.external_result


def test_simulate_file_external_josim_preserves_third_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external third-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_third_harmonic_model_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={1,0," in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning:jj_model_override_unsupported" not in report.external_result


def test_simulate_file_external_josim_preserves_third_harmonic_model_override_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external third-harmonic model-override benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_third_harmonic_model_override_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert ".model rflux_auto_j1 jj(" in generated_deck.lower()
    assert "rn=25" in generated_deck.lower()
    assert "cpr={1,0," in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_fourth_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external fourth-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_fourth_harmonic_model_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={1,0,0," in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_fourth_harmonic_model_override_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external fourth-harmonic model-override benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_fourth_harmonic_model_override_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert ".model rflux_auto_j1 jj(" in generated_deck.lower()
    assert "rn=25" in generated_deck.lower()
    assert "cpr={1,0,0," in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_fifth_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external fifth-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_fifth_harmonic_model_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={1,0,0,0," in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_fifth_harmonic_model_override_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external fifth-harmonic model-override benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_fifth_harmonic_model_override_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert ".model rflux_auto_j1 jj(" in generated_deck.lower()
    assert "rn=25" in generated_deck.lower()
    assert "cpr={1,0,0,0," in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_native_cpr_model_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external native-cpr benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_native_cpr_model_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_native_cpr_model_fourth_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external native-cpr fourth-harmonic model benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_native_cpr_model_fourth_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={1,0.2,0.05,0.01}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_native_cpr_model_fifth_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external native-cpr fifth-harmonic model benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_native_cpr_model_fifth_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={1,0.2,0.05,0.01,0.005}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_native_cpr_model_override_fifth_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external native-cpr fifth-harmonic model-override benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_native_cpr_model_override_fifth_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "rn=25" in generated_deck.lower()
    assert "cpr={1,0.2,0.05,0.01,0.005}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_native_cpr_instance_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external native-cpr instance benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_native_cpr_instance_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert ".model rflux_auto_j1 jj(" in generated_deck.lower()
    assert "cpr={" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_native_cpr_instance_fourth_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external native-cpr fourth-harmonic instance benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_native_cpr_instance_fourth_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert ".model rflux_auto_j1 jj(" in generated_deck.lower()
    assert "cpr={1,0.2,0.05,0.01}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_native_cpr_instance_fifth_harmonic_semantics_for_benchmark_asset() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external native-cpr fifth-harmonic instance benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_native_cpr_instance_fifth_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert ".model rflux_auto_j1 jj(" in generated_deck.lower()
    assert "cpr={1,0.2,0.05,0.01,0.005}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_pure_second_harmonic_semantics_without_primary_icrit() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external second-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_second_harmonic_warning_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={0,1}" in generated_deck.lower()
    if report.external_result is not None:
        assert (
            "external_josim_translation_warning:jj_second_harmonic_model_unsupported"
            not in report.external_result
        )
        assert (
            "external_josim_translation_warning:jj_second_harmonic_instance_unsupported"
            not in report.external_result
        )
        assert "external_josim_translation_warning:jj_model_override_unsupported" not in report.external_result


def test_simulate_file_external_josim_preserves_pure_third_harmonic_semantics_without_primary_icrit() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external third-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_third_harmonic_pure_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={0,0,1}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_pure_fourth_harmonic_semantics_without_primary_icrit() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external fourth-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_fourth_harmonic_pure_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={0,0,0,1}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_simulate_file_external_josim_preserves_pure_fifth_harmonic_semantics_without_primary_icrit() -> None:
    josim_override = os.environ.get("RFLOW_JOSIM_COMMAND", "").strip()
    josim = josim_override or shutil.which("josim")
    if not josim:
        pytest.skip("josim command not found; skipping external fifth-harmonic benchmark check")

    deck_path = (
        Path(__file__).resolve().parents[1]
        / "tests"
        / "benchmarks"
        / "phase6"
        / "jj_fifth_harmonic_pure_smoke.cir"
    )
    report = rflux.simulate_file(
        str(deck_path),
        simulation_mode="external_josim",
        external_command=josim,
    )

    assert report.backend == "external_completed"
    assert report.external_status_code == 0
    assert report.generated_deck_path is not None
    assert report.waveform_path is not None
    generated_deck = Path(report.generated_deck_path).read_text(encoding="utf-8")
    assert "cpr={0,0,0,0,1}" in generated_deck.lower()
    if report.external_result is not None:
        assert "external_josim_translation_warning" not in report.external_result


def test_verify_layout_propagates_external_simulator_summary(tmp_path):
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    sink = circuit.add_node("dff", "sink")
    circuit.connect(source, 0, sink, 0)

    if os.name == "nt":
        simulator = tmp_path / "josim.cmd"
        simulator.write_text(
            "@echo off\n"
            "echo RFLOW_EVENTS=7\n"
            "echo RFLOW_RESULT=PASS\n"
            "echo RFLOW_WAVEFORM=%~dpn1.raw\n"
            "echo RFLOW_VIOLATIONS=1\n"
            "echo RFLOW_WORST_DELAY_PS=13.5\n"
            "echo RFLOW_DELAY_DETAIL=name=critical_path,delay_ps=13.5,from=n0:0,to=n1:0\n"
            "echo RFLOW_VIOLATION_DETAIL=kind=hold,detail=sink_dff,at=n1:0\n",
            encoding="utf-8",
        )
    else:
        simulator = tmp_path / "josim.sh"
        simulator.write_text(
            "#!/bin/sh\n"
            "echo RFLOW_EVENTS=7\n"
            "echo RFLOW_RESULT=PASS\n"
            "echo RFLOW_WAVEFORM=${1%.sp}.raw\n"
            "echo RFLOW_VIOLATIONS=1\n"
            "echo RFLOW_WORST_DELAY_PS=13.5\n"
            "echo RFLOW_DELAY_DETAIL=name=critical_path,delay_ps=13.5,from=n0:0,to=n1:0\n"
            "echo RFLOW_VIOLATION_DETAIL=kind=hold,detail=sink_dff,at=n1:0\n",
            encoding="utf-8",
        )
        simulator.chmod(simulator.stat().st_mode | stat.S_IEXEC)

    report = rflux.verify_layout(
        circuit,
        external_command=str(simulator),
    )

    assert report.simulation_backend == "external_completed"
    assert report.simulated_events == 7
    assert report.waveform_path is not None
    assert report.waveform_path.endswith(".raw")
    assert report.reported_violations == 1
    assert report.reported_worst_delay_ps == 13.5
    assert len(report.delay_details) == 1
    assert report.delay_details[0].name == "critical_path"
    assert report.delay_details[0].delay_ps == 13.5
    assert report.delay_details[0].from_ref is not None
    assert report.delay_details[0].from_ref.raw == "n0:0"
    assert report.delay_details[0].from_ref.node == "n0"
    assert report.delay_details[0].from_ref.port == 0
    assert report.delay_details[0].to_ref is not None
    assert report.delay_details[0].to_ref.raw == "n1:0"
    assert report.delay_details[0].to_ref.node == "n1"
    assert report.delay_details[0].to_ref.port == 0
    assert len(report.violation_details) == 1
    assert report.violation_details[0].kind == "hold"
    assert report.violation_details[0].detail == "sink_dff"
    assert report.violation_details[0].at_ref is not None
    assert report.violation_details[0].at_ref.raw == "n1:0"
    assert report.violation_details[0].at_ref.node == "n1"
    assert report.violation_details[0].at_ref.port == 0
    assert report.external_result == "pass"


def test_compile_layout_accepts_fixed_node_constraints():
    circuit = rflux.Circuit("demo")
    circuit.add_node("port", "a")
    circuit.add_node("macro", "gate")

    report = rflux.compile_layout(
        circuit,
        None,
        fixed_nodes=[rflux.FixedNodePlacement(node=1, x_um=120.0, y_um=48.0)],
    )

    assert report.placed_nodes == 2
    assert report.placement_width_um == 160.0
    assert report.placement_height_um == 72.0
    assert report.clock_sinks == 1
    assert report.analyzed_timing_arcs == 0
    assert report.initial_total_detour_overhead_um >= report.total_detour_overhead_um
    assert report.total_detour_overhead_um >= 0.0


def test_compile_layout_accepts_blocked_regions():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("cell", "source")
    sink = circuit.add_node("cell", "sink")
    circuit.connect(source, 0, sink, 0)

    report = rflux.compile_layout(
        circuit,
        None,
        fixed_nodes=[rflux.FixedNodePlacement(node=sink, x_um=120.0, y_um=0.0)],
        blocked_regions=[
            rflux.BlockedRegion(
                min_x_um=50.0,
                max_x_um=70.0,
                min_y_um=-4.0,
                max_y_um=4.0,
            )
        ],
    )

    assert report.routed_nets == 1
    assert report.total_route_length_um > 100.0
    assert report.initial_hold_violations >= report.final_hold_violations
    assert report.analyzed_timing_arcs == 1
    assert report.initial_total_detour_overhead_um >= report.total_detour_overhead_um
    assert report.initial_total_detour_overhead_um > 0.0
    assert report.detoured_routes in {0, 1}


def test_merge_characterized_library_round_trip():
    circuit = rflux.Circuit("demo")
    source = circuit.add_node("port", "a")
    gate = circuit.add_node("cell", "gate")
    sink = circuit.add_node("port", "y")
    circuit.connect(source, 0, gate, 0)
    circuit.connect(gate, 0, sink, 0)

    char_report = rflux.characterize_compound_cell(circuit, cell_name="macro_buf")
    merged = rflux.merge_characterized_library([char_report.generated_library_json])
    assert "macro_buf" in merged
    assert len(merged) >= len(char_report.generated_library_json)


def test_optimize_design_with_characterized_library_workflow():
    char_circuit = rflux.Circuit("compound")
    source = char_circuit.add_node("port", "source")
    gate = char_circuit.add_node("cell", "gate")
    sink = char_circuit.add_node("port", "sink")
    char_circuit.connect(source, 0, gate, 0)
    char_circuit.connect(gate, 0, sink, 0)
    char_report = rflux.characterize_compound_cell(char_circuit, cell_name="macro_buf")

    consumer = rflux.Circuit("consumer")
    consumer_source = consumer.add_node("port", "consumer_source")
    macro_buf = consumer.add_node("macro", "macro_buf")
    consumer_sink = consumer.add_node("dff", "consumer_sink")
    consumer.connect(consumer_source, 0, macro_buf, 0)
    consumer.connect(macro_buf, 0, consumer_sink, 0)

    design = rflux.optimize_design_with_characterized_library(
        consumer,
        [char_report.generated_library_json],
    )

    assert design.characterized_cells_merged == 1
    assert design.design_optimization_score > 0.0
    assert design.baseline_statistical.analyzed_timing_arcs > 0
    assert design.ac_bias.baseline.routed_nets > 0
    assert design.placement_candidates_evaluated >= 1
    assert design.statistical_candidates_evaluated >= 2


def test_check_equivalence_reports_combinational_match():
    lhs = rflux.Circuit("lhs")
    lhs_a = lhs.add_node("port", "a")
    lhs_b = lhs.add_node("port", "b")
    lhs_and = lhs.add_node("cell", "lhs_and", logic_op="and")
    lhs_out = lhs.add_node("port", "out")
    lhs.connect(lhs_a, 0, lhs_and, 0)
    lhs.connect(lhs_b, 0, lhs_and, 1)
    lhs.connect(lhs_and, 0, lhs_out, 0)

    rhs = rflux.Circuit("rhs")
    rhs_a = rhs.add_node("port", "a")
    rhs_b = rhs.add_node("port", "b")
    rhs_and = rhs.add_node("cell", "rhs_and", logic_op="and")
    rhs_out = rhs.add_node("port", "out")
    rhs.connect(rhs_b, 0, rhs_and, 0)
    rhs.connect(rhs_a, 0, rhs_and, 1)
    rhs.connect(rhs_and, 0, rhs_out, 0)

    report = rflux.check_equivalence(lhs, rhs)

    assert report.equivalent is True
    assert report.checked_outputs == ["out"]
    assert report.counterexample_inputs == {}
    assert report.counterexample_outputs == {}
    assert report.sat_recursive_calls >= 1


def test_check_single_step_sequential_equivalence_reports_counterexample():
    lhs = rflux.Circuit("lhs_seq")
    lhs_data = lhs.add_node("port", "data")
    lhs.add_node("port", "enable")
    lhs_clock = lhs.add_node("port", "clock")
    lhs_state = lhs.add_node("dff", "state")
    lhs_out = lhs.add_node("port", "out")
    lhs.connect(lhs_data, 0, lhs_state, 0)
    lhs.connect(lhs_clock, 0, lhs_state, 1)
    lhs.connect(lhs_state, 0, lhs_out, 0)

    rhs = rflux.Circuit("rhs_seq")
    rhs_data = rhs.add_node("port", "data")
    rhs_enable = rhs.add_node("port", "enable")
    rhs_clock = rhs.add_node("port", "clock")
    rhs_state = rhs.add_node("dff", "state", logic_op="dffe")
    rhs_out = rhs.add_node("port", "out")
    rhs.connect(rhs_data, 0, rhs_state, 0)
    rhs.connect(rhs_enable, 0, rhs_state, 1)
    rhs.connect(rhs_clock, 0, rhs_state, 2)
    rhs.connect(rhs_state, 0, rhs_out, 0)

    report = rflux.check_single_step_sequential_equivalence(lhs, rhs)

    assert report.equivalent is False
    assert report.checked_outputs == ["out"]
    assert report.checked_states == ["state"]
    assert "state" in report.counterexample_states
    assert report.counterexample_states["state"].lhs_next is True
    assert report.counterexample_states["state"].rhs_next is False
    assert report.sat_recursive_calls >= 1


def test_check_bounded_sequential_equivalence_reports_first_failing_step():
    lhs = rflux.Circuit("lhs_seq")
    lhs_data = lhs.add_node("port", "data")
    lhs.add_node("port", "enable")
    lhs_clock = lhs.add_node("port", "clock")
    lhs_state = lhs.add_node("dff", "state")
    lhs_out = lhs.add_node("port", "out")
    lhs.connect(lhs_data, 0, lhs_state, 0)
    lhs.connect(lhs_clock, 0, lhs_state, 1)
    lhs.connect(lhs_state, 0, lhs_out, 0)

    rhs = rflux.Circuit("rhs_seq")
    rhs_data = rhs.add_node("port", "data")
    rhs_enable = rhs.add_node("port", "enable")
    rhs_clock = rhs.add_node("port", "clock")
    rhs_state = rhs.add_node("dff", "state", logic_op="dffe")
    rhs_out = rhs.add_node("port", "out")
    rhs.connect(rhs_data, 0, rhs_state, 0)
    rhs.connect(rhs_enable, 0, rhs_state, 1)
    rhs.connect(rhs_clock, 0, rhs_state, 2)
    rhs.connect(rhs_state, 0, rhs_out, 0)

    report = rflux.check_bounded_sequential_equivalence(lhs, rhs, depth=3)

    assert report.equivalent is False
    assert report.depth == 3
    assert report.checked_steps == 1
    assert report.unroll_mode == "state_unrolled"
    assert report.checked_outputs == ["out"]
    assert report.checked_states == ["state"]
    assert report.first_failing_step == 0
    assert len(report.steps) == 1
    assert report.steps[0].step == 0
    assert "state" in report.steps[0].report.counterexample_states
    assert report.sat_recursive_calls >= 1
