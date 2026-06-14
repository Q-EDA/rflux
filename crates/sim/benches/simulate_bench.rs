//! Microbenchmarks for the `rflux-sim` deck-parsing + event-simulation hot path.
//!
//! These benchmarks exercise `simulate_text` end-to-end (lex → parse deck →
//! resolve params → run event/transient engine) on a small representative
//! fixture, so that regressions in any of those stages show up as a regression
//! here. They are deliberately not a JoSIM-accuracy benchmark — that is covered
//! by the waveform-compare parity gate in CI.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rflux_sim::{simulate_text, SimulationConfig, SimulationMode};

/// A small deck with param substitution, a subcircuit, and a `.tran` directive —
/// representative of the structure the event-only path must walk.
const DECK_SMALL: &str = "\
.title bench_small
.param tstep=0.5p tstop=20p
.subckt stage in out
R1 in out 50
.ends
X1 n1 n2 stage
.tran tstep tstop
.end
";

/// A medium deck: more components and a longer transient window, to amplify the
/// linear-RC internal-transient solver cost relative to parsing.
const DECK_MEDIUM: &str = "\
.title bench_medium
V1 in 0 PULSE(0,1m,0,1p,1p,2p,6p)
R1 in n1 1
R2 n1 n2 1
R3 n2 n3 1
R4 n3 n4 1
C1 n1 0 1p
C2 n2 0 1p
C3 n3 0 1p
C4 n4 0 1p
.measure tran vout_peak max V(n4)
.measure tran vout_rms rms V(n4)
.tran 0.5p 30p
.end
";

fn bench_simulate_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("simulate_text");

    for (name, deck, mode) in [
        ("small/event_only", DECK_SMALL, SimulationMode::EventOnly),
        (
            "medium/internal_transient",
            DECK_MEDIUM,
            SimulationMode::InternalTransient,
        ),
    ] {
        let config = SimulationConfig {
            mode,
            ..SimulationConfig::default()
        };
        group.bench_with_input(BenchmarkId::from_parameter(name), &deck, |b, deck| {
            b.iter(|| {
                let report = simulate_text(black_box(deck), black_box(&config))
                    .expect("simulation must succeed in benchmark fixture");
                black_box(report);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_simulate_text);
criterion_main!(benches);
