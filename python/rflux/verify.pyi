from __future__ import annotations

from ._types import (
    BoundedSequentialEquivalenceReport,
    ClockDomainConstraint,
    CombinationalEquivalenceReport,
    CompilePlan,
    CrossingConstraint,
    FixedNodePlacement,
    NodeTimingConstraint,
    PinTimingConstraint,
    SingleStepSequentialEquivalenceReport,
    VerificationReport,
)


def check_equivalence(lhs, rhs) -> CombinationalEquivalenceReport: ...
def check_single_step_sequential_equivalence(lhs, rhs) -> SingleStepSequentialEquivalenceReport: ...
def check_bounded_sequential_equivalence(lhs, rhs, depth: int = 2) -> BoundedSequentialEquivalenceReport: ...
def verify_layout(circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, simulation_mode: str = "auto", external_command: str | None = None) -> VerificationReport: ...

__all__: list[str]