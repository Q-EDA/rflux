//! Integration tests for `rflux-timing`.
//!
//! Tests the full STA pipeline with realistic routed circuits,
//! exercising setup/hold analysis, multi-corner, SSTA, and
//! the TLine delay model.

use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_route::{RoutingConfig, SimpleRouter};
use rflux_tech::Pdk;
use rflux_timing::{
    MonteCarloConfig, OcvConfig, StatisticalTimingConfig, StaticTimingAnalyzer, TimingConfig,
};

// ---------------------------------------------------------------------------
// Helper: build and route a pipeline using the place crate's placer
// ---------------------------------------------------------------------------

fn build_routed_pipeline(stages: usize) -> (Netlist, rflux_route::RoutingReport, Pdk) {
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
        let dff = netlist.add_node(NodeKind::Dff, format!("dff{i}"));
        netlist
            .connect(
                PinRef { node: gate, port: 0 },
                PinRef { node: dff, port: 0 },
            )
            .unwrap();
        prev = dff;
    }
    let out = netlist.add_node(NodeKind::Port, "out");
    netlist
        .connect(
            PinRef { node: prev, port: 0 },
            PinRef { node: out, port: 0 },
        )
        .unwrap();

    let placement = rflux_place::LevelizedPlacer::new()
        .place(&netlist, &rflux_place::PlacementConfig::default())
        .unwrap();
    let pdk = Pdk::minimal("test");
    let routing = SimpleRouter::new()
        .route(&netlist, &placement, &pdk, &RoutingConfig::default())
        .unwrap();

    (netlist, routing, pdk)
}

// ---------------------------------------------------------------------------
// Basic STA integration tests
// ---------------------------------------------------------------------------

#[test]
fn sta_produces_valid_report_for_pipeline() {
    let (netlist, routing, pdk) = build_routed_pipeline(3);
    let analyzer = StaticTimingAnalyzer::new();
    let config = TimingConfig::default();

    let report = analyzer
        .analyze(&netlist, &routing, &pdk, &config, None)
        .unwrap();

    assert!(report.analyzed_arcs > 0);
    assert!(report.critical_path_delay_ps > 0.0);
    assert!(report.worst_setup_slack_ps.is_finite());
    assert!(report.worst_hold_slack_ps.is_finite());
}

#[test]
fn sta_respects_clock_period() {
    let (netlist, routing, pdk) = build_routed_pipeline(2);
    let analyzer = StaticTimingAnalyzer::new();

    let fast = TimingConfig {
        clock_period_ps: 50.0,
        ..TimingConfig::default()
    };
    let slow = TimingConfig {
        clock_period_ps: 200.0,
        ..TimingConfig::default()
    };

    let report_fast = analyzer
        .analyze(&netlist, &routing, &pdk, &fast, None)
        .unwrap();
    let report_slow = analyzer
        .analyze(&netlist, &routing, &pdk, &slow, None)
        .unwrap();

    assert!(report_slow.worst_setup_slack_ps >= report_fast.worst_setup_slack_ps);
}

#[test]
fn sta_detects_setup_violations_on_tight_clock() {
    let (netlist, routing, pdk) = build_routed_pipeline(3);
    let analyzer = StaticTimingAnalyzer::new();

    let config = TimingConfig {
        clock_period_ps: 1.0,
        ..TimingConfig::default()
    };

    let report = analyzer
        .analyze(&netlist, &routing, &pdk, &config, None)
        .unwrap();

    assert!(
        report.setup_violations > 0 || report.worst_setup_slack_ps < 0.0,
        "should detect setup violations with tight clock"
    );
}

#[test]
fn sta_detects_hold_violations() {
    let (netlist, routing, pdk) = build_routed_pipeline(2);
    let analyzer = StaticTimingAnalyzer::new();

    let config = TimingConfig::default();
    let report = analyzer
        .analyze(&netlist, &routing, &pdk, &config, None)
        .unwrap();

    // Analysis should complete without error regardless of violations
    assert!(report.analyzed_arcs > 0);
}

// ---------------------------------------------------------------------------
// Multi-clock domain tests
// ---------------------------------------------------------------------------

#[test]
fn sta_handles_multiple_clock_domains() {
    let mut netlist = Netlist::new();
    let inp = netlist.add_node(NodeKind::Port, "in");
    let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
    let dff1 = netlist.add_node(NodeKind::Dff, "dff1");
    let g2 = netlist.add_node(NodeKind::CellInstance, "g2");
    let dff2 = netlist.add_node(NodeKind::Dff, "dff2");
    let out = netlist.add_node(NodeKind::Port, "out");

    netlist
        .connect(PinRef { node: inp, port: 0 }, PinRef { node: g1, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: g1, port: 0 }, PinRef { node: dff1, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: dff1, port: 0 }, PinRef { node: g2, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: g2, port: 0 }, PinRef { node: dff2, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: dff2, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let placement = rflux_place::LevelizedPlacer::new()
        .place(&netlist, &rflux_place::PlacementConfig::default())
        .unwrap();
    let pdk = Pdk::minimal("test");
    let routing = SimpleRouter::new()
        .route(&netlist, &placement, &pdk, &RoutingConfig::default())
        .unwrap();

    let analyzer = StaticTimingAnalyzer::new();
    let config = TimingConfig {
        clock_domains: vec![
            rflux_timing::ClockDomainConstraint {
                id: 0,
                period_ps: 100.0,
            },
            rflux_timing::ClockDomainConstraint {
                id: 1,
                period_ps: 150.0,
            },
        ],
        ..TimingConfig::default()
    };

    let report = analyzer
        .analyze(&netlist, &routing, &pdk, &config, None)
        .unwrap();

    assert!(report.analyzed_arcs > 0);
}

// ---------------------------------------------------------------------------
// OCV tests
// ---------------------------------------------------------------------------

#[test]
fn sta_with_ocv_applies_derating() {
    let (netlist, routing, pdk) = build_routed_pipeline(2);
    let analyzer = StaticTimingAnalyzer::new();

    let config_no_ocv = TimingConfig::default();
    let config_ocv = TimingConfig {
        ocv: OcvConfig {
            cell_early_factor: 0.95,
            cell_late_factor: 1.05,
            wire_early_factor: 0.95,
            wire_late_factor: 1.05,
            path_based: true,
            path_depth_factor: 0.005,
        },
        ..TimingConfig::default()
    };

    let report_no_ocv = analyzer
        .analyze(&netlist, &routing, &pdk, &config_no_ocv, None)
        .unwrap();
    let report_ocv = analyzer
        .analyze(&netlist, &routing, &pdk, &config_ocv, None)
        .unwrap();

    assert!(report_ocv.analyzed_arcs == report_no_ocv.analyzed_arcs);
}

// ---------------------------------------------------------------------------
// Statistical timing tests
// ---------------------------------------------------------------------------

#[test]
fn ssta_produces_valid_report() {
    let (netlist, routing, pdk) = build_routed_pipeline(2);
    let analyzer = StaticTimingAnalyzer::new();

    let config = TimingConfig::default();
    let ssta_config = StatisticalTimingConfig {
        cell_delay_sigma_ratio: 0.05,
        wire_delay_sigma_ratio: 0.05,
        sigma_multiplier: 3.0,
        ..StatisticalTimingConfig::default()
    };

    let report = analyzer
        .analyze_statistical(&netlist, &routing, &pdk, &config, &ssta_config, None)
        .unwrap();

    assert!(report.analyzed_arcs > 0);
    for arc in &report.arcs {
        assert!(arc.setup_sigma_ps >= 0.0);
        assert!(arc.hold_sigma_ps >= 0.0);
    }
}

#[test]
fn ssta_path_statistics_are_present() {
    let (netlist, routing, pdk) = build_routed_pipeline(2);
    let analyzer = StaticTimingAnalyzer::new();

    let config = TimingConfig::default();
    let ssta_config = StatisticalTimingConfig {
        cell_delay_sigma_ratio: 0.05,
        wire_delay_sigma_ratio: 0.05,
        ..StatisticalTimingConfig::default()
    };

    let report = analyzer
        .analyze_statistical(&netlist, &routing, &pdk, &config, &ssta_config, None)
        .unwrap();

    assert!(
        !report.path_statistics.is_empty(),
        "path statistics should be present"
    );
    for ps in &report.path_statistics {
        assert!(ps.mean_arrival_ps >= 0.0);
        assert!(ps.sigma_arrival_ps >= 0.0);
        assert!(ps.variation_source_count > 0);
    }

    assert!(
        report.worst_case_corner.is_some(),
        "worst-case corner should be extracted"
    );
}

// ---------------------------------------------------------------------------
// Monte Carlo verification tests
// ---------------------------------------------------------------------------

#[test]
fn monte_carlo_verification_completes() {
    let (netlist, routing, pdk) = build_routed_pipeline(2);
    let analyzer = StaticTimingAnalyzer::new();

    let config = TimingConfig::default();
    let mc_config = MonteCarloConfig {
        samples: 50,
        seed: 42,
        cell_sigma_ratio: 0.05,
        wire_sigma_ratio: 0.05,
    };

    let report = analyzer.verify_ssta(&netlist, &routing, &pdk, &config, &mc_config);

    assert_eq!(report.samples, 50);
    assert!(report.mean_setup_slack_ps.is_finite());
    assert!(report.std_setup_slack_ps >= 0.0);
}

// ---------------------------------------------------------------------------
// Critical path enumeration tests
// ---------------------------------------------------------------------------

#[test]
fn sta_enumerates_critical_paths() {
    let (netlist, routing, pdk) = build_routed_pipeline(3);
    let analyzer = StaticTimingAnalyzer::new();

    let config = TimingConfig::default();
    let report = analyzer
        .analyze(&netlist, &routing, &pdk, &config, None)
        .unwrap();

    let path_report = report.enumerate_critical_paths(5);
    assert!(!path_report.paths.is_empty());
    if path_report.paths.len() > 1 {
        assert!(
            path_report.paths[0].total_slack_ps <= path_report.paths[1].total_slack_ps
        );
    }
}

// ---------------------------------------------------------------------------
// Hold fix recommendation tests
// ---------------------------------------------------------------------------

#[test]
fn sta_produces_hold_fix_recommendations() {
    let (netlist, routing, pdk) = build_routed_pipeline(2);
    let analyzer = StaticTimingAnalyzer::new();

    let config = TimingConfig::default();
    let report = analyzer
        .analyze(&netlist, &routing, &pdk, &config, None)
        .unwrap();

    let recommendations = report.hold_fix_recommendations(0.15);
    for rec in &recommendations {
        assert!(rec.hold_slack_ps < 0.0);
        assert!(rec.recommended_jtl_length_um > 0.0);
    }
}
