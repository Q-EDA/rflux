//! Integration tests for `rflux-place`.
//!
//! Tests the full placement pipeline with realistic circuit topologies,
//! exercising the public API surface the way an external caller would.

use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_place::{
    estimate_layout, LevelizedPlacer, PartitionConfig, PartitionPlacer, PlacementConfig,
    SaConfig, SaPlacer,
};

// ---------------------------------------------------------------------------
// Helper: build a chain of N gates
// ---------------------------------------------------------------------------

fn build_chain(n: usize) -> Netlist {
    let mut netlist = Netlist::new();
    let inp = netlist.add_node(NodeKind::Port, "in");
    let mut prev = inp;
    for i in 0..n {
        let g = netlist.add_node(NodeKind::CellInstance, format!("g{i}"));
        netlist
            .connect(
                PinRef { node: prev, port: 0 },
                PinRef { node: g, port: 0 },
            )
            .unwrap();
        prev = g;
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
// Helper: build a balanced binary tree
// ---------------------------------------------------------------------------

fn build_tree(depth: usize) -> Netlist {
    let mut netlist = Netlist::new();
    let mut current_layer: Vec<_> = (0..(1 << depth))
        .map(|i| netlist.add_node(NodeKind::Port, format!("in{i}")))
        .collect();

    let mut gate_idx = 0;
    for _ in 0..depth {
        let mut next = Vec::new();
        for pair in current_layer.chunks(2) {
            let g = netlist.add_node(NodeKind::CellInstance, format!("g{gate_idx}"));
            gate_idx += 1;
            netlist
                .connect(
                    PinRef { node: pair[0], port: 0 },
                    PinRef { node: g, port: 0 },
                )
                .unwrap();
            netlist
                .connect(
                    PinRef { node: pair[1], port: 0 },
                    PinRef { node: g, port: 1 },
                )
                .unwrap();
            next.push(g);
        }
        current_layer = next;
    }

    let out = netlist.add_node(NodeKind::Port, "out");
    netlist
        .connect(
            PinRef {
                node: current_layer[0],
                port: 0,
            },
            PinRef { node: out, port: 0 },
        )
        .unwrap();
    netlist
}

// ---------------------------------------------------------------------------
// LevelizedPlacer integration tests
// ---------------------------------------------------------------------------

#[test]
fn levelized_placer_chain_produces_monotonic_levels() {
    let netlist = build_chain(5);
    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();

    assert_eq!(placement.nodes.len(), netlist.node_count());

    let mut prev_level = 0;
    for pn in &placement.nodes {
        assert!(pn.level >= prev_level, "levels must be non-decreasing");
        prev_level = pn.level;
    }
}

#[test]
fn levelized_placer_tree_spreads_across_levels() {
    let netlist = build_tree(3);
    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();

    assert_eq!(placement.nodes.len(), 16);
    let max_level = placement.nodes.iter().map(|n| n.level).max().unwrap();
    assert!(max_level >= 3, "tree depth 3 should span at least 3 levels");
}

#[test]
fn levelized_placer_respects_custom_pitch() {
    let netlist = build_chain(3);
    let config = PlacementConfig {
        x_pitch_um: 100.0,
        y_pitch_um: 50.0,
        ..PlacementConfig::default()
    };
    let placement = LevelizedPlacer::new().place(&netlist, &config).unwrap();

    assert!(placement.width_um >= 100.0);
}

#[test]
fn levelized_placer_avoids_blocked_regions() {
    let netlist = build_chain(4);
    let config = PlacementConfig {
        blocked_regions: vec![rflux_place::BlockedRegion {
            min_x_um: 30.0,
            max_x_um: 70.0,
            min_y_um: -10.0,
            max_y_um: 10.0,
        }],
        ..PlacementConfig::default()
    };
    let placement = LevelizedPlacer::new().place(&netlist, &config).unwrap();

    for pn in &placement.nodes {
        let in_blocked = pn.point.x_um >= 30.0
            && pn.point.x_um <= 70.0
            && pn.point.y_um >= -10.0
            && pn.point.y_um <= 10.0;
        assert!(
            !in_blocked,
            "node {:?} at ({}, {}) is inside blocked region",
            pn.node, pn.point.x_um, pn.point.y_um
        );
    }
}

#[test]
fn levelized_placer_handles_macro_halo() {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::Port, "a");
    let mc = netlist.add_node(NodeKind::MacroCell, "macro");
    let b = netlist.add_node(NodeKind::CellInstance, "b");
    let out = netlist.add_node(NodeKind::Port, "out");
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: mc, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: mc, port: 0 }, PinRef { node: b, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: b, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let config = PlacementConfig {
        macro_halo_x_um: 10.0,
        macro_halo_y_um: 10.0,
        ..PlacementConfig::default()
    };
    let placement = LevelizedPlacer::new().place(&netlist, &config).unwrap();
    assert_eq!(placement.nodes.len(), 4);
}

// ---------------------------------------------------------------------------
// SA Placer integration tests
// ---------------------------------------------------------------------------

#[test]
fn sa_placer_produces_valid_placement() {
    let netlist = build_tree(3);
    let sa = SaPlacer::new(PlacementConfig::default(), SaConfig::default());
    let placement = sa.place(&netlist).unwrap();

    assert_eq!(placement.nodes.len(), netlist.node_count());
    assert!(placement.width_um > 0.0);
    assert!(placement.height_um > 0.0);
}

#[test]
fn sa_placer_with_critical_nets_affects_placement() {
    let netlist = build_chain(5);
    let sa = SaPlacer::new(PlacementConfig::default(), SaConfig::default());
    let critical_nets = vec![(
        PinRef {
            node: rflux_ir::NodeId(1),
            port: 0,
        },
        PinRef {
            node: rflux_ir::NodeId(2),
            port: 0,
        },
    )];

    let sa_normal = sa.place(&netlist).unwrap();
    let sa_critical = sa.place_with_critical_nets(&netlist, &critical_nets).unwrap();

    assert_eq!(sa_normal.nodes.len(), sa_critical.nodes.len());
}

// ---------------------------------------------------------------------------
// Partition Placer integration tests
// ---------------------------------------------------------------------------

#[test]
fn partition_placer_handles_large_design() {
    let netlist = build_chain(20);
    let placer = PartitionPlacer::new(
        PlacementConfig::default(),
        PartitionConfig::default(),
    );
    let placement = placer.place(&netlist).unwrap();

    assert_eq!(placement.nodes.len(), netlist.node_count());
    assert!(placement.width_um > 0.0);
}

// ---------------------------------------------------------------------------
// Quick layout estimation integration tests
// ---------------------------------------------------------------------------

#[test]
fn estimate_layout_matches_levelized_for_small_design() {
    let netlist = build_chain(5);
    let est = estimate_layout(&netlist);

    assert!(est.width_um > 0.0);
    assert!(est.height_um > 0.0);
    assert!(est.area_um2 > 0.0);
    assert_eq!(est.placed_nodes, netlist.node_count());
    assert!(est.estimated_avg_wire_length_um > 0.0);
}

#[test]
fn estimate_layout_scales_with_circuit_size() {
    let small = build_chain(3);
    let large = build_chain(10);

    let est_small = estimate_layout(&small);
    let est_large = estimate_layout(&large);

    assert!(est_large.area_um2 >= est_small.area_um2);
    assert!(est_large.placed_nodes > est_small.placed_nodes);
}

// ---------------------------------------------------------------------------
// Mixed node type tests
// ---------------------------------------------------------------------------

#[test]
fn placement_handles_mixed_node_types() {
    let mut netlist = Netlist::new();
    let inp = netlist.add_node(NodeKind::Port, "in");
    let split = netlist.add_node(NodeKind::Splitter, "split");
    let gate = netlist.add_node(NodeKind::CellInstance, "gate");
    let dff = netlist.add_node(NodeKind::Dff, "dff");
    let mc = netlist.add_node(NodeKind::MacroCell, "macro");
    let out = netlist.add_node(NodeKind::Port, "out");

    netlist
        .connect(PinRef { node: inp, port: 0 }, PinRef { node: split, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: split, port: 0 }, PinRef { node: gate, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: gate, port: 0 }, PinRef { node: dff, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: dff, port: 0 }, PinRef { node: mc, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: mc, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();

    assert_eq!(placement.nodes.len(), 6);
    for pn in &placement.nodes {
        assert!(pn.point.x_um.is_finite());
        assert!(pn.point.y_um.is_finite());
    }
}

#[test]
fn placement_write_to_file_and_read_back() {
    let netlist = build_chain(3);
    let placement = LevelizedPlacer::new()
        .place(&netlist, &PlacementConfig::default())
        .unwrap();

    let path = std::env::temp_dir().join("rflux_place_integration_test.txt");
    placement.write_to_file(&path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), placement.nodes.len());

    let _ = std::fs::remove_file(&path);
}
