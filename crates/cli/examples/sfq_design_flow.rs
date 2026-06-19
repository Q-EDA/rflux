//! Example: Complete SFQ design flow from netlist to timing closure.
//!
//! This example demonstrates the full rflux EDA pipeline:
//! 1. Build a netlist (4-stage pipeline with fanout)
//! 2. Run synthesis (boolean optimization, splitter/DFF insertion)
//! 3. Place the design (levelized placement)
//! 4. Route with JTL/PTL hybrid optimization
//! 5. Analyze timing (STA with setup/hold checks)
//!
//! Run with: cargo run --example sfq_design_flow

use rflux_flow::{FlowConfig, FlowRunner};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_synth::PhysicalFeasibilityEstimator;
use rflux_tech::Pdk;

fn main() {
    println!("=== rflux SFQ Design Flow Example ===\n");

    // Step 1: Build a 4-stage pipeline with fanout
    println!("1. Building netlist...");
    let mut netlist = build_example_netlist();
    println!("   Nodes: {}", netlist.node_count());
    println!("   Edges: {}", netlist.edge_count());

    // Step 2: Pre-synthesis feasibility check
    println!("\n2. Checking physical feasibility...");
    let pdk = Pdk::minimal("example");
    let estimator = PhysicalFeasibilityEstimator::new();
    let feasibility = estimator.estimate(&netlist, &pdk);
    println!("   Estimated area: {:.1} um²", feasibility.estimated_area_um2);
    println!("   Congestion ratio: {:.2}", feasibility.congestion_ratio);
    println!("   Pipeline depth: {}", feasibility.pipeline_depth);
    println!("   Feasible: {}", feasibility.feasible);
    if !feasibility.warnings.is_empty() {
        for w in &feasibility.warnings {
            println!("   Warning: {}", w);
        }
    }

    // Step 3: Run full design flow
    println!("\n3. Running full design flow...");
    let config = FlowConfig::default();
    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut netlist, &pdk, &config)
        .expect("design flow should succeed");

    // Step 4: Report results
    println!("\n=== Results ===\n");

    println!("Synthesis:");
    println!("  Connections applied: {}", report.synthesis.compile.connections_applied);
    println!("  Splitters inserted: {}", report.synthesis.compile.splitters_inserted);
    println!("  Balancing DFFs inserted: {}", report.synthesis.compile.balancing_dffs_inserted);

    println!("\nPlacement:");
    println!("  Placed nodes: {}", report.placement.placed_nodes);
    println!("  Dimensions: {:.1} x {:.1} um", report.placement.width_um, report.placement.height_um);

    println!("\nRouting:");
    println!("  Routed nets: {}", report.routing.routed_nets);
    println!("  Total wire length: {:.1} um", report.routing.total_length_um);
    println!("  JTL routes: {}", report.routing.jtl_routes);
    println!("  PTL routes: {}", report.routing.ptl_routes);
    println!("  Detoured routes: {}", report.routing.detoured_routes);

    println!("\nClock Tree:");
    println!("  Clock sinks: {}", report.clock.clock_sinks);
    println!("  Clock buffers: {}", report.clock.clock_buffers);
    println!("  Phases: {}", report.clock.phase_count);

    println!("\nTiming:");
    println!("  Analyzed arcs: {}", report.timing.analyzed_arcs);
    println!("  Critical path delay: {:.1} ps", report.timing.critical_path_delay_ps);
    println!("  Worst setup slack: {:.1} ps", report.timing.worst_setup_slack_ps);
    println!("  Worst hold slack: {:.1} ps", report.timing.worst_hold_slack_ps);
    println!("  Setup violations: {}", report.timing.setup_violations);
    println!("  Closure status: {}", report.timing_closure.status);

    println!("\n=== Design flow complete ===");
}

fn build_example_netlist() -> Netlist {
    let mut netlist = Netlist::new();

    // Input port
    let inp = netlist.add_node(NodeKind::Port, "data_in");

    // Stage 1: buffer + DFF
    let g1 = netlist.add_node(NodeKind::CellInstance, "buf1");
    let dff1 = netlist.add_node(NodeKind::Dff, "reg1");
    netlist
        .connect(PinRef { node: inp, port: 0 }, PinRef { node: g1, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: g1, port: 0 }, PinRef { node: dff1, port: 0 })
        .unwrap();

    // Stage 2: AND gate with fanout + DFF
    let g2 = netlist.add_node(NodeKind::CellInstance, "and1");
    let split1 = netlist.add_node(NodeKind::Splitter, "split1");
    let dff2 = netlist.add_node(NodeKind::Dff, "reg2");
    netlist
        .connect(PinRef { node: dff1, port: 0 }, PinRef { node: g2, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: inp, port: 1 }, PinRef { node: g2, port: 1 })
        .unwrap();
    netlist
        .connect(PinRef { node: g2, port: 0 }, PinRef { node: split1, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: split1, port: 0 }, PinRef { node: dff2, port: 0 })
        .unwrap();

    // Stage 3: XOR gate + DFF
    let g3 = netlist.add_node(NodeKind::CellInstance, "xor1");
    let dff3 = netlist.add_node(NodeKind::Dff, "reg3");
    netlist
        .connect(PinRef { node: split1, port: 1 }, PinRef { node: g3, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: dff2, port: 0 }, PinRef { node: g3, port: 1 })
        .unwrap();
    netlist
        .connect(PinRef { node: g3, port: 0 }, PinRef { node: dff3, port: 0 })
        .unwrap();

    // Stage 4: OR gate + DFF + output
    let g4 = netlist.add_node(NodeKind::CellInstance, "or1");
    let dff4 = netlist.add_node(NodeKind::Dff, "reg4");
    let out = netlist.add_node(NodeKind::Port, "data_out");
    netlist
        .connect(PinRef { node: dff3, port: 0 }, PinRef { node: g4, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: dff2, port: 1 }, PinRef { node: g4, port: 1 })
        .unwrap();
    netlist
        .connect(PinRef { node: g4, port: 0 }, PinRef { node: dff4, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: dff4, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    netlist
}
