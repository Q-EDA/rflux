# Demonstrate an end-to-end CLI diagnostics bundle workflow from a bench input.
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

    with tempfile.TemporaryDirectory(prefix="rflux-diag-example-") as tmp_dir:
        work_dir = Path(tmp_dir)
        bundle_dir = work_dir / "diagnostics_bundle"
        manifest_path = bundle_dir / "manifest.json"
        report_path = bundle_dir / "reports" / "compile-netlist-report.json"
        events_path = bundle_dir / "events.jsonl"

        run_cli(
            repo_root,
            [
                "run-with-diagnostics",
                "--kind",
                "compile-netlist",
                "--input",
                str(bench_fixture),
                "--input-kind",
                "bench",
                "--output-dir",
                str(bundle_dir),
            ],
        )

        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        report = json.loads(report_path.read_text(encoding="utf-8"))
        event_count = len([line for line in events_path.read_text(encoding="utf-8").splitlines() if line.strip()])

        summary = {
            "bench_fixture": str(bench_fixture),
            "bundle": {
                "schema_version": manifest["schema_version"],
                "kind": manifest["kind"],
                "invocation_command": manifest["invocation"]["command"],
                "status": manifest["execution"]["status"],
                "captured_input_count": manifest["summary"]["captured_input_count"],
                "captured_report_count": manifest["summary"]["captured_report_count"],
                "events_recorded": event_count,
            },
            "compile_report": {
                "schema_version": report["schema_version"],
                "kind": report["kind"],
                "node_count": report["node_count"],
                "edge_count": report["edge_count"],
            },
        }
        print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()