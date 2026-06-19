//! Integration tests for `rflux-verify`.
//!
//! Tests the equivalence checking and timing-functional verification
//! pipelines with realistic circuit topologies.

use rflux_ir::{LogicOp, Netlist, NodeKind, PinRef};
use rflux_verify::{TimingFunctionalConfig, TimingFunctionalVerifier, Verifier};

// ---------------------------------------------------------------------------
// Helper: build two equivalent AND-OR circuits
// ---------------------------------------------------------------------------

fn build_and_or_lhs() -> Netlist {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::Port, "a");
    let b = netlist.add_node(NodeKind::Port, "b");
    let and = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
    let or = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
    let out = netlist.add_node(NodeKind::Port, "out");

    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: and, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: b, port: 0 }, PinRef { node: and, port: 1 })
        .unwrap();
    netlist
        .connect(
            PinRef { node: and, port: 0 },
            PinRef { node: or, port: 0 },
        )
        .unwrap();
    netlist
        .connect(PinRef { node: a, port: 1 }, PinRef { node: or, port: 1 })
        .unwrap();
    netlist
        .connect(
            PinRef { node: or, port: 0 },
            PinRef { node: out, port: 0 },
        )
        .unwrap();

    netlist
}

fn build_and_or_rhs() -> Netlist {
    let mut netlist = Netlist::new();
    let a = netlist.add_node(NodeKind::Port, "a");
    let b = netlist.add_node(NodeKind::Port, "b");
    let and = netlist.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
    let or = netlist.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
    let out = netlist.add_node(NodeKind::Port, "out");

    netlist
        .connect(PinRef { node: b, port: 0 }, PinRef { node: and, port: 0 })
        .unwrap();
    netlist
        .connect(PinRef { node: a, port: 0 }, PinRef { node: and, port: 1 })
        .unwrap();
    netlist
        .connect(
            PinRef { node: and, port: 0 },
            PinRef { node: or, port: 0 },
        )
        .unwrap();
    netlist
        .connect(PinRef { node: b, port: 1 }, PinRef { node: or, port: 1 })
        .unwrap();
    netlist
        .connect(
            PinRef { node: or, port: 0 },
            PinRef { node: out, port: 0 },
        )
        .unwrap();

    netlist
}

// ---------------------------------------------------------------------------
// Combinational equivalence integration tests
// ---------------------------------------------------------------------------

#[test]
fn verifier_detects_equivalent_and_or_circuits() {
    let verifier = Verifier::new();
    let lhs = build_and_or_lhs();
    let rhs = build_and_or_rhs();

    let report = verifier
        .check_boolean_equivalence(&lhs, &rhs)
        .unwrap();

    assert!(report.equivalent);
    assert!(!report.checked_outputs.is_empty());
}

#[test]
fn verifier_detects_non_equivalent_circuits() {
    let verifier = Verifier::new();

    // LHS: AND
    let mut lhs = Netlist::new();
    let a = lhs.add_node(NodeKind::Port, "a");
    let b = lhs.add_node(NodeKind::Port, "b");
    let and = lhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
    let out = lhs.add_node(NodeKind::Port, "out");
    lhs.connect(PinRef { node: a, port: 0 }, PinRef { node: and, port: 0 })
        .unwrap();
    lhs.connect(PinRef { node: b, port: 0 }, PinRef { node: and, port: 1 })
        .unwrap();
    lhs.connect(
        PinRef { node: and, port: 0 },
        PinRef { node: out, port: 0 },
    )
    .unwrap();

    // RHS: OR
    let mut rhs = Netlist::new();
    let a = rhs.add_node(NodeKind::Port, "a");
    let b = rhs.add_node(NodeKind::Port, "b");
    let or = rhs.add_node_with_logic(NodeKind::CellInstance, "or0", Some(LogicOp::Or));
    let out = rhs.add_node(NodeKind::Port, "out");
    rhs.connect(PinRef { node: a, port: 0 }, PinRef { node: or, port: 0 })
        .unwrap();
    rhs.connect(PinRef { node: b, port: 0 }, PinRef { node: or, port: 1 })
        .unwrap();
    rhs.connect(
        PinRef { node: or, port: 0 },
        PinRef { node: out, port: 0 },
    )
    .unwrap();

    let report = verifier.check_boolean_equivalence(&lhs, &rhs).unwrap();

    assert!(!report.equivalent);
}

// ---------------------------------------------------------------------------
// Sequential equivalence integration tests
// ---------------------------------------------------------------------------

#[test]
fn verifier_detects_equivalent_dff_circuits() {
    let verifier = Verifier::new();

    let mut lhs = Netlist::new();
    let data = lhs.add_node(NodeKind::Port, "data");
    let clk = lhs.add_node(NodeKind::Port, "clock");
    let dff = lhs.add_node(NodeKind::Dff, "state");
    let out = lhs.add_node(NodeKind::Port, "out");
    lhs.connect(PinRef { node: data, port: 0 }, PinRef { node: dff, port: 0 })
        .unwrap();
    lhs.connect(PinRef { node: clk, port: 0 }, PinRef { node: dff, port: 1 })
        .unwrap();
    lhs.connect(PinRef { node: dff, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let mut rhs = Netlist::new();
    let data = rhs.add_node(NodeKind::Port, "data");
    let clk = rhs.add_node(NodeKind::Port, "clock");
    let dff = rhs.add_node(NodeKind::Dff, "state");
    let out = rhs.add_node(NodeKind::Port, "out");
    rhs.connect(PinRef { node: data, port: 0 }, PinRef { node: dff, port: 0 })
        .unwrap();
    rhs.connect(PinRef { node: clk, port: 0 }, PinRef { node: dff, port: 1 })
        .unwrap();
    rhs.connect(PinRef { node: dff, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let report = verifier
        .check_single_step_sequential_equivalence(&lhs, &rhs)
        .unwrap();

    assert!(report.equivalent);
}

#[test]
fn verifier_detects_sequential_non_equivalence() {
    let verifier = Verifier::new();

    // LHS: simple DFF
    let mut lhs = Netlist::new();
    let data = lhs.add_node(NodeKind::Port, "data");
    let clk = lhs.add_node(NodeKind::Port, "clock");
    let dff = lhs.add_node(NodeKind::Dff, "state");
    let out = lhs.add_node(NodeKind::Port, "out");
    lhs.connect(PinRef { node: data, port: 0 }, PinRef { node: dff, port: 0 })
        .unwrap();
    lhs.connect(PinRef { node: clk, port: 0 }, PinRef { node: dff, port: 1 })
        .unwrap();
    lhs.connect(PinRef { node: dff, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    // RHS: DFF with enable (different behavior)
    let mut rhs = Netlist::new();
    let data = rhs.add_node(NodeKind::Port, "data");
    let enable = rhs.add_node(NodeKind::Port, "enable");
    let clk = rhs.add_node(NodeKind::Port, "clock");
    let dff = rhs.add_node_with_logic(NodeKind::Dff, "state", Some(LogicOp::DffEnable));
    let out = rhs.add_node(NodeKind::Port, "out");
    rhs.connect(PinRef { node: data, port: 0 }, PinRef { node: dff, port: 0 })
        .unwrap();
    rhs.connect(PinRef { node: enable, port: 0 }, PinRef { node: dff, port: 1 })
        .unwrap();
    rhs.connect(PinRef { node: clk, port: 0 }, PinRef { node: dff, port: 2 })
        .unwrap();
    rhs.connect(PinRef { node: dff, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let report = verifier
        .check_single_step_sequential_equivalence(&lhs, &rhs)
        .unwrap();

    assert!(!report.equivalent);
}

// ---------------------------------------------------------------------------
// Bounded sequential equivalence tests
// ---------------------------------------------------------------------------

#[test]
fn verifier_bounded_equivalence_detects_counterexample() {
    let verifier = Verifier::new();

    let mut lhs = Netlist::new();
    let data = lhs.add_node(NodeKind::Port, "data");
    let clk = lhs.add_node(NodeKind::Port, "clock");
    let dff = lhs.add_node(NodeKind::Dff, "state");
    let out = lhs.add_node(NodeKind::Port, "out");
    lhs.connect(PinRef { node: data, port: 0 }, PinRef { node: dff, port: 0 })
        .unwrap();
    lhs.connect(PinRef { node: clk, port: 0 }, PinRef { node: dff, port: 1 })
        .unwrap();
    lhs.connect(PinRef { node: dff, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    // RHS: DFF with AND gate (breaks equivalence)
    let mut rhs = Netlist::new();
    let data = rhs.add_node(NodeKind::Port, "data");
    let clk = rhs.add_node(NodeKind::Port, "clock");
    let dff = rhs.add_node(NodeKind::Dff, "state");
    let and = rhs.add_node_with_logic(NodeKind::CellInstance, "and0", Some(LogicOp::And));
    let out = rhs.add_node(NodeKind::Port, "out");
    rhs.connect(PinRef { node: data, port: 0 }, PinRef { node: dff, port: 0 })
        .unwrap();
    rhs.connect(PinRef { node: clk, port: 0 }, PinRef { node: dff, port: 1 })
        .unwrap();
    rhs.connect(PinRef { node: dff, port: 0 }, PinRef { node: and, port: 0 })
        .unwrap();
    rhs.connect(PinRef { node: data, port: 1 }, PinRef { node: and, port: 1 })
        .unwrap();
    rhs.connect(PinRef { node: and, port: 0 }, PinRef { node: out, port: 0 })
        .unwrap();

    let report = verifier
        .check_bounded_sequential_equivalence(&lhs, &rhs, 4)
        .unwrap();

    assert!(!report.equivalent);
}

// ---------------------------------------------------------------------------
// Timing-functional verification integration tests
// ---------------------------------------------------------------------------

#[test]
fn timing_functional_verifier_passes_for_clean_timing() {
    let verifier = TimingFunctionalVerifier::new();
    let netlist = Netlist::new();

    let from = PinRef {
        node: rflux_ir::NodeId(0),
        port: 0,
    };
    let to = PinRef {
        node: rflux_ir::NodeId(1),
        port: 0,
    };

    // Create a timing report with good timing
    let timing = rflux_timing::TimingReport {
        arcs: vec![rflux_timing::TimingArcReport {
            from,
            to,
            is_false_path: false,
            driver_kind: rflux_tech::SfCellKind::GenericGate,
            route_mode: rflux_route::RouteMode::Jtl,
            route_length_um: 10.0,
            cell_delay_ps: 2.0,
            wire_delay_ps: 1.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: 0.0,
            launch_window_end_ps: 4.0,
            capture_window_start_ps: 2.0,
            capture_window_end_ps: 6.0,
            arrival_phase_offset_ps: 0.0,
            capture_window_slack_ps: 3.0,
            capture_window_violation: false,
            arrival_ps: 3.0,
            required_ps: 8.0,
            setup_slack_ps: 5.0,
            hold_slack_ps: 3.0,
            pulse_envelope: None,
            pulse_degradation_violation: false,
            ocv_early_arrival_ps: None,
            ocv_late_arrival_ps: None,
            ocv_early_slack_ps: None,
            ocv_late_slack_ps: None,
            reflection_margin: 0.0,
            has_reflection_risk: false,
        }],
        worst_setup_slack_ps: 5.0,
        worst_hold_slack_ps: 3.0,
        total_negative_setup_slack_ps: 0.0,
        total_negative_hold_slack_ps: 0.0,
        critical_path_delay_ps: 3.0,
        setup_violations: 0,
        hold_violations: 0,
        capture_window_violations: 0,
        analyzed_arcs: 1,
        false_path_arcs: 0,
        extraction_report: None,
        noise_margin: None,
        path_report: None,
    };

    let report = verifier
        .verify(
            &netlist,
            &timing,
            &rflux_timing::TimingConfig::default(),
            &TimingFunctionalConfig::default(),
        )
        .unwrap();

    assert!(report.passed);
    assert_eq!(report.functional_violations.len(), 0);
    assert_eq!(report.total_arcs_checked, 1);
}

#[test]
fn timing_functional_verifier_detects_pulse_misalignment() {
    let verifier = TimingFunctionalVerifier::new();
    let netlist = Netlist::new();

    let from = PinRef {
        node: rflux_ir::NodeId(0),
        port: 0,
    };
    let to = PinRef {
        node: rflux_ir::NodeId(1),
        port: 0,
    };

    // Launch window [0, 4] does NOT overlap capture window [10, 14]
    let timing = rflux_timing::TimingReport {
        arcs: vec![rflux_timing::TimingArcReport {
            from,
            to,
            is_false_path: false,
            driver_kind: rflux_tech::SfCellKind::GenericGate,
            route_mode: rflux_route::RouteMode::Jtl,
            route_length_um: 10.0,
            cell_delay_ps: 2.0,
            wire_delay_ps: 1.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: 0.0,
            launch_window_end_ps: 4.0,
            capture_window_start_ps: 10.0,
            capture_window_end_ps: 14.0,
            arrival_phase_offset_ps: 0.0,
            capture_window_slack_ps: -6.0,
            capture_window_violation: true,
            arrival_ps: 3.0,
            required_ps: 12.0,
            setup_slack_ps: 9.0,
            hold_slack_ps: 3.0,
            pulse_envelope: None,
            pulse_degradation_violation: false,
            ocv_early_arrival_ps: None,
            ocv_late_arrival_ps: None,
            ocv_early_slack_ps: None,
            ocv_late_slack_ps: None,
            reflection_margin: 0.0,
            has_reflection_risk: false,
        }],
        worst_setup_slack_ps: 9.0,
        worst_hold_slack_ps: 3.0,
        total_negative_setup_slack_ps: 0.0,
        total_negative_hold_slack_ps: 0.0,
        critical_path_delay_ps: 3.0,
        setup_violations: 0,
        hold_violations: 0,
        capture_window_violations: 1,
        analyzed_arcs: 1,
        false_path_arcs: 0,
        extraction_report: None,
        noise_margin: None,
        path_report: None,
    };

    let report = verifier
        .verify(
            &netlist,
            &timing,
            &rflux_timing::TimingConfig::default(),
            &TimingFunctionalConfig::default(),
        )
        .unwrap();

    assert!(!report.passed);
    assert!(report.pulse_alignment_violations > 0);
}

#[test]
fn timing_functional_verifier_skips_false_paths() {
    let verifier = TimingFunctionalVerifier::new();
    let netlist = Netlist::new();

    let from = PinRef {
        node: rflux_ir::NodeId(0),
        port: 0,
    };
    let to = PinRef {
        node: rflux_ir::NodeId(1),
        port: 0,
    };

    let mut arc = rflux_timing::TimingArcReport {
        from,
        to,
        is_false_path: true, // Marked as false path
        driver_kind: rflux_tech::SfCellKind::GenericGate,
        route_mode: rflux_route::RouteMode::Jtl,
        route_length_um: 10.0,
        cell_delay_ps: 2.0,
        wire_delay_ps: 1.0,
        launch_phase: 0,
        capture_phase: 0,
        launch_window_start_ps: 0.0,
        launch_window_end_ps: 4.0,
        capture_window_start_ps: 10.0,
        capture_window_end_ps: 14.0,
        arrival_phase_offset_ps: 0.0,
        capture_window_slack_ps: -6.0,
        capture_window_violation: true,
        arrival_ps: 3.0,
        required_ps: 12.0,
        setup_slack_ps: 9.0,
        hold_slack_ps: -2.0,
        pulse_envelope: None,
        pulse_degradation_violation: false,
        ocv_early_arrival_ps: None,
        ocv_late_arrival_ps: None,
        ocv_early_slack_ps: None,
        ocv_late_slack_ps: None,
        reflection_margin: 0.0,
        has_reflection_risk: false,
    };

    let timing = rflux_timing::TimingReport {
        arcs: vec![arc],
        worst_setup_slack_ps: 9.0,
        worst_hold_slack_ps: -2.0,
        total_negative_setup_slack_ps: 0.0,
        total_negative_hold_slack_ps: 2.0,
        critical_path_delay_ps: 3.0,
        setup_violations: 0,
        hold_violations: 1,
        capture_window_violations: 1,
        analyzed_arcs: 0,
        false_path_arcs: 1,
        extraction_report: None,
        noise_margin: None,
        path_report: None,
    };

    let report = verifier
        .verify(
            &netlist,
            &timing,
            &rflux_timing::TimingConfig::default(),
            &TimingFunctionalConfig::default(),
        )
        .unwrap();

    // False paths should be skipped, so no violations
    assert!(report.passed);
}

#[test]
fn timing_functional_verifier_configurable_hold_check() {
    let verifier = TimingFunctionalVerifier::new();
    let netlist = Netlist::new();

    let from = PinRef {
        node: rflux_ir::NodeId(0),
        port: 0,
    };
    let to = PinRef {
        node: rflux_ir::NodeId(1),
        port: 0,
    };

    let timing = rflux_timing::TimingReport {
        arcs: vec![rflux_timing::TimingArcReport {
            from,
            to,
            is_false_path: false,
            driver_kind: rflux_tech::SfCellKind::GenericGate,
            route_mode: rflux_route::RouteMode::Jtl,
            route_length_um: 10.0,
            cell_delay_ps: 2.0,
            wire_delay_ps: 1.0,
            launch_phase: 0,
            capture_phase: 0,
            launch_window_start_ps: 0.0,
            launch_window_end_ps: 4.0,
            capture_window_start_ps: 2.0,
            capture_window_end_ps: 6.0,
            arrival_phase_offset_ps: 0.0,
            capture_window_slack_ps: 3.0,
            capture_window_violation: false,
            arrival_ps: 3.0,
            required_ps: 8.0,
            setup_slack_ps: 5.0,
            hold_slack_ps: -2.0, // Hold violation
            pulse_envelope: None,
            pulse_degradation_violation: false,
            ocv_early_arrival_ps: None,
            ocv_late_arrival_ps: None,
            ocv_early_slack_ps: None,
            ocv_late_slack_ps: None,
            reflection_margin: 0.0,
            has_reflection_risk: false,
        }],
        worst_setup_slack_ps: 5.0,
        worst_hold_slack_ps: -2.0,
        total_negative_setup_slack_ps: 0.0,
        total_negative_hold_slack_ps: 2.0,
        critical_path_delay_ps: 3.0,
        setup_violations: 0,
        hold_violations: 1,
        capture_window_violations: 0,
        analyzed_arcs: 1,
        false_path_arcs: 0,
        extraction_report: None,
        noise_margin: None,
        path_report: None,
    };

    // With hold check disabled
    let config_no_hold = TimingFunctionalConfig {
        hold_as_functional: false,
        ..TimingFunctionalConfig::default()
    };
    let report = verifier
        .verify(&netlist, &timing, &rflux_timing::TimingConfig::default(), &config_no_hold)
        .unwrap();
    assert!(report.passed);

    // With hold check enabled
    let config_with_hold = TimingFunctionalConfig::default();
    let report = verifier
        .verify(&netlist, &timing, &rflux_timing::TimingConfig::default(), &config_with_hold)
        .unwrap();
    assert!(!report.passed);
    assert_eq!(report.hold_functional_violations, 1);
}
