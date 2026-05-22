# Demonstrate end-to-end Python equivalence checks, including a mismatch counterexample.
from __future__ import annotations

import json

import rflux


def build_and_circuit(name: str, swap_inputs: bool = False) -> rflux.Circuit:
    circuit = rflux.Circuit(name)
    src_a = circuit.add_node("port", "a")
    src_b = circuit.add_node("port", "b")
    gate = circuit.add_node("cell", f"{name}_and", logic_op="and")
    out = circuit.add_node("port", "out")
    if swap_inputs:
        circuit.connect(src_b, 0, gate, 0)
        circuit.connect(src_a, 0, gate, 1)
    else:
        circuit.connect(src_a, 0, gate, 0)
        circuit.connect(src_b, 0, gate, 1)
    circuit.connect(gate, 0, out, 0)
    return circuit


def build_or_circuit(name: str) -> rflux.Circuit:
    circuit = rflux.Circuit(name)
    src_a = circuit.add_node("port", "a")
    src_b = circuit.add_node("port", "b")
    gate = circuit.add_node("cell", f"{name}_or", logic_op="or")
    out = circuit.add_node("port", "out")
    circuit.connect(src_a, 0, gate, 0)
    circuit.connect(src_b, 0, gate, 1)
    circuit.connect(gate, 0, out, 0)
    return circuit


def report_to_json(report: rflux.CombinationalEquivalenceReport) -> dict[str, object]:
    return {
        "equivalent": report.equivalent,
        "checked_outputs": report.checked_outputs,
        "counterexample_inputs": report.counterexample_inputs,
        "counterexample_outputs": {
            name: {"lhs": mismatch.lhs, "rhs": mismatch.rhs}
            for name, mismatch in report.counterexample_outputs.items()
        },
        "sat": {
            "recursive_calls": report.sat_recursive_calls,
            "decisions": report.sat_decisions,
            "backtracks": report.sat_backtracks,
            "elapsed_ns": report.sat_elapsed_ns,
        },
    }


def main() -> None:
    equivalent = rflux.check_equivalence(
        build_and_circuit("lhs_equiv"),
        build_and_circuit("rhs_equiv", swap_inputs=True),
    )
    mismatch = rflux.check_equivalence(
        build_and_circuit("lhs_mismatch"),
        build_or_circuit("rhs_mismatch"),
    )

    summary = {
        "equivalent_case": report_to_json(equivalent),
        "mismatch_case": report_to_json(mismatch),
    }
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()