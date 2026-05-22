# Demonstrate a Python-driven CLI workflow that finds a satisfiable equivalence counterexample.
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


def build_mutated_rhs(lhs_fixture: Path, rhs_path: Path) -> None:
    payload = json.loads(lhs_fixture.read_text(encoding="utf-8"))
    for node in payload["nodes"]:
        if node.get("name") == "or1":
            node["logic_op"] = "Xor"
            break
    else:
        raise RuntimeError("expected to find node 'or1' in classic_majority3 fixture")
    rhs_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def main() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    lhs_fixture = repo_root / "crates" / "synth" / "tests" / "fixtures" / "classic_examples" / "classic_majority3.json"
    check_ref = "output:maj"

    with tempfile.TemporaryDirectory(prefix="rflux-equivalence-counterexample-") as tmp_dir:
        work_dir = Path(tmp_dir)
        rhs_fixture = work_dir / "majority3_mutated.json"
        dimacs_path = work_dir / "majority3_counterexample.cnf"
        report_path = work_dir / "equivalence_report.json"
        solve_path = work_dir / "solve_report.json"
        sidecar_path = work_dir / "majority3_counterexample.cnf.checks.json"

        build_mutated_rhs(lhs_fixture, rhs_fixture)
        run_cli(
            repo_root,
            [
                "check-equivalence",
                "--lhs",
                str(lhs_fixture),
                "--rhs",
                str(rhs_fixture),
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
            "lhs_fixture": str(lhs_fixture),
            "rhs_fixture": str(rhs_fixture),
            "mutation": {"node": "or1", "logic_op_before": "Or", "logic_op_after": "Xor"},
            "check_ref": check_ref,
            "equivalent": export_report["equivalent"],
            "counterexample_inputs": export_report["counterexample_inputs"],
            "counterexample_outputs": export_report["counterexample_outputs"],
            "dimacs_export": {
                "path": export_report["dimacs_export"]["path"],
                "sidecar_path": export_report["dimacs_export"]["sidecar_path"],
                "variables": export_report["dimacs_export"]["variables"],
                "clauses": export_report["dimacs_export"]["clauses"],
            },
            "sidecar": {
                "schema_version": sidecar["schema_version"],
                "check_count": len(sidecar["checks"]),
            },
            "solve": {
                "satisfiable": solve_report["satisfiable"],
                "model": solve_report["model"],
                "unsat_core": solve_report["unsat_core"],
                "equivalence_check": solve_report["equivalence_check"],
            },
        }
        print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()