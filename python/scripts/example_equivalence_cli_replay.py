# Demonstrate a Python-driven CLI workflow for equivalence DIMACS export and replay.
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
    fixture = repo_root / "crates" / "synth" / "tests" / "fixtures" / "classic_examples" / "classic_majority3.json"
    check_ref = "output:maj"

    with tempfile.TemporaryDirectory(prefix="rflux-equivalence-cli-") as tmp_dir:
        work_dir = Path(tmp_dir)
        dimacs_path = work_dir / "majority3.cnf"
        report_path = work_dir / "equivalence_report.json"
        solve_path = work_dir / "solve_report.json"
        sidecar_path = work_dir / "majority3.cnf.checks.json"

        run_cli(
            repo_root,
            [
                "check-equivalence",
                "--lhs",
                str(fixture),
                "--rhs",
                str(fixture),
                "--dimacs-output",
                str(dimacs_path),
                "--output",
                str(report_path),
            ],
        )
        run_cli(
            repo_root,
            [
                "solve-dimacs",
                "--input",
                str(dimacs_path),
                "--equivalence-metadata",
                str(sidecar_path),
                "--check-ref",
                check_ref,
                "--output",
                str(solve_path),
            ],
        )

        export_report = json.loads(report_path.read_text(encoding="utf-8"))
        solve_report = json.loads(solve_path.read_text(encoding="utf-8"))
        sidecar = json.loads(sidecar_path.read_text(encoding="utf-8"))

        summary = {
            "fixture": str(fixture),
            "check_ref": check_ref,
            "equivalent": export_report["equivalent"],
            "checked_outputs": export_report["checked_outputs"],
            "dimacs_export": {
                "path": export_report["dimacs_export"]["path"],
                "sidecar_path": export_report["dimacs_export"]["sidecar_path"],
                "variables": export_report["dimacs_export"]["variables"],
                "clauses": export_report["dimacs_export"]["clauses"],
                "schema_version": export_report["dimacs_export"]["schema_version"],
            },
            "sidecar": {
                "schema_version": sidecar["schema_version"],
                "check_count": len(sidecar["checks"]),
                "first_check_ref": sidecar["checks"][0]["check_ref"],
            },
            "solve": {
                "satisfiable": solve_report["satisfiable"],
                "unsat_core": solve_report["unsat_core"],
                "equivalence_check": solve_report["equivalence_check"],
            },
        }
        print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()