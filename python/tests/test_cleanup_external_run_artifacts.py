from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path


def _load_module():
    script_path = Path(__file__).resolve().parents[1] / "scripts" / "cleanup_external_run_artifacts.py"
    spec = importlib.util.spec_from_file_location("cleanup_external_run_artifacts", script_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_module = _load_module()
cleanup_external_run_artifacts = _module.cleanup_external_run_artifacts


def _make_old_dir(path: Path) -> None:
    path.mkdir(parents=True)
    old_timestamp = 1_600_000_000
    os.utime(path, (old_timestamp, old_timestamp))


def test_cleanup_external_run_artifacts_dry_run_and_delete(tmp_path: Path) -> None:
    base_temp_dir = tmp_path
    stale_dir = base_temp_dir / "rflux-ext-111-222"
    fresh_dir = base_temp_dir / "rflux-ext-333-444"
    staged_file = base_temp_dir / "rflux-ext-555-666-input.sp"

    _make_old_dir(stale_dir)
    _make_old_dir(fresh_dir)
    os.utime(fresh_dir, None)
    staged_file.write_text("deck", encoding="utf-8")

    dry_run = cleanup_external_run_artifacts(
        base_temp_dir=base_temp_dir,
        prefix="rflux-ext-",
        older_than_days=1.0,
        delete=False,
    )

    assert dry_run["removed_count"] == 0
    assert dry_run["retained_count"] == 2
    assert stale_dir.exists()
    assert fresh_dir.exists()
    assert staged_file.exists()

    delete_run = cleanup_external_run_artifacts(
        base_temp_dir=base_temp_dir,
        prefix="rflux-ext-",
        older_than_days=1.0,
        delete=True,
    )

    assert delete_run["removed_count"] == 1
    assert stale_dir.exists() is False
    assert fresh_dir.exists()
    assert staged_file.exists()
    assert delete_run["skipped_non_dirs"] == 1
    assert delete_run["removed_paths"] == [stale_dir.as_posix()]
    assert delete_run["retained_paths"] == [fresh_dir.as_posix()]
