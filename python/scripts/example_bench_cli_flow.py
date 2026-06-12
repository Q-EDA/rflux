# Demonstrate an end-to-end bench frontend flow through rflux-cli commands.
from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path


def run_cli(repo_root: Path, args: list[str]) -> None:
    completed = subprocess.run(
        ["uv", "run", "cargo", "run", "-p", "rflux-cli", "--", *args],
        cwd=str(repo_root),
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr + "\n" + completed.stdout)


def main() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    bench_fixture = (
        repo_root
        / "crates"
        / "synth"
        / "tests"
        / "fixtures"
        / "quaigh_alignment"
        / "bench"
        / "dedup_and_pair.bench"
    )

    with tempfile.TemporaryDirectory(prefix="rflux-bench-cli-") as tmp_dir:
        work_dir = Path(tmp_dir)
        compile_report_path = work_dir / "compile_report.json"
        equivalence_report_path = work_dir / "equivalence_report.json"

        run_cli(
            repo_root,
            [
                "compile-netlist",
                "--input",
                str(bench_fixture),
                "--output",
                str(compile_report_path),
            ],
        )
        run_cli(
            repo_root,
            [
                "check-equivalence",
                "--lhs",
                str(bench_fixture),
                "--rhs",
                str(bench_fixture),
                "--output",
                str(equivalence_report_path),
            ],
        )

        compile_report = json.loads(compile_report_path.read_text(encoding="utf-8"))
        equivalence_report = json.loads(equivalence_report_path.read_text(encoding="utf-8"))

        summary = {
            "bench_fixture": str(bench_fixture),
            "compile": {
                "schema_version": compile_report["schema_version"],
                "node_count": compile_report["node_count"],
                "edge_count": compile_report["edge_count"],
                "gate_count_before": compile_report["bool_opt"]["gate_count_before"],
                "gate_count_after": compile_report["bool_opt"]["gate_count_after"],
                "mapped_nodes": compile_report["tech_map"]["mapped_nodes"],
            },
            "equivalence": {
                "schema_version": equivalence_report["schema_version"],
                "kind": equivalence_report["kind"],
                "equivalent": equivalence_report["equivalent"],
                "checked_outputs": equivalence_report["checked_outputs"],
                "sat_recursive_calls": equivalence_report["sat_stats"]["recursive_calls"],
            },
        }
        print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()