//! Microbenchmarks for the `rflux-flow` end-to-end layout flow.
//!
//! `FlowRunner::compile_layout` drives the full synthesis → placement →
//! routing → clock-tree → timing-closure pipeline in one call, which makes it
//! the right place to catch whole-flow regressions. Two sizes are exercised so
//! that fixed overhead (config build, PDK lookup) is separable from per-node
//! cost (placement levels, route segments, timing arcs).
//!
//! Additional benchmarks isolate individual pipeline stages (synthesis,
//! placement, routing, timing) so that regressions in a single stage can be
//! pinpointed without re-running the entire flow.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rflux_flow::{FlowConfig, FlowRunner};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_place::{LevelizedPlacer, PlacementConfig};
use rflux_route::{RoutingConfig, SimpleRouter};
use rflux_synth::{CompilePlan, Compiler, ConnectionSpec, SynthesisConfig};
use rflux_tech::Pdk;
use rflux_timing::{StaticTimingAnalyzer, TimingConfig};

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

    for leaf_count in [4usize, 16, 64, 256] {
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

fn bench_analyze_timing(c: &mut Criterion) {
    let mut group = c.benchmark_group("analyze_timing");

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
                            .analyze_timing(
                                black_box(&mut netlist),
                                black_box(&Pdk::minimal("bench")),
                                black_box(&config),
                            )
                            .expect("timing analysis must succeed in benchmark fixture");
                        black_box(report);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_synthesis(c: &mut Criterion) {
    let mut group = c.benchmark_group("synthesis");
    let pdk = Pdk::minimal("bench");

    for leaf_count in [4usize, 16, 64, 256] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("leaves={leaf_count}")),
            &leaf_count,
            |b, &leaf_count| {
                b.iter_batched(
                    || {
                        let (netlist, plan) = build_tree_netlist(leaf_count);
                        let config = SynthesisConfig {
                            plan,
                            ..SynthesisConfig::default()
                        };
                        (netlist, config)
                    },
                    |(mut netlist, config)| {
                        let mut compiler = Compiler::new();
                        let report = compiler
                            .compile_netlist(
                                black_box(&mut netlist),
                                black_box(&pdk),
                                black_box(&config),
                            )
                            .expect("synthesis must succeed in benchmark fixture");
                        black_box(report);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_placement(c: &mut Criterion) {
    let mut group = c.benchmark_group("placement");
    let pdk = Pdk::minimal("bench");

    for leaf_count in [4usize, 16, 64, 256] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("leaves={leaf_count}")),
            &leaf_count,
            |b, &leaf_count| {
                b.iter_batched(
                    || {
                        let (mut netlist, plan) = build_tree_netlist(leaf_count);
                        let synth_config = SynthesisConfig {
                            plan,
                            ..SynthesisConfig::default()
                        };
                        let mut compiler = Compiler::new();
                        compiler
                            .compile_netlist(&mut netlist, &pdk, &synth_config)
                            .expect("synthesis setup must succeed");
                        netlist
                    },
                    |netlist| {
                        let placer = LevelizedPlacer::new();
                        let placement = placer
                            .place(
                                black_box(&netlist),
                                black_box(&PlacementConfig::default()),
                            )
                            .expect("placement must succeed in benchmark fixture");
                        black_box(placement);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_routing(c: &mut Criterion) {
    let mut group = c.benchmark_group("routing");
    let pdk = Pdk::minimal("bench");

    for leaf_count in [4usize, 16, 64, 256] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("leaves={leaf_count}")),
            &leaf_count,
            |b, &leaf_count| {
                b.iter_batched(
                    || {
                        let (mut netlist, plan) = build_tree_netlist(leaf_count);
                        let synth_config = SynthesisConfig {
                            plan,
                            ..SynthesisConfig::default()
                        };
                        let mut compiler = Compiler::new();
                        compiler
                            .compile_netlist(&mut netlist, &pdk, &synth_config)
                            .expect("synthesis setup must succeed");
                        let placer = LevelizedPlacer::new();
                        let placement = placer
                            .place(&netlist, &PlacementConfig::default())
                            .expect("placement setup must succeed");
                        (netlist, placement)
                    },
                    |(netlist, placement)| {
                        let router = SimpleRouter::new();
                        let report = router
                            .route(
                                black_box(&netlist),
                                black_box(&placement),
                                black_box(&pdk),
                                black_box(&RoutingConfig::default()),
                            )
                            .expect("routing must succeed in benchmark fixture");
                        black_box(report);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_timing_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("timing_analysis");
    let pdk = Pdk::minimal("bench");

    for leaf_count in [4usize, 16, 64, 256] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("leaves={leaf_count}")),
            &leaf_count,
            |b, &leaf_count| {
                b.iter_batched(
                    || {
                        let (mut netlist, plan) = build_tree_netlist(leaf_count);
                        let synth_config = SynthesisConfig {
                            plan,
                            ..SynthesisConfig::default()
                        };
                        let mut compiler = Compiler::new();
                        compiler
                            .compile_netlist(&mut netlist, &pdk, &synth_config)
                            .expect("synthesis setup must succeed");
                        let placer = LevelizedPlacer::new();
                        let placement = placer
                            .place(&netlist, &PlacementConfig::default())
                            .expect("placement setup must succeed");
                        let router = SimpleRouter::new();
                        let routing = router
                            .route(&netlist, &placement, &pdk, &RoutingConfig::default())
                            .expect("routing setup must succeed");
                        (netlist, routing)
                    },
                    |(netlist, routing)| {
                        let analyzer = StaticTimingAnalyzer::new();
                        let report = analyzer
                            .analyze(
                                black_box(&netlist),
                                black_box(&routing),
                                black_box(&pdk),
                                black_box(&TimingConfig::default()),
                                black_box(None),
                            )
                            .expect("timing must succeed in benchmark fixture");
                        black_box(report);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_compile_layout,
    bench_analyze_timing,
    bench_synthesis,
    bench_placement,
    bench_routing,
    bench_timing_analysis,
);
criterion_main!(benches);
