from __future__ import annotations

from .pdk import Pdk
from ._types import (
    AdvancedConstraintReport,
    AdvancedConstraintViolation,
    ClockDomainConstraint,
    CompilePlan,
    CrossingConstraint,
    FixedNodePlacement,
    MultiCornerTimingAnalysisReport,
    NodeTimingConstraint,
    PinRef,
    PinTimingConstraint,
    StatisticalTimingAnalysisReport,
    StatisticalTimingArcReport,
    TimingAnalysisReport,
    TimingArcReport,
    TimingClosureAction,
    TimingClosureLoopReport,
    TimingClosureSummary,
    TimingCornerAnalysisReport,
)


def analyze_advanced_constraints(circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, max_estimated_thermal_load_uw: float = 8.0, max_estimated_mechanical_stress_score: float = 0.75, max_jtl_density_per_100um: float = 8.0, max_detour_overhead_ratio: float = 0.35, max_ptl_coupling_ratio: float = 0.65) -> AdvancedConstraintReport: ...
def analyze_timing(circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, characterized_library_json: str | None = None, characterized_library_entries: list[str] | None = None) -> TimingAnalysisReport: ...
def analyze_timing_corners(circuit, pdk: Pdk, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None) -> MultiCornerTimingAnalysisReport: ...
def analyze_timing_statistical(circuit, plan: CompilePlan | None = None, fixed_nodes: list[FixedNodePlacement] | None = None, blocked_regions: list | None = None, timing_constraints: list[NodeTimingConstraint] | None = None, pin_timing_constraints: list[PinTimingConstraint] | None = None, clock_domains: list[ClockDomainConstraint] | None = None, crossing_constraints: list[CrossingConstraint] | None = None, characterized_library_json: str | None = None, characterized_library_entries: list[str] | None = None, cell_delay_sigma_ratio: float = 0.05, wire_delay_sigma_ratio: float = 0.05, global_cell_delay_sigma_ratio: float = 0.0, global_wire_delay_sigma_ratio: float = 0.0, clock_uncertainty_sigma_ps: float = 0.0, cross_domain_uncertainty_sigma_ps: float = 0.0, max_delay_cross_domain_uncertainty_sigma_ps: float = 0.0, multicycle_cross_domain_uncertainty_sigma_ps: float = 0.0, sigma_multiplier: float = 3.0) -> StatisticalTimingAnalysisReport: ...

__all__: list[str]