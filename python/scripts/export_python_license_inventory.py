from __future__ import annotations

import argparse
import json
import os
import tomllib
import urllib.request
import zipfile
from email.parser import Parser
from io import BytesIO
from pathlib import Path
from urllib.parse import urlparse


def _parse_metadata_text(metadata_text: str) -> dict[str, object]:
    metadata = Parser().parsestr(metadata_text, headersonly=True)
    classifiers = [
        classifier
        for classifier in metadata.get_all("Classifier", [])
        if classifier.startswith("License ::")
    ]
    return {
        "license_expression": metadata.get("License-Expression"),
        "license": metadata.get("License"),
        "license_classifiers": classifiers,
    }


def _read_metadata_from_archive_dir(archive_dir: Path) -> dict[str, object]:
    metadata_path = next(archive_dir.glob("*.dist-info/METADATA"))
    metadata_text = metadata_path.read_text(encoding="utf-8", errors="replace")
    return _parse_metadata_text(metadata_text)


def _extract_metadata_headers_from_wheel(url: str) -> dict[str, object]:
    with urllib.request.urlopen(url, timeout=60) as response:
        wheel_bytes = response.read()

    with zipfile.ZipFile(BytesIO(wheel_bytes)) as wheel_zip:
        metadata_name = next(
            name for name in wheel_zip.namelist() if name.endswith(".dist-info/METADATA")
        )
        metadata_text = wheel_zip.read(metadata_name).decode("utf-8", errors="replace")

    return _parse_metadata_text(metadata_text)


def _default_uv_cache_dir() -> Path:
    override = os.environ.get("UV_CACHE_DIR", "").strip()
    if override:
        return Path(override)

    local_app_data = os.environ.get("LOCALAPPDATA", "").strip()
    if local_app_data:
        return Path(local_app_data) / "uv" / "cache"

    return Path.home() / ".cache" / "uv"


def _network_fetch_disabled() -> bool:
    return os.environ.get("UV_OFFLINE", "").strip().lower() in {"1", "true", "yes", "on"}


def _wheel_cache_key(package_name: str, wheel_url: str) -> str | None:
    parsed = urlparse(wheel_url)
    wheel_name = Path(parsed.path).name
    if not wheel_name.endswith(".whl"):
        return None

    wheel_stem = wheel_name.removesuffix(".whl")
    normalized_name = package_name.replace("-", "_")
    prefix = f"{normalized_name}-"
    if not wheel_stem.startswith(prefix):
        return None
    return wheel_stem[len(prefix) :]


def _extract_metadata_headers_from_uv_cache(
    cache_dir: Path,
    package_name: str,
    wheel_url: str,
) -> dict[str, object] | None:
    cache_key = _wheel_cache_key(package_name, wheel_url)
    if cache_key is None:
        return None

    pointer_path = cache_dir / "wheels-v6" / "pypi" / package_name / cache_key
    if not pointer_path.is_file():
        return None

    archive_rel_path = pointer_path.read_text(encoding="utf-8", errors="replace").strip()
    if not archive_rel_path:
        return None

    archive_dir = cache_dir / archive_rel_path
    if not archive_dir.is_dir():
        return None

    return _read_metadata_from_archive_dir(archive_dir)


def _normalize_license_entry(package: dict, cache_dir: Path) -> dict[str, object]:
    wheel_url = None
    wheels = package.get("wheels", [])
    if wheels:
        wheel_url = wheels[0].get("url")

    if wheel_url is None:
        return {
            "name": package["name"],
            "version": package["version"],
            "metadata_source_url": None,
            "license_expression": None,
            "license": None,
            "license_classifiers": [],
            "status": "missing-wheel-metadata",
        }

    try:
        metadata = _extract_metadata_headers_from_uv_cache(cache_dir, package["name"], wheel_url)
        metadata_source = "uv-cache"
        if metadata is None:
            if _network_fetch_disabled():
                raise RuntimeError("OfflineMode")
            metadata = _extract_metadata_headers_from_wheel(wheel_url)
            metadata_source = "wheel-url"
    except Exception as error:
        return {
            "name": package["name"],
            "version": package["version"],
            "metadata_source_url": wheel_url,
            "metadata_source_kind": "wheel-url",
            "license_expression": None,
            "license": None,
            "license_classifiers": [],
            "status": "metadata-fetch-failed",
            "error": type(error).__name__,
        }

    has_license_metadata = any(
        [
            metadata["license_expression"],
            metadata["license"],
            metadata["license_classifiers"],
        ]
    )
    return {
        "name": package["name"],
        "version": package["version"],
        "metadata_source_url": wheel_url,
        "metadata_source_kind": metadata_source,
        "license_expression": metadata["license_expression"],
        "license": metadata["license"],
        "license_classifiers": metadata["license_classifiers"],
        "status": "ok" if has_license_metadata else "missing-license-metadata",
    }


def _build_summary(packages: list[dict[str, object]]) -> dict[str, object]:
    missing_packages = [package for package in packages if package["status"] != "ok"]
    counts_by_status: dict[str, int] = {}
    for package in packages:
        status = str(package["status"])
        counts_by_status[status] = counts_by_status.get(status, 0) + 1

    return {
        "ok_count": counts_by_status.get("ok", 0),
        "missing_count": len(missing_packages),
        "counts_by_status": counts_by_status,
        "missing_packages": [
            {
                "name": package["name"],
                "version": package["version"],
                "status": package["status"],
            }
            for package in missing_packages
        ],
    }


def build_license_inventory(lock_path: Path) -> dict[str, object]:
    payload = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    cache_dir = _default_uv_cache_dir()
    packages = [
        package
        for package in payload.get("package", [])
        if package.get("source", {}).get("virtual") != "."
    ]
    normalized_packages = sorted(
        (_normalize_license_entry(package, cache_dir) for package in packages),
        key=lambda package: (str(package["name"]), str(package["version"])),
    )

    return {
        "tool": "uv",
        "metadata_source": "wheel-metadata",
        "lock_version": payload["version"],
        "lock_revision": payload["revision"],
        "requires_python": payload["requires-python"],
        "package_count": len(normalized_packages),
        "summary": _build_summary(normalized_packages),
        "packages": normalized_packages,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export a machine-readable Python license inventory from uv.lock.",
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

    inventory = build_license_inventory(args.lock)
    rendered = json.dumps(inventory, indent=2, sort_keys=True)
    print(rendered)

    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()