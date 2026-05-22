# Demonstrate end-to-end Python simulation from a benchmark deck file in the repository.
from __future__ import annotations

import json
from pathlib import Path

import rflux


def main() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    deck_path = repo_root / "python" / "tests" / "benchmarks" / "phase6" / "t_delay_smoke.cir"
    report = rflux.simulate_file(str(deck_path), simulation_mode="internal_transient")

    summary = {
        "deck_path": str(deck_path),
        "backend": report.backend,
        "simulated_events": report.simulated_events,
        "reported_worst_delay_ps": report.reported_worst_delay_ps,
        "external_result": report.external_result,
        "waveform_path": report.waveform_path,
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()