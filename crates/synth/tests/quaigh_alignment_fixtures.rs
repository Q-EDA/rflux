use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use rflux_io::read_ir_json;
use rflux_synth::{BoolOptConfig, Compiler};
use rflux_verify::Verifier;

#[derive(Debug, Clone)]
struct FixtureCase {
    file_name: &'static str,
    expected_before: usize,
    expected_after: usize,
    config: BoolOptConfig,
}

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("quaigh_alignment")
        .join(file_name)
}

fn fixture_metrics_csv_path() -> PathBuf {
    if let Ok(custom_path) = env::var("RFLUX_QUAIGH_METRICS_CSV") {
        return PathBuf::from(custom_path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("quaigh_fixture_sat_metrics.csv")
}

fn write_fixture_metrics_csv(
    rows: &[(String, u128, usize, usize, usize, usize, usize, usize)],
    total_recursive_calls: usize,
    total_decisions: usize,
    total_backtracks: usize,
    total_restarts: usize,
    max_elapsed_ns: u128,
) -> io::Result<PathBuf> {
    let output_path = fixture_metrics_csv_path();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(&output_path)?;
    writeln!(
        file,
        "fixture,sat_elapsed_ns,recursive_calls,decisions,unit_assignments,pure_literal_assignments,backtracks,restarts"
    )?;
    for (fixture, sat_elapsed_ns, recursive_calls, decisions, unit_assignments, pure_literal_assignments, backtracks, restarts) in rows {
        writeln!(
            file,
            "{fixture},{sat_elapsed_ns},{recursive_calls},{decisions},{unit_assignments},{pure_literal_assignments},{backtracks},{restarts}"
        )?;
    }
    writeln!(
        file,
        "summary,{max_elapsed_ns},{total_recursive_calls},{total_decisions},0,0,{total_backtracks},{total_restarts}"
    )?;

    Ok(output_path)
}

#[test]
fn quaigh_alignment_fixture_cases() {
    let cases = vec![
        FixtureCase {
            file_name: "dedup_and_pair_from_bench.json",
            expected_before: 2,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "dedup_and_pair.json",
            expected_before: 2,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "flatten_and_deep.json",
            expected_before: 3,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "factor_or_of_and_common_term.json",
            expected_before: 3,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "factor_and_of_or_common_term.json",
            expected_before: 3,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "absorb_and_subset.json",
            expected_before: 3,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "absorb_or_subset.json",
            expected_before: 3,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "xor_from_and_pattern.json",
            expected_before: 5,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "mux_from_and_pattern.json",
            expected_before: 4,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "xor_from_or_pattern.json",
            expected_before: 5,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "mux_from_or_pattern.json",
            expected_before: 4,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "factor_then_xor_from_and_pattern.json",
            expected_before: 5,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "factor_then_mux_from_and_pattern.json",
            expected_before: 4,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "factor_then_xor_from_or_pattern.json",
            expected_before: 5,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "factor_then_mux_from_or_pattern.json",
            expected_before: 4,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "consensus_or_redundancy.json",
            expected_before: 5,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "consensus_and_redundancy.json",
            expected_before: 5,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "xor3_chain_from_bench.json",
            expected_before: 2,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi31_from_bench.json",
            expected_before: 4,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai31_from_bench.json",
            expected_before: 4,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi211_from_bench.json",
            expected_before: 4,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai211_from_bench.json",
            expected_before: 4,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi311_from_bench.json",
            expected_before: 5,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai311_from_bench.json",
            expected_before: 5,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi321_from_bench.json",
            expected_before: 6,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai321_from_bench.json",
            expected_before: 6,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi322_from_bench.json",
            expected_before: 6,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai322_from_bench.json",
            expected_before: 6,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi421_from_bench.json",
            expected_before: 7,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai421_from_bench.json",
            expected_before: 7,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi422_from_bench.json",
            expected_before: 8,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai422_from_bench.json",
            expected_before: 8,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi431_from_bench.json",
            expected_before: 8,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai431_from_bench.json",
            expected_before: 8,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi432_from_bench.json",
            expected_before: 9,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai432_from_bench.json",
            expected_before: 9,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi433_from_bench.json",
            expected_before: 10,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai433_from_bench.json",
            expected_before: 10,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi441_from_bench.json",
            expected_before: 9,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai441_from_bench.json",
            expected_before: 9,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi442_from_bench.json",
            expected_before: 10,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai442_from_bench.json",
            expected_before: 10,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi443_from_bench.json",
            expected_before: 11,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai443_from_bench.json",
            expected_before: 11,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi444_from_bench.json",
            expected_before: 12,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai444_from_bench.json",
            expected_before: 12,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi2221_from_bench.json",
            expected_before: 7,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai2221_from_bench.json",
            expected_before: 7,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi222_from_bench.json",
            expected_before: 6,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai222_from_bench.json",
            expected_before: 6,
            expected_after: 5,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi221_from_bench.json",
            expected_before: 5,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai221_from_bench.json",
            expected_before: 5,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi22_from_bench.json",
            expected_before: 4,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai22_from_bench.json",
            expected_before: 4,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "aoi21_from_bench.json",
            expected_before: 3,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "oai21_from_bench.json",
            expected_before: 3,
            expected_after: 3,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "majority3_from_bench.json",
            expected_before: 5,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "andn4_chain_from_bench.json",
            expected_before: 3,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "nand_nor_pair_from_bench.json",
            expected_before: 4,
            expected_after: 4,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "iscas_c17_from_bench.json",
            expected_before: 12,
            expected_after: 12,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "xorn4_chain_from_bench.json",
            expected_before: 3,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "xnor_pair_from_bench.json",
            expected_before: 2,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "mux_data_order_distinct.json",
            expected_before: 2,
            expected_after: 2,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "xor_toggle_pair_enabled.json",
            expected_before: 2,
            expected_after: 1,
            config: BoolOptConfig::default(),
        },
        FixtureCase {
            file_name: "xor_toggle_pair.json",
            expected_before: 2,
            expected_after: 2,
            config: BoolOptConfig {
                share_logic_flattening_limit: 8,
                infer_xor_mux: false,
                infer_dffe: true,
            },
        },
    ];

    let mut total_decisions = 0usize;
    let mut total_backtracks = 0usize;
    let mut total_restarts = 0usize;
    let mut total_recursive_calls = 0usize;
    let mut max_elapsed_ns = 0u128;
    let mut csv_rows: Vec<(String, u128, usize, usize, usize, usize, usize, usize)> = Vec::new();
    let verifier = Verifier::new();

    for case in cases {
        let path = fixture_path(case.file_name);
        let mut netlist = read_ir_json(&path).expect("fixture should load as valid rflux ir json");
        let baseline = netlist.clone();
        let mut compiler = Compiler::new();

        let report = compiler.optimize_boolean_network(&mut netlist, &case.config);

        assert_eq!(
            report.gate_count_before,
            case.expected_before,
            "unexpected pre-opt gate count for fixture {}",
            case.file_name
        );
        assert_eq!(
            report.gate_count_after,
            case.expected_after,
            "unexpected post-opt gate count for fixture {}",
            case.file_name
        );

        let eq = verifier
            .check_boolean_equivalence(&baseline, &netlist)
            .expect("sat equivalence check should succeed for fixture optimization");
        assert!(
            eq.equivalent,
            "optimized fixture is not equivalent to baseline: {}",
            case.file_name
        );

        total_decisions += eq.sat_stats.decisions;
        total_backtracks += eq.sat_stats.backtracks;
        total_restarts += eq.sat_stats.restarts;
        total_recursive_calls += eq.sat_stats.recursive_calls;
        max_elapsed_ns = max_elapsed_ns.max(eq.sat_elapsed_ns);
        assert!(eq.sat_stats.recursive_calls >= 1, "sat stats missing recursive calls for fixture {}", case.file_name);

        println!(
            "fixture={} sat_elapsed_ns={} recursive_calls={} decisions={} unit_assignments={} pure_literal_assignments={} backtracks={} restarts={}",
            case.file_name,
            eq.sat_elapsed_ns,
            eq.sat_stats.recursive_calls,
            eq.sat_stats.decisions,
            eq.sat_stats.unit_assignments,
            eq.sat_stats.pure_literal_assignments,
            eq.sat_stats.backtracks,
            eq.sat_stats.restarts,
        );

        csv_rows.push((
            case.file_name.to_string(),
            eq.sat_elapsed_ns,
            eq.sat_stats.recursive_calls,
            eq.sat_stats.decisions,
            eq.sat_stats.unit_assignments,
            eq.sat_stats.pure_literal_assignments,
            eq.sat_stats.backtracks,
            eq.sat_stats.restarts,
        ));
    }

    let csv_path = write_fixture_metrics_csv(
        &csv_rows,
        total_recursive_calls,
        total_decisions,
        total_backtracks,
        total_restarts,
        max_elapsed_ns,
    )
    .expect("fixture sat metrics csv should be writable");

    println!(
        "quaigh_fixture_sat_summary total_recursive_calls={} total_decisions={} total_backtracks={} total_restarts={} max_elapsed_ns={}",
        total_recursive_calls,
        total_decisions,
        total_backtracks,
        total_restarts,
        max_elapsed_ns,
    );
    println!("quaigh_fixture_sat_metrics_csv path={}", csv_path.display());
}
