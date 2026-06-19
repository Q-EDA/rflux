//! Example: SFQ circuit analysis and characterization.
//!
//! This example demonstrates:
//! 1. Building an SFQ netlist with various cell types
//! 2. Physical feasibility estimation
//! 3. Testability analysis
//! 4. Running the full design flow
//! 5. Timing analysis results
//!
//! Run with: cargo run --example sfq_analysis

use rflux_flow::{FlowConfig, FlowRunner};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_synth::{analyze_testability, PhysicalFeasibilityEstimator};
use rflux_tech::Pdk;

fn main() {
    println!("=== rflux SFQ Circuit Analysis Example ===\n");

    // Step 1: Build a circuit with various SFQ cell types
    println!("1. Building SFQ netlist...");
    let mut netlist = build_analysis_netlist();
    println!("   Nodes: {}", netlist.node_count());
    println!("   Edges: {}", netlist.edge_count());

    // Step 2: Physical feasibility
    println!("\n2. Physical feasibility analysis...");
    let pdk = Pdk::minimal("analysis");
    let estimator = PhysicalFeasibilityEstimator::new();
    let feasibility = estimator.estimate(&netlist, &pdk);
    println!("   Area: {:.1} um²", feasibility.estimated_area_um2);
    println!("   Congestion: {:.2}", feasibility.congestion_ratio);
    println!("   Splitters: {}", feasibility.splitter_count);
    println!("   DFFs: {}", feasibility.dff_count);
    println!("   Pipeline depth: {}", feasibility.pipeline_depth);
    println!("   Feasible: {}", feasibility.feasible);

    // Step 3: Testability analysis
    println!("\n3. Testability analysis...");
    let testability = analyze_testability(&netlist);
    println!("   Controllable nodes: {}", testability.controllable_nodes);
    println!("   Observable nodes: {}", testability.observable_nodes);
    println!("   Fault coverage: {:.1}%", testability.fault_coverage_percent);
    if !testability.untestable_nodes.is_empty() {
        println!("   Untestable: {:?}", testability.untestable_nodes);
    }

    // Step 4: Run full design flow
    println!("\n4. Running full design flow...");
    let config = FlowConfig::default();
    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut netlist, &pdk, &config)
        .expect("design flow should succeed");

    println!("   Placed: {} nodes", report.placement.placed_nodes);
    println!("   Area: {:.1} x {:.1} um", report.placement.width_um, report.placement.height_um);
    println!("   Routes: {}", report.routing.routed_nets);
    println!("   JTL: {}, PTL: {}", report.routing.jtl_routes, report.routing.ptl_routes);
    println!("   Clock phases: {}", report.clock.phase_count);

    // Step 5: Timing results
    println!("\n5. Timing Analysis Results:");
    println!("   Arcs: {}", report.timing.analyzed_arcs);
    println!("   Critical path: {:.1} ps", report.timing.critical_path_delay_ps);
    println!("   Setup slack: {:.1} ps", report.timing.worst_setup_slack_ps);
    println!("   Hold slack: {:.1} ps", report.timing.worst_hold_slack_ps);
    println!("   Setup violations: {}", report.timing.setup_violations);
    println!("   Closure status: {}", report.timing_closure.status);

    // Closure actions
    if report.timing_closure.action_count > 0 {
        println!("\n   Closure actions: {}", report.timing_closure.action_count);
        if let Some(primary) = &report.timing_closure.primary_action {
            println!("   Primary action: {:?}", primary.remediation_kind);
        }
    }

    println!("\n=== Analysis complete ===");
}

fn build_analysis_netlist() -> Netlist {
    let mut netlist = Netlist::new();

    // Inputs
    let a = netlist.add_node(NodeKind::Port, "a");
    let b = netlist.add_node(NodeKind::Port, "b");

    // AND gate with splitter
    let and = netlist.add_node(NodeKind::CellInstance, "and0");
    let split = netlist.add_node(NodeKind::Splitter, "split0");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: and, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: b, port: 0 }, PinRef { node: and, port: 1 })
        .unwrap();
    netlist
        .connect(PinRef { node: and, port: 0 }, PinRef { node: split, port: 0 })
        .unwrap();

    // Path through XOR
    let xor = netlist.add_node(NodeKind::CellInstance, "xor0");
    let dff1 = netlist.add_node(NodeKind::Dff, "dff1");
    netlist
        .connect(PinRef { node: split, port: 0 }, PinRef { node: xor, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: b, port: 1 }, PinRef { node: xor, port: 1 })
        .unwrap();
    netlist
        .connect(PinRef { node: xor, port: 0 }, PinRef { node: dff1, port: 0 })
        .unwrap();

    // Path through OR
    let or = netlist.add_node(NodeKind::CellInstance, "or0");
    let dff2 = netlist.add_node(NodeKind::Dff, "dff2");
    netlist
        .connect(PinRef { node: split, port: 1 }, PinRef { node: or, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: a, port: 1 }, PinRef { node: or, port: 1 })
        .unwrap();
    netlist
        .connect(PinRef { node: or, port: 0 }, PinRef { node: dff2, port: 0 })
        .unwrap();

    // Output stage
    let out = netlist.add_node(NodeKind::Port, "out");
    netlist
        .connect(PinRef { node: dff1, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    netlist
}
