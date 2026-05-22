use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::BTreeMap, env};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rflux_flow::{FlowConfig, FlowRunner, SimulationConfig, SimulationMode};
use rflux_io::{read_ir_json, read_pdk_json, write_pdk_json, IoError};
use rflux_sat::{solve_with_metrics, CnfFormula, IncrementalSolver, Lit, SolveResult, SolveStats};
use rflux_sim::{simulate_file, SimulationReport};
use rflux_tech::Pdk;
use rflux_verify::Verifier;
use serde_json::{json, Value};

const CLI_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Parser)]
#[command(name = "rflux", about = "rflux CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    PdkMinimal(PdkMinimalArgs),
    LintInput(LintInputArgs),
    CollectDiagnostics(CollectDiagnosticsArgs),
    CompileNetlist(CompileNetlistArgs),
    CompileLayout(LayoutCommandArgs),
    AnalyzeTiming(LayoutCommandArgs),
    VerifyLayout(VerifyLayoutArgs),
    SimulateFile(SimulateFileArgs),
    SolveDimacs(SolveDimacsArgs),
    CheckEquivalence(CheckEquivalenceArgs),
}

#[derive(Debug, Args)]
struct PdkMinimalArgs {
    #[arg(long, default_value = "minimal-sfq")]
    name: String,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct LintInputArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum)]
    kind: CliInputKind,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct CollectDiagnosticsArgs {
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long)]
    command: Option<String>,
    #[arg(long)]
    input: Option<PathBuf>,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, value_enum)]
    mode: Option<CliSimulationMode>,
    #[arg(long)]
    external_command: Option<String>,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Args)]
struct CompileNetlistArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    netlist_output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct LayoutCommandArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct VerifyLayoutArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = CliSimulationMode::Auto)]
    mode: CliSimulationMode,
    #[arg(long)]
    external_command: Option<String>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct SimulateFileArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum, default_value_t = CliSimulationMode::Auto)]
    mode: CliSimulationMode,
    #[arg(long)]
    external_command: Option<String>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct SolveDimacsArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    assumptions: Option<String>,
    #[arg(long)]
    equivalence_metadata: Option<PathBuf>,
    #[arg(long)]
    check_ref: Option<String>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct CheckEquivalenceArgs {
    #[arg(long)]
    lhs: PathBuf,
    #[arg(long)]
    rhs: PathBuf,
    #[arg(long, value_enum, default_value_t = CliEquivalenceKind::Combinational)]
    kind: CliEquivalenceKind,
    #[arg(long)]
    dimacs_output: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliSimulationMode {
    #[value(name = "auto")]
    Auto,
    #[value(name = "event_only", alias = "event-only")]
    EventOnly,
    #[value(name = "external_josim", alias = "external-josim")]
    ExternalJosim,
    #[value(name = "internal_transient", alias = "internal-transient")]
    InternalTransient,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliEquivalenceKind {
    #[value(name = "combinational")]
    Combinational,
    #[value(name = "single_step_sequential", alias = "single-step-sequential")]
    SingleStepSequential,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliInputKind {
    #[value(name = "ir")]
    Ir,
    #[value(name = "pdk")]
    Pdk,
}

fn main() {
    let cli = Cli::parse();

    if let Err(error) = run(cli) {
        eprintln!("{}", render_cli_error(&error));
        std::process::exit(1);
    }
}

fn render_cli_error(error: &anyhow::Error) -> String {
    if let Some(io_error) = find_io_error(error) {
        return format!(
            "error[{}]: {}\n  detail: {}\n  next: {}",
            io_error.code(),
            error,
            io_error,
            io_error.suggestion()
        );
    }

    format!("error: {error:#}")
}

fn find_io_error(error: &anyhow::Error) -> Option<&IoError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<IoError>())
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::PdkMinimal(args) => run_pdk_minimal(args),
        Commands::LintInput(args) => run_lint_input(args),
        Commands::CollectDiagnostics(args) => run_collect_diagnostics(args),
        Commands::CompileNetlist(args) => run_compile_netlist(args),
        Commands::CompileLayout(args) => run_compile_layout(args),
        Commands::AnalyzeTiming(args) => run_analyze_timing(args),
        Commands::VerifyLayout(args) => run_verify_layout(args),
        Commands::SimulateFile(args) => run_simulate_file(args),
        Commands::SolveDimacs(args) => run_solve_dimacs(args),
        Commands::CheckEquivalence(args) => run_check_equivalence(args),
    }
}

fn run_pdk_minimal(args: PdkMinimalArgs) -> Result<()> {
    let pdk = Pdk::minimal(args.name);

    if let Some(output_path) = args.output.as_deref() {
        write_pdk_json(output_path, &pdk)
            .with_context(|| format!("failed to write PDK JSON to {}", output_path.display()))?;
        return Ok(());
    }

    println!("{}", pdk.to_json().context("failed to serialize minimal PDK")?);
    Ok(())
}

fn run_lint_input(args: LintInputArgs) -> Result<()> {
    let report = match args.kind {
        CliInputKind::Ir => {
            read_ir_json(&args.input)
                .with_context(|| format!("failed to validate IR JSON from {}", args.input.display()))?;
            lint_input_report(&args.input, "ir", "rflux_ir_netlist")?
        }
        CliInputKind::Pdk => {
            read_pdk_json(&args.input)
                .with_context(|| format!("failed to validate PDK JSON from {}", args.input.display()))?;
            lint_input_report(&args.input, "pdk", "rflux_pdk")?
        }
    };

    emit_json(&with_schema_version(report), args.output.as_deref())
}

fn run_collect_diagnostics(args: CollectDiagnosticsArgs) -> Result<()> {
    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create diagnostics directory {}", args.output_dir.display()))?;
    let inputs_dir = args.output_dir.join("inputs");
    fs::create_dir_all(&inputs_dir)
        .with_context(|| format!("failed to create diagnostics inputs directory {}", inputs_dir.display()))?;
    let event_log_path = args.output_dir.join("events.jsonl");
    let mut event_log = Vec::new();
    event_log.push(diagnostics_event(
        "bundle_started",
        json!({
            "output_dir": display_path(&args.output_dir),
        }),
    )?);

    let mut captured_inputs = Vec::new();
    if let Some(input) = args.input.as_deref() {
        captured_inputs.push(capture_diagnostics_input(&inputs_dir, "input", input)?);
        event_log.push(diagnostics_event(
            "input_captured",
            json!({
                "role": "input",
                "source_path": input.display().to_string(),
            }),
        )?);
    }
    if let Some(pdk) = args.pdk.as_deref() {
        captured_inputs.push(capture_diagnostics_input(&inputs_dir, "pdk", pdk)?);
        event_log.push(diagnostics_event(
            "input_captured",
            json!({
                "role": "pdk",
                "source_path": pdk.display().to_string(),
            }),
        )?);
    }
    let mut captured_reports = Vec::new();
    if let Some(report) = args.report.as_deref() {
        let reports_dir = args.output_dir.join("reports");
        fs::create_dir_all(&reports_dir)
            .with_context(|| format!("failed to create diagnostics reports directory {}", reports_dir.display()))?;
        captured_reports.push(capture_diagnostics_report(&reports_dir, report)?);
        event_log.push(diagnostics_event(
            "report_captured",
            json!({
                "source_path": report.display().to_string(),
            }),
        )?);
    }
    let summary = build_diagnostics_summary(args.command.as_deref(), &captured_inputs, &captured_reports);
    let configuration = build_diagnostics_configuration(&args);
    event_log.push(diagnostics_event(
        "manifest_prepared",
        json!({
            "captured_input_count": captured_inputs.len(),
            "captured_report_count": captured_reports.len(),
        }),
    )?);
    write_diagnostics_event_log(&event_log_path, &event_log)?;

    let manifest = with_schema_version(json!({
        "kind": "diagnostics_bundle",
        "bundle_version": 1,
        "created_at_unix_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before unix epoch")?
            .as_millis(),
        "tool": {
            "name": "rflux",
            "version": env!("CARGO_PKG_VERSION"),
            "cli_schema_version": CLI_SCHEMA_VERSION,
        },
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
        },
        "invocation": {
            "command": args.command,
            "working_directory": env::current_dir()
                .context("failed to read current working directory")?
                .display()
                .to_string(),
            "mode": args.mode.map(simulation_mode_name),
            "external_command": args.external_command,
            "notes": args.notes,
        },
        "environment": collect_diagnostics_environment(),
        "configuration": configuration,
        "summary": summary,
        "structured_logs": {
            "events_path": display_path(&event_log_path),
            "event_count": event_log.len(),
            "format": "jsonl",
        },
        "captured_inputs": captured_inputs,
        "captured_reports": captured_reports,
    }));

    emit_json(&manifest, Some(args.output_dir.join("manifest.json").as_path()))
}

fn capture_diagnostics_input(inputs_dir: &Path, role: &str, source: &Path) -> Result<Value> {
    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow!("{role} path {} does not name a file", source.display()))?;
    let destination = inputs_dir.join(file_name);
    fs::copy(source, &destination).with_context(|| {
        format!(
            "failed to copy diagnostics input {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    let metadata = fs::metadata(source)
        .with_context(|| format!("failed to stat diagnostics input {}", source.display()))?;

    Ok(json!({
        "role": role,
        "source_path": source.display().to_string(),
        "bundle_path": destination.display().to_string(),
        "bytes": metadata.len(),
        "contract": diagnostics_contract_snapshot(role, source),
    }))
}

fn capture_diagnostics_report(reports_dir: &Path, source: &Path) -> Result<Value> {
    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow!("report path {} does not name a file", source.display()))?;
    let destination = reports_dir.join(file_name);
    fs::copy(source, &destination).with_context(|| {
        format!(
            "failed to copy diagnostics report {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    let metadata = fs::metadata(source)
        .with_context(|| format!("failed to stat diagnostics report {}", source.display()))?;

    Ok(json!({
        "source_path": source.display().to_string(),
        "bundle_path": destination.display().to_string(),
        "bytes": metadata.len(),
        "report": diagnostics_report_snapshot(source),
    }))
}

fn collect_diagnostics_environment() -> Value {
    json!({
        "rust_log": env::var("RUST_LOG").ok(),
        "rust_backtrace": env::var("RUST_BACKTRACE").ok(),
        "uv_offline": env::var("UV_OFFLINE").ok(),
        "present_prefixed_vars": collect_present_prefixed_env_var_names(),
    })
}

fn collect_present_prefixed_env_var_names() -> Value {
    let mut grouped = BTreeMap::<&'static str, Vec<String>>::new();
    grouped.insert("RFLOW_*", Vec::new());
    grouped.insert("JOSIM_*", Vec::new());

    for (name, _) in env::vars() {
        if name.starts_with("RFLOW_") {
            grouped.get_mut("RFLOW_*").expect("group should exist").push(name);
        } else if name.starts_with("JOSIM_") {
            grouped.get_mut("JOSIM_*").expect("group should exist").push(name);
        }
    }

    for names in grouped.values_mut() {
        names.sort();
    }

    json!(grouped)
}

fn build_diagnostics_configuration(args: &CollectDiagnosticsArgs) -> Value {
    json!({
        "command": args.command,
        "notes": args.notes,
        "paths": {
            "input": args.input.as_ref().map(|path| display_path(path)),
            "pdk": args.pdk.as_ref().map(|path| display_path(path)),
            "report": args.report.as_ref().map(|path| display_path(path)),
            "output_dir": display_path(&args.output_dir),
        },
        "simulation": {
            "mode": args.mode.map(simulation_mode_name),
            "external_command": args.external_command,
        },
    })
}

fn build_diagnostics_summary(
    command: Option<&str>,
    captured_inputs: &[Value],
    captured_reports: &[Value],
) -> Value {
    let mut legacy_compatibility_inputs = Vec::new();
    let mut inspection_failures = Vec::new();
    let mut report_kinds = Vec::new();
    let mut report_inspection_failures = Vec::new();

    for captured_input in captured_inputs {
        let role = captured_input
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let contract = captured_input.get("contract").unwrap_or(&Value::Null);
        if contract
            .get("legacy_compatibility_used")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            legacy_compatibility_inputs.push(role.to_string());
        }
        if let Some(error) = contract.get("inspection_error").and_then(Value::as_str) {
            inspection_failures.push(json!({
                "role": role,
                "error": error,
            }));
        }
    }

    for captured_report in captured_reports {
        let report = captured_report.get("report").unwrap_or(&Value::Null);
        if let Some(kind) = report.get("kind").and_then(Value::as_str) {
            report_kinds.push(kind.to_string());
        }
        if let Some(error) = report.get("inspection_error").and_then(Value::as_str) {
            report_inspection_failures.push(json!({
                "source_path": captured_report["source_path"].clone(),
                "error": error,
            }));
        }
    }

    json!({
        "command": command,
        "captured_input_count": captured_inputs.len(),
        "captured_report_count": captured_reports.len(),
        "legacy_compatibility_inputs": legacy_compatibility_inputs,
        "inspection_failure_count": inspection_failures.len(),
        "inspection_failures": inspection_failures,
        "report_kinds": report_kinds,
        "report_inspection_failure_count": report_inspection_failures.len(),
        "report_inspection_failures": report_inspection_failures,
    })
}

fn diagnostics_event(kind: &str, fields: Value) -> Result<Value> {
    Ok(json!({
        "timestamp_unix_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before unix epoch")?
            .as_millis(),
        "event": kind,
        "fields": fields,
    }))
}

fn write_diagnostics_event_log(path: &Path, events: &[Value]) -> Result<()> {
    let mut file = fs::File::create(path)
        .with_context(|| format!("failed to create diagnostics event log {}", path.display()))?;
    for event in events {
        let rendered = serde_json::to_string(event)
            .context("failed to serialize diagnostics event")?;
        writeln!(file, "{rendered}")
            .with_context(|| format!("failed to write diagnostics event log {}", path.display()))?;
    }
    Ok(())
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn diagnostics_contract_snapshot(role: &str, source: &Path) -> Value {
    let contract = match role {
        "input" => Some(("ir", "rflux_ir_netlist")),
        "pdk" => Some(("pdk", "rflux_pdk")),
        _ => None,
    };

    let Some((input_kind, contract_kind)) = contract else {
        return Value::Null;
    };

    match inspect_json_contract(source) {
        Ok((schema_format, input_schema_version)) => json!({
            "input_kind": input_kind,
            "contract_kind": contract_kind,
            "schema_format": schema_format,
            "input_schema_version": input_schema_version,
            "legacy_compatibility_used": schema_format == "legacy_raw_json",
            "inspection_error": Value::Null,
        }),
        Err(error) => json!({
            "input_kind": input_kind,
            "contract_kind": contract_kind,
            "schema_format": Value::Null,
            "input_schema_version": Value::Null,
            "legacy_compatibility_used": Value::Null,
            "inspection_error": error.to_string(),
        }),
    }
}

fn diagnostics_report_snapshot(source: &Path) -> Value {
    let raw = match fs::read_to_string(source) {
        Ok(raw) => raw,
        Err(error) => {
            return json!({
                "kind": Value::Null,
                "schema_version": Value::Null,
                "inspection_error": error.to_string(),
            });
        }
    };

    match serde_json::from_str::<Value>(&raw) {
        Ok(json) => json!({
            "kind": json.get("kind").and_then(Value::as_str),
            "schema_version": json.get("schema_version").and_then(Value::as_u64),
            "inspection_error": Value::Null,
        }),
        Err(error) => json!({
            "kind": Value::Null,
            "schema_version": Value::Null,
            "inspection_error": error.to_string(),
        }),
    }
}

fn simulation_mode_name(mode: CliSimulationMode) -> &'static str {
    match mode {
        CliSimulationMode::Auto => "auto",
        CliSimulationMode::EventOnly => "event_only",
        CliSimulationMode::ExternalJosim => "external_josim",
        CliSimulationMode::InternalTransient => "internal_transient",
    }
}

fn lint_input_report(input: &Path, input_kind: &str, contract_kind: &str) -> Result<Value> {
    let (schema_format, input_schema_version) = inspect_json_contract(input)?;
    Ok(json!({
        "kind": "lint_input",
        "input": input.display().to_string(),
        "input_kind": input_kind,
        "contract_kind": contract_kind,
        "valid": true,
        "schema_format": schema_format,
        "input_schema_version": input_schema_version,
        "legacy_compatibility_used": schema_format == "legacy_raw_json",
    }))
}

fn inspect_json_contract(input: &Path) -> Result<(&'static str, Option<u64>)> {
    let raw = fs::read_to_string(input)
        .with_context(|| format!("failed to read input JSON from {}", input.display()))?;
    let json: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse input JSON from {}", input.display()))?;
    let schema_version = json.get("schema_version").and_then(Value::as_u64);
    let looks_like_envelope = json
        .as_object()
        .map(|object| {
            object.contains_key("schema_version")
                || object.contains_key("kind")
                || object.contains_key("payload")
        })
        .unwrap_or(false);
    if looks_like_envelope {
        Ok(("versioned_envelope", schema_version))
    } else {
        Ok(("legacy_raw_json", None))
    }
}

fn run_compile_netlist(args: CompileNetlistArgs) -> Result<()> {
    let (mut netlist, pdk) = load_cli_netlist_and_pdk(&args.input, args.pdk)?;
    let report = with_flow_runner(|flow| {
        flow.compile_artifacts_for_cli_netlist(&mut netlist, &pdk)
            .context("compile-netlist failed")
    })?;

    if let Some(netlist_output) = args.netlist_output.as_deref() {
        rflux_io::write_ir_json(netlist_output, &netlist).with_context(|| {
            format!("failed to write compiled netlist JSON to {}", netlist_output.display())
        })?;
    }

    emit_json(
        &with_schema_version(synthesis_report_to_json(&report)),
        args.output.as_deref(),
    )
}

fn run_compile_layout(args: LayoutCommandArgs) -> Result<()> {
    run_flow_json_command(
        &args.input,
        args.pdk,
        args.output.as_deref(),
        |flow, netlist, pdk| {
            flow.compile_layout(netlist, pdk, &FlowConfig::default())
                .context("compile-layout failed")
        },
        layout_report_to_json,
    )
}

fn run_analyze_timing(args: LayoutCommandArgs) -> Result<()> {
    run_flow_json_command(
        &args.input,
        args.pdk,
        args.output.as_deref(),
        |flow, netlist, pdk| {
            flow.analyze_timing(netlist, pdk, &FlowConfig::default())
                .context("analyze-timing failed")
        },
        timing_analysis_to_json,
    )
}

fn run_verify_layout(args: VerifyLayoutArgs) -> Result<()> {
    let simulation_config = args.simulation_config();
    run_flow_json_command(
        &args.input,
        args.pdk,
        args.output.as_deref(),
        |flow, netlist, pdk| {
            flow.verify_layout(netlist, pdk, &FlowConfig::default(), &simulation_config)
                .context("verify-layout failed")
        },
        verification_report_to_json,
    )
}

fn run_simulate_file(args: SimulateFileArgs) -> Result<()> {
    let simulation_config = args.simulation_config();
    let report = simulate_file(&args.input, &simulation_config)
        .with_context(|| format!("simulate-file failed for {}", args.input.display()))?;

    emit_json(
        &with_schema_version(simulation_report_to_json(&report)),
        args.output.as_deref(),
    )
}

fn run_solve_dimacs(args: SolveDimacsArgs) -> Result<()> {
    let raw = fs::read_to_string(&args.input)
        .with_context(|| format!("failed to read DIMACS from {}", args.input.display()))?;
    let cnf = CnfFormula::from_dimacs(&raw).map_err(|error| {
        anyhow!("failed to parse DIMACS from {}: {:?}", args.input.display(), error)
    })?;
    let mut assumptions = parse_assumptions_option(args.assumptions.as_deref(), cnf.var_count())?;
    let metadata_selection = load_equivalence_check_selection(
        args.equivalence_metadata.as_deref(),
        args.check_ref.as_deref(),
        &cnf,
    )?;
    if let Some(selection) = &metadata_selection {
        assumptions.extend_from_slice(&selection.assumptions);
    }
    let (result, metrics, unsat_core) = if assumptions.is_empty() {
        let (result, metrics) = solve_with_metrics(&cnf);
        (result, metrics, None)
    } else {
        let solver = IncrementalSolver::from_formula(cnf.clone());
        let (result, metrics) = solver.solve_with_assumptions_and_metrics(&assumptions);
        let unsat_core = if matches!(result, SolveResult::Unsatisfiable) {
            solver.unsat_core_of_assumptions(&assumptions)
        } else {
            None
        };
        (result, metrics, unsat_core)
    };

    emit_json(
        &with_schema_version(dimacs_solve_report_to_json(
            &args.input,
            &cnf,
            &assumptions,
            unsat_core.as_deref(),
            metadata_selection.as_ref(),
            &result,
            &metrics,
        )),
        args.output.as_deref(),
    )
}

fn run_check_equivalence(args: CheckEquivalenceArgs) -> Result<()> {
    let (lhs_netlist, rhs_netlist) = load_equivalence_netlists(&args.lhs, &args.rhs)?;
    let verifier = Verifier::new();

    match args.kind {
        CliEquivalenceKind::Combinational => {
            let report = verifier
                .check_boolean_equivalence(&lhs_netlist, &rhs_netlist)
                .context("combinational equivalence check failed")?;
            emit_equivalence_report(
                combinational_equivalence_report_to_json(&report),
                args.dimacs_output.as_deref(),
                args.output.as_deref(),
                || {
                    verifier
                        .build_boolean_equivalence_problem(&lhs_netlist, &rhs_netlist)
                        .context("combinational equivalence DIMACS export failed")
                },
            )
        }
        CliEquivalenceKind::SingleStepSequential => {
            let report = verifier
                .check_single_step_sequential_equivalence(&lhs_netlist, &rhs_netlist)
                .context("single-step sequential equivalence check failed")?;
            emit_equivalence_report(
                single_step_sequential_equivalence_report_to_json(&report),
                args.dimacs_output.as_deref(),
                args.output.as_deref(),
                || {
                    verifier
                        .build_single_step_sequential_equivalence_problem(&lhs_netlist, &rhs_netlist)
                        .context("single-step sequential equivalence DIMACS export failed")
                },
            )
        }
    }
}

fn load_pdk(path: Option<PathBuf>) -> Result<Pdk> {
    match path {
        Some(path) => read_pdk_json(&path)
            .with_context(|| format!("failed to read PDK JSON from {}", path.display())),
        None => Ok(Pdk::minimal("minimal-sfq")),
    }
}

fn load_cli_netlist(input: &Path) -> Result<rflux_ir::Netlist> {
    read_ir_json(input).with_context(|| format!("failed to read IR JSON from {}", input.display()))
}

fn load_cli_netlist_and_pdk(input: &Path, pdk_path: Option<PathBuf>) -> Result<(rflux_ir::Netlist, Pdk)> {
    let netlist = load_cli_netlist(input)?;
    let pdk = load_pdk(pdk_path)?;
    Ok((netlist, pdk))
}

fn load_equivalence_netlists(
    lhs: &Path,
    rhs: &Path,
) -> Result<(rflux_ir::Netlist, rflux_ir::Netlist)> {
    let lhs_netlist = load_cli_netlist(lhs)
        .with_context(|| format!("failed to read lhs IR JSON from {}", lhs.display()))?;
    let rhs_netlist = load_cli_netlist(rhs)
        .with_context(|| format!("failed to read rhs IR JSON from {}", rhs.display()))?;
    Ok((lhs_netlist, rhs_netlist))
}

fn with_flow_runner<T>(callback: impl FnOnce(&mut FlowRunner) -> Result<T>) -> Result<T> {
    let mut flow = FlowRunner::new();
    callback(&mut flow)
}

fn with_loaded_flow_inputs<T>(
    input: &Path,
    pdk_path: Option<PathBuf>,
    callback: impl FnOnce(&mut FlowRunner, &mut rflux_ir::Netlist, &Pdk) -> Result<T>,
) -> Result<T> {
    let (mut netlist, pdk) = load_cli_netlist_and_pdk(input, pdk_path)?;
    with_flow_runner(|flow| callback(flow, &mut netlist, &pdk))
}

fn run_flow_json_command<T>(
    input: &Path,
    pdk_path: Option<PathBuf>,
    output: Option<&Path>,
    execute: impl FnOnce(&mut FlowRunner, &mut rflux_ir::Netlist, &Pdk) -> Result<T>,
    render: impl FnOnce(&T) -> Value,
) -> Result<()> {
    let report = with_loaded_flow_inputs(input, pdk_path, execute)?;
    emit_json(&with_schema_version(render(&report)), output)
}

fn emit_json(value: &Value, output: Option<&Path>) -> Result<()> {
    let rendered = serde_json::to_string_pretty(value).context("failed to serialize JSON output")?;
    if let Some(output) = output {
        fs::write(output, rendered)
            .with_context(|| format!("failed to write JSON output to {}", output.display()))?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

fn with_schema_version(value: Value) -> Value {
    match value {
        Value::Object(mut object) => {
            object.insert("schema_version".to_string(), json!(CLI_SCHEMA_VERSION));
            Value::Object(object)
        }
        other => other,
    }
}

fn emit_equivalence_report(
    mut report: Value,
    dimacs_output: Option<&Path>,
    output: Option<&Path>,
    build_problem: impl FnOnce() -> Result<rflux_verify::ExportedEquivalenceSatProblem>,
) -> Result<()> {
    let dimacs_export = dimacs_output
        .map(|path| build_problem().and_then(|problem| write_equivalence_dimacs_bundle(path, &problem)))
        .transpose()?;
    attach_dimacs_export(&mut report, dimacs_export);
    emit_json(&with_schema_version(report), output)
}

impl CliSimulationMode {
    fn into_simulation_mode(self) -> SimulationMode {
        match self {
            Self::Auto => SimulationMode::Auto,
            Self::EventOnly => SimulationMode::EventOnly,
            Self::ExternalJosim => SimulationMode::ExternalJosim,
            Self::InternalTransient => SimulationMode::InternalTransient,
        }
    }
}

impl VerifyLayoutArgs {
    fn simulation_config(&self) -> SimulationConfig {
        build_simulation_config(self.mode, self.external_command.clone())
    }
}

impl SimulateFileArgs {
    fn simulation_config(&self) -> SimulationConfig {
        build_simulation_config(self.mode, self.external_command.clone())
    }
}

fn build_simulation_config(mode: CliSimulationMode, external_command: Option<String>) -> SimulationConfig {
    SimulationConfig {
        mode: mode.into_simulation_mode(),
        external_command,
    }
}

fn synthesis_report_to_json(report: &rflux_synth::SynthesisReport) -> Value {
    json!({
        "compile": {
            "connections_applied": report.compile.connections_applied,
            "splitters_inserted": report.compile.splitters_inserted,
            "balancing_dffs_inserted": report.compile.balancing_dffs_inserted,
        },
        "bool_opt": {
            "gate_count_before": report.bool_opt.gate_count_before,
            "gate_count_after": report.bool_opt.gate_count_after,
        },
        "tech_map": {
            "mapped_nodes": report.tech_map.mapped_nodes,
            "total_area_um2": report.tech_map.total_area_um2,
        },
        "path_balance": {
            "node_levels": report.path_balance.node_levels,
            "needs": report.path_balance.needs.iter().map(|need| json!({
                "sink_node": need.sink_node,
                "source": pin_ref_to_json(need.source),
                "deficit": need.deficit,
            })).collect::<Vec<_>>(),
        },
        "bool_opt_compatibility": {
            "input_nodes": report.bool_opt_compatibility.input_nodes,
            "output_candidates": report.bool_opt_compatibility.output_candidates,
            "issues": report.bool_opt_compatibility.issues.iter().map(|issue| json!({
                "node": issue.node,
                "kind": format!("{:?}", issue.kind),
                "detail": issue.detail,
            })).collect::<Vec<_>>(),
            "compatible": report.bool_opt_compatibility.is_compatible(),
        },
        "node_count": report.node_count,
        "edge_count": report.edge_count,
    })
}

fn layout_report_to_json(report: &rflux_flow::LayoutReport) -> Value {
    json!({
        "synthesis": synthesis_report_to_json(&report.synthesis),
        "placement": {
            "placed_nodes": report.placement.placed_nodes,
            "width_um": report.placement.width_um,
            "height_um": report.placement.height_um,
        },
        "routing": {
            "routed_nets": report.routing.routed_nets,
            "total_length_um": report.routing.total_length_um,
            "total_detour_overhead_um": report.routing.total_detour_overhead_um,
            "detoured_routes": report.routing.detoured_routes,
            "jtl_routes": report.routing.jtl_routes,
            "ptl_routes": report.routing.ptl_routes,
        },
        "clock": {
            "clock_sinks": report.clock.clock_sinks,
            "clock_buffers": report.clock.clock_buffers,
            "phase_count": report.clock.phase_count,
            "assigned_phases": report.clock.assigned_phases,
        },
        "timing": {
            "worst_setup_slack_ps": report.timing.worst_setup_slack_ps,
            "worst_hold_slack_ps": report.timing.worst_hold_slack_ps,
            "critical_path_delay_ps": report.timing.critical_path_delay_ps,
            "analyzed_arcs": report.timing.analyzed_arcs,
            "false_path_arcs": report.timing.false_path_arcs,
            "setup_violations": report.timing.setup_violations,
            "initial_hold_violations": report.timing.initial_hold_violations,
            "final_hold_violations": report.timing.final_hold_violations,
            "hold_fix_applied": report.timing.hold_fix_applied,
        },
        "initial_total_detour_overhead_um": report.initial_total_detour_overhead_um,
        "detour_feedback_applied": report.detour_feedback_applied,
    })
}

fn timing_analysis_to_json(report: &rflux_flow::TimingAnalysisReport) -> Value {
    json!({
        "worst_setup_slack_ps": report.worst_setup_slack_ps,
        "worst_hold_slack_ps": report.worst_hold_slack_ps,
        "critical_path_delay_ps": report.critical_path_delay_ps,
        "analyzed_arcs": report.analyzed_arcs,
        "false_path_arcs": report.false_path_arcs,
        "setup_violations": report.setup_violations,
        "hold_violations": report.hold_violations,
        "detour_feedback_applied": report.detour_feedback_applied,
        "hold_fix_applied": report.hold_fix_applied,
        "timing_arcs": report.timing_arcs.iter().map(|arc| json!({
            "from": pin_ref_to_json(arc.from),
            "to": pin_ref_to_json(arc.to),
            "is_false_path": arc.is_false_path,
            "route_mode": format!("{:?}", arc.route_mode),
            "route_length_um": arc.route_length_um,
            "from_domain": arc.from_domain,
            "to_domain": arc.to_domain,
            "arrival_ps": arc.arrival_ps,
            "required_ps": arc.required_ps,
            "setup_slack_ps": arc.setup_slack_ps,
            "hold_slack_ps": arc.hold_slack_ps,
        })).collect::<Vec<_>>()
    })
}

fn verification_report_to_json(report: &rflux_flow::VerificationReport) -> Value {
    json!({
        "checked_routes": report.checked_routes,
        "checked_ptl_routes": report.checked_ptl_routes,
        "structural_violations": report.structural_violations,
        "ptl_macro_boundary_violations": report.ptl_macro_boundary_violations,
        "ptl_forbidden_length_violations": report.ptl_forbidden_length_violations,
        "simulation": simulation_report_to_json(&report.simulation),
    })
}

fn simulation_report_to_json(report: &SimulationReport) -> Value {
    json!({
        "backend": format!("{:?}", report.backend),
        "simulated_events": report.simulated_events,
        "generated_deck_lines": report.generated_deck_lines,
        "generated_deck_path": report.generated_deck_path,
        "waveform_path": report.waveform_path,
        "reported_violations": report.reported_violations,
        "reported_worst_delay_ps": report.reported_worst_delay_ps,
        "delay_details": report.delay_details.iter().map(|detail| json!({
            "name": detail.name,
            "delay_ps": detail.delay_ps,
            "from_ref": detail.from_ref.as_ref().map(endpoint_ref_to_json),
            "to_ref": detail.to_ref.as_ref().map(endpoint_ref_to_json),
        })).collect::<Vec<_>>(),
        "violation_details": report.violation_details.iter().map(|detail| json!({
            "kind": detail.kind,
            "detail": detail.detail,
            "at_ref": detail.at_ref.as_ref().map(endpoint_ref_to_json),
        })).collect::<Vec<_>>(),
        "external_status_code": report.external_status_code,
        "external_result": report.external_result,
    })
}

fn dimacs_solve_report_to_json(
    input: &Path,
    formula: &CnfFormula,
    assumptions: &[Lit],
    unsat_core: Option<&[Lit]>,
    metadata_selection: Option<&EquivalenceCheckSelection>,
    result: &SolveResult,
    metrics: &rflux_sat::SolveMetrics,
) -> Value {
    json!({
        "kind": "dimacs_sat",
        "input": input.display().to_string(),
        "variables": formula.var_count(),
        "clauses": formula.clauses().len(),
        "assumptions": assumptions.iter().map(|lit| lit_to_dimacs_value(*lit)).collect::<Vec<_>>(),
        "unsat_core": unsat_core.map(|core| core.iter().map(|lit| lit_to_dimacs_value(*lit)).collect::<Vec<_>>()),
        "equivalence_check": metadata_selection.map(|selection| json!({
            "metadata_path": selection.metadata_path.display().to_string(),
            "check_ref": selection.check_ref,
            "assumptions": selection.assumptions.iter().map(|lit| lit_to_dimacs_value(*lit)).collect::<Vec<_>>(),
        })),
        "satisfiable": matches!(result, SolveResult::Satisfiable(_)),
        "model": match result {
            SolveResult::Satisfiable(model) => Some(model_to_json(model, formula.var_count())),
            SolveResult::Unsatisfiable => None,
        },
        "sat_stats": solve_stats_to_json(&metrics.stats),
        "sat_elapsed_ns": metrics.elapsed_ns,
    })
}

fn model_to_json(model: &rflux_sat::Model, var_count: usize) -> Value {
    let mut object = serde_json::Map::new();
    for var in 1..=var_count {
        if let Some(value) = model.value(var) {
            object.insert(var.to_string(), json!(value));
        }
    }
    Value::Object(object)
}

fn equivalence_dimacs_export_to_json(
    path: &Path,
    problem: &rflux_verify::ExportedEquivalenceSatProblem,
) -> Value {
    let sidecar_path = equivalence_sidecar_path(path);
    json!({
        "schema_version": CLI_SCHEMA_VERSION,
        "generated_by": "rflux-cli check-equivalence",
        "path": path.display().to_string(),
        "sidecar_path": sidecar_path.display().to_string(),
        "variables": problem.formula.var_count(),
        "clauses": problem.formula.clauses().len(),
        "formula_signature": {
            "variables": problem.formula.var_count(),
            "clauses": problem.formula.clauses().len(),
            "dimacs_path_hint": path.display().to_string(),
        },
        "checks": problem.checks.iter().map(|check| json!({
            "kind": equivalence_check_kind_label(check.kind),
            "name": check.name,
            "check_ref": format!("{}:{}", equivalence_check_kind_label(check.kind), check.name),
            "assumptions": check.assumptions.iter().map(|lit| lit_to_dimacs_value(*lit)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn write_equivalence_dimacs_bundle(
    path: &Path,
    problem: &rflux_verify::ExportedEquivalenceSatProblem,
) -> Result<Value> {
    write_dimacs_export(path, problem)?;
    let export_json = equivalence_dimacs_export_to_json(path, problem);
    let sidecar_path = equivalence_sidecar_path(path);
    fs::write(
        &sidecar_path,
        serde_json::to_string_pretty(&export_json).context("failed to serialize DIMACS sidecar JSON")?,
    )
    .with_context(|| format!("failed to write DIMACS sidecar to {}", sidecar_path.display()))?;
    Ok(export_json)
}

fn write_dimacs_export(
    path: &Path,
    problem: &rflux_verify::ExportedEquivalenceSatProblem,
) -> Result<()> {
    fs::write(path, problem.formula.to_dimacs())
        .with_context(|| format!("failed to write DIMACS export to {}", path.display()))
}

fn equivalence_sidecar_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| format!("{}.checks.json", name.to_string_lossy()))
        .unwrap_or_else(|| "equivalence.checks.json".to_string());
    path.with_file_name(file_name)
}

fn attach_dimacs_export(report: &mut Value, dimacs_export: Option<Value>) {
    let Some(dimacs_export) = dimacs_export else {
        return;
    };
    let Value::Object(object) = report else {
        return;
    };
    object.insert("dimacs_export".to_string(), dimacs_export);
}

fn equivalence_check_kind_label(kind: rflux_verify::EquivalenceCheckKind) -> &'static str {
    match kind {
        rflux_verify::EquivalenceCheckKind::Output => "output",
        rflux_verify::EquivalenceCheckKind::State => "state",
    }
}

fn lit_to_dimacs_value(lit: Lit) -> i64 {
    if lit.negated {
        -(lit.var as i64)
    } else {
        lit.var as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EquivalenceCheckSelection {
    metadata_path: PathBuf,
    check_ref: String,
    assumptions: Vec<Lit>,
}

fn load_equivalence_check_selection(
    metadata_path: Option<&Path>,
    check_ref: Option<&str>,
    formula: &CnfFormula,
) -> Result<Option<EquivalenceCheckSelection>> {
    match (metadata_path, check_ref) {
        (None, None) => Ok(None),
        (Some(_), None) => bail!("--equivalence-metadata requires --check-ref KIND:NAME"),
        (None, Some(_)) => bail!("--check-ref requires --equivalence-metadata PATH"),
        (Some(metadata_path), Some(check_ref)) => {
            let metadata_path = metadata_path.to_path_buf();
            let check_ref = check_ref.to_string();
            let content = fs::read_to_string(&metadata_path).with_context(|| {
                format!("failed to read equivalence metadata from {}", metadata_path.display())
            })?;
            let json: Value = serde_json::from_str(&content).with_context(|| {
                format!("failed to parse equivalence metadata JSON from {}", metadata_path.display())
            })?;
            let schema_version = json
                .get("schema_version")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if schema_version != 1 {
                bail!(
                    "unsupported equivalence metadata schema version {} in {}",
                    schema_version,
                    metadata_path.display()
                );
            }
            let signature = json
                .get("formula_signature")
                .and_then(Value::as_object)
                .ok_or_else(|| anyhow!("equivalence metadata is missing formula_signature"))?;
            let metadata_var_count = signature
                .get("variables")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow!("equivalence metadata formula_signature is missing variables"))?
                as usize;
            let metadata_clause_count = signature
                .get("clauses")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow!("equivalence metadata formula_signature is missing clauses"))?
                as usize;
            if metadata_var_count != formula.var_count() || metadata_clause_count != formula.clauses().len() {
                bail!(
                    "equivalence metadata formula signature does not match loaded CNF: metadata=({}, {}), cnf=({}, {})",
                    metadata_var_count,
                    metadata_clause_count,
                    formula.var_count(),
                    formula.clauses().len()
                );
            }
            let checks = json
                .get("checks")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("equivalence metadata is missing a checks array"))?;

            let matching = checks
                .iter()
                .find(|check| check.get("check_ref").and_then(Value::as_str) == Some(check_ref.as_str()))
                .ok_or_else(|| anyhow!("check ref not found in equivalence metadata: {}", check_ref))?;
            let assumptions_json = matching
                .get("assumptions")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("equivalence metadata check is missing assumptions: {}", check_ref))?;
            let assumptions = assumptions_json
                .iter()
                .map(|value| {
                    let literal = value
                        .as_i64()
                        .ok_or_else(|| anyhow!("equivalence metadata assumption must be an integer"))?;
                    if literal == 0 {
                        bail!("equivalence metadata assumptions must be non-zero");
                    }
                    let var = literal.unsigned_abs() as usize;
                    if var > formula.var_count() {
                        bail!(
                            "equivalence metadata assumption variable {} is out of range for formula with {} variables",
                            var,
                            formula.var_count()
                        );
                    }
                    Ok(if literal > 0 {
                        Lit::pos(var)
                    } else {
                        Lit::neg(var)
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(Some(EquivalenceCheckSelection {
                metadata_path,
                check_ref,
                assumptions,
            }))
        }
    }
}

fn combinational_equivalence_report_to_json(
    report: &rflux_verify::CombinationalEquivalenceReport,
) -> Value {
    json!({
        "kind": "combinational",
        "equivalent": report.equivalent,
        "checked_outputs": report.checked_outputs,
        "counterexample_inputs": report.counterexample_inputs,
        "counterexample_outputs": report.counterexample_outputs.as_ref().map(output_mismatch_map_to_json),
        "sat_stats": solve_stats_to_json(&report.sat_stats),
        "sat_elapsed_ns": report.sat_elapsed_ns,
    })
}

fn single_step_sequential_equivalence_report_to_json(
    report: &rflux_verify::SingleStepSequentialEquivalenceReport,
) -> Value {
    json!({
        "kind": "single_step_sequential",
        "equivalent": report.equivalent,
        "checked_outputs": report.checked_outputs,
        "checked_states": report.checked_states,
        "counterexample_inputs": report.counterexample_inputs,
        "counterexample_present_states": report.counterexample_present_states,
        "counterexample_outputs": report.counterexample_outputs.as_ref().map(output_mismatch_map_to_json),
        "counterexample_states": report.counterexample_states.as_ref().map(state_mismatch_map_to_json),
        "sat_stats": solve_stats_to_json(&report.sat_stats),
        "sat_elapsed_ns": report.sat_elapsed_ns,
    })
}

fn output_mismatch_map_to_json(
    mismatches: &std::collections::BTreeMap<String, rflux_verify::SatOutputMismatch>,
) -> Value {
    let mut object = serde_json::Map::new();
    for (name, mismatch) in mismatches {
        object.insert(
            name.clone(),
            json!({
                "lhs": mismatch.lhs,
                "rhs": mismatch.rhs,
            }),
        );
    }
    Value::Object(object)
}

fn state_mismatch_map_to_json(
    mismatches: &std::collections::BTreeMap<String, rflux_verify::SatStateTransitionMismatch>,
) -> Value {
    let mut object = serde_json::Map::new();
    for (name, mismatch) in mismatches {
        object.insert(
            name.clone(),
            json!({
                "lhs_next": mismatch.lhs_next,
                "rhs_next": mismatch.rhs_next,
                "lhs_clock": mismatch.lhs_clock,
                "rhs_clock": mismatch.rhs_clock,
            }),
        );
    }
    Value::Object(object)
}

fn solve_stats_to_json(stats: &SolveStats) -> Value {
    json!({
        "recursive_calls": stats.recursive_calls,
        "decisions": stats.decisions,
        "unit_assignments": stats.unit_assignments,
        "pure_literal_assignments": stats.pure_literal_assignments,
        "backtracks": stats.backtracks,
        "restarts": stats.restarts,
    })
}

fn pin_ref_to_json(pin: rflux_ir::PinRef) -> Value {
    json!({
        "node": pin.node.0,
        "port": pin.port,
    })
}

fn endpoint_ref_to_json(endpoint: &rflux_sim::SimulationEndpointRef) -> Value {
    json!({
        "raw": endpoint.raw,
        "node": endpoint.node,
        "port": endpoint.port,
    })
}

fn parse_assumptions_option(raw: Option<&str>, var_count: usize) -> Result<Vec<Lit>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    let mut assumptions = Vec::new();
    for token in raw.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
        if token.is_empty() {
            continue;
        }
        let literal = token
            .parse::<i32>()
            .with_context(|| format!("invalid assumption literal: {token}"))?;
        if literal == 0 {
            bail!("assumption literals must be non-zero");
        }
        let var = literal.unsigned_abs() as usize;
        if var > var_count {
            bail!(
                "assumption variable {} is out of range for formula with {} variables",
                var,
                var_count
            );
        }
        assumptions.push(if literal > 0 {
            Lit::pos(var)
        } else {
            Lit::neg(var)
        });
    }

    Ok(assumptions)
}

trait FlowRunnerCliExt {
    fn compile_artifacts_for_cli_netlist(
        &mut self,
        netlist: &mut rflux_ir::Netlist,
        pdk: &Pdk,
    ) -> Result<rflux_synth::SynthesisReport>;
}

impl FlowRunnerCliExt for FlowRunner {
    fn compile_artifacts_for_cli_netlist(
        &mut self,
        netlist: &mut rflux_ir::Netlist,
        pdk: &Pdk,
    ) -> Result<rflux_synth::SynthesisReport> {
        let mut compiler = rflux_synth::Compiler::new();
        compiler
            .compile_netlist(netlist, pdk, &Default::default())
            .context("synthesis failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::{LogicOp, Netlist, NodeKind, PinRef};

    fn unique_test_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("cli-tests")
            .join(format!("{}-{}-{}", name, std::process::id(), stamp));
        fs::create_dir_all(&dir).expect("test directory should be created");
        dir
    }

    #[test]
    fn dimacs_solve_report_serializes_sat_model() {
        let dimacs = "
            p cnf 2 2
            1 0
            2 0
        ";
        let input = Path::new("example.cnf");
        let cnf = CnfFormula::from_dimacs(dimacs).expect("dimacs should parse");
        let (result, metrics) = solve_with_metrics(&cnf);

        let report = dimacs_solve_report_to_json(
            input,
            &cnf,
            &[Lit::pos(1)],
            None,
            None,
            &result,
            &metrics,
        );

        assert_eq!(report["kind"], "dimacs_sat");
        assert_eq!(report["input"], "example.cnf");
        assert_eq!(report["variables"], 2);
        assert_eq!(report["clauses"], 2);
        assert_eq!(report["assumptions"][0], 1);
        assert!(report["unsat_core"].is_null());
        assert_eq!(report["satisfiable"], true);
        assert_eq!(report["model"]["1"], true);
        assert_eq!(report["model"]["2"], true);
    }

    #[test]
    fn with_schema_version_adds_root_schema_version() {
        let wrapped = with_schema_version(json!({
            "kind": "demo"
        }));

        assert_eq!(wrapped["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(wrapped["kind"], "demo");
    }

    #[test]
    fn dimacs_solve_report_serializes_unsat_without_model() {
        let dimacs = "
            p cnf 1 2
            1 0
            -1 0
        ";
        let input = Path::new("contradiction.cnf");
        let cnf = CnfFormula::from_dimacs(dimacs).expect("dimacs should parse");
        let (result, metrics) = solve_with_metrics(&cnf);

        let report = dimacs_solve_report_to_json(input, &cnf, &[], None, None, &result, &metrics);

        assert_eq!(report["satisfiable"], false);
        assert!(report["model"].is_null());
        assert!(report["unsat_core"].is_null());
        assert!(report["sat_elapsed_ns"].as_u64().is_some());
    }

    #[test]
    fn dimacs_solve_report_serializes_unsat_core() {
        let dimacs = "
            p cnf 3 1
            1 2 0
        ";
        let input = Path::new("core.cnf");
        let cnf = CnfFormula::from_dimacs(dimacs).expect("dimacs should parse");
        let solver = IncrementalSolver::from_formula(cnf.clone());
        let assumptions = vec![Lit::neg(1), Lit::neg(2), Lit::pos(3)];
        let (result, metrics) = solver.solve_with_assumptions_and_metrics(&assumptions);
        let unsat_core = solver.unsat_core_of_assumptions(&assumptions);

        let report = dimacs_solve_report_to_json(
            input,
            &cnf,
            &assumptions,
            unsat_core.as_deref(),
            None,
            &result,
            &metrics,
        );

        assert_eq!(report["satisfiable"], false);
        assert_eq!(report["unsat_core"], json!([-1, -2]));
    }

    #[test]
    fn equivalence_sidecar_path_uses_dimacs_file_name() {
        let path = Path::new("target/example.cnf");

        let sidecar = equivalence_sidecar_path(path);

        assert_eq!(sidecar, PathBuf::from("target/example.cnf.checks.json"));
    }

    #[test]
    fn parse_assumptions_accepts_comma_and_whitespace_separated_literals() {
        let assumptions = parse_assumptions_option(Some("1, -2 3"), 3)
            .expect("assumptions should parse");

        assert_eq!(assumptions, vec![Lit::pos(1), Lit::neg(2), Lit::pos(3)]);
    }

    #[test]
    fn load_equivalence_check_selection_reads_assumptions_from_sidecar() {
        let dir = unique_test_dir("equivalence-metadata");
        let sidecar_path = dir.join("example.cnf.checks.json");
        let formula = CnfFormula::new(20);
        fs::write(
            &sidecar_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "generated_by": "test",
                "formula_signature": {
                    "variables": 20,
                    "clauses": 0,
                    "dimacs_path_hint": "example.cnf"
                },
                "checks": [
                    {
                        "check_ref": "output:maj",
                        "assumptions": [14]
                    }
                ]
            }))
            .expect("sidecar should serialize"),
        )
        .expect("sidecar should write");
        let selection = load_equivalence_check_selection(
            Some(sidecar_path.as_path()),
            Some("output:maj"),
            &formula,
        )
            .expect("selection should load")
            .expect("selection should exist");

        assert_eq!(selection.check_ref, "output:maj");
        assert_eq!(selection.assumptions, vec![Lit::pos(14)]);
    }

    #[test]
    fn load_equivalence_check_selection_rejects_formula_signature_mismatch() {
        let dir = unique_test_dir("equivalence-metadata-mismatch");
        let sidecar_path = dir.join("example.cnf.checks.json");
        let formula = CnfFormula::new(4);
        fs::write(
            &sidecar_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "generated_by": "test",
                "formula_signature": {
                    "variables": 5,
                    "clauses": 0,
                    "dimacs_path_hint": "wrong.cnf"
                },
                "checks": [
                    {
                        "check_ref": "output:maj",
                        "assumptions": [1]
                    }
                ]
            }))
            .expect("sidecar should serialize"),
        )
        .expect("sidecar should write");
        let error = load_equivalence_check_selection(
            Some(sidecar_path.as_path()),
            Some("output:maj"),
            &formula,
        )
            .expect_err("mismatched sidecar should be rejected");

        assert!(error.to_string().contains("formula signature does not match"));
    }

    #[test]
    fn run_check_equivalence_writes_dimacs_export() {
        let dir = unique_test_dir("equivalence-export");
        let lhs_path = dir.join("lhs.ir.json");
        let rhs_path = dir.join("rhs.ir.json");
        let report_path = dir.join("report.json");
        let dimacs_path = dir.join("equivalence.cnf");

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let b_l = lhs.add_node(NodeKind::Port, "b");
        let and_l = lhs.add_node_with_logic(NodeKind::CellInstance, "lhs_and", Some(LogicOp::And));
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(PinRef { node: a_l, port: 0 }, PinRef { node: and_l, port: 0 }).expect("a->and");
        lhs.connect(PinRef { node: b_l, port: 0 }, PinRef { node: and_l, port: 1 }).expect("b->and");
        lhs.connect(PinRef { node: and_l, port: 0 }, PinRef { node: out_l, port: 0 }).expect("and->out");

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let b_r = rhs.add_node(NodeKind::Port, "b");
        let and_r = rhs.add_node_with_logic(NodeKind::CellInstance, "rhs_and", Some(LogicOp::And));
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(PinRef { node: b_r, port: 0 }, PinRef { node: and_r, port: 0 }).expect("b->and");
        rhs.connect(PinRef { node: a_r, port: 0 }, PinRef { node: and_r, port: 1 }).expect("a->and");
        rhs.connect(PinRef { node: and_r, port: 0 }, PinRef { node: out_r, port: 0 }).expect("and->out");

        rflux_io::write_ir_json(&lhs_path, &lhs).expect("lhs should be written");
        rflux_io::write_ir_json(&rhs_path, &rhs).expect("rhs should be written");

        run_check_equivalence(CheckEquivalenceArgs {
            lhs: lhs_path.clone(),
            rhs: rhs_path.clone(),
            kind: CliEquivalenceKind::Combinational,
            dimacs_output: Some(dimacs_path.clone()),
            output: Some(report_path.clone()),
        })
        .expect("check-equivalence should succeed");

        let dimacs = fs::read_to_string(&dimacs_path).expect("dimacs export should exist");
        let sidecar_path = PathBuf::from(format!("{}.checks.json", dimacs_path.display()));
        let sidecar = fs::read_to_string(&sidecar_path).expect("dimacs sidecar should exist");
        let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).expect("report should exist"))
            .expect("report should be valid json");
        let sidecar_json: Value = serde_json::from_str(&sidecar).expect("sidecar should be json");

        assert!(dimacs.starts_with("p cnf "));
        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["dimacs_export"]["schema_version"], 1);
        assert_eq!(report["dimacs_export"]["path"], dimacs_path.display().to_string());
        assert_eq!(report["dimacs_export"]["sidecar_path"], sidecar_path.display().to_string());
        assert_eq!(report["dimacs_export"]["checks"][0]["kind"], "output");
        assert_eq!(report["dimacs_export"]["checks"][0]["name"], "out");
        assert_eq!(sidecar_json["schema_version"], 1);
        assert_eq!(
            sidecar_json["formula_signature"]["variables"],
            report["dimacs_export"]["variables"]
        );
        assert_eq!(
            sidecar_json["formula_signature"]["clauses"],
            report["dimacs_export"]["clauses"]
        );
        assert_eq!(sidecar_json["checks"][0]["check_ref"], "output:out");
    }

    #[test]
    fn clap_accepts_existing_underscore_value_names() {
        let cli = Cli::try_parse_from([
            "rflux",
            "simulate-file",
            "--input",
            "example.cir",
            "--mode",
            "internal_transient",
        ])
        .expect("simulate-file args should parse");

        match cli.command {
            Commands::SimulateFile(args) => {
                assert_eq!(args.mode, CliSimulationMode::InternalTransient);
            }
            other => panic!("expected simulate-file command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "check-equivalence",
            "--lhs",
            "lhs.json",
            "--rhs",
            "rhs.json",
            "--kind",
            "single_step_sequential",
        ])
        .expect("check-equivalence args should parse");

        match cli.command {
            Commands::CheckEquivalence(args) => {
                assert_eq!(args.kind, CliEquivalenceKind::SingleStepSequential);
            }
            other => panic!("expected check-equivalence command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "lint-input",
            "--input",
            "example.ir.json",
            "--kind",
            "ir",
        ])
        .expect("lint-input args should parse");

        match cli.command {
            Commands::LintInput(args) => {
                assert_eq!(args.kind, CliInputKind::Ir);
                assert_eq!(args.input, PathBuf::from("example.ir.json"));
            }
            other => panic!("expected lint-input command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "collect-diagnostics",
            "--output-dir",
            "bundle",
            "--report",
            "report.json",
            "--mode",
            "internal_transient",
        ])
        .expect("collect-diagnostics args should parse");

        match cli.command {
            Commands::CollectDiagnostics(args) => {
                assert_eq!(args.output_dir, PathBuf::from("bundle"));
                assert_eq!(args.report, Some(PathBuf::from("report.json")));
                assert_eq!(args.mode, Some(CliSimulationMode::InternalTransient));
            }
            other => panic!("expected collect-diagnostics command, got {other:?}"),
        }
    }

    #[test]
    fn run_collect_diagnostics_writes_manifest_and_copies_inputs() {
        let dir = unique_test_dir("collect-diagnostics");
        let input_path = dir.join("example.ir.json");
        let pdk_path = dir.join("example.pdk.json");
        let report_path = dir.join("simulate-report.json");
        let output_dir = dir.join("bundle");

        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "in");
        rflux_io::write_ir_json(&input_path, &netlist).expect("ir json should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");
        fs::write(
            &report_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "kind": "simulate_file",
                "status": "ok"
            }))
            .expect("report should serialize"),
        )
        .expect("report should write");

        run_collect_diagnostics(CollectDiagnosticsArgs {
            output_dir: output_dir.clone(),
            command: Some("simulate-file".to_string()),
            input: Some(input_path.clone()),
            pdk: Some(pdk_path.clone()),
            report: Some(report_path.clone()),
            mode: Some(CliSimulationMode::InternalTransient),
            external_command: Some("josim".to_string()),
            notes: Some("capture for support reproduction".to_string()),
        })
        .expect("collect-diagnostics should succeed");

        let manifest_path = output_dir.join("manifest.json");
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(&manifest_path).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(manifest["kind"], "diagnostics_bundle");
        assert_eq!(manifest["bundle_version"], 1);
        assert_eq!(manifest["invocation"]["command"], "simulate-file");
        assert!(manifest["invocation"]["working_directory"].as_str().is_some());
        assert_eq!(manifest["invocation"]["mode"], "internal_transient");
        assert_eq!(manifest["invocation"]["external_command"], "josim");
        assert_eq!(manifest["configuration"]["paths"]["input"], input_path.display().to_string());
        assert_eq!(manifest["configuration"]["paths"]["pdk"], pdk_path.display().to_string());
        assert_eq!(manifest["configuration"]["paths"]["report"], report_path.display().to_string());
        assert_eq!(manifest["configuration"]["simulation"]["mode"], "internal_transient");
        assert_eq!(manifest["summary"]["command"], "simulate-file");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["inspection_failure_count"], 0);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["simulate_file"]));
        assert_eq!(manifest["summary"]["report_inspection_failure_count"], 0);
        assert_eq!(manifest["structured_logs"]["format"], "jsonl");
        assert_eq!(manifest["structured_logs"]["event_count"], 5);
        assert!(manifest["summary"]["legacy_compatibility_inputs"]
            .as_array()
            .expect("legacy inputs should be array")
            .is_empty());
        assert!(manifest["environment"]["present_prefixed_vars"]["RFLOW_*"]
            .as_array()
            .expect("RFLOW env list should be an array")
            .is_empty());
        let captured_inputs = manifest["captured_inputs"]
            .as_array()
            .expect("captured inputs should be array");
        let captured_reports = manifest["captured_reports"]
            .as_array()
            .expect("captured reports should be array");
        assert_eq!(captured_inputs.len(), 2);
        assert_eq!(captured_reports.len(), 1);
        assert_eq!(captured_inputs[0]["contract"]["contract_kind"], "rflux_ir_netlist");
        assert_eq!(captured_inputs[0]["contract"]["schema_format"], "versioned_envelope");
        assert_eq!(captured_inputs[0]["contract"]["input_schema_version"], 1);
        assert_eq!(captured_inputs[0]["contract"]["legacy_compatibility_used"], false);
        assert!(captured_inputs[0]["contract"]["inspection_error"].is_null());
        assert_eq!(captured_inputs[1]["contract"]["contract_kind"], "rflux_pdk");
        assert_eq!(captured_inputs[1]["contract"]["schema_format"], "versioned_envelope");
        assert_eq!(captured_reports[0]["report"]["kind"], "simulate_file");
        assert_eq!(captured_reports[0]["report"]["schema_version"], 1);
        assert!(captured_reports[0]["report"]["inspection_error"].is_null());

        let bundled_input = output_dir.join("inputs").join("example.ir.json");
        let bundled_pdk = output_dir.join("inputs").join("example.pdk.json");
        let bundled_report = output_dir.join("reports").join("simulate-report.json");
        let event_log_path = output_dir.join("events.jsonl");
        let event_log = fs::read_to_string(&event_log_path).expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();
        assert_eq!(fs::read_to_string(&bundled_input).expect("bundled input should exist"), fs::read_to_string(&input_path).expect("source input should exist"));
        assert_eq!(fs::read_to_string(&bundled_pdk).expect("bundled pdk should exist"), fs::read_to_string(&pdk_path).expect("source pdk should exist"));
        assert_eq!(fs::read_to_string(&bundled_report).expect("bundled report should exist"), fs::read_to_string(&report_path).expect("source report should exist"));
        assert_eq!(event_lines.len(), 5);
        let first_event: Value = serde_json::from_str(event_lines[0]).expect("first event should be json");
        let report_event: Value = serde_json::from_str(event_lines[3]).expect("report event should be json");
        let last_event: Value = serde_json::from_str(event_lines[4]).expect("last event should be json");
        assert_eq!(first_event["event"], "bundle_started");
        assert_eq!(report_event["event"], "report_captured");
        assert_eq!(last_event["event"], "manifest_prepared");
    }

    #[test]
    fn diagnostics_report_snapshot_reports_parse_failures() {
        let dir = unique_test_dir("collect-diagnostics-report");
        let report_path = dir.join("broken-report.json");
        fs::write(&report_path, "not valid json").expect("broken report should write");

        let report = diagnostics_report_snapshot(&report_path);

        assert!(report["kind"].is_null());
        assert!(report["schema_version"].is_null());
        assert!(report["inspection_error"].as_str().is_some());
    }

    #[test]
    fn diagnostics_contract_snapshot_reports_legacy_and_parse_failures() {
        let dir = unique_test_dir("collect-diagnostics-contract");
        let legacy_ir_path = dir.join("legacy.ir.json");
        let broken_pdk_path = dir.join("broken.pdk.json");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "legacy");
        fs::write(
            &legacy_ir_path,
            serde_json::to_string_pretty(&netlist).expect("legacy ir should serialize"),
        )
        .expect("legacy ir should write");
        fs::write(&broken_pdk_path, "not valid json").expect("broken pdk should write");

        let legacy = diagnostics_contract_snapshot("input", &legacy_ir_path);
        let broken = diagnostics_contract_snapshot("pdk", &broken_pdk_path);

        assert_eq!(legacy["schema_format"], "legacy_raw_json");
        assert_eq!(legacy["legacy_compatibility_used"], true);
        assert!(legacy["inspection_error"].is_null());

        assert!(broken["schema_format"].is_null());
        assert!(broken["input_schema_version"].is_null());
        assert!(broken["legacy_compatibility_used"].is_null());
        assert!(broken["inspection_error"].as_str().is_some());
    }

    #[test]
    fn diagnostics_summary_reports_legacy_and_failures() {
        let captured_inputs = vec![
            json!({
                "role": "input",
                "contract": {
                    "legacy_compatibility_used": true,
                    "inspection_error": Value::Null,
                }
            }),
            json!({
                "role": "pdk",
                "contract": {
                    "legacy_compatibility_used": Value::Null,
                    "inspection_error": "parse failed",
                }
            }),
        ];

        let captured_reports = vec![
            json!({
                "source_path": "report.json",
                "report": {
                    "kind": "simulate_file",
                    "inspection_error": Value::Null,
                }
            }),
            json!({
                "source_path": "broken-report.json",
                "report": {
                    "kind": Value::Null,
                    "inspection_error": "parse failed",
                }
            }),
        ];

        let summary = build_diagnostics_summary(Some("simulate-file"), &captured_inputs, &captured_reports);

        assert_eq!(summary["command"], "simulate-file");
        assert_eq!(summary["captured_input_count"], 2);
        assert_eq!(summary["captured_report_count"], 2);
        assert_eq!(summary["legacy_compatibility_inputs"], json!(["input"]));
        assert_eq!(summary["inspection_failure_count"], 1);
        assert_eq!(summary["inspection_failures"][0]["role"], "pdk");
        assert_eq!(summary["inspection_failures"][0]["error"], "parse failed");
        assert_eq!(summary["report_kinds"], json!(["simulate_file"]));
        assert_eq!(summary["report_inspection_failure_count"], 1);
        assert_eq!(summary["report_inspection_failures"][0]["source_path"], "broken-report.json");
        assert_eq!(summary["report_inspection_failures"][0]["error"], "parse failed");
    }

    #[test]
    fn run_lint_input_reports_versioned_ir_contract() {
        let dir = unique_test_dir("lint-input-ir");
        let input_path = dir.join("input.ir.json");
        let output_path = dir.join("lint-report.json");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "in");
        rflux_io::write_ir_json(&input_path, &netlist).expect("ir json should write");

        run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Ir,
            output: Some(output_path.clone()),
        })
        .expect("lint-input should succeed");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("lint report should exist"),
        )
        .expect("lint report should be valid json");

        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["kind"], "lint_input");
        assert_eq!(report["input_kind"], "ir");
        assert_eq!(report["contract_kind"], "rflux_ir_netlist");
        assert_eq!(report["schema_format"], "versioned_envelope");
        assert_eq!(report["input_schema_version"], 1);
        assert_eq!(report["legacy_compatibility_used"], false);
    }

    #[test]
    fn run_lint_input_reports_legacy_ir_contract() {
        let dir = unique_test_dir("lint-input-ir-legacy");
        let input_path = dir.join("legacy.ir.json");
        let output_path = dir.join("lint-report.json");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "legacy");
        fs::write(
            &input_path,
            serde_json::to_string_pretty(&netlist).expect("legacy ir should serialize"),
        )
        .expect("legacy ir json should write");

        run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Ir,
            output: Some(output_path.clone()),
        })
        .expect("legacy lint-input should succeed");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("lint report should exist"),
        )
        .expect("lint report should be valid json");

        assert_eq!(report["schema_format"], "legacy_raw_json");
        assert!(report["input_schema_version"].is_null());
        assert_eq!(report["legacy_compatibility_used"], true);
    }

    #[test]
    fn render_cli_error_uses_rflow_code_for_io_errors() {
        let error = read_ir_json("missing-file.ir.json")
            .with_context(|| "failed to read IR JSON from missing-file.ir.json")
            .expect_err("missing file should fail");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-INPUT-001]: failed to read IR JSON from missing-file.ir.json"));
        assert!(rendered.contains("detail: io error:"));
        assert!(rendered.contains("next: Check that the input path exists and is readable, then retry."));
    }

    #[test]
    fn render_cli_error_falls_back_for_non_io_errors() {
        let error = anyhow!("plain failure");

        let rendered = render_cli_error(&error);

        assert_eq!(rendered, "error: plain failure");
    }
}