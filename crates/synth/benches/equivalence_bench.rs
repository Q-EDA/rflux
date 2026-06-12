use criterion::{black_box, criterion_group, criterion_main, Criterion};

use rflux_ir::{LogicOp, Netlist, NodeKind, PinRef};
use rflux_synth::Compiler;

fn bench_boolean_equivalence_small(c: &mut Criterion) {
    // Build two identical AND circuits
    fn build_and_netlist() -> Netlist {
        let mut n = Netlist::new();
        let a = n.add_node(NodeKind::Port, "a");
        let b = n.add_node(NodeKind::Port, "b");
        let gate = n.add_node_with_logic(NodeKind::CellInstance, "gate", Some(LogicOp::And));
        let o = n.add_node(NodeKind::Port, "o");
        n.connect(
            PinRef { node: a, port: 0 },
            PinRef {
                node: gate,
                port: 0,
            },
        )
        .ok();
        n.connect(
            PinRef { node: b, port: 0 },
            PinRef {
                node: gate,
                port: 1,
            },
        )
        .ok();
        n.connect(
            PinRef {
                node: gate,
                port: 0,
            },
            PinRef { node: o, port: 0 },
        )
        .ok();
        n
    }

    let lhs = build_and_netlist();
    let rhs = build_and_netlist();
    let compiler = Compiler::new();

    c.bench_function("synth/boolean_equivalence/and", |b| {
        b.iter(|| {
            let result = compiler.check_boolean_equivalence_sat(black_box(&lhs), black_box(&rhs));
            black_box(result)
        });
    });
}

fn bench_boolean_equivalence_large(c: &mut Criterion) {
    // Build bigger circuits with XOR/MUX chain
    fn build_chain(len: usize) -> Netlist {
        let mut n = Netlist::new();
        let a = n.add_node(NodeKind::Port, "a");
        let b = n.add_node(NodeKind::Port, "b");
        let mut prev = n.add_node_with_logic(NodeKind::CellInstance, "xor0", Some(LogicOp::Xor));
        n.connect(
            PinRef { node: a, port: 0 },
            PinRef {
                node: prev,
                port: 0,
            },
        )
        .ok();
        n.connect(
            PinRef { node: b, port: 0 },
            PinRef {
                node: prev,
                port: 1,
            },
        )
        .ok();

        for i in 1..len {
            let gate = n.add_node_with_logic(
                NodeKind::CellInstance,
                format!("xor{i}"),
                Some(LogicOp::Xor),
            );
            n.connect(
                PinRef {
                    node: prev,
                    port: 0,
                },
                PinRef {
                    node: gate,
                    port: 0,
                },
            )
            .ok();
            n.connect(
                PinRef { node: b, port: 0 },
                PinRef {
                    node: gate,
                    port: 1,
                },
            )
            .ok();
            prev = gate;
        }

        let o = n.add_node(NodeKind::Port, "o");
        n.connect(
            PinRef {
                node: prev,
                port: 0,
            },
            PinRef { node: o, port: 0 },
        )
        .ok();
        n
    }

    let mut group = c.benchmark_group("synth/boolean_equivalence/chain");
    for len in [4, 8, 16] {
        let lhs = build_chain(len);
        let rhs = build_chain(len);
        let compiler = Compiler::new();
        group.bench_with_input(
            criterion::BenchmarkId::new("equivalent", len),
            &(lhs, rhs, compiler),
            |b, (l, r, comp)| {
                b.iter(|| {
                    let result = comp.check_boolean_equivalence_sat(black_box(l), black_box(r));
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_boolean_equivalence_small,
    bench_boolean_equivalence_large
);
criterion_main!(benches);
