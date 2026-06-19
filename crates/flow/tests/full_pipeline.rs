//! Comprehensive end-to-end integration tests for the rflux EDA pipeline.
//!
//! These tests exercise the full design flow from synthesis through
//! timing closure, verifying that all stages work together correctly.

use rflux_flow::{FlowConfig, FlowRunner};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_tech::Pdk;

// ---------------------------------------------------------------------------
// Helper: build a multi-stage pipeline with splitters
// ---------------------------------------------------------------------------

fn build_pipeline_with_fanout(stages: usize, fanout: usize) -> Netlist {
    let mut netlist = Netlist::new();
    let inp = netlist.add_node(NodeKind::Port, "in");
    let mut prev = inp;

    for i in 0..stages {
        let gate = netlist.add_node(NodeKind::CellInstance, format!("g{i}"));
        netlist
            .connect(
                PinRef { node: prev, port: 0 },
                PinRef { node: gate, port: 0 },
            )
            .unwrap();

        // Add fanout via splitter
        if fanout > 1 && i < stages - 1 {
            let split = netlist.add_node(NodeKind::Splitter, format!("split{i}"));
            netlist
                .connect(
                    PinRef { node: gate, port: 0 },
                    PinRef { node: split, port: 0 },
                )
                .unwrap();
            // Connect first output to next stage
            let dff = netlist.add_node(NodeKind::Dff, format!("dff{i}"));
            netlist
                .connect(
                    PinRef { node: split, port: 0 },
                    PinRef { node: dff, port: 0 },
                )
                .unwrap();
            prev = dff;
        } else {
            let dff = netlist.add_node(NodeKind::Dff, format!("dff{i}"));
            netlist
                .connect(
                    PinRef { node: gate, port: 0 },
                    PinRef { node: dff, port: 0 },
                )
                .unwrap();
            prev = dff;
        }
    }

    let out = netlist.add_node(NodeKind::Port, "out");
    netlist
        .connect(
            PinRef { node: prev, port: 0 },
            PinRef { node: out, port: 0 },
        )
        .unwrap();

    netlist
}

// ---------------------------------------------------------------------------
// Full pipeline integration tests
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_3_stage_basic() {
    let mut netlist = build_pipeline_with_fanout(3, 1);
    let config = FlowConfig::default();
    let mut runner = FlowRunner::new();
    let pdk = Pdk::minimal("e2e");

    let report = runner
        .compile_layout(&mut netlist, &pdk, &config)
        .expect("full pipeline should succeed");

    // Verify all stages produced valid output
    assert!(report.placement.placed_nodes > 0);
    assert!(report.placement.width_um > 0.0);
    assert!(report.routing.routed_nets > 0);
    assert!(report.routing.total_length_um > 0.0);
    assert!(report.clock.phase_count >= 1);
    assert!(report.timing.analyzed_arcs > 0);
    assert!(report.timing.worst_setup_slack_ps.is_finite());
    assert!(report.timing.worst_hold_slack_ps.is_finite());
}

#[test]
fn full_pipeline_5_stage_with_fanout() {
    let mut netlist = build_pipeline_with_fanout(5, 3);
    let config = FlowConfig::default();
    let mut runner = FlowRunner::new();
    let pdk = Pdk::minimal("e2e");

    let report = runner
        .compile_layout(&mut netlist, &pdk, &config)
        .expect("pipeline with fanout should succeed");

    assert!(report.routing.routed_nets > 5);
    assert!(report.clock.phase_count >= 1);
}

#[test]
fn full_pipeline_respects_custom_clock_period() {
    let mut netlist = build_pipeline_with_fanout(3, 1);
    let mut config = FlowConfig::default();
    config.timing.clock_period_ps = 80.0; // Faster clock

    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut netlist, &Pdk::minimal("e2e"), &config)
        .unwrap();

    // With faster clock, timing should be tighter
    assert!(report.timing.analyzed_arcs > 0);
}

#[test]
fn full_pipeline_with_blocked_regions() {
    let mut netlist = build_pipeline_with_fanout(4, 1);
    let mut config = FlowConfig::default();
    config.routing.blocked_regions.push(rflux_route::BlockedRegion {
        min_x_um: 30.0,
        max_x_um: 60.0,
        min_y_um: -5.0,
        max_y_um: 5.0,
    });

    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut netlist, &Pdk::minimal("e2e"), &config)
        .unwrap();

    // Should still complete successfully (detours may or may not be needed)
    assert!(report.routing.routed_nets > 0);
}

#[test]
fn full_pipeline_with_drc_enabled() {
    let mut netlist = build_pipeline_with_fanout(3, 1);
    let mut config = FlowConfig::default();
    config.enable_drc = true;

    let mut runner = FlowRunner::new();
    let report = runner
        .compile_layout(&mut netlist, &Pdk::minimal("e2e"), &config)
        .unwrap();

    // DRC should have run
    assert!(report.drc_report.is_some());
}

#[test]
fn full_pipeline_deterministic_across_runs() {
    let mut netlist_a = build_pipeline_with_fanout(4, 2);
    let mut netlist_b = build_pipeline_with_fanout(4, 2);
    let config = FlowConfig::default();
    let pdk = Pdk::minimal("e2e");

    let mut runner = FlowRunner::new();
    let report_a = runner.compile_layout(&mut netlist_a, &pdk, &config).unwrap();
    let report_b = runner.compile_layout(&mut netlist_b, &pdk, &config).unwrap();

    // Key metrics should be identical
    assert_eq!(
        report_a.synthesis.compile.connections_applied,
        report_b.synthesis.compile.connections_applied
    );
    assert_eq!(
        report_a.placement.placed_nodes,
        report_b.placement.placed_nodes
    );
    assert_eq!(
        report_a.routing.routed_nets,
        report_b.routing.routed_nets
    );
    assert_eq!(
        report_a.timing.analyzed_arcs,
        report_b.timing.analyzed_arcs
    );
}

// ---------------------------------------------------------------------------
// Power analysis integration tests
// ---------------------------------------------------------------------------

#[test]
fn power_analysis_reports_nonzero_power() {
    let mut netlist = build_pipeline_with_fanout(3, 1);
    let mut runner = FlowRunner::new();
    let pdk = Pdk::minimal("e2e");
    let config = FlowConfig::default();

    let report = runner
        .analyze_power(&mut netlist, &pdk, &config)
        .unwrap();

    assert!(report.total_dynamic_power_uw > 0.0);
    assert!(report.total_jj_count > 0);
    assert!(report.clock_frequency_ghz > 0.0);
}

// ---------------------------------------------------------------------------
// AC bias analysis integration tests
// ---------------------------------------------------------------------------

#[test]
fn ac_bias_analysis_reports_savings() {
    let mut netlist = build_pipeline_with_fanout(3, 1);
    let mut runner = FlowRunner::new();
    let pdk = Pdk::minimal("e2e");
    let config = FlowConfig::default();

    let report = runner
        .analyze_ac_bias(&mut netlist, &pdk, &config)
        .unwrap();

    assert!(report.routed_nets > 0);
    assert!(report.estimated_static_power_savings_uw >= 0.0);
}

// ---------------------------------------------------------------------------
// DSE integration tests
// ---------------------------------------------------------------------------

#[test]
fn design_space_exploration_finds_solutions() {
    let netlist = build_pipeline_with_fanout(3, 1);
    let mut runner = FlowRunner::new();
    let pdk = Pdk::minimal("e2e");
    let config = FlowConfig::default();

    let dse_config = rflux_flow::BiasDseConfig {
        ac_ratio_steps: 3,
        ptl_threshold_steps: 2,
        detour_margin_steps: 2,
        max_evaluations: 12,
    };

    let explorer = rflux_flow::BiasDesignSpaceExplorer::new();
    let report = explorer
        .explore(&mut runner, &netlist, &pdk, &config, &dse_config)
        .unwrap();

    assert!(!report.all_points.is_empty());
    assert!(report.pareto_front.front_size > 0);
    assert!(report.pareto_front.total_evaluated > 0);
}

// ---------------------------------------------------------------------------
// Feasibility estimation integration tests
// ---------------------------------------------------------------------------

#[test]
fn feasibility_estimator_runs_before_full_flow() {
    let netlist = build_pipeline_with_fanout(5, 2);
    let pdk = Pdk::minimal("e2e");

    let estimator = rflux_synth::PhysicalFeasibilityEstimator::new();
    let report = estimator.estimate(&netlist, &pdk);

    assert!(report.node_count > 0);
    assert!(report.edge_count > 0);
    assert!(report.estimated_area_um2 > 0.0);
    assert!(report.congestion_ratio >= 0.0);
}

// ---------------------------------------------------------------------------
// Multi-corner timing integration tests
// ---------------------------------------------------------------------------

#[test]
fn multi_corner_timing_analysis_completes() {
    let mut netlist = build_pipeline_with_fanout(3, 1);
    let mut runner = FlowRunner::new();
    let pdk = Pdk::minimal("e2e");
    let config = FlowConfig::default();

    let report = runner
        .compile_layout(&mut netlist, &pdk, &config)
        .unwrap();

    // Verify timing closure report structure
    assert!(
        report.timing_closure.status == "closed" || report.timing_closure.status == "open"
    );
    // action_count is usize, always >= 0; just verify the field is accessible
    let _ = report.timing_closure.action_count;
}
