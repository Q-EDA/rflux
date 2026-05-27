from __future__ import annotations

import argparse
import difflib
import importlib
import inspect
import json
from pathlib import Path


DEFAULT_MODULES = [
    "rflux",
    "rflux.flow",
    "rflux.timing",
    "rflux.sim",
    "rflux.verify",
    "rflux.pdk",
]


def _safe_signature(obj: object) -> str | None:
    try:
        return str(inspect.signature(obj))
    except (TypeError, ValueError):
        return None


def _symbol_entry(name: str, value: object) -> dict[str, object]:
    if inspect.ismodule(value):
        return {"name": name, "kind": "module"}

    if inspect.isclass(value):
        return {
            "name": name,
            "kind": "class",
            "init_signature": _safe_signature(value),
        }

    if callable(value):
        return {
            "name": name,
            "kind": "callable",
            "signature": _safe_signature(value),
        }

    return {
        "name": name,
        "kind": "value",
        "type": type(value).__name__,
    }


def collect_module_surface(module_name: str) -> dict[str, object]:
    module = importlib.import_module(module_name)
    exported = list(getattr(module, "__all__", []))

    symbols: list[dict[str, object]] = []
    for name in exported:
        value = getattr(module, name)
        symbols.append(_symbol_entry(name, value))

    return {
        "module": module_name,
        "exported_count": len(exported),
        "exported": exported,
        "symbols": symbols,
    }


def build_surface_payload(module_names: list[str]) -> dict[str, object]:
    modules = {name: collect_module_surface(name) for name in module_names}
    return {
        "schema_version": 1,
        "kind": "python_api_surface",
        "modules": modules,
    }


def _canonical_json(payload: dict[str, object]) -> str:
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def assert_surfaces_match(expected: dict[str, object], actual: dict[str, object]) -> None:
    if expected == actual:
        return

    expected_text = _canonical_json(expected).splitlines(keepends=True)
    actual_text = _canonical_json(actual).splitlines(keepends=True)
    diff = "".join(
        difflib.unified_diff(
            expected_text,
            actual_text,
            fromfile="expected",
            tofile="actual",
        )
    )
    raise ValueError(f"python API surface mismatch:\n{diff}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export and optionally validate the public Python API surface contract.",
    )
    parser.add_argument(
        "--modules",
        nargs="+",
        default=DEFAULT_MODULES,
        help="Module list to scan for exported API surface.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("python/tests/contracts/python_api_surface.json"),
        help="Where to write the collected API surface JSON.",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=Path("python/tests/contracts/python_api_surface.json"),
        help="Baseline JSON used by --check.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Compare collected surface to the baseline and fail on mismatch.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    payload = build_surface_payload(args.modules)

    output_path = args.output if args.output.is_absolute() else (repo_root / args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(_canonical_json(payload), encoding="utf-8")

    if args.check:
        baseline_path = (
            args.baseline if args.baseline.is_absolute() else (repo_root / args.baseline)
        )
        if not baseline_path.exists():
            raise SystemExit(f"baseline file does not exist: {baseline_path}")
        baseline_payload = json.loads(baseline_path.read_text(encoding="utf-8"))
        try:
            assert_surfaces_match(baseline_payload, payload)
        except ValueError as exc:
            raise SystemExit(str(exc)) from exc


if __name__ == "__main__":
    main()
