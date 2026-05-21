# Convert a small Quaigh/bench subset into rflux IR JSON fixture for synth parity tests.
from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path


_ASSIGN_RE = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([A-Za-z_][A-Za-z0-9_]*)\s*\((.*)\)\s*$")
_INPUT_RE = re.compile(r"^\s*INPUT\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*$", re.IGNORECASE)
_OUTPUT_RE = re.compile(r"^\s*OUTPUT\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*$", re.IGNORECASE)


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

    return inputs, outputs, gates


def _logic_op_for_bench_gate(op: str) -> str | None:
    if op == "AND":
        return "And"
    if op == "OR":
        return "Or"
    if op == "XOR":
        return "Xor"
    if op in {"BUF", "BUFF"}:
        return "Buf"
    if op == "MUX":
        return "Mux2"
    return None


def convert_bench_to_ir_json(text: str) -> dict:
    input_names, output_names, gates = _parse_bench(text)

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
        logic_op = _logic_op_for_bench_gate(gate.op)
        if logic_op is None:
            raise ValueError(
                f"Unsupported gate op '{gate.op}'. Supported: AND/OR/XOR/BUF/BUFF/MUX"
            )

        expected = 3 if gate.op == "MUX" else (1 if gate.op in {"BUF", "BUFF"} else 2)
        if len(gate.inputs) != expected:
            raise ValueError(
                f"Gate '{gate.output}' op {gate.op} expects {expected} input(s), got {len(gate.inputs)}"
            )

        node_id = len(nodes)
        nodes.append(
            {
                "id": node_id,
                "kind": "CellInstance",
                "name": gate.output,
                "logic_op": logic_op,
            }
        )

        for port, input_signal in enumerate(gate.inputs):
            if input_signal not in signal_driver:
                raise ValueError(f"Signal '{input_signal}' used before definition")
            src_id = signal_driver[input_signal]
            src_pin = {"node": src_id, "port": alloc_output_port(input_signal)}
            dst_pin = {"node": node_id, "port": port}
            edges.append([src_pin, dst_pin])

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
