from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
import importlib.util


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "python" / "scripts" / "convert_quaigh_bench_to_ir_fixture.py"
QUAIGH_FIXTURE_DIR = REPO_ROOT / "crates" / "synth" / "tests" / "fixtures" / "quaigh_alignment"
BENCH_FIXTURE_DIR = QUAIGH_FIXTURE_DIR / "bench"
SEQUENTIAL_BENCH_FIXTURE_DIR = QUAIGH_FIXTURE_DIR / "bench_sequential"

_SPEC = importlib.util.spec_from_file_location("convert_quaigh_bench_to_ir_fixture", SCRIPT_PATH)
assert _SPEC and _SPEC.loader
_MODULE = importlib.util.module_from_spec(_SPEC)
sys.modules[_SPEC.name] = _MODULE
_SPEC.loader.exec_module(_MODULE)
convert_bench_to_ir_json = _MODULE.convert_bench_to_ir_json


def _checked_in_bench_fixture_pairs() -> list[tuple[Path, Path]]:
    pairs: list[tuple[Path, Path]] = []
    for expected_json_path in sorted(QUAIGH_FIXTURE_DIR.glob("*_from_bench.json")):
        bench_stem = expected_json_path.stem.removesuffix("_from_bench")
        bench_path = BENCH_FIXTURE_DIR / f"{bench_stem}.bench"
        if not bench_path.exists():
            bench_path = SEQUENTIAL_BENCH_FIXTURE_DIR / f"{bench_stem}.bench"
        assert bench_path.exists(), f"missing bench fixture for {expected_json_path.name}"
        pairs.append((bench_path, expected_json_path))
    assert pairs, "expected at least one checked-in bench/json fixture pair"
    return pairs


def test_convert_quaigh_bench_to_ir_fixture_cli_matches_all_checked_in_repo_fixtures(
    tmp_path: Path,
) -> None:
    for bench_path, expected_json_path in _checked_in_bench_fixture_pairs():
        output_path = tmp_path / expected_json_path.name

        completed = subprocess.run(
            [
                sys.executable,
                str(SCRIPT_PATH),
                "--input-bench",
                str(bench_path),
                "--output-json",
                str(output_path),
            ],
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
        )

        assert completed.returncode == 0, (
            f"conversion failed for {bench_path.name}:\n{completed.stderr}\n{completed.stdout}"
        )
        actual = json.loads(output_path.read_text(encoding="utf-8"))
        expected = json.loads(expected_json_path.read_text(encoding="utf-8"))
        assert actual == expected, f"fixture mismatch for {bench_path.name}"


def test_convert_quaigh_bench_to_ir_fixture_supports_nand_and_nor_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nn0 = NAND(a, b)\nn1 = NOR(a, b)\nOUTPUT(n0)\nOUTPUT(n1)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "Not", "Or", "Not"]
    assert payload["nodes"][2]["name"] == "n0__bench_nand_inner"
    assert payload["nodes"][3]["name"] == "n0"
    assert payload["nodes"][4]["name"] == "n1__bench_nor_inner"
    assert payload["nodes"][5]["name"] == "n1"


def test_convert_quaigh_bench_to_ir_fixture_supports_bracketed_signal_names() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a[0])\nINPUT(b[0])\nsum[0] = XOR(a[0], b[0])\nOUTPUT(sum[0])\n"
    )

    assert [node["name"] for node in payload["nodes"]] == ["a[0]", "b[0]", "sum[0]", "sum[0]"]
    assert payload["nodes"][2]["logic_op"] == "Xor"
    assert payload["nodes"][3]["kind"] == "Port"


def test_convert_quaigh_bench_to_ir_fixture_supports_forward_gate_references() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\ny = BUF(n0)\nn0 = AND(a, b)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "Buf"]
    assert payload["nodes"][2]["name"] == "n0"
    assert payload["nodes"][3]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_input_output_passthrough() -> None:
    payload = convert_bench_to_ir_json("INPUT(a)\nOUTPUT(a)\n")

    assert [node["kind"] for node in payload["nodes"]] == ["Port", "Port"]
    assert [node["name"] for node in payload["nodes"]] == ["a", "a"]
    assert payload["edges"] == [[{"node": 0, "port": 0}, {"node": 1, "port": 0}]]


def test_convert_quaigh_bench_to_ir_fixture_rejects_gate_dependency_cycles() -> None:
    try:
        convert_bench_to_ir_json("INPUT(a)\nn0 = BUF(n1)\nn1 = BUF(n0)\nOUTPUT(n0)\n")
    except ValueError as error:
        assert "Bench gate dependency cycle or self-reference detected" in str(error)
    else:
        raise AssertionError("expected cycle to be rejected")


def test_convert_quaigh_bench_to_ir_fixture_rejects_duplicate_gate_outputs() -> None:
    try:
        convert_bench_to_ir_json("INPUT(a)\nINPUT(b)\nn0 = AND(a, b)\nn0 = OR(a, b)\nOUTPUT(n0)\n")
    except ValueError as error:
        assert "Signal 'n0' defined more than once" in str(error)
    else:
        raise AssertionError("expected duplicate output to be rejected")


def test_convert_quaigh_bench_to_ir_fixture_rejects_duplicate_input_declarations() -> None:
    try:
        convert_bench_to_ir_json("INPUT(a)\nINPUT(a)\nOUTPUT(a)\n")
    except ValueError as error:
        assert "Signal 'a' defined more than once" in str(error)
    else:
        raise AssertionError("expected duplicate INPUT to be rejected")


def test_convert_quaigh_bench_to_ir_fixture_rejects_duplicate_output_declarations() -> None:
    try:
        convert_bench_to_ir_json("INPUT(a)\nOUTPUT(y)\nOUTPUT(y)\ny = BUF(a)\n")
    except ValueError as error:
        assert "Signal 'y' defined more than once" in str(error)
    else:
        raise AssertionError("expected duplicate OUTPUT to be rejected")


def test_convert_quaigh_bench_to_ir_fixture_rejects_gate_outputs_that_redefine_inputs() -> None:
    try:
        convert_bench_to_ir_json("INPUT(a)\nINPUT(b)\na = AND(a, b)\nOUTPUT(a)\n")
    except ValueError as error:
        assert "Signal 'a' defined more than once" in str(error)
    else:
        raise AssertionError("expected gate output/input name conflict to be rejected")


def test_convert_quaigh_bench_to_ir_fixture_supports_dff_gate() -> None:
    payload = convert_bench_to_ir_json("INPUT(d)\nINPUT(clk)\nq = DFF(d, clk)\nOUTPUT(q)\n")

    assert [node["kind"] for node in payload["nodes"]] == ["Port", "Port", "Dff", "Port"]
    assert payload["nodes"][2]["name"] == "q"


def test_convert_quaigh_bench_to_ir_fixture_supports_dffe_gate() -> None:
    payload = convert_bench_to_ir_json("INPUT(d)\nINPUT(en)\nINPUT(clk)\nq = DFFE(d, en, clk)\nOUTPUT(q)\n")

    assert [node["kind"] for node in payload["nodes"]] == ["Port", "Port", "Port", "Dff", "Port"]
    assert payload["nodes"][3]["name"] == "q"
    assert payload["nodes"][3]["logic_op"] == "DffEnable"


def test_convert_quaigh_bench_to_ir_fixture_supports_maj_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\ny = MAJ(a, b, c)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "Or", "Or"]
    assert payload["nodes"][3]["name"] == "y__bench_maj_ab"
    assert payload["nodes"][4]["name"] == "y__bench_maj_ac"
    assert payload["nodes"][5]["name"] == "y__bench_maj_bc"
    assert payload["nodes"][6]["name"] == "y__bench_maj_or0"
    assert payload["nodes"][7]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi21_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\ny = AOI21(a, b, c)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "Or", "Not"]
    assert payload["nodes"][3]["name"] == "y__bench_aoi21_and"
    assert payload["nodes"][4]["name"] == "y__bench_aoi21_or"
    assert payload["nodes"][5]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai21_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\ny = OAI21(a, b, c)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "And", "Not"]
    assert payload["nodes"][3]["name"] == "y__bench_oai21_or"
    assert payload["nodes"][4]["name"] == "y__bench_oai21_and"
    assert payload["nodes"][5]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi22_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\ny = AOI22(a, b, c, d)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "Or", "Not"]
    assert payload["nodes"][4]["name"] == "y__bench_aoi22_and0"
    assert payload["nodes"][5]["name"] == "y__bench_aoi22_and1"
    assert payload["nodes"][6]["name"] == "y__bench_aoi22_or"
    assert payload["nodes"][7]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai22_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\ny = OAI22(a, b, c, d)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "And", "Not"]
    assert payload["nodes"][4]["name"] == "y__bench_oai22_or0"
    assert payload["nodes"][5]["name"] == "y__bench_oai22_or1"
    assert payload["nodes"][6]["name"] == "y__bench_oai22_and"
    assert payload["nodes"][7]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi31_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\ny = AOI31(a, b, c, d)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "Or", "Not"]
    assert payload["nodes"][4]["name"] == "y__bench_aoi31_and0"
    assert payload["nodes"][5]["name"] == "y__bench_aoi31_and1"
    assert payload["nodes"][6]["name"] == "y__bench_aoi31_or"
    assert payload["nodes"][7]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai31_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\ny = OAI31(a, b, c, d)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "And", "Not"]
    assert payload["nodes"][4]["name"] == "y__bench_oai31_or0"
    assert payload["nodes"][5]["name"] == "y__bench_oai31_or1"
    assert payload["nodes"][6]["name"] == "y__bench_oai31_and"
    assert payload["nodes"][7]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi211_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\ny = AOI211(a, b, c, d)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "Or", "Or", "Not"]
    assert payload["nodes"][4]["name"] == "y__bench_aoi211_and"
    assert payload["nodes"][5]["name"] == "y__bench_aoi211_or0"
    assert payload["nodes"][6]["name"] == "y__bench_aoi211_or1"
    assert payload["nodes"][7]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai211_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\ny = OAI211(a, b, c, d)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "And", "And", "Not"]
    assert payload["nodes"][4]["name"] == "y__bench_oai211_or"
    assert payload["nodes"][5]["name"] == "y__bench_oai211_and0"
    assert payload["nodes"][6]["name"] == "y__bench_oai211_and1"
    assert payload["nodes"][7]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi311_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\ny = AOI311(a, b, c, d, e)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][5]["name"] == "y__bench_aoi311_and0"
    assert payload["nodes"][6]["name"] == "y__bench_aoi311_and1"
    assert payload["nodes"][7]["name"] == "y__bench_aoi311_or0"
    assert payload["nodes"][8]["name"] == "y__bench_aoi311_or1"
    assert payload["nodes"][9]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai311_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\ny = OAI311(a, b, c, d, e)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][5]["name"] == "y__bench_oai311_or0"
    assert payload["nodes"][6]["name"] == "y__bench_oai311_or1"
    assert payload["nodes"][7]["name"] == "y__bench_oai311_and0"
    assert payload["nodes"][8]["name"] == "y__bench_oai311_and1"
    assert payload["nodes"][9]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi321_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\ny = AOI321(a, b, c, d, e, f)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][6]["name"] == "y__bench_aoi321_and0"
    assert payload["nodes"][7]["name"] == "y__bench_aoi321_and1"
    assert payload["nodes"][8]["name"] == "y__bench_aoi321_and2"
    assert payload["nodes"][9]["name"] == "y__bench_aoi321_or0"
    assert payload["nodes"][10]["name"] == "y__bench_aoi321_or1"
    assert payload["nodes"][11]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai321_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\ny = OAI321(a, b, c, d, e, f)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][6]["name"] == "y__bench_oai321_or0"
    assert payload["nodes"][7]["name"] == "y__bench_oai321_or1"
    assert payload["nodes"][8]["name"] == "y__bench_oai321_or2"
    assert payload["nodes"][9]["name"] == "y__bench_oai321_and0"
    assert payload["nodes"][10]["name"] == "y__bench_oai321_and1"
    assert payload["nodes"][11]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi322_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\ny = AOI322(a, b, c, d, e, f, g)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][7]["name"] == "y__bench_aoi322_and0"
    assert payload["nodes"][8]["name"] == "y__bench_aoi322_and1"
    assert payload["nodes"][9]["name"] == "y__bench_aoi322_and2"
    assert payload["nodes"][10]["name"] == "y__bench_aoi322_or0"
    assert payload["nodes"][11]["name"] == "y__bench_aoi322_or1"
    assert payload["nodes"][12]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai322_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\ny = OAI322(a, b, c, d, e, f, g)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][7]["name"] == "y__bench_oai322_or0"
    assert payload["nodes"][8]["name"] == "y__bench_oai322_or1"
    assert payload["nodes"][9]["name"] == "y__bench_oai322_or2"
    assert payload["nodes"][10]["name"] == "y__bench_oai322_and0"
    assert payload["nodes"][11]["name"] == "y__bench_oai322_and1"
    assert payload["nodes"][12]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi421_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\ny = AOI421(a, b, c, d, e, f, g)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][7]["name"] == "y__bench_aoi421_and0"
    assert payload["nodes"][8]["name"] == "y__bench_aoi421_and1"
    assert payload["nodes"][9]["name"] == "y__bench_aoi421_and2"
    assert payload["nodes"][10]["name"] == "y__bench_aoi421_and3"
    assert payload["nodes"][11]["name"] == "y__bench_aoi421_or0"
    assert payload["nodes"][12]["name"] == "y__bench_aoi421_or1"
    assert payload["nodes"][13]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai421_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\ny = OAI421(a, b, c, d, e, f, g)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][7]["name"] == "y__bench_oai421_or0"
    assert payload["nodes"][8]["name"] == "y__bench_oai421_or1"
    assert payload["nodes"][9]["name"] == "y__bench_oai421_or2"
    assert payload["nodes"][10]["name"] == "y__bench_oai421_or3"
    assert payload["nodes"][11]["name"] == "y__bench_oai421_and0"
    assert payload["nodes"][12]["name"] == "y__bench_oai421_and1"
    assert payload["nodes"][13]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi422_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\ny = AOI422(a, b, c, d, e, f, g, h)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][8]["name"] == "y__bench_aoi422_and0"
    assert payload["nodes"][9]["name"] == "y__bench_aoi422_and1"
    assert payload["nodes"][10]["name"] == "y__bench_aoi422_and2"
    assert payload["nodes"][11]["name"] == "y__bench_aoi422_and3"
    assert payload["nodes"][12]["name"] == "y__bench_aoi422_and4"
    assert payload["nodes"][13]["name"] == "y__bench_aoi422_or0"
    assert payload["nodes"][14]["name"] == "y__bench_aoi422_or1"
    assert payload["nodes"][15]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai422_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\ny = OAI422(a, b, c, d, e, f, g, h)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][8]["name"] == "y__bench_oai422_or0"
    assert payload["nodes"][9]["name"] == "y__bench_oai422_or1"
    assert payload["nodes"][10]["name"] == "y__bench_oai422_or2"
    assert payload["nodes"][11]["name"] == "y__bench_oai422_or3"
    assert payload["nodes"][12]["name"] == "y__bench_oai422_or4"
    assert payload["nodes"][13]["name"] == "y__bench_oai422_and0"
    assert payload["nodes"][14]["name"] == "y__bench_oai422_and1"
    assert payload["nodes"][15]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi431_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\ny = AOI431(a, b, c, d, e, f, g, h)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][8]["name"] == "y__bench_aoi431_and0"
    assert payload["nodes"][9]["name"] == "y__bench_aoi431_and1"
    assert payload["nodes"][10]["name"] == "y__bench_aoi431_and2"
    assert payload["nodes"][11]["name"] == "y__bench_aoi431_and3"
    assert payload["nodes"][12]["name"] == "y__bench_aoi431_and4"
    assert payload["nodes"][13]["name"] == "y__bench_aoi431_or0"
    assert payload["nodes"][14]["name"] == "y__bench_aoi431_or1"
    assert payload["nodes"][15]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai431_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\ny = OAI431(a, b, c, d, e, f, g, h)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][8]["name"] == "y__bench_oai431_or0"
    assert payload["nodes"][9]["name"] == "y__bench_oai431_or1"
    assert payload["nodes"][10]["name"] == "y__bench_oai431_or2"
    assert payload["nodes"][11]["name"] == "y__bench_oai431_or3"
    assert payload["nodes"][12]["name"] == "y__bench_oai431_or4"
    assert payload["nodes"][13]["name"] == "y__bench_oai431_and0"
    assert payload["nodes"][14]["name"] == "y__bench_oai431_and1"
    assert payload["nodes"][15]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi432_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\ny = AOI432(a, b, c, d, e, f, g, h, i)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][9]["name"] == "y__bench_aoi432_and0"
    assert payload["nodes"][10]["name"] == "y__bench_aoi432_and1"
    assert payload["nodes"][11]["name"] == "y__bench_aoi432_and2"
    assert payload["nodes"][12]["name"] == "y__bench_aoi432_and3"
    assert payload["nodes"][13]["name"] == "y__bench_aoi432_and4"
    assert payload["nodes"][14]["name"] == "y__bench_aoi432_and5"
    assert payload["nodes"][15]["name"] == "y__bench_aoi432_or0"
    assert payload["nodes"][16]["name"] == "y__bench_aoi432_or1"
    assert payload["nodes"][17]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai432_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\ny = OAI432(a, b, c, d, e, f, g, h, i)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][9]["name"] == "y__bench_oai432_or0"
    assert payload["nodes"][10]["name"] == "y__bench_oai432_or1"
    assert payload["nodes"][11]["name"] == "y__bench_oai432_or2"
    assert payload["nodes"][12]["name"] == "y__bench_oai432_or3"
    assert payload["nodes"][13]["name"] == "y__bench_oai432_or4"
    assert payload["nodes"][14]["name"] == "y__bench_oai432_or5"
    assert payload["nodes"][15]["name"] == "y__bench_oai432_and0"
    assert payload["nodes"][16]["name"] == "y__bench_oai432_and1"
    assert payload["nodes"][17]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi433_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\ny = AOI433(a, b, c, d, e, f, g, h, i, j)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][10]["name"] == "y__bench_aoi433_and0"
    assert payload["nodes"][11]["name"] == "y__bench_aoi433_and1"
    assert payload["nodes"][12]["name"] == "y__bench_aoi433_and2"
    assert payload["nodes"][13]["name"] == "y__bench_aoi433_and3"
    assert payload["nodes"][14]["name"] == "y__bench_aoi433_and4"
    assert payload["nodes"][15]["name"] == "y__bench_aoi433_and5"
    assert payload["nodes"][16]["name"] == "y__bench_aoi433_and6"
    assert payload["nodes"][17]["name"] == "y__bench_aoi433_or0"
    assert payload["nodes"][18]["name"] == "y__bench_aoi433_or1"
    assert payload["nodes"][19]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai433_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\ny = OAI433(a, b, c, d, e, f, g, h, i, j)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][10]["name"] == "y__bench_oai433_or0"
    assert payload["nodes"][11]["name"] == "y__bench_oai433_or1"
    assert payload["nodes"][12]["name"] == "y__bench_oai433_or2"
    assert payload["nodes"][13]["name"] == "y__bench_oai433_or3"
    assert payload["nodes"][14]["name"] == "y__bench_oai433_or4"
    assert payload["nodes"][15]["name"] == "y__bench_oai433_or5"
    assert payload["nodes"][16]["name"] == "y__bench_oai433_or6"
    assert payload["nodes"][17]["name"] == "y__bench_oai433_and0"
    assert payload["nodes"][18]["name"] == "y__bench_oai433_and1"
    assert payload["nodes"][19]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi441_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\ny = AOI441(a, b, c, d, e, f, g, h, i)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][9]["name"] == "y__bench_aoi441_and0"
    assert payload["nodes"][10]["name"] == "y__bench_aoi441_and1"
    assert payload["nodes"][11]["name"] == "y__bench_aoi441_and2"
    assert payload["nodes"][12]["name"] == "y__bench_aoi441_and3"
    assert payload["nodes"][13]["name"] == "y__bench_aoi441_and4"
    assert payload["nodes"][14]["name"] == "y__bench_aoi441_and5"
    assert payload["nodes"][15]["name"] == "y__bench_aoi441_or0"
    assert payload["nodes"][16]["name"] == "y__bench_aoi441_or1"
    assert payload["nodes"][17]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai441_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\ny = OAI441(a, b, c, d, e, f, g, h, i)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][9]["name"] == "y__bench_oai441_or0"
    assert payload["nodes"][10]["name"] == "y__bench_oai441_or1"
    assert payload["nodes"][11]["name"] == "y__bench_oai441_or2"
    assert payload["nodes"][12]["name"] == "y__bench_oai441_or3"
    assert payload["nodes"][13]["name"] == "y__bench_oai441_or4"
    assert payload["nodes"][14]["name"] == "y__bench_oai441_or5"
    assert payload["nodes"][15]["name"] == "y__bench_oai441_and0"
    assert payload["nodes"][16]["name"] == "y__bench_oai441_and1"
    assert payload["nodes"][17]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi442_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\ny = AOI442(a, b, c, d, e, f, g, h, i, j)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][10]["name"] == "y__bench_aoi442_and0"
    assert payload["nodes"][11]["name"] == "y__bench_aoi442_and1"
    assert payload["nodes"][12]["name"] == "y__bench_aoi442_and2"
    assert payload["nodes"][13]["name"] == "y__bench_aoi442_and3"
    assert payload["nodes"][14]["name"] == "y__bench_aoi442_and4"
    assert payload["nodes"][15]["name"] == "y__bench_aoi442_and5"
    assert payload["nodes"][16]["name"] == "y__bench_aoi442_and6"
    assert payload["nodes"][17]["name"] == "y__bench_aoi442_or0"
    assert payload["nodes"][18]["name"] == "y__bench_aoi442_or1"
    assert payload["nodes"][19]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai442_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\ny = OAI442(a, b, c, d, e, f, g, h, i, j)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][10]["name"] == "y__bench_oai442_or0"
    assert payload["nodes"][11]["name"] == "y__bench_oai442_or1"
    assert payload["nodes"][12]["name"] == "y__bench_oai442_or2"
    assert payload["nodes"][13]["name"] == "y__bench_oai442_or3"
    assert payload["nodes"][14]["name"] == "y__bench_oai442_or4"
    assert payload["nodes"][15]["name"] == "y__bench_oai442_or5"
    assert payload["nodes"][16]["name"] == "y__bench_oai442_or6"
    assert payload["nodes"][17]["name"] == "y__bench_oai442_and0"
    assert payload["nodes"][18]["name"] == "y__bench_oai442_and1"
    assert payload["nodes"][19]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi443_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\ny = AOI443(a, b, c, d, e, f, g, h, i, j, k)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][11]["name"] == "y__bench_aoi443_and0"
    assert payload["nodes"][12]["name"] == "y__bench_aoi443_and1"
    assert payload["nodes"][13]["name"] == "y__bench_aoi443_and2"
    assert payload["nodes"][14]["name"] == "y__bench_aoi443_and3"
    assert payload["nodes"][15]["name"] == "y__bench_aoi443_and4"
    assert payload["nodes"][16]["name"] == "y__bench_aoi443_and5"
    assert payload["nodes"][17]["name"] == "y__bench_aoi443_and6"
    assert payload["nodes"][18]["name"] == "y__bench_aoi443_and7"
    assert payload["nodes"][19]["name"] == "y__bench_aoi443_or0"
    assert payload["nodes"][20]["name"] == "y__bench_aoi443_or1"
    assert payload["nodes"][21]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai443_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\ny = OAI443(a, b, c, d, e, f, g, h, i, j, k)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][11]["name"] == "y__bench_oai443_or0"
    assert payload["nodes"][12]["name"] == "y__bench_oai443_or1"
    assert payload["nodes"][13]["name"] == "y__bench_oai443_or2"
    assert payload["nodes"][14]["name"] == "y__bench_oai443_or3"
    assert payload["nodes"][15]["name"] == "y__bench_oai443_or4"
    assert payload["nodes"][16]["name"] == "y__bench_oai443_or5"
    assert payload["nodes"][17]["name"] == "y__bench_oai443_or6"
    assert payload["nodes"][18]["name"] == "y__bench_oai443_or7"
    assert payload["nodes"][19]["name"] == "y__bench_oai443_and0"
    assert payload["nodes"][20]["name"] == "y__bench_oai443_and1"
    assert payload["nodes"][21]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi444_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\nINPUT(l)\ny = AOI444(a, b, c, d, e, f, g, h, i, j, k, l)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "And", "And", "And", "And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][12]["name"] == "y__bench_aoi444_and0"
    assert payload["nodes"][13]["name"] == "y__bench_aoi444_and1"
    assert payload["nodes"][14]["name"] == "y__bench_aoi444_and2"
    assert payload["nodes"][15]["name"] == "y__bench_aoi444_and3"
    assert payload["nodes"][16]["name"] == "y__bench_aoi444_and4"
    assert payload["nodes"][17]["name"] == "y__bench_aoi444_and5"
    assert payload["nodes"][18]["name"] == "y__bench_aoi444_and6"
    assert payload["nodes"][19]["name"] == "y__bench_aoi444_and7"
    assert payload["nodes"][20]["name"] == "y__bench_aoi444_and8"
    assert payload["nodes"][21]["name"] == "y__bench_aoi444_or0"
    assert payload["nodes"][22]["name"] == "y__bench_aoi444_or1"
    assert payload["nodes"][23]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai444_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\nINPUT(h)\nINPUT(i)\nINPUT(j)\nINPUT(k)\nINPUT(l)\ny = OAI444(a, b, c, d, e, f, g, h, i, j, k, l)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "Or", "Or", "Or", "Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][12]["name"] == "y__bench_oai444_or0"
    assert payload["nodes"][13]["name"] == "y__bench_oai444_or1"
    assert payload["nodes"][14]["name"] == "y__bench_oai444_or2"
    assert payload["nodes"][15]["name"] == "y__bench_oai444_or3"
    assert payload["nodes"][16]["name"] == "y__bench_oai444_or4"
    assert payload["nodes"][17]["name"] == "y__bench_oai444_or5"
    assert payload["nodes"][18]["name"] == "y__bench_oai444_or6"
    assert payload["nodes"][19]["name"] == "y__bench_oai444_or7"
    assert payload["nodes"][20]["name"] == "y__bench_oai444_or8"
    assert payload["nodes"][21]["name"] == "y__bench_oai444_and0"
    assert payload["nodes"][22]["name"] == "y__bench_oai444_and1"
    assert payload["nodes"][23]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi221_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\ny = AOI221(a, b, c, d, e)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][5]["name"] == "y__bench_aoi221_and0"
    assert payload["nodes"][6]["name"] == "y__bench_aoi221_and1"
    assert payload["nodes"][7]["name"] == "y__bench_aoi221_or0"
    assert payload["nodes"][8]["name"] == "y__bench_aoi221_or1"
    assert payload["nodes"][9]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai221_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\ny = OAI221(a, b, c, d, e)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][5]["name"] == "y__bench_oai221_or0"
    assert payload["nodes"][6]["name"] == "y__bench_oai221_or1"
    assert payload["nodes"][7]["name"] == "y__bench_oai221_and0"
    assert payload["nodes"][8]["name"] == "y__bench_oai221_and1"
    assert payload["nodes"][9]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi222_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\ny = AOI222(a, b, c, d, e, f)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "Or", "Or", "Not"]
    assert payload["nodes"][6]["name"] == "y__bench_aoi222_and0"
    assert payload["nodes"][7]["name"] == "y__bench_aoi222_and1"
    assert payload["nodes"][8]["name"] == "y__bench_aoi222_and2"
    assert payload["nodes"][9]["name"] == "y__bench_aoi222_or0"
    assert payload["nodes"][10]["name"] == "y__bench_aoi222_or1"
    assert payload["nodes"][11]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai222_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\ny = OAI222(a, b, c, d, e, f)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "And", "And", "Not"]
    assert payload["nodes"][6]["name"] == "y__bench_oai222_or0"
    assert payload["nodes"][7]["name"] == "y__bench_oai222_or1"
    assert payload["nodes"][8]["name"] == "y__bench_oai222_or2"
    assert payload["nodes"][9]["name"] == "y__bench_oai222_and0"
    assert payload["nodes"][10]["name"] == "y__bench_oai222_and1"
    assert payload["nodes"][11]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_aoi2221_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\ny = AOI2221(a, b, c, d, e, f, g)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["And", "And", "And", "Or", "Or", "Or", "Not"]
    assert payload["nodes"][7]["name"] == "y__bench_aoi2221_and0"
    assert payload["nodes"][8]["name"] == "y__bench_aoi2221_and1"
    assert payload["nodes"][9]["name"] == "y__bench_aoi2221_and2"
    assert payload["nodes"][10]["name"] == "y__bench_aoi2221_or0"
    assert payload["nodes"][11]["name"] == "y__bench_aoi2221_or1"
    assert payload["nodes"][12]["name"] == "y__bench_aoi2221_or2"
    assert payload["nodes"][13]["name"] == "y"


def test_convert_quaigh_bench_to_ir_fixture_supports_oai2221_lowering() -> None:
    payload = convert_bench_to_ir_json(
        "INPUT(a)\nINPUT(b)\nINPUT(c)\nINPUT(d)\nINPUT(e)\nINPUT(f)\nINPUT(g)\ny = OAI2221(a, b, c, d, e, f, g)\nOUTPUT(y)\n"
    )

    logic_ops = [node.get("logic_op") for node in payload["nodes"] if node["kind"] == "CellInstance"]
    assert logic_ops == ["Or", "Or", "Or", "And", "And", "And", "Not"]
    assert payload["nodes"][7]["name"] == "y__bench_oai2221_or0"
    assert payload["nodes"][8]["name"] == "y__bench_oai2221_or1"
    assert payload["nodes"][9]["name"] == "y__bench_oai2221_or2"
    assert payload["nodes"][10]["name"] == "y__bench_oai2221_and0"
    assert payload["nodes"][11]["name"] == "y__bench_oai2221_and1"
    assert payload["nodes"][12]["name"] == "y__bench_oai2221_and2"
    assert payload["nodes"][13]["name"] == "y"
