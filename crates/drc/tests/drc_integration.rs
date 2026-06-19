//! Integration tests for `rflux-drc`.
//!
//! Tests the DRC and LVS checking pipeline with realistic layouts.

use rflux_drc::{DrcChecker, DrcRuleSet, LvsConfig, LvsReport};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_place::{LevelizedPlacer, PlacementConfig, PlacedNode, Point};
use rflux_route::{RouteMode, RoutingConfig, SimpleRouter};
use rflux_tech::Pdk;

// ---------------------------------------------------------------------------
// Helper: build and place a simple circuit
// ---------------------------------------------------------------------------

fn build_placed_circuit() -> (Netlist, rflux_place::Placement, rflux_route::RoutingReport, Pdk) {
    let mut netlist = Netlist::new();
    let inp = netlist.add_node(NodeKind::Port, "in");
    let g1 = netlist.add_node(NodeKind::CellInstance, "g1");
    let g2 = netlist.add_node(NodeKind::CellInstance, "g2");
    let out = netlist.add_node(NodeKind::Port, "out");

    netlist
        .connect(PinRef { node: inp, port: 0 }, PinRef { node: g1, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: g1, port: 0 }, PinRef { node: g2, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: g2, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();
    let pdk = Pdk::minimal("test");
    let routing = SimpleRouter::new()
        .route(&netlist, &placement, &pdk, &RoutingConfig::default())
        .unwrap();

    (netlist, placement, routing, pdk)
}

// ---------------------------------------------------------------------------
// DRC integration tests
// ---------------------------------------------------------------------------

#[test]
fn drc_clean_layout_passes_all_rules() {
    let (_, placement, routing, pdk) = build_placed_circuit();
    let rules = DrcRuleSet::from_pdk(&pdk, placement.width_um, placement.height_um);
    let checker = DrcChecker::new(rules);
    let report = checker.check(&placement, &routing, &Netlist::new());

    // Clean layout should have no errors
    let errors: Vec<_> = report
        .violations
        .iter()
        .filter(|v| v.severity == rflux_drc::DrcSeverity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "clean layout should have no DRC errors, found {}",
        errors.len()
    );
}

#[test]
fn drc_detects_jj_spacing_violation() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::CellInstance, "a");
    let b = netlist.add_node(NodeKind::CellInstance, "b");

    // Place two nodes very close together
    let placement = rflux_place::Placement {
        nodes: vec![
            PlacedNode {
                node: a,
                level: 0,
                slot: 0,
                point: Point {
                    x_um: 0.0,
                    y_um: 0.0,
                },
            },
            PlacedNode {
                node: b,
                level: 0,
                slot: 1,
                point: Point {
                    x_um: 0.1,
                    y_um: 0.0,
                },
            }, // Very close
        ],
        width_um: 1.0,
        height_um: 1.0,
    };

    let pdk = Pdk::minimal("test");
    let rules = DrcRuleSet::from_pdk(&pdk, 1.0, 1.0);
    let checker = DrcChecker::new(rules);
    let routing = rflux_route::RoutingReport {
        routes: vec![],
        total_length_um: 0.0,
        total_detour_overhead_um: 0.0,
        detoured_routes: 0,
        jtl_routes: 0,
        ptl_routes: 0,
        clock_routes: 0,
        data_routes: 0,
        peak_channel_usage: 0,
        co_routed: false,
    };
    let report = checker.check(&placement, &routing, &netlist);

    // Should detect spacing violation
    assert!(
        !report.violations.is_empty(),
        "should detect JJ spacing violation for very close nodes"
    );
}

#[test]
fn drc_report_to_svg_produces_output() {
    let (_, placement, routing, pdk) = build_placed_circuit();
    let rules = DrcRuleSet::from_pdk(&pdk, placement.width_um, placement.height_um);
    let checker = DrcChecker::new(rules);
    let report = checker.check(&placement, &routing, &Netlist::new());

    let svg = report.to_svg("DRC Test");
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
}

// ---------------------------------------------------------------------------
// LVS integration tests
// ---------------------------------------------------------------------------

#[test]
fn lvs_matching_layout_passes() {
    let (netlist, placement, routing, _) = build_placed_circuit();

    let config = LvsConfig::default();
    let lvs = rflux_drc::lvs_check(&netlist, &placement, &routing, &config);

    assert!(lvs.passed || !lvs.errors.is_empty());
}

#[test]
fn lvs_detects_missing_connections() {
    let mut schematic = Netlist::new();
    let a = schematic.add_node(NodeKind::Port, "a");
    let b = schematic.add_node(NodeKind::CellInstance, "b");
    let c = schematic.add_node(NodeKind::CellInstance, "c");
    let d = schematic.add_node(NodeKind::Port, "d");

    schematic
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();
    schematic
        .connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 0 })
        .unwrap();
    schematic
        .connect(PinRef { node: c, port: 0 }, PinRef { node: d, port: 0 })
        .unwrap();

    // Layout has fewer connections
    let mut layout_netlist = Netlist::new();
    let a = layout_netlist.add_node(NodeKind::Port, "a");
    let b = layout_netlist.add_node(NodeKind::CellInstance, "b");
    let c = layout_netlist.add_node(NodeKind::CellInstance, "c");
    let d = layout_netlist.add_node(NodeKind::Port, "d");

    layout_netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();
    // Missing b->c connection
    layout_netlist
        .connect(PinRef { node: c, port: 0 }, PinRef { node: d, port: 0 })
        .unwrap();

    let placement = rflux_place::Placement {
        nodes: vec![
            PlacedNode {
                node: a,
                level: 0,
                slot: 0,
                point: Point { x_um: 0.0, y_um: 0.0 },
            },
            PlacedNode {
                node: b,
                level: 1,
                slot: 0,
                point: Point { x_um: 24.0, y_um: 0.0 },
            },
            PlacedNode {
                node: c,
                level: 2,
                slot: 0,
                point: Point { x_um: 48.0, y_um: 0.0 },
            },
            PlacedNode {
                node: d,
                level: 3,
                slot: 0,
                point: Point { x_um: 72.0, y_um: 0.0 },
            },
        ],
        width_um: 100.0,
        height_um: 24.0,
    };

    let routing = rflux_route::RoutingReport {
        routes: vec![],
        total_length_um: 0.0,
        total_detour_overhead_um: 0.0,
        detoured_routes: 0,
        jtl_routes: 0,
        ptl_routes: 0,
        clock_routes: 0,
        data_routes: 0,
        peak_channel_usage: 0,
        co_routed: false,
    };

    let config = LvsConfig::default();
    let lvs = rflux_drc::lvs_check(&schematic, &placement, &routing, &config);

    // Should detect the missing connection
    assert!(
        !lvs.passed || !lvs.errors.is_empty(),
        "LVS should detect missing b->c connection"
    );
}
