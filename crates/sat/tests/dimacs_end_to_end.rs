use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use rflux_sat::{solve_with_metrics, CnfFormula, SolveResult};

#[derive(Debug, Clone)]
struct DimacsCase {
    file_name: &'static str,
    expect_sat: bool,
}

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(file_name)
}

fn metrics_csv_path() -> PathBuf {
    if let Ok(custom_path) = env::var("RFLUX_DIMACS_METRICS_CSV") {
        return PathBuf::from(custom_path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("dimacs_sat_metrics.csv")
}

fn write_metrics_csv(
    rows: &[(String, bool, usize, usize, u128, usize, usize, usize, usize)],
    total_recursive_calls: usize,
    total_decisions: usize,
    total_backtracks: usize,
    total_restarts: usize,
    max_elapsed_ns: u128,
) -> io::Result<PathBuf> {
    let output_path = metrics_csv_path();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(&output_path)?;
    writeln!(
        file,
        "fixture,sat,vars,clauses,elapsed_ns,recursive_calls,decisions,backtracks,restarts"
    )?;
    for (
        fixture,
        sat,
        vars,
        clauses,
        elapsed_ns,
        recursive_calls,
        decisions,
        backtracks,
        restarts,
    ) in rows
    {
        writeln!(
            file,
            "{fixture},{sat},{vars},{clauses},{elapsed_ns},{recursive_calls},{decisions},{backtracks},{restarts}"
        )?;
    }
    writeln!(
        file,
        "summary,true,0,0,{max_elapsed_ns},{total_recursive_calls},{total_decisions},{total_backtracks},{total_restarts}"
    )?;

    Ok(output_path)
}

#[test]
fn dimacs_classic_examples_end_to_end() {
    let cases = vec![
        DimacsCase {
            file_name: "sat_3var_implication.cnf",
            expect_sat: true,
        },
        DimacsCase {
            file_name: "sat_wrapped_multiclause.cnf",
            expect_sat: true,
        },
        DimacsCase {
            file_name: "sat_exactly_one_4.cnf",
            expect_sat: true,
        },
        DimacsCase {
            file_name: "unsat_unit_contradiction.cnf",
            expect_sat: false,
        },
        DimacsCase {
            file_name: "unsat_pigeonhole_3_2.cnf",
            expect_sat: false,
        },
    ];

    let mut total_recursive_calls = 0usize;
    let mut total_decisions = 0usize;
    let mut total_backtracks = 0usize;
    let mut total_restarts = 0usize;
    let mut max_elapsed_ns = 0u128;
    let mut csv_rows: Vec<(String, bool, usize, usize, u128, usize, usize, usize, usize)> =
        Vec::new();

    for case in cases {
        let path = fixture_path(case.file_name);
        let raw = fs::read_to_string(&path).expect("dimacs fixture should be readable");
        let cnf = CnfFormula::from_dimacs(&raw).expect("fixture should parse as valid dimacs");
        let (result, metrics) = solve_with_metrics(&cnf);

        let sat = matches!(result, SolveResult::Satisfiable(_));
        assert_eq!(
            sat, case.expect_sat,
            "unexpected SAT result for fixture {}",
            case.file_name
        );
        assert!(
            metrics.stats.recursive_calls >= 1,
            "recursive call metrics missing for fixture {}",
            case.file_name
        );
        assert!(
            metrics.elapsed_ns > 0,
            "elapsed metrics missing for fixture {}",
            case.file_name
        );

        total_recursive_calls += metrics.stats.recursive_calls;
        total_decisions += metrics.stats.decisions;
        total_backtracks += metrics.stats.backtracks;
        total_restarts += metrics.stats.restarts;
        max_elapsed_ns = max_elapsed_ns.max(metrics.elapsed_ns);
        csv_rows.push((
            case.file_name.to_string(),
            sat,
            cnf.var_count(),
            cnf.clauses().len(),
            metrics.elapsed_ns,
            metrics.stats.recursive_calls,
            metrics.stats.decisions,
            metrics.stats.backtracks,
            metrics.stats.restarts,
        ));

        println!(
            "dimacs_fixture={} sat={} vars={} clauses={} elapsed_ns={} recursive_calls={} decisions={} backtracks={} restarts={}",
            case.file_name,
            sat,
            cnf.var_count(),
            cnf.clauses().len(),
            metrics.elapsed_ns,
            metrics.stats.recursive_calls,
            metrics.stats.decisions,
            metrics.stats.backtracks,
            metrics.stats.restarts,
        );
    }

    let csv_path = write_metrics_csv(
        &csv_rows,
        total_recursive_calls,
        total_decisions,
        total_backtracks,
        total_restarts,
        max_elapsed_ns,
    )
    .expect("dimacs metrics csv should be writable");

    println!(
        "dimacs_sat_summary total_recursive_calls={} total_decisions={} total_backtracks={} total_restarts={} max_elapsed_ns={}",
        total_recursive_calls,
        total_decisions,
        total_backtracks,
        total_restarts,
        max_elapsed_ns,
    );
    println!("dimacs_sat_metrics_csv path={}", csv_path.display());
}
