from __future__ import annotations

import argparse
import json
import tomllib
from pathlib import Path


def _normalize_package(package: dict) -> dict[str, object]:
    source = package.get("source", {})
    source_kind = "unknown"
    source_value = None
    if isinstance(source, dict):
        if "registry" in source:
            source_kind = "registry"
            source_value = source["registry"]
        elif "virtual" in source:
            source_kind = "virtual"
            source_value = source["virtual"]

    dependencies = []
    for dependency in package.get("dependencies", []):
        dependencies.append(
            {
                "name": dependency["name"],
                "marker": dependency.get("marker"),
            }
        )

    return {
        "name": package["name"],
        "version": package["version"],
        "source_kind": source_kind,
        "source": source_value,
        "dependencies": dependencies,
    }


def build_inventory(lock_path: Path) -> dict[str, object]:
    payload = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    packages = payload.get("package", [])
    normalized_packages = sorted(
        (_normalize_package(package) for package in packages),
        key=lambda package: (str(package["name"]), str(package["version"])),
    )

    root_package = next(
        (package for package in packages if package.get("source", {}).get("virtual") == "."),
        None,
    )
    root_project = None
    if root_package is not None:
        requires_dev = root_package.get("metadata", {}).get("requires-dev", {})
        root_project = {
            "name": root_package["name"],
            "version": root_package["version"],
            "requires_dev": {
                group: [dependency["name"] for dependency in dependencies]
                for group, dependencies in sorted(requires_dev.items())
            },
        }

    return {
        "tool": "uv",
        "lock_version": payload["version"],
        "lock_revision": payload["revision"],
        "requires_python": payload["requires-python"],
        "root_project": root_project,
        "package_count": len(normalized_packages),
        "packages": normalized_packages,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export a machine-readable Python dependency inventory from uv.lock.",
    )
    parser.add_argument(
        "--lock",
        type=Path,
        default=Path("uv.lock"),
        help="Path to uv.lock",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Optional JSON output path",
    )
    args = parser.parse_args()

    inventory = build_inventory(args.lock)
    rendered = json.dumps(inventory, indent=2, sort_keys=True)
    print(rendered)

    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()