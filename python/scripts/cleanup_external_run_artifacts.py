from __future__ import annotations

import argparse
import json
import shutil
import tempfile
import time
from pathlib import Path


def _is_retained_run_dir(path: Path, prefix: str) -> bool:
    return path.is_dir() and path.name.startswith(prefix)


def cleanup_external_run_artifacts(
    *,
    base_temp_dir: Path,
    prefix: str,
    older_than_days: float,
    delete: bool,
) -> dict[str, object]:
    cutoff_seconds = max(0.0, older_than_days) * 24.0 * 60.0 * 60.0
    now = time.time()
    removed: list[str] = []
    retained: list[str] = []
    skipped_non_dirs = 0

    if not base_temp_dir.exists():
        return {
            "base_temp_dir": base_temp_dir.as_posix(),
            "prefix": prefix,
            "older_than_days": older_than_days,
            "delete": delete,
            "removed_count": 0,
            "retained_count": 0,
            "skipped_non_dirs": 0,
            "removed_paths": [],
            "retained_paths": [],
        }

    for path in sorted(base_temp_dir.iterdir()):
        if not path.name.startswith(prefix):
            continue
        if not path.is_dir():
            skipped_non_dirs += 1
            continue

        age_seconds = now - path.stat().st_mtime
        if age_seconds < cutoff_seconds:
            retained.append(path.as_posix())
            continue

        if delete:
            shutil.rmtree(path)
            removed.append(path.as_posix())
        else:
            retained.append(path.as_posix())

    return {
        "base_temp_dir": base_temp_dir.as_posix(),
        "prefix": prefix,
        "older_than_days": older_than_days,
        "delete": delete,
        "removed_count": len(removed),
        "retained_count": len(retained),
        "skipped_non_dirs": skipped_non_dirs,
        "removed_paths": removed,
        "retained_paths": retained,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Dry-run or delete retained external JoSIM run directories older than a cutoff.",
    )
    parser.add_argument(
        "--base-temp-dir",
        type=Path,
        default=Path(""),
        help="Base temp directory to scan. Defaults to the current platform temp directory.",
    )
    parser.add_argument(
        "--prefix",
        type=str,
        default="rflux-ext-",
        help="Directory name prefix used by retained external run directories.",
    )
    parser.add_argument(
        "--older-than-days",
        type=float,
        default=7.0,
        help="Only delete directories older than this many days.",
    )
    parser.add_argument(
        "--delete",
        action="store_true",
        help="Actually delete matched directories. Without this flag the command only reports what would be removed.",
    )
    args = parser.parse_args()

    base_temp_dir = args.base_temp_dir if args.base_temp_dir.as_posix() else Path(tempfile.gettempdir())

    result = cleanup_external_run_artifacts(
        base_temp_dir=base_temp_dir,
        prefix=args.prefix,
        older_than_days=args.older_than_days,
        delete=args.delete,
    )
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
