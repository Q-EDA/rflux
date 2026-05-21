# Phase 2 Progress

Date: 2026-05-19

## Implemented in this step

- Added `rflux-place` crate with a minimal levelized placer:
  - topological longest-path level assignment
  - boundary-aware port placement: input ports on the left, output ports on the right
  - deterministic slot assignment within each level
  - explicit fixed-node constraints snapped to placement grid
  - explicit blocked-region avoidance during placement legalization
  - deterministic legalization when fixed nodes request the same slot
  - placement canvas width/height reporting
- Added `rflux-route` crate with a minimal Manhattan router:
  - routes every IR edge from placement coordinates
  - chooses `PTL` for long internal links when the PDK allows the length
  - keeps boundary port access nets on `JTL`
  - detours around explicit keep-out regions using Manhattan doglegs
  - falls back to `JTL` when the PTL span is forbidden
  - reports total routed length, detour overhead, and JTL/PTL route counts
- Added `rflux-flow` crate with a minimal end-to-end compile-to-layout runner:
  - runs synthesis, placement, and routing in sequence over one netlist
  - forwards fixed-node placement constraints into the placement stage
  - accepts keep-out constraints through the Python-facing `compile_layout` path and forwards them into both placement and routing
  - performs one detour-driven re-place/re-route pass when the initial route incurs avoidable overhead
  - exposes detoured-route counts and total detour overhead in the layout summary
  - returns a unified layout report for physical bootstrap validation
- Added unit tests for placement layering, cycle rejection, PTL selection, and PTL-forbidden fallback.

## Current Phase 2 Scope

This is a bootstrap implementation, not full SFQ physical design yet.

Status: The current Phase 2 prototype scope is complete.

What now exists:

- an executable placement API over `rflux-ir`
- boundary-aware input/output port placement
- fixed-node placement constraints with grid-snapped legalization
- blocked-region aware placement legalization
- macro-aware halo constraints for `MacroCell`
- simple congestion-aware legalization via per-level spill
- an executable routing API over placement + `rflux-tech::Pdk`
- boundary-aware routing that keeps IO access nets on JTL
- keep-out aware routing with grid-based JTL path search
- an executable compile-to-layout API over synth + place + route
- detour-cost reporting suitable for later route-driven feedback
- a minimal one-pass route-driven feedback loop in `rflux-flow`
- a minimal multi-phase clock distribution summary in `rflux-flow`
- a minimal timing-driven hold-fix reroute pass in `rflux-flow`
- a minimal place/route closed loop suitable for later timing-driven iteration

## Beyond Phase 2 Prototype Scope

- stronger congestion models than simple level spill
- full clock-tree synthesis instead of the current summary/prototype
- richer timing analysis than the current hold-fix reroute heuristic
- production-quality obstacle-aware routing beyond the current grid JTL search
