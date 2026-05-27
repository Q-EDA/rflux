# Convert a small Quaigh/bench subset into rflux IR JSON fixture for synth parity tests.
from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path


_SIGNAL_RE = r"[^\s(),=#]+"
_ASSIGN_RE = re.compile(rf"^\s*({_SIGNAL_RE})\s*=\s*([A-Za-z_][A-Za-z0-9_]*)\s*\((.*)\)\s*$")
_INPUT_RE = re.compile(rf"^\s*INPUT\s*\(\s*({_SIGNAL_RE})\s*\)\s*$", re.IGNORECASE)
_OUTPUT_RE = re.compile(rf"^\s*OUTPUT\s*\(\s*({_SIGNAL_RE})\s*\)\s*$", re.IGNORECASE)


@dataclass
class GateSpec:
    output: str
    op: str
    inputs: list[str]


def _clean_lines(text: str) -> list[str]:
    lines: list[str] = []
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if line:
            lines.append(line)
    return lines


def _parse_bench(text: str) -> tuple[list[str], list[str], list[GateSpec]]:
    inputs: list[str] = []
    outputs: list[str] = []
    gates: list[GateSpec] = []

    for line in _clean_lines(text):
        if input_match := _INPUT_RE.match(line):
            inputs.append(input_match.group(1))
            continue
        if output_match := _OUTPUT_RE.match(line):
            outputs.append(output_match.group(1))
            continue

        assign_match = _ASSIGN_RE.match(line)
        if not assign_match:
            raise ValueError(f"Unsupported line format: {line}")

        out_name = assign_match.group(1)
        op_name = assign_match.group(2).upper()
        arg_text = assign_match.group(3).strip()
        args = [segment.strip() for segment in arg_text.split(",") if segment.strip()]
        gates.append(GateSpec(output=out_name, op=op_name, inputs=args))

    _ensure_unique_signal_names(inputs)
    _ensure_unique_signal_names(outputs)
    return inputs, outputs, gates


def _ensure_unique_signal_names(names: list[str]) -> None:
    seen: set[str] = set()
    for name in names:
        if name in seen:
            raise ValueError(f"Signal '{name}' defined more than once")
        seen.add(name)


def _order_gates(gates: list[GateSpec], input_names: list[str]) -> list[GateSpec]:
    input_set = set(input_names)
    gate_outputs: set[str] = set()
    for gate in gates:
        if gate.output in input_set:
            raise ValueError(f"Signal '{gate.output}' defined more than once")
        if gate.output in gate_outputs:
            raise ValueError(f"Signal '{gate.output}' defined more than once")
        gate_outputs.add(gate.output)

    for gate in gates:
        for input_signal in gate.inputs:
            if input_signal not in input_set and input_signal not in gate_outputs:
                raise ValueError(f"Signal '{input_signal}' used before definition")

    known_signals = set(input_names)
    remaining = list(gates)
    ordered: list[GateSpec] = []

    while remaining:
        ready = [gate for gate in remaining if all(input_signal in known_signals for input_signal in gate.inputs)]
        if not ready:
            raise ValueError("Bench gate dependency cycle or self-reference detected")
        for gate in ready:
            ordered.append(gate)
            known_signals.add(gate.output)
        ready_outputs = {gate.output for gate in ready}
        remaining = [gate for gate in remaining if gate.output not in ready_outputs]

    return ordered


def _logic_op_for_bench_gate(op: str) -> str | None:
    if op == "AND":
        return "And"
    if op == "OR":
        return "Or"
    if op == "XOR":
        return "Xor"
    if op == "XNOR":
        return None
    if op == "NOT":
        return "Not"
    if op in {"BUF", "BUFF"}:
        return "Buf"
    if op == "MUX":
        return "Mux2"
    return None


def convert_bench_to_ir_json(text: str) -> dict:
    input_names, output_names, gates = _parse_bench(text)
    gates = _order_gates(gates, input_names)

    nodes: list[dict] = []
    edges: list[list[dict]] = []
    signal_driver: dict[str, int] = {}
    next_output_port: dict[str, int] = {}

    def alloc_output_port(signal: str) -> int:
        current = next_output_port.get(signal, 0)
        next_output_port[signal] = current + 1
        return current

    for name in input_names:
        node_id = len(nodes)
        nodes.append({"id": node_id, "kind": "Port", "name": name})
        signal_driver[name] = node_id

    for gate in gates:
        if gate.op in {"MUX", "MAJ", "AOI21", "OAI21", "DFFE"}:
            expected = 3
        elif gate.op == "DFF":
            expected = 2
        elif gate.op in {"AOI22", "OAI22", "AOI31", "OAI31", "AOI211", "OAI211"}:
            expected = 4
        elif gate.op in {"AOI311", "OAI311", "AOI221", "OAI221"}:
            expected = 5
        elif gate.op in {"AOI321", "OAI321", "AOI222", "OAI222"}:
            expected = 6
        elif gate.op in {"AOI322", "OAI322", "AOI421", "OAI421", "AOI2221", "OAI2221"}:
            expected = 7
        elif gate.op in {"AOI422", "OAI422", "AOI431", "OAI431"}:
            expected = 8
        elif gate.op in {"AOI432", "OAI432", "AOI441", "OAI441"}:
            expected = 9
        elif gate.op in {"AOI433", "OAI433", "AOI442", "OAI442"}:
            expected = 10
        elif gate.op in {"AOI443", "OAI443"}:
            expected = 11
        elif gate.op in {"AOI444", "OAI444"}:
            expected = 12
        elif gate.op in {"BUF", "BUFF", "NOT"}:
            expected = 1
        elif gate.op in {"AND", "OR", "XOR", "XNOR", "NAND", "NOR"}:
            expected = 2
        else:
            expected = None
        if expected is None:
            raise ValueError(
                f"Unsupported gate op '{gate.op}'. Supported: AND/OR/XOR/XNOR/NOT/NAND/NOR/BUF/BUFF/MUX/DFF/DFFE/MAJ/AOI21/OAI21/AOI22/OAI22/AOI31/OAI31/AOI211/OAI211/AOI311/OAI311/AOI321/OAI321/AOI221/OAI221/AOI222/OAI222/AOI322/OAI322/AOI421/OAI421/AOI422/OAI422/AOI431/OAI431/AOI432/OAI432/AOI433/OAI433/AOI441/OAI441/AOI442/OAI442/AOI443/OAI443/AOI444/OAI444/AOI2221/OAI2221"
            )
        if len(gate.inputs) != expected:
            raise ValueError(
                f"Gate '{gate.output}' op {gate.op} expects {expected} input(s), got {len(gate.inputs)}"
            )

        def connect_inputs(target_node: int) -> None:
            for port, input_signal in enumerate(gate.inputs):
                if input_signal not in signal_driver:
                    raise ValueError(f"Signal '{input_signal}' used before definition")
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": target_node, "port": port}
                edges.append([src_pin, dst_pin])

        if gate.op == "DFF":
            node_id = len(nodes)
            nodes.append({"id": node_id, "kind": "Dff", "name": gate.output})
            connect_inputs(node_id)
        elif gate.op == "DFFE":
            node_id = len(nodes)
            nodes.append({"id": node_id, "kind": "Dff", "name": gate.output, "logic_op": "DffEnable"})
            connect_inputs(node_id)
        elif gate.op == "XNOR":
            inner_id = len(nodes)
            nodes.append(
                {
                    "id": inner_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_xnor_inner",
                    "logic_op": "Xor",
                }
            )
            connect_inputs(inner_id)

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": inner_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "NAND":
            inner_id = len(nodes)
            nodes.append(
                {
                    "id": inner_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_nand_inner",
                    "logic_op": "And",
                }
            )
            connect_inputs(inner_id)

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": inner_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "NOR":
            inner_id = len(nodes)
            nodes.append(
                {
                    "id": inner_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_nor_inner",
                    "logic_op": "Or",
                }
            )
            connect_inputs(inner_id)

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": inner_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "MAJ":
            ab_id = len(nodes)
            nodes.append(
                {
                    "id": ab_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_maj_ab",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": ab_id, "port": port}
                edges.append([src_pin, dst_pin])

            ac_id = len(nodes)
            nodes.append(
                {
                    "id": ac_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_maj_ac",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[2])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": ac_id, "port": port}
                edges.append([src_pin, dst_pin])

            bc_id = len(nodes)
            nodes.append(
                {
                    "id": bc_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_maj_bc",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[1], gate.inputs[2])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": bc_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_maj_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": ab_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": ac_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
            edges.append([
                {"node": bc_id, "port": 0},
                {"node": node_id, "port": 1},
            ])
        elif gate.op == "AOI21":
            and_id = len(nodes)
            nodes.append(
                {
                    "id": and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi21_and",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and_id, "port": port}
                edges.append([src_pin, dst_pin])

            or_id = len(nodes)
            nodes.append(
                {
                    "id": or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi21_or",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and_id, "port": 0},
                {"node": or_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI21":
            or_id = len(nodes)
            nodes.append(
                {
                    "id": or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai21_or",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or_id, "port": port}
                edges.append([src_pin, dst_pin])

            and_id = len(nodes)
            nodes.append(
                {
                    "id": and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai21_and",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or_id, "port": 0},
                {"node": and_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI22":
            left_and_id = len(nodes)
            nodes.append(
                {
                    "id": left_and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi22_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": left_and_id, "port": port}
                edges.append([src_pin, dst_pin])

            right_and_id = len(nodes)
            nodes.append(
                {
                    "id": right_and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi22_and1",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": right_and_id, "port": port}
                edges.append([src_pin, dst_pin])

            or_id = len(nodes)
            nodes.append(
                {
                    "id": or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi22_or",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": left_and_id, "port": 0},
                {"node": or_id, "port": 0},
            ])
            edges.append([
                {"node": right_and_id, "port": 0},
                {"node": or_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI22":
            left_or_id = len(nodes)
            nodes.append(
                {
                    "id": left_or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai22_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": left_or_id, "port": port}
                edges.append([src_pin, dst_pin])

            right_or_id = len(nodes)
            nodes.append(
                {
                    "id": right_or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai22_or1",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": right_or_id, "port": port}
                edges.append([src_pin, dst_pin])

            and_id = len(nodes)
            nodes.append(
                {
                    "id": and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai22_and",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": left_or_id, "port": 0},
                {"node": and_id, "port": 0},
            ])
            edges.append([
                {"node": right_or_id, "port": 0},
                {"node": and_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI31":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi31_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi31_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            or_id = len(nodes)
            nodes.append(
                {
                    "id": or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi31_or",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": or_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI31":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai31_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai31_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            and_id = len(nodes)
            nodes.append(
                {
                    "id": and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai31_and",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": and_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI211":
            and_id = len(nodes)
            nodes.append(
                {
                    "id": and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi211_and",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi211_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi211_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI211":
            or_id = len(nodes)
            nodes.append(
                {
                    "id": or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai211_or",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai211_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai211_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI311":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi311_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi311_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi311_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi311_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[4]], "port": alloc_output_port(gate.inputs[4])},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI311":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai311_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai311_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai311_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai311_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[4]], "port": alloc_output_port(gate.inputs[4])},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI321":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi321_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi321_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi321_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi321_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[4]], "port": alloc_output_port(gate.inputs[4])},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi321_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[5]], "port": alloc_output_port(gate.inputs[5])},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI321":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai321_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai321_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai321_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai321_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[4]], "port": alloc_output_port(gate.inputs[4])},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai321_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[5]], "port": alloc_output_port(gate.inputs[5])},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI322":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi322_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1], gate.inputs[2])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi322_and1",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[3], gate.inputs[4])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and1_id, "port": port}
                edges.append([src_pin, dst_pin])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi322_and2",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[5], gate.inputs[6])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and2_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi322_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi322_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI322":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai322_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1], gate.inputs[2])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai322_or1",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[3], gate.inputs[4])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or1_id, "port": port}
                edges.append([src_pin, dst_pin])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai322_or2",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[5], gate.inputs[6])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or2_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai322_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai322_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI221":
            left_and_id = len(nodes)
            nodes.append(
                {
                    "id": left_and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi221_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": left_and_id, "port": port}
                edges.append([src_pin, dst_pin])

            right_and_id = len(nodes)
            nodes.append(
                {
                    "id": right_and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi221_and1",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": right_and_id, "port": port}
                edges.append([src_pin, dst_pin])

            left_or_id = len(nodes)
            nodes.append(
                {
                    "id": left_or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi221_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": left_and_id, "port": 0},
                {"node": left_or_id, "port": 0},
            ])
            edges.append([
                {"node": right_and_id, "port": 0},
                {"node": left_or_id, "port": 1},
            ])

            right_or_id = len(nodes)
            nodes.append(
                {
                    "id": right_or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi221_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": left_or_id, "port": 0},
                {"node": right_or_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[4]], "port": alloc_output_port(gate.inputs[4])},
                {"node": right_or_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": right_or_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI221":
            left_or_id = len(nodes)
            nodes.append(
                {
                    "id": left_or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai221_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": left_or_id, "port": port}
                edges.append([src_pin, dst_pin])

            right_or_id = len(nodes)
            nodes.append(
                {
                    "id": right_or_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai221_or1",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": right_or_id, "port": port}
                edges.append([src_pin, dst_pin])

            left_and_id = len(nodes)
            nodes.append(
                {
                    "id": left_and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai221_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": left_or_id, "port": 0},
                {"node": left_and_id, "port": 0},
            ])
            edges.append([
                {"node": right_or_id, "port": 0},
                {"node": left_and_id, "port": 1},
            ])

            right_and_id = len(nodes)
            nodes.append(
                {
                    "id": right_and_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai221_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": left_and_id, "port": 0},
                {"node": right_and_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[4]], "port": alloc_output_port(gate.inputs[4])},
                {"node": right_and_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": right_and_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI222":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi222_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi222_and1",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and1_id, "port": port}
                edges.append([src_pin, dst_pin])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi222_and2",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and2_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi222_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi222_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI222":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai222_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai222_or1",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or1_id, "port": port}
                edges.append([src_pin, dst_pin])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai222_or2",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or2_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai222_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai222_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI421":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi421_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi421_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi421_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            and3_id = len(nodes)
            nodes.append(
                {
                    "id": and3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi421_and3",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi421_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and3_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi421_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI421":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai421_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai421_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai421_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            or3_id = len(nodes)
            nodes.append(
                {
                    "id": or3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai421_or3",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai421_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or3_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai421_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI422":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi422_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi422_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi422_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            and3_id = len(nodes)
            nodes.append(
                {
                    "id": and3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi422_and3",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append(
                {
                    "id": and4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi422_and4",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[6], gate.inputs[7])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and4_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi422_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and3_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi422_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": and4_id, "port": 0},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI422":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai422_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai422_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai422_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            or3_id = len(nodes)
            nodes.append(
                {
                    "id": or3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai422_or3",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append(
                {
                    "id": or4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai422_or4",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[6], gate.inputs[7])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or4_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai422_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or3_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai422_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": or4_id, "port": 0},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI431":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi431_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi431_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi431_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            and3_id = len(nodes)
            nodes.append(
                {
                    "id": and3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi431_and3",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append(
                {
                    "id": and4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi431_and4",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and3_id, "port": 0},
                {"node": and4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": and4_id, "port": 1},
            ])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi431_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and4_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi431_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI431":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai431_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai431_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai431_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            or3_id = len(nodes)
            nodes.append(
                {
                    "id": or3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai431_or3",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append(
                {
                    "id": or4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai431_or4",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or3_id, "port": 0},
                {"node": or4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": or4_id, "port": 1},
            ])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai431_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or4_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai431_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI432":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            and3_id = len(nodes)
            nodes.append(
                {
                    "id": and3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_and3",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append(
                {
                    "id": and4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_and4",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and3_id, "port": 0},
                {"node": and4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": and4_id, "port": 1},
            ])

            and5_id = len(nodes)
            nodes.append(
                {
                    "id": and5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_and5",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[7], gate.inputs[8])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and5_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and4_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi432_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": and5_id, "port": 0},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI432":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            or3_id = len(nodes)
            nodes.append(
                {
                    "id": or3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_or3",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append(
                {
                    "id": or4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_or4",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or3_id, "port": 0},
                {"node": or4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": or4_id, "port": 1},
            ])

            or5_id = len(nodes)
            nodes.append(
                {
                    "id": or5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_or5",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[7], gate.inputs[8])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or5_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or4_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai432_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": or5_id, "port": 0},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI433":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            and3_id = len(nodes)
            nodes.append(
                {
                    "id": and3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_and3",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append(
                {
                    "id": and4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_and4",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and3_id, "port": 0},
                {"node": and4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": and4_id, "port": 1},
            ])

            and5_id = len(nodes)
            nodes.append(
                {
                    "id": and5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_and5",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[7], gate.inputs[8])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and5_id, "port": port}
                edges.append([src_pin, dst_pin])

            and6_id = len(nodes)
            nodes.append(
                {
                    "id": and6_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_and6",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and5_id, "port": 0},
                {"node": and6_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[9]], "port": alloc_output_port(gate.inputs[9])},
                {"node": and6_id, "port": 1},
            ])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and4_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi433_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": and6_id, "port": 0},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI433":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            or3_id = len(nodes)
            nodes.append(
                {
                    "id": or3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_or3",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append(
                {
                    "id": or4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_or4",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or3_id, "port": 0},
                {"node": or4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": or4_id, "port": 1},
            ])

            or5_id = len(nodes)
            nodes.append(
                {
                    "id": or5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_or5",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[7], gate.inputs[8])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or5_id, "port": port}
                edges.append([src_pin, dst_pin])

            or6_id = len(nodes)
            nodes.append(
                {
                    "id": or6_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_or6",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or5_id, "port": 0},
                {"node": or6_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[9]], "port": alloc_output_port(gate.inputs[9])},
                {"node": or6_id, "port": 1},
            ])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or4_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai433_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": or6_id, "port": 0},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI441":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            and3_id = len(nodes)
            nodes.append(
                {
                    "id": and3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_and3",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append(
                {
                    "id": and4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_and4",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and3_id, "port": 0},
                {"node": and4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": and4_id, "port": 1},
            ])

            and5_id = len(nodes)
            nodes.append(
                {
                    "id": and5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_and5",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and4_id, "port": 0},
                {"node": and5_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])},
                {"node": and5_id, "port": 1},
            ])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and5_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi441_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[8]], "port": alloc_output_port(gate.inputs[8])},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI441":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            or3_id = len(nodes)
            nodes.append(
                {
                    "id": or3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_or3",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append(
                {
                    "id": or4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_or4",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or3_id, "port": 0},
                {"node": or4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": or4_id, "port": 1},
            ])

            or5_id = len(nodes)
            nodes.append(
                {
                    "id": or5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_or5",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or4_id, "port": 0},
                {"node": or5_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])},
                {"node": or5_id, "port": 1},
            ])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or5_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai441_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[8]], "port": alloc_output_port(gate.inputs[8])},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI442":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": and2_id, "port": 1},
            ])

            and3_id = len(nodes)
            nodes.append(
                {
                    "id": and3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_and3",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append(
                {
                    "id": and4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_and4",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and3_id, "port": 0},
                {"node": and4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": and4_id, "port": 1},
            ])

            and5_id = len(nodes)
            nodes.append(
                {
                    "id": and5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_and5",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and4_id, "port": 0},
                {"node": and5_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])},
                {"node": and5_id, "port": 1},
            ])

            and6_id = len(nodes)
            nodes.append(
                {
                    "id": and6_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_and6",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[8], gate.inputs[9])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and6_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and5_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi442_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": and6_id, "port": 0},
                {"node": or1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI442":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])},
                {"node": or2_id, "port": 1},
            ])

            or3_id = len(nodes)
            nodes.append(
                {
                    "id": or3_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_or3",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append(
                {
                    "id": or4_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_or4",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or3_id, "port": 0},
                {"node": or4_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": or4_id, "port": 1},
            ])

            or5_id = len(nodes)
            nodes.append(
                {
                    "id": or5_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_or5",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or4_id, "port": 0},
                {"node": or5_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])},
                {"node": or5_id, "port": 1},
            ])

            or6_id = len(nodes)
            nodes.append(
                {
                    "id": or6_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_or6",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[8], gate.inputs[9])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or6_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or5_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai442_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": or6_id, "port": 0},
                {"node": and1_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "AOI443":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi443_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append({"id": and1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_and1", "logic_op": "And"})
            edges.append([{"node": and0_id, "port": 0}, {"node": and1_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])}, {"node": and1_id, "port": 1}])

            and2_id = len(nodes)
            nodes.append({"id": and2_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_and2", "logic_op": "And"})
            edges.append([{"node": and1_id, "port": 0}, {"node": and2_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])}, {"node": and2_id, "port": 1}])

            and3_id = len(nodes)
            nodes.append({"id": and3_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_and3", "logic_op": "And"})
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append({"id": and4_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_and4", "logic_op": "And"})
            edges.append([{"node": and3_id, "port": 0}, {"node": and4_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])}, {"node": and4_id, "port": 1}])

            and5_id = len(nodes)
            nodes.append({"id": and5_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_and5", "logic_op": "And"})
            edges.append([{"node": and4_id, "port": 0}, {"node": and5_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])}, {"node": and5_id, "port": 1}])

            and6_id = len(nodes)
            nodes.append({"id": and6_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_and6", "logic_op": "And"})
            for port, input_signal in enumerate((gate.inputs[8], gate.inputs[9])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and6_id, "port": port}
                edges.append([src_pin, dst_pin])

            and7_id = len(nodes)
            nodes.append({"id": and7_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_and7", "logic_op": "And"})
            edges.append([{"node": and6_id, "port": 0}, {"node": and7_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[10]], "port": alloc_output_port(gate.inputs[10])}, {"node": and7_id, "port": 1}])

            or0_id = len(nodes)
            nodes.append({"id": or0_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_or0", "logic_op": "Or"})
            edges.append([{"node": and2_id, "port": 0}, {"node": or0_id, "port": 0}])
            edges.append([{"node": and5_id, "port": 0}, {"node": or0_id, "port": 1}])

            or1_id = len(nodes)
            nodes.append({"id": or1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi443_or1", "logic_op": "Or"})
            edges.append([{"node": or0_id, "port": 0}, {"node": or1_id, "port": 0}])
            edges.append([{"node": and7_id, "port": 0}, {"node": or1_id, "port": 1}])

            node_id = len(nodes)
            nodes.append({"id": node_id, "kind": "CellInstance", "name": gate.output, "logic_op": "Not"})
            edges.append([{"node": or1_id, "port": 0}, {"node": node_id, "port": 0}])
        elif gate.op == "OAI443":
            or0_id = len(nodes)
            nodes.append({"id": or0_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or0", "logic_op": "Or"})
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append({"id": or1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or1", "logic_op": "Or"})
            edges.append([{"node": or0_id, "port": 0}, {"node": or1_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])}, {"node": or1_id, "port": 1}])

            or2_id = len(nodes)
            nodes.append({"id": or2_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or2", "logic_op": "Or"})
            edges.append([{"node": or1_id, "port": 0}, {"node": or2_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])}, {"node": or2_id, "port": 1}])

            or3_id = len(nodes)
            nodes.append({"id": or3_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or3", "logic_op": "Or"})
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append({"id": or4_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or4", "logic_op": "Or"})
            edges.append([{"node": or3_id, "port": 0}, {"node": or4_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])}, {"node": or4_id, "port": 1}])

            or5_id = len(nodes)
            nodes.append({"id": or5_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or5", "logic_op": "Or"})
            edges.append([{"node": or4_id, "port": 0}, {"node": or5_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])}, {"node": or5_id, "port": 1}])

            or6_id = len(nodes)
            nodes.append({"id": or6_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or6", "logic_op": "Or"})
            for port, input_signal in enumerate((gate.inputs[8], gate.inputs[9])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or6_id, "port": port}
                edges.append([src_pin, dst_pin])

            or7_id = len(nodes)
            nodes.append({"id": or7_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_or7", "logic_op": "Or"})
            edges.append([{"node": or6_id, "port": 0}, {"node": or7_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[10]], "port": alloc_output_port(gate.inputs[10])}, {"node": or7_id, "port": 1}])

            and0_id = len(nodes)
            nodes.append({"id": and0_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_and0", "logic_op": "And"})
            edges.append([{"node": or2_id, "port": 0}, {"node": and0_id, "port": 0}])
            edges.append([{"node": or5_id, "port": 0}, {"node": and0_id, "port": 1}])

            and1_id = len(nodes)
            nodes.append({"id": and1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai443_and1", "logic_op": "And"})
            edges.append([{"node": and0_id, "port": 0}, {"node": and1_id, "port": 0}])
            edges.append([{"node": or7_id, "port": 0}, {"node": and1_id, "port": 1}])

            node_id = len(nodes)
            nodes.append({"id": node_id, "kind": "CellInstance", "name": gate.output, "logic_op": "Not"})
            edges.append([{"node": and1_id, "port": 0}, {"node": node_id, "port": 0}])
        elif gate.op == "AOI444":
            and0_id = len(nodes)
            nodes.append({"id": and0_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and0", "logic_op": "And"})
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append({"id": and1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and1", "logic_op": "And"})
            edges.append([{"node": and0_id, "port": 0}, {"node": and1_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])}, {"node": and1_id, "port": 1}])

            and2_id = len(nodes)
            nodes.append({"id": and2_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and2", "logic_op": "And"})
            edges.append([{"node": and1_id, "port": 0}, {"node": and2_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])}, {"node": and2_id, "port": 1}])

            and3_id = len(nodes)
            nodes.append({"id": and3_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and3", "logic_op": "And"})
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and3_id, "port": port}
                edges.append([src_pin, dst_pin])

            and4_id = len(nodes)
            nodes.append({"id": and4_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and4", "logic_op": "And"})
            edges.append([{"node": and3_id, "port": 0}, {"node": and4_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])}, {"node": and4_id, "port": 1}])

            and5_id = len(nodes)
            nodes.append({"id": and5_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and5", "logic_op": "And"})
            edges.append([{"node": and4_id, "port": 0}, {"node": and5_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])}, {"node": and5_id, "port": 1}])

            and6_id = len(nodes)
            nodes.append({"id": and6_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and6", "logic_op": "And"})
            for port, input_signal in enumerate((gate.inputs[8], gate.inputs[9])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and6_id, "port": port}
                edges.append([src_pin, dst_pin])

            and7_id = len(nodes)
            nodes.append({"id": and7_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and7", "logic_op": "And"})
            edges.append([{"node": and6_id, "port": 0}, {"node": and7_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[10]], "port": alloc_output_port(gate.inputs[10])}, {"node": and7_id, "port": 1}])

            and8_id = len(nodes)
            nodes.append({"id": and8_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_and8", "logic_op": "And"})
            edges.append([{"node": and7_id, "port": 0}, {"node": and8_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[11]], "port": alloc_output_port(gate.inputs[11])}, {"node": and8_id, "port": 1}])

            or0_id = len(nodes)
            nodes.append({"id": or0_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_or0", "logic_op": "Or"})
            edges.append([{"node": and2_id, "port": 0}, {"node": or0_id, "port": 0}])
            edges.append([{"node": and5_id, "port": 0}, {"node": or0_id, "port": 1}])

            or1_id = len(nodes)
            nodes.append({"id": or1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_aoi444_or1", "logic_op": "Or"})
            edges.append([{"node": or0_id, "port": 0}, {"node": or1_id, "port": 0}])
            edges.append([{"node": and8_id, "port": 0}, {"node": or1_id, "port": 1}])

            node_id = len(nodes)
            nodes.append({"id": node_id, "kind": "CellInstance", "name": gate.output, "logic_op": "Not"})
            edges.append([{"node": or1_id, "port": 0}, {"node": node_id, "port": 0}])
        elif gate.op == "OAI444":
            or0_id = len(nodes)
            nodes.append({"id": or0_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or0", "logic_op": "Or"})
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append({"id": or1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or1", "logic_op": "Or"})
            edges.append([{"node": or0_id, "port": 0}, {"node": or1_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[2]], "port": alloc_output_port(gate.inputs[2])}, {"node": or1_id, "port": 1}])

            or2_id = len(nodes)
            nodes.append({"id": or2_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or2", "logic_op": "Or"})
            edges.append([{"node": or1_id, "port": 0}, {"node": or2_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[3]], "port": alloc_output_port(gate.inputs[3])}, {"node": or2_id, "port": 1}])

            or3_id = len(nodes)
            nodes.append({"id": or3_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or3", "logic_op": "Or"})
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or3_id, "port": port}
                edges.append([src_pin, dst_pin])

            or4_id = len(nodes)
            nodes.append({"id": or4_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or4", "logic_op": "Or"})
            edges.append([{"node": or3_id, "port": 0}, {"node": or4_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])}, {"node": or4_id, "port": 1}])

            or5_id = len(nodes)
            nodes.append({"id": or5_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or5", "logic_op": "Or"})
            edges.append([{"node": or4_id, "port": 0}, {"node": or5_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[7]], "port": alloc_output_port(gate.inputs[7])}, {"node": or5_id, "port": 1}])

            or6_id = len(nodes)
            nodes.append({"id": or6_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or6", "logic_op": "Or"})
            for port, input_signal in enumerate((gate.inputs[8], gate.inputs[9])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or6_id, "port": port}
                edges.append([src_pin, dst_pin])

            or7_id = len(nodes)
            nodes.append({"id": or7_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or7", "logic_op": "Or"})
            edges.append([{"node": or6_id, "port": 0}, {"node": or7_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[10]], "port": alloc_output_port(gate.inputs[10])}, {"node": or7_id, "port": 1}])

            or8_id = len(nodes)
            nodes.append({"id": or8_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_or8", "logic_op": "Or"})
            edges.append([{"node": or7_id, "port": 0}, {"node": or8_id, "port": 0}])
            edges.append([{"node": signal_driver[gate.inputs[11]], "port": alloc_output_port(gate.inputs[11])}, {"node": or8_id, "port": 1}])

            and0_id = len(nodes)
            nodes.append({"id": and0_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_and0", "logic_op": "And"})
            edges.append([{"node": or2_id, "port": 0}, {"node": and0_id, "port": 0}])
            edges.append([{"node": or5_id, "port": 0}, {"node": and0_id, "port": 1}])

            and1_id = len(nodes)
            nodes.append({"id": and1_id, "kind": "CellInstance", "name": f"{gate.output}__bench_oai444_and1", "logic_op": "And"})
            edges.append([{"node": and0_id, "port": 0}, {"node": and1_id, "port": 0}])
            edges.append([{"node": or8_id, "port": 0}, {"node": and1_id, "port": 1}])

            node_id = len(nodes)
            nodes.append({"id": node_id, "kind": "CellInstance", "name": gate.output, "logic_op": "Not"})
            edges.append([{"node": and1_id, "port": 0}, {"node": node_id, "port": 0}])
        elif gate.op == "AOI2221":
            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi2221_and0",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and0_id, "port": port}
                edges.append([src_pin, dst_pin])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi2221_and1",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and1_id, "port": port}
                edges.append([src_pin, dst_pin])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi2221_and2",
                    "logic_op": "And",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": and2_id, "port": port}
                edges.append([src_pin, dst_pin])

            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi2221_or0",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": or0_id, "port": 0},
            ])
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": or0_id, "port": 1},
            ])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi2221_or1",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": or1_id, "port": 0},
            ])
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": or1_id, "port": 1},
            ])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_aoi2221_or2",
                    "logic_op": "Or",
                }
            )
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": or2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": or2_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        elif gate.op == "OAI2221":
            or0_id = len(nodes)
            nodes.append(
                {
                    "id": or0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai2221_or0",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[0], gate.inputs[1])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or0_id, "port": port}
                edges.append([src_pin, dst_pin])

            or1_id = len(nodes)
            nodes.append(
                {
                    "id": or1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai2221_or1",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[2], gate.inputs[3])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or1_id, "port": port}
                edges.append([src_pin, dst_pin])

            or2_id = len(nodes)
            nodes.append(
                {
                    "id": or2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai2221_or2",
                    "logic_op": "Or",
                }
            )
            for port, input_signal in enumerate((gate.inputs[4], gate.inputs[5])):
                src_id = signal_driver[input_signal]
                src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
                dst_pin = {"node": or2_id, "port": port}
                edges.append([src_pin, dst_pin])

            and0_id = len(nodes)
            nodes.append(
                {
                    "id": and0_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai2221_and0",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": or0_id, "port": 0},
                {"node": and0_id, "port": 0},
            ])
            edges.append([
                {"node": or1_id, "port": 0},
                {"node": and0_id, "port": 1},
            ])

            and1_id = len(nodes)
            nodes.append(
                {
                    "id": and1_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai2221_and1",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and0_id, "port": 0},
                {"node": and1_id, "port": 0},
            ])
            edges.append([
                {"node": or2_id, "port": 0},
                {"node": and1_id, "port": 1},
            ])

            and2_id = len(nodes)
            nodes.append(
                {
                    "id": and2_id,
                    "kind": "CellInstance",
                    "name": f"{gate.output}__bench_oai2221_and2",
                    "logic_op": "And",
                }
            )
            edges.append([
                {"node": and1_id, "port": 0},
                {"node": and2_id, "port": 0},
            ])
            edges.append([
                {"node": signal_driver[gate.inputs[6]], "port": alloc_output_port(gate.inputs[6])},
                {"node": and2_id, "port": 1},
            ])

            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": "Not",
                }
            )
            edges.append([
                {"node": and2_id, "port": 0},
                {"node": node_id, "port": 0},
            ])
        else:
            logic_op = _logic_op_for_bench_gate(gate.op)
            assert logic_op is not None
            node_id = len(nodes)
            nodes.append(
                {
                    "id": node_id,
                    "kind": "CellInstance",
                    "name": gate.output,
                    "logic_op": logic_op,
                }
            )
            connect_inputs(node_id)

        signal_driver[gate.output] = node_id

    for out_name in output_names:
        if out_name not in signal_driver:
            raise ValueError(f"Output signal '{out_name}' has no driver")

        node_id = len(nodes)
        nodes.append({"id": node_id, "kind": "Port", "name": out_name})
        src_id = signal_driver[out_name]
        src_pin = {"node": src_id, "port": alloc_output_port(out_name)}
        dst_pin = {"node": node_id, "port": 0}
        edges.append([src_pin, dst_pin])

    return {"nodes": nodes, "edges": edges}


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Convert a Quaigh/bench subset to rflux IR JSON fixture.",
    )
    parser.add_argument("--input-bench", type=Path, required=True, help="Input .bench path")
    parser.add_argument("--output-json", type=Path, required=True, help="Output IR JSON path")
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting existing output JSON file.",
    )
    args = parser.parse_args()

    if args.output_json.exists() and not args.overwrite:
        raise SystemExit(
            f"Output file already exists: {args.output_json}. Use --overwrite to replace it."
        )

    bench_text = args.input_bench.read_text(encoding="utf-8")
    payload = convert_bench_to_ir_json(bench_text)

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
