# Demonstrate end-to-end Python transient simulation on a small RC deck using the internal backend.
from __future__ import annotations

import json

import rflux


DECK = """.title rc_demo
V1 in 0 PULSE(0,1m,0,1p,1p,2p,6p)
R1 in out 1
C1 out 0 1p
.tran 1p 6p
.end
"""


def main() -> None:
    report = rflux.simulate_text(DECK, simulation_mode="internal_transient")
    summary = {
        "backend": report.backend,
        "simulated_events": report.simulated_events,
        "reported_worst_delay_ps": report.reported_worst_delay_ps,
        "waveform_path": report.waveform_path,
        "delay_details": [
            {
                "name": detail.name,
                "delay_ps": detail.delay_ps,
                "from_ref": None if detail.from_ref is None else detail.from_ref.raw,
                "to_ref": None if detail.to_ref is None else detail.to_ref.raw,
            }
            for detail in report.delay_details
        ],
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()