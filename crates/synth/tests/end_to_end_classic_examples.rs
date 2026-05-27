use std::path::PathBuf;

use rflux_io::read_ir_json;
use rflux_synth::{BoolOptConfig, Compiler};

#[derive(Debug, Clone)]
struct ClassicCase {
    file_name: &'static str,
    expected_before: usize,
    expected_after: Option<usize>,
}

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("classic_examples")
        .join(file_name)
}

#[test]
fn classic_examples_end_to_end_sat_equivalence() {
    let cases = vec![
        ClassicCase {
            file_name: "classic_and8_chain.json",
            expected_before: 7,
            expected_after: Some(1),
        },
        ClassicCase {
            file_name: "classic_xor8_chain.json",
            expected_before: 7,
            expected_after: Some(1),
        },
        ClassicCase {
            file_name: "classic_mux4_tree.json",
            expected_before: 3,
            expected_after: Some(3),
        },
        ClassicCase {
            file_name: "classic_mux8_tree.json",
            expected_before: 7,
            expected_after: Some(7),
        },
        ClassicCase {
            file_name: "classic_majority3.json",
            expected_before: 5,
            expected_after: None,
        },
        ClassicCase {
            file_name: "classic_dual_product4.json",
            expected_before: 6,
            expected_after: Some(1),
        },
        ClassicCase {
            file_name: "classic_full_adder.json",
            expected_before: 5,
            expected_after: None,
        },
        ClassicCase {
            file_name: "classic_ripple_adder4.json",
            expected_before: 20,
            expected_after: None,
        },
    ];

    for case in cases {
        let path = fixture_path(case.file_name);
        let mut netlist = read_ir_json(&path).expect("classic fixture should load");
        let baseline = netlist.clone();

        let mut compiler = Compiler::new();
        let report = compiler.optimize_boolean_network(&mut netlist, &BoolOptConfig::default());

        assert_eq!(
            report.gate_count_before, case.expected_before,
            "unexpected pre-opt gate count for fixture {}",
            case.file_name
        );
        assert!(
            report.gate_count_after <= report.gate_count_before,
            "optimization should not increase gate count for fixture {}",
            case.file_name
        );

        if let Some(expected_after) = case.expected_after {
            assert_eq!(
                report.gate_count_after, expected_after,
                "unexpected post-opt gate count for fixture {}",
                case.file_name
            );
        }

        let eq = compiler
            .check_boolean_equivalence_sat(&baseline, &netlist)
            .expect("sat equivalence should succeed for classic fixture");
        assert!(
            eq.equivalent,
            "optimized classic fixture is not equivalent to baseline: {}",
            case.file_name
        );
        assert!(
            eq.sat_stats.recursive_calls >= 1,
            "sat stats should be populated for fixture {}",
            case.file_name
        );
        assert!(
            eq.sat_elapsed_ns > 0,
            "sat elapsed time should be populated for fixture {}",
            case.file_name
        );

        println!(
            "classic_fixture={} gates_before={} gates_after={} sat_elapsed_ns={} recursive_calls={} decisions={} backtracks={} restarts={}",
            case.file_name,
            report.gate_count_before,
            report.gate_count_after,
            eq.sat_elapsed_ns,
            eq.sat_stats.recursive_calls,
            eq.sat_stats.decisions,
            eq.sat_stats.backtracks,
            eq.sat_stats.restarts,
        );
    }
}
