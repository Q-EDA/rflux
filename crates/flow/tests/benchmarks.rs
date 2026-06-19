use std::path::PathBuf;

use rflux_flow::{FlowConfig, FlowRunner};
use rflux_io::read_bench_netlist;
use rflux_tech::Pdk;

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(file_name)
}

fn run_bench_fixture(file_name: &str) {
    let path = fixture_path(file_name);
    let mut netlist = read_bench_netlist(&path)
        .unwrap_or_else(|e| panic!("failed to load bench fixture {file_name}: {e}"));

    let pdk = Pdk::minimal("benchmark");
    let config = FlowConfig::default();

    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut netlist, &pdk, &config)
        .unwrap_or_else(|e| panic!("compile_layout failed for {file_name}: {e}"));

    assert!(
        report.placement.placed_nodes > 0,
        "{file_name}: expected at least one placed node"
    );
    assert!(
        report.placement.width_um > 0.0,
        "{file_name}: placement width must be positive"
    );
    assert!(
        report.placement.height_um > 0.0,
        "{file_name}: placement height must be positive"
    );
    assert!(
        report.routing.routed_nets > 0,
        "{file_name}: expected at least one routed net"
    );
    assert!(
        report.timing.analyzed_arcs > 0,
        "{file_name}: expected at least one analyzed timing arc"
    );
    assert!(
        report.timing_closure.status == "closed" || report.timing_closure.status == "open",
        "{file_name}: closure status must be 'closed' or 'open', got '{}'",
        report.timing_closure.status,
    );
}

#[test]
fn benchmark_iscas_c17() {
    run_bench_fixture("iscas_c17.bench");
}

#[test]
fn benchmark_iscas_c432() {
    run_bench_fixture("iscas_c432.bench");
}

#[test]
fn benchmark_simple_pipeline() {
    run_bench_fixture("simple_pipeline.bench");
}

#[test]
fn benchmark_nand_chain() {
    run_bench_fixture("nand_chain.bench");
}

#[test]
fn benchmark_majority_chain() {
    run_bench_fixture("majority_chain.bench");
}

#[test]
fn benchmark_determinism() {
    let path = fixture_path("iscas_c17.bench");
    let pdk = Pdk::minimal("benchmark");
    let config = FlowConfig::default();

    let mut netlist_a = read_bench_netlist(&path).expect("load fixture a");
    let mut netlist_b = read_bench_netlist(&path).expect("load fixture b");

    let mut runner = FlowRunner::new();
    let report_a = runner
        .compile_layout(&mut netlist_a, &pdk, &config)
        .expect("first run");
    let report_b = runner
        .compile_layout(&mut netlist_b, &pdk, &config)
        .expect("second run");

    assert_eq!(
        report_a.placement.placed_nodes, report_b.placement.placed_nodes,
        "placed_nodes must be deterministic"
    );
    assert_eq!(
        report_a.placement.width_um, report_b.placement.width_um,
        "placement width must be deterministic"
    );
    assert_eq!(
        report_a.placement.height_um, report_b.placement.height_um,
        "placement height must be deterministic"
    );
    assert_eq!(
        report_a.routing.routed_nets, report_b.routing.routed_nets,
        "routed_nets must be deterministic"
    );
    assert_eq!(
        report_a.routing.total_length_um, report_b.routing.total_length_um,
        "total routing length must be deterministic"
    );
    assert_eq!(
        report_a.timing.analyzed_arcs, report_b.timing.analyzed_arcs,
        "analyzed_arcs must be deterministic"
    );
    assert_eq!(
        report_a.timing.worst_setup_slack_ps, report_b.timing.worst_setup_slack_ps,
        "worst setup slack must be deterministic"
    );
}
