from __future__ import annotations

import json
import os
import subprocess
import sys
import textwrap
import zipfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "python" / "scripts" / "export_python_dependency_inventory.py"
LICENSE_SCRIPT_PATH = REPO_ROOT / "python" / "scripts" / "export_python_license_inventory.py"


def test_export_python_dependency_inventory_script_outputs_expected_fields() -> None:
    completed = subprocess.run(
        [sys.executable, str(SCRIPT_PATH)],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
    )

    assert completed.returncode == 0, completed.stderr + "\n" + completed.stdout
    payload = json.loads(completed.stdout)

    assert payload["tool"] == "uv"
    assert payload["lock_version"] == 1
    assert payload["requires_python"] == ">=3.12"
    assert payload["root_project"]["name"] == "rflux"
    assert payload["root_project"]["requires_dev"]["dev"] == ["maturin", "pytest", "ruff"]
    assert payload["package_count"] >= 1

    package_names = {package["name"] for package in payload["packages"]}
    assert {"pytest", "ruff", "maturin"}.issubset(package_names)


def test_export_python_license_inventory_reads_wheel_metadata(tmp_path: Path) -> None:
    lock_path = tmp_path / "uv.lock"
    cache_dir = tmp_path / "uv-cache"
    pointer_path = cache_dir / "wheels-v6" / "pypi" / "demo-pkg" / "1.2.3-py3-none-any"
    archive_dir = cache_dir / "archive-v0" / "demo-wheel"
    archive_dist_info = archive_dir / "demo_pkg-1.2.3.dist-info"
    archive_dist_info.mkdir(parents=True)
    (archive_dist_info / "METADATA").write_text(
        textwrap.dedent(
            """\
            Metadata-Version: 2.4
            Name: demo-pkg
            Version: 1.2.3
            License-Expression: Apache-2.0
            Classifier: License :: OSI Approved :: Apache Software License
            """
        ),
        encoding="utf-8",
    )
    pointer_path.parent.mkdir(parents=True)
    pointer_path.write_text("archive-v0/demo-wheel", encoding="utf-8")

    lock_path.write_text(
        textwrap.dedent(
            f"""\
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [[package]]
            name = "demo-pkg"
            version = "1.2.3"
            source = {{ registry = "https://pypi.org/simple" }}
            wheels = [
                {{ url = "https://files.pythonhosted.org/packages/demo/demo_pkg-1.2.3-py3-none-any.whl", hash = "sha256:test", size = 123, upload-time = "2026-05-22T00:00:00Z" }},
            ]
            """
        ),
        encoding="utf-8",
    )

    env = os.environ.copy()
    env["UV_CACHE_DIR"] = str(cache_dir)
    completed = subprocess.run(
        [sys.executable, str(LICENSE_SCRIPT_PATH), "--lock", str(lock_path)],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
        env=env,
    )

    assert completed.returncode == 0, completed.stderr + "\n" + completed.stdout
    payload = json.loads(completed.stdout)

    assert payload["metadata_source"] == "wheel-metadata"
    assert payload["package_count"] == 1
    assert payload["summary"] == {
        "ok_count": 1,
        "missing_count": 0,
        "counts_by_status": {"ok": 1},
        "missing_packages": [],
    }
    package = payload["packages"][0]
    assert package["name"] == "demo-pkg"
    assert package["metadata_source_kind"] == "uv-cache"
    assert package["license_expression"] == "Apache-2.0"
    assert package["license_classifiers"] == [
        "License :: OSI Approved :: Apache Software License"
    ]
    assert package["status"] == "ok"


def test_export_python_license_inventory_reports_missing_license_metadata(tmp_path: Path) -> None:
    lock_path = tmp_path / "uv.lock"
    cache_dir = tmp_path / "uv-cache"
    pointer_path = cache_dir / "wheels-v6" / "pypi" / "demo-pkg" / "1.2.3-py3-none-any"
    archive_dir = cache_dir / "archive-v0" / "demo-wheel"
    archive_dist_info = archive_dir / "demo_pkg-1.2.3.dist-info"
    archive_dist_info.mkdir(parents=True)
    (archive_dist_info / "METADATA").write_text(
        textwrap.dedent(
            """\
            Metadata-Version: 2.4
            Name: demo-pkg
            Version: 1.2.3
            Summary: package without declared license metadata
            """
        ),
        encoding="utf-8",
    )
    pointer_path.parent.mkdir(parents=True)
    pointer_path.write_text("archive-v0/demo-wheel", encoding="utf-8")

    lock_path.write_text(
        textwrap.dedent(
            f"""\
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [[package]]
            name = "demo-pkg"
            version = "1.2.3"
            source = {{ registry = "https://pypi.org/simple" }}
            wheels = [
                {{ url = "https://files.pythonhosted.org/packages/demo/demo_pkg-1.2.3-py3-none-any.whl", hash = "sha256:test", size = 123, upload-time = "2026-05-22T00:00:00Z" }},
            ]
            """
        ),
        encoding="utf-8",
    )

    env = os.environ.copy()
    env["UV_CACHE_DIR"] = str(cache_dir)
    completed = subprocess.run(
        [sys.executable, str(LICENSE_SCRIPT_PATH), "--lock", str(lock_path)],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
        env=env,
    )

    assert completed.returncode == 0, completed.stderr + "\n" + completed.stdout
    payload = json.loads(completed.stdout)

    assert payload["summary"] == {
        "ok_count": 0,
        "missing_count": 1,
        "counts_by_status": {"missing-license-metadata": 1},
        "missing_packages": [
            {
                "name": "demo-pkg",
                "version": "1.2.3",
                "status": "missing-license-metadata",
            }
        ],
    }
    package = payload["packages"][0]
    assert package["status"] == "missing-license-metadata"


def test_export_python_license_inventory_reports_fetch_failures_without_aborting(tmp_path: Path) -> None:
    lock_path = tmp_path / "uv.lock"
    lock_path.write_text(
        textwrap.dedent(
            """\
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [[package]]
            name = "demo-pkg"
            version = "1.2.3"
            source = { registry = "https://pypi.org/simple" }
            wheels = [
                { url = "https://127.0.0.1:9/demo_pkg-1.2.3-py3-none-any.whl", hash = "sha256:test", size = 123, upload-time = "2026-05-22T00:00:00Z" },
            ]
            """
        ),
        encoding="utf-8",
    )

    env = os.environ.copy()
    env["UV_CACHE_DIR"] = str(tmp_path / "missing-cache")
    completed = subprocess.run(
        [sys.executable, str(LICENSE_SCRIPT_PATH), "--lock", str(lock_path)],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
        env=env,
    )

    assert completed.returncode == 0, completed.stderr + "\n" + completed.stdout
    payload = json.loads(completed.stdout)
    assert payload["summary"] == {
        "ok_count": 0,
        "missing_count": 1,
        "counts_by_status": {"metadata-fetch-failed": 1},
        "missing_packages": [
            {
                "name": "demo-pkg",
                "version": "1.2.3",
                "status": "metadata-fetch-failed",
            }
        ],
    }
    package = payload["packages"][0]
    assert package["metadata_source_kind"] == "wheel-url"
    assert package["status"] == "metadata-fetch-failed"
    assert package["error"]


def test_export_python_license_inventory_respects_uv_offline_mode(tmp_path: Path) -> None:
    lock_path = tmp_path / "uv.lock"
    lock_path.write_text(
        textwrap.dedent(
            """\
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [[package]]
            name = "demo-pkg"
            version = "1.2.3"
            source = { registry = "https://pypi.org/simple" }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/demo/demo_pkg-1.2.3-py3-none-any.whl", hash = "sha256:test", size = 123, upload-time = "2026-05-22T00:00:00Z" },
            ]
            """
        ),
        encoding="utf-8",
    )

    env = os.environ.copy()
    env["UV_CACHE_DIR"] = str(tmp_path / "missing-cache")
    env["UV_OFFLINE"] = "1"
    completed = subprocess.run(
        [sys.executable, str(LICENSE_SCRIPT_PATH), "--lock", str(lock_path)],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
        env=env,
    )

    assert completed.returncode == 0, completed.stderr + "\n" + completed.stdout
    payload = json.loads(completed.stdout)
    package = payload["packages"][0]
    assert package["status"] == "metadata-fetch-failed"
    assert package["error"] == "RuntimeError"