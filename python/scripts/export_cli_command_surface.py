from __future__ import annotations

import argparse
import difflib
import json
import re
from pathlib import Path


DEFAULT_SOURCE = Path("crates/cli/src/main.rs")
DEFAULT_CONTRACT = Path("python/tests/contracts/cli_command_surface.json")


def _camel_to_kebab(name: str) -> str:
    text = re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()
    return text.replace("_", "-")


def _extract_arg_attr(raw_attr: str) -> dict[str, object]:
    attr = raw_attr.strip()
    long_value: str | None = None
    long_present = bool(re.search(r"\blong\b", attr))
    match_long = re.search(r'\blong\s*=\s*"([^"]+)"', attr)
    if match_long:
        long_value = match_long.group(1)

    short_value: str | None = None
    match_short_char = re.search(r"\bshort\s*=\s*'([^'])'", attr)
    if match_short_char:
        short_value = match_short_char.group(1)
    else:
        match_short_string = re.search(r'\bshort\s*=\s*"([^"]+)"', attr)
        if match_short_string:
            short_value = match_short_string.group(1)

    return {
        "long_present": long_present,
        "long_value": long_value,
        "short": short_value,
        "value_enum": bool(re.search(r"\bvalue_enum\b", attr)),
        "has_default": bool(re.search(r"\bdefault_value(?:_t)?\b", attr)),
    }


def _extract_struct_options(lines: list[str], start_index: int) -> tuple[list[dict[str, object]], int]:
    options: list[dict[str, object]] = []
    pending_arg_attr: str | None = None
    brace_depth = 0
    index = start_index
    while index < len(lines):
        line = lines[index]
        brace_depth += line.count("{")
        brace_depth -= line.count("}")

        stripped = line.strip()
        if stripped.startswith("#[arg("):
            pending_arg_attr = stripped[len("#[arg(") :]
            while pending_arg_attr is not None and ")]" not in pending_arg_attr and index + 1 < len(lines):
                index += 1
                pending_arg_attr += lines[index].strip()
            if pending_arg_attr is not None:
                pending_arg_attr = pending_arg_attr.rsplit(")]", 1)[0]
            index += 1
            continue

        field_match = re.match(r"\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*:\s*([^,]+),", line)
        if field_match and pending_arg_attr is not None:
            field_name = field_match.group(1)
            field_type = field_match.group(2).strip()
            attr = _extract_arg_attr(pending_arg_attr)
            pending_arg_attr = None

            if not attr["long_present"]:
                index += 1
                continue

            long_name = attr["long_value"] or field_name.replace("_", "-")
            option_entry: dict[str, object] = {
                "long": str(long_name),
                "required": not field_type.startswith("Option<"),
                "has_default": bool(attr["has_default"]),
                "value_type": field_type,
                "value_enum": bool(attr["value_enum"]),
            }
            if attr["short"]:
                option_entry["short"] = attr["short"]
            options.append(option_entry)

        if brace_depth == 0:
            break
        index += 1
    return options, index


def _extract_commands(lines: list[str]) -> tuple[list[dict[str, str]], dict[str, list[dict[str, object]]]]:
    commands: list[dict[str, str]] = []
    struct_options: dict[str, list[dict[str, object]]] = {}

    index = 0
    while index < len(lines):
        line = lines[index]

        enum_match = re.match(r"\s*enum\s+Commands\s*\{", line)
        if enum_match:
            index += 1
            while index < len(lines):
                variant_line = lines[index].strip()
                if variant_line == "}":
                    break
                variant_match = re.match(r"([A-Za-z0-9_]+)\(([^)]+)\),", variant_line)
                if variant_match:
                    variant_name = variant_match.group(1)
                    args_struct = variant_match.group(2).strip()
                    commands.append(
                        {
                            "variant": variant_name,
                            "command": _camel_to_kebab(variant_name),
                            "args_struct": args_struct,
                        }
                    )
                index += 1

        struct_match = re.match(r"\s*struct\s+([A-Za-z0-9_]+Args)\s*\{", line)
        if struct_match:
            struct_name = struct_match.group(1)
            options, end_index = _extract_struct_options(lines, index)
            options.sort(key=lambda item: str(item["long"]))
            struct_options[struct_name] = options
            index = end_index

        index += 1

    return commands, struct_options


def _extract_value_enums(lines: list[str]) -> dict[str, list[str]]:
    enums: dict[str, list[str]] = {}
    derive_has_value_enum = False
    index = 0
    while index < len(lines):
        stripped = lines[index].strip()
        if stripped.startswith("#[derive("):
            derive_has_value_enum = "ValueEnum" in stripped
            index += 1
            continue

        enum_match = re.match(r"\s*enum\s+([A-Za-z0-9_]+)\s*\{", lines[index])
        if enum_match and derive_has_value_enum:
            enum_name = enum_match.group(1)
            values: list[str] = []
            pending_value_attr: str | None = None
            index += 1
            while index < len(lines):
                current = lines[index].strip()
                if current == "}":
                    break
                if current.startswith("#[value("):
                    pending_value_attr = current[len("#[value(") :]
                    while pending_value_attr is not None and ")]" not in pending_value_attr and index + 1 < len(lines):
                        index += 1
                        pending_value_attr += lines[index].strip()
                    if pending_value_attr is not None:
                        pending_value_attr = pending_value_attr.rsplit(")]", 1)[0]
                variant_match = re.match(r"([A-Za-z0-9_]+),", current)
                if variant_match:
                    variant_name = variant_match.group(1)
                    explicit = None
                    if pending_value_attr:
                        explicit_match = re.search(r'name\s*=\s*"([^"]+)"', pending_value_attr)
                        if explicit_match:
                            explicit = explicit_match.group(1)
                    values.append(explicit or _camel_to_kebab(variant_name))
                    pending_value_attr = None
                index += 1
            enums[enum_name] = values
            derive_has_value_enum = False
        else:
            derive_has_value_enum = False
        index += 1
    return enums


def build_surface_payload(source_path: Path) -> dict[str, object]:
    lines = source_path.read_text(encoding="utf-8").splitlines()
    commands, struct_options = _extract_commands(lines)
    value_enums = _extract_value_enums(lines)

    command_payload: list[dict[str, object]] = []
    for entry in commands:
        struct_name = entry["args_struct"]
        options: list[dict[str, object]] = []
        for option in struct_options.get(struct_name, []):
            option_copy = dict(option)
            if bool(option_copy.get("value_enum")):
                value_type = str(option_copy.get("value_type", ""))
                enum_type = value_type
                if enum_type.startswith("Option<") and enum_type.endswith(">"):
                    enum_type = enum_type[len("Option<") : -1]
                enum_values = value_enums.get(enum_type)
                if enum_values:
                    option_copy["enum_values"] = enum_values
            options.append(option_copy)

        command_payload.append(
            {
                "command": entry["command"],
                "variant": entry["variant"],
                "args_struct": struct_name,
                "option_count": len(options),
                "options": options,
            }
        )

    return {
        "schema_version": 1,
        "kind": "cli_command_surface",
        "source": source_path.as_posix(),
        "command_count": len(command_payload),
        "commands": command_payload,
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
    raise ValueError(f"CLI command surface mismatch:\n{diff}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export and optionally validate the CLI command/option surface contract.",
    )
    parser.add_argument(
        "--source",
        type=Path,
        default=DEFAULT_SOURCE,
        help="Path to the Rust CLI source used for static surface extraction.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_CONTRACT,
        help="Where to write the collected CLI surface JSON.",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=DEFAULT_CONTRACT,
        help="Baseline JSON used by --check.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Compare extracted CLI surface to baseline and fail on mismatch.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    source_path = args.source if args.source.is_absolute() else (repo_root / args.source)
    payload = build_surface_payload(source_path)

    output_path = args.output if args.output.is_absolute() else (repo_root / args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(_canonical_json(payload), encoding="utf-8")

    if args.check:
        baseline_path = args.baseline if args.baseline.is_absolute() else (repo_root / args.baseline)
        if not baseline_path.exists():
            raise SystemExit(f"baseline file does not exist: {baseline_path}")
        baseline_payload = json.loads(baseline_path.read_text(encoding="utf-8"))
        try:
            assert_surfaces_match(baseline_payload, payload)
        except ValueError as exc:
            raise SystemExit(str(exc)) from exc


if __name__ == "__main__":
    main()
