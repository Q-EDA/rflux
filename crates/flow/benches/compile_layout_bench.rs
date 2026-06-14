//! Microbenchmarks for the `rflux-flow` end-to-end layout flow.
//!
//! `FlowRunner::compile_layout` drives the full synthesis → placement →
//! routing → clock-tree → timing-closure pipeline in one call, which makes it
//! the right place to catch whole-flow regressions. Two sizes are exercised so
//! that fixed overhead (config build, PDK lookup) is separable from per-node
//! cost (placement levels, route segments, timing arcs).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rflux_flow::{FlowConfig, FlowRunner};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_synth::{CompilePlan, ConnectionSpec};
use rflux_tech::Pdk;

/// Build a "balanced binary tree of cells" netlist with `leaf_count` leaf
/// ports fanning into a chain of 2-input gates. This keeps the flow structurally
/// simple while exercising fan-in/fan-out, splitter insertion, levelized
/// placement, and multi-arc timing.
fn build_tree_netlist(leaf_count: usize) -> (Netlist, CompilePlan) {
    let mut netlist = Netlist::new();
    let ports: Vec<_> = (0..leaf_count)
        .map(|i| netlist.add_node(NodeKind::Port, format!("p{i}")))
        .collect();

    // Walk the leaves pairwise, inserting a gate at each merge until one root.
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

fn bench_compile_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile_layout");

    for leaf_count in [4usize, 16, 64] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("leaves={leaf_count}")),
            &leaf_count,
            |b, &leaf_count| {
                b.iter_batched(
                    || {
                        let (netlist, plan) = build_tree_netlist(leaf_count);
                        let mut config = FlowConfig::default();
                        config.synthesis.plan = plan;
                        (netlist, config)
                    },
                    |(mut netlist, config)| {
                        let mut runner = FlowRunner::new();
                        let report = runner
                            .compile_layout(
                                black_box(&mut netlist),
                                black_box(&Pdk::minimal("bench")),
                                black_box(&config),
                            )
                            .expect("flow must succeed in benchmark fixture");
                        black_box(report);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_compile_layout);
criterion_main!(benches);
