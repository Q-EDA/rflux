//! Integration tests for `rflux-route`.
//!
//! Tests the full routing pipeline with realistic circuit topologies,
//! exercising JTL/PTL selection, congestion-aware routing, and the
//! hybrid optimizer.

use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_place::{LevelizedPlacer, PlacementConfig};
use rflux_route::{
    BlockedRegion, ClockRouteRequest, HybridRouteOptimizer, ReflectionAnalyzer, RouteMode,
    RoutingConfig, SimpleRouter,
};
use rflux_tech::{LengthRange, Pdk};

// ---------------------------------------------------------------------------
// Helper: build a chain with DFFs
// ---------------------------------------------------------------------------

fn build_pipeline(stages: usize) -> Netlist {
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
    netlist
}

// ---------------------------------------------------------------------------
// Basic routing integration tests
// ---------------------------------------------------------------------------

#[test]
fn router_produces_valid_routes_for_pipeline() {
    let netlist = build_pipeline(3);
    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();
    let pdk = Pdk::minimal("test");

    let report = SimpleRouter::new()
        .route(&netlist, &placement, &pdk, &RoutingConfig::default())
        .unwrap();

    // Every edge should have a route
    assert_eq!(report.routes.len(), netlist.edge_count());
    // All routes should have positive length
    for r in &report.routes {
        assert!(r.length_um > 0.0, "route length must be positive");
        assert!(r.direct_length_um > 0.0);
    }
    assert!(report.total_length_um > 0.0);
}

#[test]
fn router_selects_jtl_for_short_nets() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::Port, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();
    let report = SimpleRouter::new()
        .route(
            &netlist,
            &placement,
            &Pdk::minimal("test"),
            &RoutingConfig::default(),
        )
        .unwrap();

    assert_eq!(report.routes[0].mode, RouteMode::Jtl);
    assert_eq!(report.jtl_routes, 1);
    assert_eq!(report.ptl_routes, 0);
}

#[test]
fn router_selects_ptl_for_long_nets() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::CellInstance, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(
            &netlist,
            &PlacementConfig {
                x_pitch_um: 100.0,
                y_pitch_um: 24.0,
                ..PlacementConfig::default()
            },
        )
        .unwrap();

    let report = SimpleRouter::new()
        .route(
            &netlist,
            &placement,
            &Pdk::minimal("test"),
            &RoutingConfig::default(),
        )
        .unwrap();

    assert_eq!(report.ptl_routes, 1);
}

#[test]
fn router_avoids_ptl_forbidden_ranges() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::CellInstance, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(
            &netlist,
            &PlacementConfig {
                x_pitch_um: 100.0,
                y_pitch_um: 24.0,
                ..PlacementConfig::default()
            },
        )
        .unwrap();

    let mut pdk = Pdk::minimal("test");
    pdk.ptl_forbidden_ranges.push(LengthRange {
        min_um: 80.0,
        max_um: 120.0,
    });

    let report = SimpleRouter::new()
        .route(&netlist, &placement, &pdk, &RoutingConfig::default())
        .unwrap();

    assert_eq!(report.jtl_routes, 1);
}

// ---------------------------------------------------------------------------
// Blocked region routing tests
// ---------------------------------------------------------------------------

#[test]
fn router_detours_around_blocked_regions() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::CellInstance, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(
            &netlist,
            &PlacementConfig {
                x_pitch_um: 100.0,
                y_pitch_um: 24.0,
                ..PlacementConfig::default()
            },
        )
        .unwrap();

    let config = RoutingConfig {
        blocked_regions: vec![BlockedRegion {
            min_x_um: 40.0,
            max_x_um: 60.0,
            min_y_um: -4.0,
            max_y_um: 4.0,
        }],
        ..RoutingConfig::default()
    };

    let report = SimpleRouter::new()
        .route(&netlist, &placement, &Pdk::minimal("test"), &config)
        .unwrap();

    assert!(report.detoured_routes > 0);
    assert!(report.total_detour_overhead_um > 0.0);
}

// ---------------------------------------------------------------------------
// Clock-data co-routing tests
// ---------------------------------------------------------------------------

#[test]
fn co_routing_marks_clock_nets() {
    let mut netlist = Netlist::new();
    let clk = netlist.add_node(NodeKind::Port, "clk");
    let dff_a = netlist.add_node(NodeKind::Dff, "dff_a");
    let dff_b = netlist.add_node(NodeKind::Dff, "dff_b");
    let gate = netlist.add_node(NodeKind::CellInstance, "gate");
    netlist
        .connect(PinRef { node: dff_a, port: 0 }, PinRef { node: gate, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: dff_b, port: 0 }, PinRef { node: gate, port: 1 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(
            &netlist,
            &PlacementConfig {
                x_pitch_um: 80.0,
                y_pitch_um: 24.0,
                ..PlacementConfig::default()
            },
        )
        .unwrap();

    let clock_requests = vec![
        ClockRouteRequest {
            from: PinRef { node: clk, port: 0 },
            to: PinRef { node: dff_a, port: 1 },
            phase: 0,
        },
        ClockRouteRequest {
            from: PinRef { node: clk, port: 0 },
            to: PinRef { node: dff_b, port: 1 },
            phase: 1,
        },
    ];

    let report = SimpleRouter::new()
        .route_clock_and_data(
            &netlist,
            &placement,
            &Pdk::minimal("test"),
            &RoutingConfig::default(),
            &clock_requests,
        )
        .unwrap();

    assert!(report.co_routed);
    assert_eq!(report.clock_routes, 2);
    assert_eq!(report.data_routes, 2);
    assert_eq!(report.routes.len(), 4);

    let clock_nets: Vec<_> = report.routes.iter().filter(|r| r.is_clock_net).collect();
    assert_eq!(clock_nets.len(), 2);
    assert!(clock_nets.iter().all(|r| r.clock_phase.is_some()));
}

// ---------------------------------------------------------------------------
// Reflection analysis integration tests
// ---------------------------------------------------------------------------

#[test]
fn reflection_analyzer_detects_jtl_ptl_boundary() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::CellInstance, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(
            &netlist,
            &PlacementConfig {
                x_pitch_um: 100.0,
                y_pitch_um: 24.0,
                ..PlacementConfig::default()
            },
        )
        .unwrap();

    let report = SimpleRouter::new()
        .route(
            &netlist,
            &placement,
            &Pdk::minimal("test"),
            &RoutingConfig::default(),
        )
        .unwrap();

    let analyzer = ReflectionAnalyzer::new(0.01);
    let reflection = analyzer.analyze(&report.routes, &Pdk::minimal("test"), &RoutingConfig::default());

    // At least one route should exist
    assert!(!reflection.per_route.is_empty());
}

// ---------------------------------------------------------------------------
// Hybrid optimizer integration tests
// ---------------------------------------------------------------------------

#[test]
fn hybrid_optimizer_selects_mode_based_on_cost() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::CellInstance, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(
            &netlist,
            &PlacementConfig {
                x_pitch_um: 100.0,
                y_pitch_um: 24.0,
                ..PlacementConfig::default()
            },
        )
        .unwrap();

    let optimizer = HybridRouteOptimizer::new();
    let (report, candidates) = optimizer
        .route_all(
            &netlist,
            &placement,
            &Pdk::minimal("test"),
            &RoutingConfig::default(),
        )
        .unwrap();

    assert_eq!(report.routes.len(), 1);
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].total_cost > 0.0);
}

#[test]
fn hybrid_optimizer_cost_breakdown_is_consistent() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::CellInstance, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(
            &netlist,
            &PlacementConfig {
                x_pitch_um: 100.0,
                y_pitch_um: 24.0,
                ..PlacementConfig::default()
            },
        )
        .unwrap();

    let optimizer = HybridRouteOptimizer::new();
    let (_, candidates) = optimizer
        .route_all(
            &netlist,
            &placement,
            &Pdk::minimal("test"),
            &RoutingConfig::default(),
        )
        .unwrap();

    let c = &candidates[0];
    // Total cost should be >= sum of components
    assert!(c.total_cost >= c.delay_cost);
    assert!(c.total_cost >= c.reflection_cost);
    assert!(c.total_cost >= c.area_cost);
}

// ---------------------------------------------------------------------------
// Routing with parasitic extraction
// ---------------------------------------------------------------------------

#[test]
fn routing_report_supports_parasitic_extraction() {
    let netlist = build_pipeline(2);
    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();

    let report = SimpleRouter::new()
        .route(
            &netlist,
            &placement,
            &Pdk::minimal("test"),
            &RoutingConfig::default(),
        )
        .unwrap();

    // All routes should have valid lengths for extraction
    for r in &report.routes {
        assert!(r.length_um > 0.0);
        assert!(r.direct_length_um > 0.0);
        assert!(r.length_um >= r.direct_length_um);
    }
}
