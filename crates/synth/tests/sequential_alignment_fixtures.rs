use std::path::PathBuf;

use rflux_io::read_ir_json;
use rflux_ir::{LogicOp, NodeKind};
use rflux_synth::{BoolOptConfig, Compiler};
use rflux_verify::Verifier;

#[derive(Debug, Clone)]
struct SequentialCase {
    file_name: &'static str,
    expected_before: usize,
    expected_after: usize,
    expected_dffe: usize,
    expected_not: usize,
    expected_jtl: usize,
    expected_ptl: usize,
    expected_mux: usize,
}

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("quaigh_alignment")
        .join(file_name)
}

#[test]
fn quaigh_alignment_sequential_fixture_cases() {
    let verifier = Verifier::new();
    let cases = vec![
        SequentialCase {
            file_name: "dffe_feedback_wrapped.json",
            expected_before: 1,
            expected_after: 0,
            expected_dffe: 1,
            expected_not: 0,
            expected_jtl: 0,
            expected_ptl: 0,
            expected_mux: 0,
        },
        SequentialCase {
            file_name: "dffe_clock_wrapped.json",
            expected_before: 1,
            expected_after: 0,
            expected_dffe: 1,
            expected_not: 0,
            expected_jtl: 0,
            expected_ptl: 0,
            expected_mux: 0,
        },
        SequentialCase {
            file_name: "dffe_inverted_wrapped.json",
            expected_before: 1,
            expected_after: 1,
            expected_dffe: 1,
            expected_not: 1,
            expected_jtl: 0,
            expected_ptl: 0,
            expected_mux: 0,
        },
        SequentialCase {
            file_name: "dffe_inverted_clock_wrapped.json",
            expected_before: 1,
            expected_after: 1,
            expected_dffe: 1,
            expected_not: 1,
            expected_jtl: 0,
            expected_ptl: 0,
            expected_mux: 0,
        },
    ];

    for case in cases {
        let path = fixture_path(case.file_name);
        let mut netlist =
            read_ir_json(&path).expect("sequential fixture should load as valid rflux ir json");
        let baseline = netlist.clone();
        let mut compiler = Compiler::new();

        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(
            report.gate_count_before, case.expected_before,
            "unexpected pre-opt gate count for sequential fixture {}",
            case.file_name
        );
        assert_eq!(
            report.gate_count_after, case.expected_after,
            "unexpected post-opt gate count for sequential fixture {}",
            case.file_name
        );

        let dffe_count = netlist
            .nodes()
            .iter()
            .filter(|node| {
                matches!(node.kind, NodeKind::Dff) && node.logic_op == Some(LogicOp::DffEnable)
            })
            .count();
        let not_count = netlist
            .nodes()
            .iter()
            .filter(|node| node.logic_op == Some(LogicOp::Not))
            .count();
        let jtl_count = netlist
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::Jtl))
            .count();
        let ptl_count = netlist
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::Ptl))
            .count();
        let mux_count = netlist
            .nodes()
            .iter()
            .filter(|node| node.logic_op == Some(LogicOp::Mux2))
            .count();

        assert_eq!(
            dffe_count, case.expected_dffe,
            "unexpected DffEnable count for sequential fixture {}",
            case.file_name
        );
        assert_eq!(
            not_count, case.expected_not,
            "unexpected Not count for sequential fixture {}",
            case.file_name
        );
        assert_eq!(
            jtl_count, case.expected_jtl,
            "unexpected Jtl count for sequential fixture {}",
            case.file_name
        );
        assert_eq!(
            ptl_count, case.expected_ptl,
            "unexpected Ptl count for sequential fixture {}",
            case.file_name
        );
        assert_eq!(
            mux_count, case.expected_mux,
            "unexpected Mux2 count for sequential fixture {}",
            case.file_name
        );

        let eq = verifier
            .check_single_step_sequential_equivalence(&baseline, &netlist)
            .expect("sequential equivalence should succeed for sequential fixture");
        assert!(
            eq.equivalent,
            "optimized sequential fixture is not equivalent to baseline: {}",
            case.file_name
        );
        assert!(
            eq.sat_stats.decisions + eq.sat_stats.unit_assignments >= 1,
            "sequential sat stats should be populated for fixture {}",
            case.file_name
        );
        assert!(
            eq.sat_elapsed_ns > 0,
            "sequential sat elapsed time should be populated for fixture {}",
            case.file_name
        );
    }
}
