//! End-to-end integration test for `rflux-flow`'s `compile_layout` pipeline.
//!
//! Unlike the inline `#[cfg(test)] mod tests` in `flow/src/lib.rs` (which
//! exercise individual invariants against the crate-internal API surface), this
//! file drives the flow purely through `FlowRunner`'s public method, the way an
//! external caller (the CLI, the Python binding) would. It exists to catch
//! regressions in the public contract — e.g. a field on `LayoutReport` being
//! silently renamed or a step being skipped — that a focused unit test on the
//! relevant submodule would not notice.

use rflux_flow::{FlowConfig, FlowRunner};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_synth::{CompilePlan, ConnectionSpec};
use rflux_tech::Pdk;

fn build_balanced_tree(leaf_count: usize) -> (Netlist, CompilePlan) {
    let mut netlist = Netlist::new();
    let ports: Vec<_> = (0..leaf_count)
        .map(|i| netlist.add_node(NodeKind::Port, format!("p{i}")))
        .collect();

    let mut layer = ports;
    let mut gate_index = 0usize;
    let mut connections = Vec::new();
    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len() / 2 + 1);
        for pair in layer.chunks(2) {
            if pair.len() == 2 {
                let gate = netlist.add_node(
                    NodeKind::CellInstance,
                    format!("g{}", gate_index),
                );
                gate_index += 1;
                connections.push(ConnectionSpec {
                    from: PinRef { node: pair[0], port: 0 },
                    to: PinRef { node: gate, port: 0 },
                });
                connections.push(ConnectionSpec {
                    from: PinRef { node: pair[1], port: 0 },
                    to: PinRef { node: gate, port: 1 },
                });
                next.push(gate);
            } else {
                next.push(pair[0]);
            }
        }
        layer = next;
    }

    let plan = CompilePlan {
        connections,
        ..CompilePlan::default()
    };
    (netlist, plan)
}

#[test]
fn compile_layout_drives_full_pipeline_on_minimal_tree() {
    // 4-leaf balanced tree: 4 ports + 3 gates = 7 nodes pre-synthesis.
    let (mut netlist, plan) = build_balanced_tree(4);
    let mut config = FlowConfig::default();
    config.synthesis.plan = plan;

    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut netlist, &Pdk::minimal("e2e"), &config)
        .expect("compile_layout must succeed on a minimal tree");

    // Synthesis stage applied all explicit connections.
    assert_eq!(report.synthesis.compile.connections_applied, 6);

    // Placement put every node on a grid coordinate.
    assert!(report.placement.placed_nodes > 0);
    assert!(report.placement.width_um > 0.0);
    assert!(report.placement.height_um > 0.0);

    // Routing produced at least one net with a non-zero total length.
    assert!(report.routing.routed_nets > 0);
    assert!(report.routing.total_length_um > 0.0);

    // Clock tree synthesized at least one phase.
    assert!(report.clock.phase_count >= 1);

    // Timing ran on at least one arc and produced a closure verdict.
    assert!(report.timing.analyzed_arcs > 0);
    assert!(
        report.timing_closure.status == "closed"
            || report.timing_closure.status == "open",
        "closure status must be one of the documented enum values, got: {}",
        report.timing_closure.status,
    );

    // Detour overhead accounting stays internally consistent: the final value
    // must not exceed the initial one (the loop only ever reduces it).
    assert!(report.initial_total_detour_overhead_um >= report.routing.total_detour_overhead_um);
    assert!(report.routing.total_detour_overhead_um >= 0.0);
}

#[test]
fn compile_layout_is_deterministic_across_invocations() {
    // The flow must not depend on hidden mutable state across runs: invoking it
    // twice on identical inputs must produce identical reports on the fields a
    // user would snapshot. This catches accidental introduction of iteration
    // order dependence, hash-randomization leaks, or stale caches in FlowRunner.
    let (mut netlist_a, plan) = build_balanced_tree(8);
    let (mut netlist_b, _) = build_balanced_tree(8);
    let mut config = FlowConfig::default();
    config.synthesis.plan = plan;

    let mut runner = FlowRunner::new();
    let report_a = runner.compile_layout(&mut netlist_a, &Pdk::minimal("e2e"), &config).unwrap();
    let report_b = runner.compile_layout(&mut netlist_b, &Pdk::minimal("e2e"), &config).unwrap();

    assert_eq!(report_a.synthesis.compile.connections_applied, report_b.synthesis.compile.connections_applied);
    assert_eq!(report_a.placement.placed_nodes, report_b.placement.placed_nodes);
    assert_eq!(report_a.placement.width_um, report_b.placement.width_um);
    assert_eq!(report_a.placement.height_um, report_b.placement.height_um);
    assert_eq!(report_a.routing.routed_nets, report_b.routing.routed_nets);
    assert_eq!(report_a.routing.total_length_um, report_b.routing.total_length_um);
    assert_eq!(report_a.clock.phase_count, report_b.clock.phase_count);
    assert_eq!(report_a.timing.analyzed_arcs, report_b.timing.analyzed_arcs);
    assert_eq!(report_a.timing.worst_setup_slack_ps, report_b.timing.worst_setup_slack_ps);
}
