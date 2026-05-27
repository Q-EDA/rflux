from __future__ import annotations

import argparse
import json
import re
import shutil
from pathlib import Path


def normalize_platform_key(raw: str) -> str:
    normalized = raw.strip().lower()
    if not normalized:
        raise ValueError("platform key must not be empty")
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]*", normalized):
        raise ValueError(f"invalid platform key: {raw}")
    return normalized


def _resolve_path(repo_root: Path, raw: str) -> Path:
    candidate = Path(raw)
    if candidate.is_absolute():
        return candidate
    return (repo_root / candidate).resolve()


def _load_candidate_summary(path: Path) -> dict[str, object]:
    if not path.is_file():
        raise FileNotFoundError(f"candidate summary JSON not found: {path}")
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("candidate summary JSON must be an object payload")
    return payload


def promote_waveform_approved_baseline(
    *,
    repo_root: Path,
    platform_key: str,
    benchmark_dir: Path,
    candidate_json: Path,
    candidate_md: Path | None,
) -> dict[str, Path]:
    normalized_platform = normalize_platform_key(platform_key)
    resolved_benchmark_dir = benchmark_dir if benchmark_dir.is_absolute() else (repo_root / benchmark_dir)
    resolved_benchmark_dir.mkdir(parents=True, exist_ok=True)

    resolved_candidate_json = candidate_json if candidate_json.is_absolute() else (repo_root / candidate_json)
    payload = _load_candidate_summary(resolved_candidate_json)
    failures = int(payload.get("failures", 0))
    if failures > 0:
        raise ValueError(
            "candidate baseline summary contains failures; only zero-failure summaries can be promoted"
        )

    target_json = (resolved_benchmark_dir / f"waveform_compare_summary.{normalized_platform}-approved-baseline.json").resolve()
    shutil.copy2(resolved_candidate_json, target_json)

    promoted = {"json": target_json}
    if candidate_md is not None:
        resolved_candidate_md = candidate_md if candidate_md.is_absolute() else (repo_root / candidate_md)
        if not resolved_candidate_md.is_file():
            raise FileNotFoundError(f"candidate summary markdown not found: {resolved_candidate_md}")
        target_md = (resolved_benchmark_dir / f"waveform_compare_summary.{normalized_platform}-approved-baseline.md").resolve()
        shutil.copy2(resolved_candidate_md, target_md)
        promoted["md"] = target_md

    return promoted


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Promote a reviewed waveform candidate summary to a repo-tracked platform approved baseline.",
    )
    parser.add_argument(
        "--platform",
        type=str,
        required=True,
        help="Platform key for approved baseline filename, e.g. linux or windows.",
    )
    parser.add_argument(
        "--benchmark-dir",
        type=str,
        default="python/tests/benchmarks/phase6",
        help="Repo-relative or absolute benchmark directory for approved baseline files.",
    )
    parser.add_argument(
        "--candidate-json",
        type=str,
        default="target/waveform-compare/waveform_compare_summary.candidate-baseline.json",
        help="Repo-relative or absolute candidate baseline summary JSON path.",
    )
    parser.add_argument(
        "--candidate-md",
        type=str,
        default="target/waveform-compare/waveform_compare_summary.candidate-baseline.md",
        help="Repo-relative or absolute candidate baseline summary markdown path.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    promoted = promote_waveform_approved_baseline(
        repo_root=repo_root,
        platform_key=args.platform,
        benchmark_dir=_resolve_path(repo_root, args.benchmark_dir),
        candidate_json=_resolve_path(repo_root, args.candidate_json),
        candidate_md=_resolve_path(repo_root, args.candidate_md),
    )
    print(f"promoted_json={promoted['json']}")
    if "md" in promoted:
        print(f"promoted_md={promoted['md']}")


if __name__ == "__main__":
    main()
