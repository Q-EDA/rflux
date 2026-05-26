use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::BTreeMap, env};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rflux_flow::{FlowConfig, FlowRunner, SimulationConfig, SimulationMode};
use rflux_io::{
    detect_netlist_input_format, read_netlist, read_netlist_as, read_pdk_json,
    write_pdk_json, IoError, NetlistInputFormat,
};
use rflux_sat::{solve_with_metrics, CnfFormula, IncrementalSolver, Lit, SolveResult, SolveStats};
use rflux_ir::NodeKind;
use rflux_sim::{simulate_file, SimulationError, SimulationReport};
use rflux_tech::Pdk;
use rflux_verify::{SynthError, Verifier};
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
    PdkValidate(PdkValidateArgs),
    LintInput(LintInputArgs),
    CollectDiagnostics(CollectDiagnosticsArgs),
    RunWithDiagnostics(RunWithDiagnosticsArgs),
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
struct PdkValidateArgs {
    #[arg(long)]
    input: PathBuf,
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
struct RunWithDiagnosticsArgs {
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, value_enum)]
    kind: DiagnosticsCommandKind,
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long)]
    rhs: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = CliSimulationMode::Auto)]
    mode: CliSimulationMode,
    #[arg(long)]
    external_command: Option<String>,
    #[arg(long)]
    notes: Option<String>,
    #[arg(long)]
    assumptions: Option<String>,
    #[arg(long)]
    equivalence_metadata: Option<PathBuf>,
    #[arg(long)]
    check_ref: Option<String>,
    #[arg(long, value_enum)]
    equivalence_kind: Option<CliEquivalenceKind>,
    #[arg(long)]
    dimacs_output: Option<PathBuf>,
    #[arg(long, value_enum)]
    input_kind: Option<CliInputKind>,
    #[arg(long, value_enum, default_value_t = CliNetlistInputFormat::Auto)]
    input_format: CliNetlistInputFormat,
    #[arg(long, value_enum, default_value_t = CliNetlistInputFormat::Auto)]
    rhs_format: CliNetlistInputFormat,
}

#[derive(Debug, Args)]
struct CompileNetlistArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum, default_value_t = CliNetlistInputFormat::Auto)]
    input_format: CliNetlistInputFormat,
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
    #[arg(long, value_enum, default_value_t = CliNetlistInputFormat::Auto)]
    input_format: CliNetlistInputFormat,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct VerifyLayoutArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum, default_value_t = CliNetlistInputFormat::Auto)]
    input_format: CliNetlistInputFormat,
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
    #[arg(long, value_enum, default_value_t = CliNetlistInputFormat::Auto)]
    lhs_format: CliNetlistInputFormat,
    #[arg(long)]
    rhs: PathBuf,
    #[arg(long, value_enum, default_value_t = CliNetlistInputFormat::Auto)]
    rhs_format: CliNetlistInputFormat,
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
    #[value(name = "bench")]
    Bench,
    #[value(name = "pdk")]
    Pdk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliNetlistInputFormat {
    #[value(name = "auto")]
    Auto,
    #[value(name = "ir", alias = "ir-json")]
    Ir,
    #[value(name = "bench")]
    Bench,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum DiagnosticsCommandKind {
    #[value(name = "analyze-timing")]
    AnalyzeTiming,
    #[value(name = "check-equivalence")]
    CheckEquivalence,
    #[value(name = "compile-netlist")]
    CompileNetlist,
    #[value(name = "compile-layout")]
    CompileLayout,
    #[value(name = "lint-input")]
    LintInput,
    #[value(name = "pdk-validate")]
    PdkValidate,
    #[value(name = "solve-dimacs")]
    SolveDimacs,
    #[value(name = "simulate-file")]
    SimulateFile,
    #[value(name = "verify-layout")]
    VerifyLayout,
}

fn main() {
    let cli = Cli::parse();

    if let Err(error) = run(cli) {
        eprintln!("{}", render_cli_error(&error));
        std::process::exit(1);
    }
}

fn render_cli_error(error: &anyhow::Error) -> String {
    let classification = classify_cli_error(error);
    format!(
        "error[{}]: {}\n  detail: {}\n  next: {}",
        classification.code,
        error,
        cli_error_detail(error),
        classification.suggestion
    )
}

fn find_io_error(error: &anyhow::Error) -> Option<&IoError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<IoError>())
}

fn find_simulation_error(error: &anyhow::Error) -> Option<&SimulationError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<SimulationError>())
}
fn find_synth_error(error: &anyhow::Error) -> Option<&SynthError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<SynthError>())
}

struct CliErrorClassification {
    code: &'static str,
    suggestion: &'static str,
}

fn classify_cli_error(error: &anyhow::Error) -> CliErrorClassification {
    if let Some(io_error) = find_io_error(error) {
        return CliErrorClassification {
            code: io_error.code(),
            suggestion: io_error.suggestion(),
        };
    }

    if let Some(simulation_error) = find_simulation_error(error) {
        return classify_simulation_error(simulation_error);
    }
    if let Some(synth_error) = find_synth_error(error) {
        return classify_synth_error(synth_error);
    }

    match error.to_string().as_str() {
        "compile-netlist failed" => CliErrorClassification {
            code: "RFLOW-FLOW-001",
            suggestion: "Validate the IR/PDK inputs and current SFQ mapping constraints, then retry compile-netlist.",
        },
        "analyze-timing failed" => CliErrorClassification {
            code: "RFLOW-FLOW-004",
            suggestion: "Check that the netlist and PDK provide the timing data required by analyze-timing, then retry.",
        },
        "verify-layout failed" => CliErrorClassification {
            code: "RFLOW-VERIFY-003",
            suggestion: "Inspect the verification report or rerun with diagnostics to identify the violated structural or simulation-backed layout checks.",
        },
        _ => CliErrorClassification {
            code: "RFLOW-INTERNAL-001",
            suggestion: "Retry with run-with-diagnostics or collect-diagnostics and attach the bundle when reporting the issue.",
        },
    }
}

fn classify_simulation_error(error: &SimulationError) -> CliErrorClassification {
    match error {
        SimulationError::Io { .. } => CliErrorClassification {
            code: "RFLOW-INPUT-001",
            suggestion: "Check that the deck path exists and is readable, then retry simulate-file.",
        },
        SimulationError::MissingTran
        | SimulationError::InvalidSubcktHeader(_)
        | SimulationError::MissingEnds(_)
        | SimulationError::MismatchedEnds { .. }
        | SimulationError::UnknownSubckt(_)
        | SimulationError::InvalidSubcktInstance(_)
        | SimulationError::InvalidParamAssignment(_)
        | SimulationError::InvalidNumericValue(_)
        | SimulationError::UnknownParameter(_)
        | SimulationError::InvalidTran(_) => CliErrorClassification {
            code: "RFLOW-SIM-001",
            suggestion: "Validate the deck syntax against the supported SPICE/JoSIM subset, then retry simulate-file.",
        },
        SimulationError::IncludeWithoutBase(_)
        | SimulationError::NestedSubcktDefinition(_)
        | SimulationError::UnsupportedSubcktControl { .. }
        | SimulationError::UnsupportedSubcktInstanceSyntax(_)
        | SimulationError::UnsupportedExpression(_) => CliErrorClassification {
            code: "RFLOW-SIM-002",
            suggestion: "Run with --mode external_josim or simplify the deck to the currently supported internal subset.",
        },
    }
}
fn classify_synth_error(error: &SynthError) -> CliErrorClassification {
    match error {
        SynthError::SatInterfaceMismatch(_) => CliErrorClassification {
            code: "RFLOW-VERIFY-001",
            suggestion: "Align the compared netlists so they expose the same named inputs, outputs, and state interface before retrying check-equivalence.",
        },
        SynthError::SatUnsupportedNodeKind {
            kind: NodeKind::Dff,
            ..
        } => CliErrorClassification {
            code: "RFLOW-VERIFY-002",
            suggestion: "Use --kind single_step_sequential for sequential netlists, or reduce the check to a combinational subset.",
        },
        SynthError::SatEncoding(message)
            if message == "DffEnable is sequential and not supported in combinational SAT check" =>
        {
            CliErrorClassification {
                code: "RFLOW-VERIFY-002",
                suggestion: "Use --kind single_step_sequential for sequential netlists, or reduce the check to a combinational subset.",
            }
        }
        _ => CliErrorClassification {
            code: "RFLOW-INTERNAL-001",
            suggestion: "Retry with run-with-diagnostics or collect-diagnostics and attach the bundle when reporting the issue.",
        },
    }
}

fn cli_error_detail(error: &anyhow::Error) -> String {
    error
        .chain()
        .nth(1)
        .map(|cause| cause.to_string())
        .unwrap_or_else(|| error.to_string())
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::PdkMinimal(args) => run_pdk_minimal(args),
        Commands::PdkValidate(args) => run_pdk_validate(args),
        Commands::LintInput(args) => run_lint_input(args),
        Commands::CollectDiagnostics(args) => run_collect_diagnostics(args),
        Commands::RunWithDiagnostics(args) => run_with_diagnostics(args),
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

fn run_pdk_validate(args: PdkValidateArgs) -> Result<()> {
    let report = build_pdk_validate_report(&args.input)?;
    emit_json(&with_schema_version(report), args.output.as_deref())
}

fn run_lint_input(args: LintInputArgs) -> Result<()> {
    let report = build_lint_input_report(&args.input, args.kind)?;

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

fn run_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    match args.kind {
        DiagnosticsCommandKind::AnalyzeTiming => run_analyze_timing_with_diagnostics(args),
        DiagnosticsCommandKind::CheckEquivalence => run_check_equivalence_with_diagnostics(args),
        DiagnosticsCommandKind::CompileNetlist => run_compile_netlist_with_diagnostics(args),
        DiagnosticsCommandKind::CompileLayout => run_compile_layout_with_diagnostics(args),
        DiagnosticsCommandKind::LintInput => run_lint_input_with_diagnostics(args),
        DiagnosticsCommandKind::PdkValidate => run_pdk_validate_with_diagnostics(args),
        DiagnosticsCommandKind::SolveDimacs => run_solve_dimacs_with_diagnostics(args),
        DiagnosticsCommandKind::SimulateFile => run_simulate_file_with_diagnostics(args),
        DiagnosticsCommandKind::VerifyLayout => run_verify_layout_with_diagnostics(args),
    }
}

fn prepare_diagnostics_bundle(
    output_dir: &Path,
    kind: DiagnosticsCommandKind,
) -> Result<(PathBuf, PathBuf, PathBuf, Vec<Value>)> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create diagnostics directory {}", output_dir.display()))?;
    let inputs_dir = output_dir.join("inputs");
    let reports_dir = output_dir.join("reports");
    fs::create_dir_all(&inputs_dir)
        .with_context(|| format!("failed to create diagnostics inputs directory {}", inputs_dir.display()))?;
    fs::create_dir_all(&reports_dir)
        .with_context(|| format!("failed to create diagnostics reports directory {}", reports_dir.display()))?;

    let event_log_path = output_dir.join("events.jsonl");
    let event_log = vec![diagnostics_event(
        "bundle_started",
        json!({
            "output_dir": display_path(output_dir),
            "kind": diagnostics_command_kind_name(kind),
        }),
    )?];

    Ok((inputs_dir, reports_dir, event_log_path, event_log))
}

fn capture_input_and_optional_pdk_for_bundle(
    event_log: &mut Vec<Value>,
    inputs_dir: &Path,
    input: &Path,
    pdk: Option<&Path>,
) -> Result<Vec<Value>> {
    let mut captured_inputs = vec![capture_diagnostics_input_for_bundle(
        event_log,
        inputs_dir,
        "input",
        input,
    )?];
    if let Some(pdk) = pdk {
        captured_inputs.push(capture_diagnostics_input_for_bundle(
            event_log,
            inputs_dir,
            "pdk",
            pdk,
        )?);
    }
    Ok(captured_inputs)
}

fn push_input_and_optional_pdk_command_started_event(
    event_log: &mut Vec<Value>,
    kind: DiagnosticsCommandKind,
    input: &Path,
    pdk: Option<&Path>,
) -> Result<()> {
    event_log.push(diagnostics_event(
        "command_started",
        json!({
            "kind": diagnostics_command_kind_name(kind),
            "input": input.display().to_string(),
            "pdk": pdk.map(|path| path.display().to_string()),
        }),
    )?);
    Ok(())
}

fn push_input_pdk_simulation_command_started_event(
    event_log: &mut Vec<Value>,
    kind: DiagnosticsCommandKind,
    input: &Path,
    pdk: Option<&Path>,
    mode: CliSimulationMode,
    external_command: Option<&str>,
) -> Result<()> {
    event_log.push(diagnostics_event(
        "command_started",
        json!({
            "kind": diagnostics_command_kind_name(kind),
            "input": input.display().to_string(),
            "pdk": pdk.map(|path| path.display().to_string()),
            "mode": simulation_mode_name(mode),
            "external_command": external_command,
        }),
    )?);
    Ok(())
}

fn diagnostics_input_pdk_command_configuration(
    kind: DiagnosticsCommandKind,
    notes: Option<&str>,
    input: &Path,
    pdk: Option<&Path>,
    output_dir: &Path,
    extra: Value,
) -> Value {
    let mut configuration = json!({
        "command": diagnostics_command_kind_name(kind),
        "notes": notes,
        "paths": {
            "input": display_path(input),
            "pdk": pdk.map(display_path),
            "output_dir": display_path(output_dir),
        },
    });

    if let (Value::Object(configuration_map), Value::Object(extra_map)) = (&mut configuration, extra) {
        configuration_map.extend(extra_map);
    }

    configuration
}

fn run_lint_input_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let input_kind = args
        .input_kind
        .ok_or_else(|| anyhow!("run-with-diagnostics --kind lint-input requires --input-kind ir|bench|pdk"))?;
    let input_role = lint_input_role(input_kind);

    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;
    let captured_input = capture_diagnostics_input_for_bundle(
        &mut event_log,
        &inputs_dir,
        input_role,
        &args.input,
    )?;

    event_log.push(diagnostics_event(
        "command_started",
        json!({
            "kind": diagnostics_command_kind_name(args.kind),
            "input": args.input.display().to_string(),
            "input_kind": cli_input_kind_name(input_kind),
        }),
    )?);

    let run_result = build_lint_input_report(&args.input, input_kind).map(with_schema_version);

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report_json) => diagnostics_success_outcome(
            &reports_dir,
            "lint-input-report.json",
            &report_json,
            json!({
                "status": "succeeded",
                "report_kind": report_json["kind"].clone(),
                "input_kind": report_json["input_kind"].clone(),
                "delay_detail_count": 0,
                "measurement_detail_count": 0,
                "measurement_warning_count": 0,
                "violation_detail_count": 0,
            }),
        )?,
        Err(error) => diagnostics_failure_outcome(error)?,
    };
    event_log.push(completion_event);

    let captured_inputs = vec![captured_input];
    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = json!({
        "command": diagnostics_command_kind_name(args.kind),
        "notes": args.notes,
        "paths": {
            "input": display_path(&args.input),
            "output_dir": display_path(&args.output_dir),
        },
        "lint_input": {
            "kind": cli_input_kind_name(input_kind),
        },
    });
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        Value::Null,
        Value::Null,
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )

}

fn run_pdk_validate_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;
    let captured_input = capture_diagnostics_input_for_bundle(
        &mut event_log,
        &inputs_dir,
        "pdk",
        &args.input,
    )?;

    event_log.push(diagnostics_event(
        "command_started",
        json!({
            "kind": diagnostics_command_kind_name(args.kind),
            "input": args.input.display().to_string(),
        }),
    )?);

    let run_result = build_pdk_validate_report(&args.input).map(with_schema_version);

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report_json) => diagnostics_success_outcome(
            &reports_dir,
            "pdk-validate-report.json",
            &report_json,
            json!({
                "status": "succeeded",
                "report_kind": report_json["kind"].clone(),
                "ok": report_json["ok"].clone(),
            }),
        )?,
        Err(error) => diagnostics_failure_outcome(error)?,
    };
    event_log.push(completion_event);

    let captured_inputs = vec![captured_input];
    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = json!({
        "command": diagnostics_command_kind_name(args.kind),
        "notes": args.notes,
        "paths": {
            "input": display_path(&args.input),
            "output_dir": display_path(&args.output_dir),
        },
        "pdk_validate": {
            "input_kind": "pdk",
        },
    });
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        Value::Null,
        Value::Null,
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )
}

fn run_check_equivalence_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let rhs = args
        .rhs
        .as_deref()
        .ok_or_else(|| anyhow!("run-with-diagnostics --kind check-equivalence requires --rhs PATH"))?;
    let equivalence_kind = args.equivalence_kind.unwrap_or(CliEquivalenceKind::Combinational);

    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;

    let mut captured_inputs = vec![capture_diagnostics_input_for_bundle(
        &mut event_log,
        &inputs_dir,
        "lhs",
        &args.input,
    )?];
    captured_inputs.push(capture_diagnostics_input_for_bundle(
        &mut event_log,
        &inputs_dir,
        "rhs",
        rhs,
    )?);

    event_log.push(diagnostics_event(
        "command_started",
        json!({
            "kind": diagnostics_command_kind_name(args.kind),
            "lhs": args.input.display().to_string(),
            "rhs": rhs.display().to_string(),
            "equivalence_kind": equivalence_kind_name(equivalence_kind),
            "dimacs_output": args.dimacs_output.as_ref().map(|path| path.display().to_string()),
            "lhs_format": cli_netlist_input_format_name(args.input_format),
            "rhs_format": cli_netlist_input_format_name(args.rhs_format),
        }),
    )?);

    let run_result = (|| -> Result<Value> {
        let (lhs_netlist, rhs_netlist) = load_equivalence_netlists(
            &args.input,
            args.input_format,
            rhs,
            args.rhs_format,
        )?;
        let verifier = Verifier::new();

        match equivalence_kind {
            CliEquivalenceKind::Combinational => {
                let report = verifier
                    .check_boolean_equivalence(&lhs_netlist, &rhs_netlist)
                    .context("combinational equivalence check failed")?;
                let mut report_json = combinational_equivalence_report_to_json(&report);
                let dimacs_export = args
                    .dimacs_output
                    .as_deref()
                    .map(|path| {
                        verifier
                            .build_boolean_equivalence_problem(&lhs_netlist, &rhs_netlist)
                            .context("combinational equivalence DIMACS export failed")
                            .and_then(|problem| write_equivalence_dimacs_bundle(path, &problem))
                    })
                    .transpose()?;
                attach_dimacs_export(&mut report_json, dimacs_export);
                Ok(with_schema_version(report_json))
            }
            CliEquivalenceKind::SingleStepSequential => {
                let report = verifier
                    .check_single_step_sequential_equivalence(&lhs_netlist, &rhs_netlist)
                    .context("single-step sequential equivalence check failed")?;
                let mut report_json = single_step_sequential_equivalence_report_to_json(&report);
                let dimacs_export = args
                    .dimacs_output
                    .as_deref()
                    .map(|path| {
                        verifier
                            .build_single_step_sequential_equivalence_problem(&lhs_netlist, &rhs_netlist)
                            .context("single-step sequential equivalence DIMACS export failed")
                            .and_then(|problem| write_equivalence_dimacs_bundle(path, &problem))
                    })
                    .transpose()?;
                attach_dimacs_export(&mut report_json, dimacs_export);
                Ok(with_schema_version(report_json))
            }
        }
    })();

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report_json) => diagnostics_success_outcome(
            &reports_dir,
            "check-equivalence-report.json",
            &report_json,
            json!({
                "status": "succeeded",
                "report_kind": report_json["kind"].clone(),
                "equivalent": report_json["equivalent"].clone(),
            }),
        ),
        Err(error) => diagnostics_failure_outcome(error),
    }?;
    event_log.push(completion_event);

    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = json!({
        "command": diagnostics_command_kind_name(args.kind),
        "notes": args.notes,
        "paths": {
            "lhs": display_path(&args.input),
            "rhs": display_path(rhs),
            "dimacs_output": args.dimacs_output.as_ref().map(|path| display_path(path)),
            "output_dir": display_path(&args.output_dir),
        },
        "equivalence": {
            "kind": equivalence_kind_name(equivalence_kind),
            "lhs_format": cli_netlist_input_format_name(args.input_format),
            "rhs_format": cli_netlist_input_format_name(args.rhs_format),
        },
    });
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        Value::Null,
        Value::Null,
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )
}

fn run_solve_dimacs_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;

    let mut captured_inputs = vec![capture_diagnostics_input_for_bundle(
        &mut event_log,
        &inputs_dir,
        "dimacs_input",
        &args.input,
    )?];
    if let Some(metadata) = args.equivalence_metadata.as_deref() {
        captured_inputs.push(capture_diagnostics_input_for_bundle(
            &mut event_log,
            &inputs_dir,
            "equivalence_metadata",
            metadata,
        )?);
    }

    event_log.push(diagnostics_event(
        "command_started",
        json!({
            "kind": diagnostics_command_kind_name(args.kind),
            "input": args.input.display().to_string(),
            "assumptions": args.assumptions,
            "equivalence_metadata": args.equivalence_metadata.as_ref().map(|path| path.display().to_string()),
            "check_ref": args.check_ref,
        }),
    )?);

    let run_result = (|| -> Result<Value> {
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

        Ok(with_schema_version(dimacs_solve_report_to_json(
            &args.input,
            &cnf,
            &assumptions,
            unsat_core.as_deref(),
            metadata_selection.as_ref(),
            &result,
            &metrics,
        )))
    })();

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report_json) => diagnostics_success_outcome(
            &reports_dir,
            "solve-dimacs-report.json",
            &report_json,
            json!({
                "status": "succeeded",
                "report_kind": "dimacs_sat",
                "satisfiable": report_json["satisfiable"].clone(),
                "variables": report_json["variables"].clone(),
                "clauses": report_json["clauses"].clone(),
            }),
        ),
        Err(error) => diagnostics_failure_outcome(error),
    }?;
    event_log.push(completion_event);

    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = json!({
        "command": diagnostics_command_kind_name(args.kind),
        "notes": args.notes,
        "paths": {
            "input": display_path(&args.input),
            "equivalence_metadata": args.equivalence_metadata.as_ref().map(|path| display_path(path)),
            "output_dir": display_path(&args.output_dir),
        },
        "solve": {
            "assumptions": args.assumptions,
            "check_ref": args.check_ref,
        },
    });
    event_log.push(diagnostics_event(
        "manifest_prepared",
        json!({
            "captured_input_count": captured_inputs.len(),
            "captured_report_count": captured_reports.len(),
            "execution_status": execution["status"].clone(),
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
            "command": diagnostics_command_kind_name(args.kind),
            "working_directory": env::current_dir()
                .context("failed to read current working directory")?
                .display()
                .to_string(),
            "mode": Value::Null,
            "external_command": Value::Null,
            "notes": args.notes,
        },
        "environment": collect_diagnostics_environment(),
        "configuration": configuration,
        "summary": summary,
        "execution": execution,
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

fn run_compile_netlist_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;

    let captured_inputs = capture_input_and_optional_pdk_for_bundle(
        &mut event_log,
        &inputs_dir,
        &args.input,
        args.pdk.as_deref(),
    )?;

    push_input_and_optional_pdk_command_started_event(
        &mut event_log,
        args.kind,
        &args.input,
        args.pdk.as_deref(),
    )?;

    let run_result = load_cli_netlist_and_pdk(&args.input, args.input_format, args.pdk.clone()).and_then(|(mut netlist, pdk)| {
        with_flow_runner(|flow| {
            flow.compile_artifacts_for_cli_netlist(&mut netlist, &pdk)
                .context("compile-netlist failed")
        })
    });

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report) => {
            let mut report_json = synthesis_report_to_json(&report);
            if let Value::Object(ref mut object) = report_json {
                object.insert("kind".to_string(), json!("compile_netlist"));
            }
            let report_json = with_schema_version(report_json);
            diagnostics_success_outcome(
                &reports_dir,
                "compile-netlist-report.json",
                &report_json,
                json!({
                    "status": "succeeded",
                    "report_kind": "compile_netlist",
                    "connections_applied": report.compile.connections_applied,
                    "mapped_nodes": report.tech_map.mapped_nodes,
                }),
            )
        }
        Err(error) => diagnostics_failure_outcome(error),
    }?;
    event_log.push(completion_event);

    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = diagnostics_input_pdk_command_configuration(
        args.kind,
        args.notes.as_deref(),
        &args.input,
        args.pdk.as_deref(),
        &args.output_dir,
        json!({
            "flow": {
                "uses_default_flow_config": true,
            },
            "input_format": cli_netlist_input_format_name(args.input_format),
        }),
    );
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        Value::Null,
        Value::Null,
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )
}

fn run_analyze_timing_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;

    let captured_inputs = capture_input_and_optional_pdk_for_bundle(
        &mut event_log,
        &inputs_dir,
        &args.input,
        args.pdk.as_deref(),
    )?;

    push_input_and_optional_pdk_command_started_event(
        &mut event_log,
        args.kind,
        &args.input,
        args.pdk.as_deref(),
    )?;

    let run_result = with_loaded_flow_inputs(&args.input, args.input_format, args.pdk.clone(), |flow, netlist, pdk| {
        flow.analyze_timing(netlist, pdk, &FlowConfig::default())
            .context("analyze-timing failed")
    });

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report) => {
            let mut report_json = timing_analysis_to_json(&report);
            if let Value::Object(ref mut object) = report_json {
                object.insert("kind".to_string(), json!("timing_analysis"));
            }
            let report_json = with_schema_version(report_json);
            diagnostics_success_outcome(
                &reports_dir,
                "analyze-timing-report.json",
                &report_json,
                json!({
                    "status": "succeeded",
                    "report_kind": "timing_analysis",
                    "analyzed_arcs": report.analyzed_arcs,
                    "critical_path_delay_ps": report.critical_path_delay_ps,
                }),
            )
        }
        Err(error) => diagnostics_failure_outcome(error),
    }?;
    event_log.push(completion_event);

    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = diagnostics_input_pdk_command_configuration(
        args.kind,
        args.notes.as_deref(),
        &args.input,
        args.pdk.as_deref(),
        &args.output_dir,
        json!({
            "flow": {
                "uses_default_flow_config": true,
            },
            "input_format": cli_netlist_input_format_name(args.input_format),
        }),
    );
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        Value::Null,
        Value::Null,
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )
}

fn run_compile_layout_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;

    let captured_inputs = capture_input_and_optional_pdk_for_bundle(
        &mut event_log,
        &inputs_dir,
        &args.input,
        args.pdk.as_deref(),
    )?;

    push_input_and_optional_pdk_command_started_event(
        &mut event_log,
        args.kind,
        &args.input,
        args.pdk.as_deref(),
    )?;

    let run_result = with_loaded_flow_inputs(&args.input, args.input_format, args.pdk.clone(), |flow, netlist, pdk| {
        flow.compile_layout(netlist, pdk, &FlowConfig::default())
            .context("compile-layout failed")
    });

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report) => {
            let mut report_json = layout_report_to_json(&report);
            if let Value::Object(ref mut object) = report_json {
                object.insert("kind".to_string(), json!("compile_layout"));
            }
            let report_json = with_schema_version(report_json);
            diagnostics_success_outcome(
                &reports_dir,
                "compile-layout-report.json",
                &report_json,
                json!({
                    "status": "succeeded",
                    "report_kind": "compile_layout",
                    "placed_nodes": report.placement.placed_nodes,
                    "routed_nets": report.routing.routed_nets,
                }),
            )
        }
        Err(error) => diagnostics_failure_outcome(error),
    }?;
    event_log.push(completion_event);

    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = diagnostics_input_pdk_command_configuration(
        args.kind,
        args.notes.as_deref(),
        &args.input,
        args.pdk.as_deref(),
        &args.output_dir,
        json!({
            "flow": {
                "uses_default_flow_config": true,
            },
            "input_format": cli_netlist_input_format_name(args.input_format),
        }),
    );
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        Value::Null,
        Value::Null,
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )
}

fn run_simulate_file_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;

    let captured_input = capture_diagnostics_input_for_bundle(&mut event_log, &inputs_dir, "input", &args.input)?;
    let captured_inputs = vec![captured_input];
    event_log.push(diagnostics_event(
        "command_started",
        json!({
            "kind": diagnostics_command_kind_name(args.kind),
            "input": args.input.display().to_string(),
            "mode": simulation_mode_name(args.mode),
            "external_command": args.external_command,
        }),
    )?);

    let simulation_config = build_simulation_config(args.mode, args.external_command.clone());
    let run_result = simulate_file(&args.input, &simulation_config)
        .with_context(|| format!("simulate-file failed for {}", args.input.display()));

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report) => {
            let mut report_json = simulation_report_to_json(&report);
            if let Value::Object(ref mut object) = report_json {
                object.insert("kind".to_string(), json!("simulate_file"));
            }
            let report_json = with_schema_version(report_json);
            diagnostics_success_outcome(
                &reports_dir,
                "simulate-file-report.json",
                &report_json,
                json!({
                    "status": "succeeded",
                    "report_kind": "simulate_file",
                    "simulated_events": report.simulated_events,
                    "reported_violations": report.reported_violations,
                    "delay_detail_count": report.delay_details.len(),
                    "measurement_detail_count": report.measurement_details.len(),
                    "measurement_warning_count": report.measurement_warnings.len(),
                    "violation_detail_count": report.violation_details.len(),
                }),
            )
        }
        Err(error) => diagnostics_failure_outcome(error),
    }?;
    event_log.push(completion_event);

    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = json!({
        "command": diagnostics_command_kind_name(args.kind),
        "notes": args.notes,
        "paths": {
            "input": display_path(&args.input),
            "pdk": args.pdk.as_ref().map(|path| display_path(path)),
            "output_dir": display_path(&args.output_dir),
        },
        "simulation": {
            "mode": simulation_mode_name(args.mode),
            "external_command": args.external_command,
        },
    });
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        json!(simulation_mode_name(args.mode)),
        json!(args.external_command),
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )
}

fn run_verify_layout_with_diagnostics(args: RunWithDiagnosticsArgs) -> Result<()> {
    let (inputs_dir, reports_dir, event_log_path, mut event_log) =
        prepare_diagnostics_bundle(&args.output_dir, args.kind)?;

    let captured_inputs = capture_input_and_optional_pdk_for_bundle(
        &mut event_log,
        &inputs_dir,
        &args.input,
        args.pdk.as_deref(),
    )?;

    push_input_pdk_simulation_command_started_event(
        &mut event_log,
        args.kind,
        &args.input,
        args.pdk.as_deref(),
        args.mode,
        args.external_command.as_deref(),
    )?;

    let simulation_config = build_simulation_config(args.mode, args.external_command.clone());
    let run_result = with_loaded_flow_inputs(&args.input, args.input_format, args.pdk.clone(), |flow, netlist, pdk| {
        flow.verify_layout(netlist, pdk, &FlowConfig::default(), &simulation_config)
            .context("verify-layout failed")
    });

    let (captured_reports, execution, completion_event) = match run_result {
        Ok(report) => {
            let mut report_json = verification_report_to_json(&report);
            if let Value::Object(ref mut object) = report_json {
                object.insert("kind".to_string(), json!("verify_layout"));
            }
            let report_json = with_schema_version(report_json);
            diagnostics_success_outcome(
                &reports_dir,
                "verify-layout-report.json",
                &report_json,
                json!({
                    "status": "succeeded",
                    "report_kind": "verify_layout",
                    "checked_routes": report.checked_routes,
                    "structural_violations": report.structural_violations,
                    "simulated_events": report.simulation.simulated_events,
                    "delay_detail_count": report.simulation.delay_details.len(),
                    "measurement_detail_count": report.simulation.measurement_details.len(),
                    "measurement_warning_count": report.simulation.measurement_warnings.len(),
                    "violation_detail_count": report.simulation.violation_details.len(),
                }),
            )
        }
        Err(error) => diagnostics_failure_outcome(error),
    }?;
    event_log.push(completion_event);

    let summary = build_diagnostics_summary(
        Some(diagnostics_command_kind_name(args.kind)),
        &captured_inputs,
        &captured_reports,
    );
    let configuration = diagnostics_input_pdk_command_configuration(
        args.kind,
        args.notes.as_deref(),
        &args.input,
        args.pdk.as_deref(),
        &args.output_dir,
        json!({
            "simulation": {
                "mode": simulation_mode_name(args.mode),
                "external_command": args.external_command,
            },
            "input_format": cli_netlist_input_format_name(args.input_format),
        }),
    );
    write_diagnostics_bundle_manifest(
        &args.output_dir,
        diagnostics_command_kind_name(args.kind),
        json!(simulation_mode_name(args.mode)),
        json!(args.external_command),
        args.notes.as_deref(),
        configuration,
        summary,
        execution,
        captured_inputs,
        captured_reports,
        &event_log_path,
        &mut event_log,
    )
}

fn capture_diagnostics_input_for_bundle(
    event_log: &mut Vec<Value>,
    inputs_dir: &Path,
    role: &str,
    source: &Path,
) -> Result<Value> {
    match capture_diagnostics_input(inputs_dir, role, source) {
        Ok(captured_input) => {
            event_log.push(diagnostics_event(
                "input_captured",
                json!({
                    "role": role,
                    "source_path": source.display().to_string(),
                }),
            )?);
            Ok(captured_input)
        }
        Err(error) => {
            event_log.push(diagnostics_event(
                "input_capture_failed",
                json!({
                    "role": role,
                    "source_path": source.display().to_string(),
                    "error": error.to_string(),
                }),
            )?);
            Ok(diagnostics_uncaptured_input(role, source, error))
        }
    }
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

fn diagnostics_uncaptured_input(role: &str, source: &Path, error: anyhow::Error) -> Value {
    json!({
        "role": role,
        "source_path": source.display().to_string(),
        "bundle_path": Value::Null,
        "bytes": Value::Null,
        "contract": Value::Null,
        "capture_error": error.to_string(),
    })
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

fn write_generated_diagnostics_report(reports_dir: &Path, file_name: &str, report: &Value) -> Result<Value> {
    let destination = reports_dir.join(file_name);
    emit_json(report, Some(destination.as_path()))?;
    let metadata = fs::metadata(&destination)
        .with_context(|| format!("failed to stat generated diagnostics report {}", destination.display()))?;

    Ok(json!({
        "source_path": Value::Null,
        "bundle_path": destination.display().to_string(),
        "bytes": metadata.len(),
        "report": diagnostics_report_snapshot(&destination),
    }))
}

fn diagnostics_success_outcome(
    reports_dir: &Path,
    file_name: &str,
    report: &Value,
    completion_fields: Value,
) -> Result<(Vec<Value>, Value, Value)> {
    let captured_report = write_generated_diagnostics_report(reports_dir, file_name, report)?;
    Ok((
        vec![captured_report.clone()],
        json!({
            "status": "succeeded",
            "error_code": Value::Null,
            "error_message": Value::Null,
            "stdout_summary": empty_stream_summary(),
            "stderr_summary": empty_stream_summary(),
            "report_path": captured_report["bundle_path"].clone(),
        }),
        diagnostics_event("command_completed", completion_fields)?,
    ))
}

fn diagnostics_failure_outcome(error: anyhow::Error) -> Result<(Vec<Value>, Value, Value)> {
    let rendered = render_cli_error(&error);
    let error_code = diagnostics_error_code(&error);
    Ok((
        Vec::new(),
        json!({
            "status": "failed",
            "error_code": error_code,
            "error_message": rendered,
            "stdout_summary": empty_stream_summary(),
            "stderr_summary": stream_summary_from_text(&rendered),
            "report_path": Value::Null,
        }),
        diagnostics_event(
            "command_failed",
            json!({
                "status": "failed",
                "error_code": diagnostics_error_code(&error),
            }),
        )?,
    ))
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
    let mut delay_detail_count = 0usize;
    let mut measurement_detail_count = 0usize;
    let mut measurement_warning_count = 0usize;
    let mut violation_detail_count = 0usize;

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
        delay_detail_count += diagnostics_report_detail_count(report, "delay_details", "delay_detail_count");
        measurement_detail_count += diagnostics_report_detail_count(
            report,
            "measurement_details",
            "measurement_detail_count",
        );
        measurement_warning_count += diagnostics_report_detail_count(
            report,
            "measurement_warnings",
            "measurement_warning_count",
        );
        violation_detail_count += diagnostics_report_detail_count(
            report,
            "violation_details",
            "violation_detail_count",
        );
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
        "delay_detail_count": delay_detail_count,
        "measurement_detail_count": measurement_detail_count,
        "measurement_warning_count": measurement_warning_count,
        "violation_detail_count": violation_detail_count,
        "report_inspection_failure_count": report_inspection_failures.len(),
        "report_inspection_failures": report_inspection_failures,
    })
}
fn diagnostics_report_detail_count(report: &Value, details_key: &str, count_key: &str) -> usize {
    report
        .get(details_key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .or_else(|| {
            report
                .get(count_key)
                .and_then(Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
        })
        .or_else(|| {
            report
                .get("simulation")
                .map(|simulation| diagnostics_report_detail_count(simulation, details_key, count_key))
        })
        .unwrap_or(0)
}

fn diagnostics_command_kind_name(kind: DiagnosticsCommandKind) -> &'static str {
    match kind {
        DiagnosticsCommandKind::AnalyzeTiming => "analyze-timing",
        DiagnosticsCommandKind::CheckEquivalence => "check-equivalence",
        DiagnosticsCommandKind::CompileNetlist => "compile-netlist",
        DiagnosticsCommandKind::CompileLayout => "compile-layout",
        DiagnosticsCommandKind::LintInput => "lint-input",
        DiagnosticsCommandKind::PdkValidate => "pdk-validate",
        DiagnosticsCommandKind::SolveDimacs => "solve-dimacs",
        DiagnosticsCommandKind::SimulateFile => "simulate-file",
        DiagnosticsCommandKind::VerifyLayout => "verify-layout",
    }
}

fn equivalence_kind_name(kind: CliEquivalenceKind) -> &'static str {
    match kind {
        CliEquivalenceKind::Combinational => "combinational",
        CliEquivalenceKind::SingleStepSequential => "single_step_sequential",
    }
}

fn cli_input_kind_name(kind: CliInputKind) -> &'static str {
    match kind {
        CliInputKind::Ir => "ir",
        CliInputKind::Bench => "bench",
        CliInputKind::Pdk => "pdk",
    }
}

fn lint_input_role(kind: CliInputKind) -> &'static str {
    match kind {
        CliInputKind::Ir => "input",
        CliInputKind::Bench => "input",
        CliInputKind::Pdk => "pdk",
    }
}

fn cli_netlist_input_format_name(format: CliNetlistInputFormat) -> &'static str {
    match format {
        CliNetlistInputFormat::Auto => "auto",
        CliNetlistInputFormat::Ir => "ir",
        CliNetlistInputFormat::Bench => "bench",
    }
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

fn write_diagnostics_bundle_manifest(
    output_dir: &Path,
    command: &str,
    invocation_mode: Value,
    external_command: Value,
    notes: Option<&str>,
    configuration: Value,
    summary: Value,
    execution: Value,
    captured_inputs: Vec<Value>,
    captured_reports: Vec<Value>,
    event_log_path: &Path,
    event_log: &mut Vec<Value>,
) -> Result<()> {
    event_log.push(diagnostics_event(
        "manifest_prepared",
        json!({
            "captured_input_count": captured_inputs.len(),
            "captured_report_count": captured_reports.len(),
            "execution_status": execution["status"].clone(),
        }),
    )?);
    write_diagnostics_event_log(event_log_path, event_log)?;

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
            "command": command,
            "working_directory": env::current_dir()
                .context("failed to read current working directory")?
                .display()
                .to_string(),
            "mode": invocation_mode,
            "external_command": external_command,
            "notes": notes,
        },
        "environment": collect_diagnostics_environment(),
        "configuration": configuration,
        "summary": summary,
        "execution": execution,
        "structured_logs": {
            "events_path": display_path(event_log_path),
            "event_count": event_log.len(),
            "format": "jsonl",
        },
        "captured_inputs": captured_inputs,
        "captured_reports": captured_reports,
    }));

    emit_json(&manifest, Some(output_dir.join("manifest.json").as_path()))
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn empty_stream_summary() -> Value {
    json!({
        "line_count": 0,
        "preview": Vec::<String>::new(),
    })
}

fn stream_summary_from_text(text: &str) -> Value {
    let preview = text.lines().take(20).map(str::to_string).collect::<Vec<_>>();
    json!({
        "line_count": text.lines().count(),
        "preview": preview,
    })
}

fn diagnostics_error_code(error: &anyhow::Error) -> String {
    classify_cli_error(error).code.to_string()
}

fn diagnostics_contract_snapshot(role: &str, source: &Path) -> Value {
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    let contract = match role {
        "input" | "lhs" | "rhs" if extension.as_deref() == Some("bench") => {
            Some(("bench", "quaigh_bench_subset"))
        }
        "input" | "lhs" | "rhs" => Some(("ir", "rflux_ir_netlist")),
        "pdk" => Some(("pdk", "rflux_pdk")),
        _ => None,
    };

    let Some((input_kind, contract_kind)) = contract else {
        return Value::Null;
    };

    if input_kind == "bench" {
        return json!({
            "input_kind": input_kind,
            "contract_kind": contract_kind,
            "schema_format": "bench_text",
            "input_schema_version": Value::Null,
            "legacy_compatibility_used": false,
            "inspection_error": Value::Null,
        });
    }

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
            "delay_detail_count": diagnostics_report_detail_count(&json, "delay_details", "delay_detail_count"),
            "measurement_detail_count": diagnostics_report_detail_count(
                &json,
                "measurement_details",
                "measurement_detail_count",
            ),
            "measurement_warning_count": diagnostics_report_detail_count(
                &json,
                "measurement_warnings",
                "measurement_warning_count",
            ),
            "violation_detail_count": diagnostics_report_detail_count(
                &json,
                "violation_details",
                "violation_detail_count",
            ),
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

fn lint_netlist_report(
    input: &Path,
    input_kind: &str,
    contract_kind: &str,
    format: NetlistInputFormat,
) -> Result<Value> {
    let validation_context = match format {
        NetlistInputFormat::IrJson => "IR JSON".to_string(),
        NetlistInputFormat::Bench => format!("{input_kind} netlist"),
    };
    let netlist = read_netlist_as(input, format).with_context(|| {
        format!(
            "failed to validate {} from {}",
            validation_context,
            input.display()
        )
    })?;
    let (schema_format, input_schema_version, legacy_compatibility_used) = match format {
        NetlistInputFormat::IrJson => {
            let (schema_format, input_schema_version) = inspect_json_contract(input)?;
            (
                schema_format,
                input_schema_version,
                schema_format == "legacy_raw_json",
            )
        }
        NetlistInputFormat::Bench => ("bench_text", None, false),
    };

    Ok(json!({
        "kind": "lint_input",
        "input": input.display().to_string(),
        "input_kind": input_kind,
        "contract_kind": contract_kind,
        "valid": true,
        "schema_format": schema_format,
        "input_schema_version": input_schema_version,
        "legacy_compatibility_used": legacy_compatibility_used,
        "netlist_summary": {
            "node_count": netlist.node_count(),
            "edge_count": netlist.edge_count(),
        },
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
    let (mut netlist, pdk) = load_cli_netlist_and_pdk(&args.input, args.input_format, args.pdk)?;
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
        args.input_format,
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
        args.input_format,
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
        args.input_format,
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
    let (lhs_netlist, rhs_netlist) = load_equivalence_netlists(
        &args.lhs,
        args.lhs_format,
        &args.rhs,
        args.rhs_format,
    )?;
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

fn resolve_cli_netlist_input_format(input: &Path, format: CliNetlistInputFormat) -> NetlistInputFormat {
    match format {
        CliNetlistInputFormat::Auto => detect_netlist_input_format(input),
        CliNetlistInputFormat::Ir => NetlistInputFormat::IrJson,
        CliNetlistInputFormat::Bench => NetlistInputFormat::Bench,
    }
}

fn load_cli_netlist(input: &Path, format: CliNetlistInputFormat) -> Result<rflux_ir::Netlist> {
    let resolved_format = resolve_cli_netlist_input_format(input, format);
    let load_result = match format {
        CliNetlistInputFormat::Auto => read_netlist(input),
        CliNetlistInputFormat::Ir | CliNetlistInputFormat::Bench => read_netlist_as(input, resolved_format),
    };
    load_result.with_context(|| match resolved_format {
        NetlistInputFormat::IrJson => format!("failed to read IR JSON from {}", input.display()),
        NetlistInputFormat::Bench => format!("failed to read bench netlist from {}", input.display()),
    })
}

fn load_cli_netlist_and_pdk(
    input: &Path,
    input_format: CliNetlistInputFormat,
    pdk_path: Option<PathBuf>,
) -> Result<(rflux_ir::Netlist, Pdk)> {
    let netlist = load_cli_netlist(input, input_format)?;
    let pdk = load_pdk(pdk_path)?;
    Ok((netlist, pdk))
}

fn load_equivalence_netlists(
    lhs: &Path,
    lhs_format: CliNetlistInputFormat,
    rhs: &Path,
    rhs_format: CliNetlistInputFormat,
) -> Result<(rflux_ir::Netlist, rflux_ir::Netlist)> {
    let lhs_netlist = load_cli_netlist(lhs, lhs_format)
        .with_context(|| format!("failed to read lhs netlist from {}", lhs.display()))?;
    let rhs_netlist = load_cli_netlist(rhs, rhs_format)
        .with_context(|| format!("failed to read rhs netlist from {}", rhs.display()))?;
    Ok((lhs_netlist, rhs_netlist))
}

fn with_flow_runner<T>(callback: impl FnOnce(&mut FlowRunner) -> Result<T>) -> Result<T> {
    let mut flow = FlowRunner::new();
    callback(&mut flow)
}

fn with_loaded_flow_inputs<T>(
    input: &Path,
    input_format: CliNetlistInputFormat,
    pdk_path: Option<PathBuf>,
    callback: impl FnOnce(&mut FlowRunner, &mut rflux_ir::Netlist, &Pdk) -> Result<T>,
) -> Result<T> {
    let (mut netlist, pdk) = load_cli_netlist_and_pdk(input, input_format, pdk_path)?;
    with_flow_runner(|flow| callback(flow, &mut netlist, &pdk))
}

fn run_flow_json_command<T>(
    input: &Path,
    input_format: CliNetlistInputFormat,
    pdk_path: Option<PathBuf>,
    output: Option<&Path>,
    execute: impl FnOnce(&mut FlowRunner, &mut rflux_ir::Netlist, &Pdk) -> Result<T>,
    render: impl FnOnce(&T) -> Value,
) -> Result<()> {
    let report = with_loaded_flow_inputs(input, input_format, pdk_path, execute)?;
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
        "measurement_details": report.measurement_details.iter().map(|detail| json!({
            "name": detail.name,
            "kind": detail.kind,
            "measured_value": detail.measured_value,
            "at_ref": detail.at_ref.as_ref().map(endpoint_ref_to_json),
        })).collect::<Vec<_>>(),
        "measurement_warnings": report.measurement_warnings.iter().map(|warning| json!({
            "name": warning.name,
            "kind": warning.kind,
            "reason": warning.reason,
            "at_ref": warning.at_ref.as_ref().map(endpoint_ref_to_json),
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
    use rflux_io::read_ir_json;
    use rflux_ir::{LogicOp, Netlist, NodeKind, PinRef};
    use rflux_tech::{
        CharacterizationArcDelay, CharacterizationArtifactMetadata, LengthRange,
        NamedCharacterizationMetadata,
    };

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

    fn quaigh_alignment_bench_fixture_paths() -> Vec<PathBuf> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("synth")
            .join("tests")
            .join("fixtures")
            .join("quaigh_alignment")
            .join("bench");
        let mut paths: Vec<PathBuf> = fs::read_dir(&fixture_dir)
            .expect("bench fixture directory should exist")
            .map(|entry| entry.expect("bench fixture entry should read").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "bench"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "expected checked-in bench fixtures");
        paths
    }

    fn quaigh_alignment_sequential_bench_fixture_paths() -> Vec<PathBuf> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("synth")
            .join("tests")
            .join("fixtures")
            .join("quaigh_alignment")
            .join("bench_sequential");
        let mut paths: Vec<PathBuf> = fs::read_dir(&fixture_dir)
            .expect("sequential bench fixture directory should exist")
            .map(|entry| entry.expect("sequential bench fixture entry should read").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "bench"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "expected checked-in sequential bench fixtures");
        paths
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
            lhs_format: CliNetlistInputFormat::Auto,
            rhs: rhs_path.clone(),
            rhs_format: CliNetlistInputFormat::Auto,
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
    fn run_check_equivalence_accepts_bench_inputs() {
        let dir = unique_test_dir("equivalence-bench");
        let lhs_path = dir.join("lhs.bench");
        let rhs_path = dir.join("rhs.bench");
        let report_path = dir.join("report.json");

        fs::write(
            &lhs_path,
            "INPUT(a)\nINPUT(b)\nout = XOR(a, b)\nOUTPUT(out)\n",
        )
        .expect("lhs bench should write");
        fs::write(
            &rhs_path,
            "INPUT(a)\nINPUT(b)\nout = XOR(b, a)\nOUTPUT(out)\n",
        )
        .expect("rhs bench should write");

        run_check_equivalence(CheckEquivalenceArgs {
            lhs: lhs_path,
            lhs_format: CliNetlistInputFormat::Auto,
            rhs: rhs_path,
            rhs_format: CliNetlistInputFormat::Auto,
            kind: CliEquivalenceKind::Combinational,
            dimacs_output: None,
            output: Some(report_path.clone()),
        })
        .expect("bench check-equivalence should succeed");

        let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).expect("report should exist"))
            .expect("report should be valid json");
        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["kind"], "combinational");
        assert_eq!(report["equivalent"], true);
    }

    #[test]
    fn run_check_equivalence_accepts_explicit_bench_format_without_bench_extension() {
        let dir = unique_test_dir("equivalence-explicit-bench");
        let lhs_path = dir.join("lhs.logic");
        let rhs_path = dir.join("rhs.logic");
        let report_path = dir.join("report.json");

        fs::write(
            &lhs_path,
            "INPUT(a)\nINPUT(b)\nout = XOR(a, b)\nOUTPUT(out)\n",
        )
        .expect("lhs bench text should write");
        fs::write(
            &rhs_path,
            "INPUT(a)\nINPUT(b)\nout = XOR(b, a)\nOUTPUT(out)\n",
        )
        .expect("rhs bench text should write");

        run_check_equivalence(CheckEquivalenceArgs {
            lhs: lhs_path,
            lhs_format: CliNetlistInputFormat::Bench,
            rhs: rhs_path,
            rhs_format: CliNetlistInputFormat::Bench,
            kind: CliEquivalenceKind::Combinational,
            dimacs_output: None,
            output: Some(report_path.clone()),
        })
        .expect("explicit bench-format check-equivalence should succeed");

        let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).expect("report should exist"))
            .expect("report should be valid json");
        assert_eq!(report["kind"], "combinational");
        assert_eq!(report["equivalent"], true);
    }

    #[test]
    fn run_check_equivalence_accepts_nand_nor_bench_inputs() {
        let dir = unique_test_dir("equivalence-bench-nand-nor");
        let lhs_path = dir.join("lhs.bench");
        let rhs_path = dir.join("rhs.bench");
        let report_path = dir.join("report.json");

        fs::write(
            &lhs_path,
            "INPUT(a)\nINPUT(b)\nn1 = NAND(a, b)\nn2 = NOR(a, b)\nout = XOR(n1, n2)\nOUTPUT(out)\n",
        )
        .expect("lhs bench should write");
        fs::write(
            &rhs_path,
            "INPUT(a)\nINPUT(b)\ninv_and = NAND(b, a)\ninv_or = NOR(b, a)\nout = XOR(inv_and, inv_or)\nOUTPUT(out)\n",
        )
        .expect("rhs bench should write");

        run_check_equivalence(CheckEquivalenceArgs {
            lhs: lhs_path,
            lhs_format: CliNetlistInputFormat::Auto,
            rhs: rhs_path,
            rhs_format: CliNetlistInputFormat::Auto,
            kind: CliEquivalenceKind::Combinational,
            dimacs_output: None,
            output: Some(report_path.clone()),
        })
        .expect("NAND/NOR bench check-equivalence should succeed");

        let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).expect("report should exist"))
            .expect("report should be valid json");
        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["kind"], "combinational");
        assert_eq!(report["equivalent"], true);
    }

    #[test]
    fn run_check_equivalence_accepts_checked_in_bench_fixtures() {
        let dir = unique_test_dir("equivalence-bench-fixtures");

        for fixture_path in quaigh_alignment_bench_fixture_paths() {
            let report_path = dir.join(format!(
                "{}.report.json",
                fixture_path
                    .file_stem()
                    .expect("bench fixture should have stem")
                    .to_string_lossy()
            ));

            run_check_equivalence(CheckEquivalenceArgs {
                lhs: fixture_path.clone(),
                lhs_format: CliNetlistInputFormat::Auto,
                rhs: fixture_path,
                rhs_format: CliNetlistInputFormat::Auto,
                kind: CliEquivalenceKind::Combinational,
                dimacs_output: None,
                output: Some(report_path.clone()),
            })
            .expect("checked-in bench fixture check-equivalence should succeed");

            let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).expect("report should exist"))
                .expect("report should be valid json");
            assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
            assert_eq!(report["kind"], "combinational");
            assert_eq!(report["equivalent"], true);
        }
    }

    #[test]
    fn run_check_equivalence_accepts_checked_in_sequential_bench_fixtures() {
        let dir = unique_test_dir("equivalence-sequential-bench-fixtures");

        for fixture_path in quaigh_alignment_sequential_bench_fixture_paths() {
            let report_path = dir.join(format!(
                "{}.report.json",
                fixture_path
                    .file_stem()
                    .expect("sequential bench fixture should have stem")
                    .to_string_lossy()
            ));

            run_check_equivalence(CheckEquivalenceArgs {
                lhs: fixture_path.clone(),
                lhs_format: CliNetlistInputFormat::Auto,
                rhs: fixture_path,
                rhs_format: CliNetlistInputFormat::Auto,
                kind: CliEquivalenceKind::SingleStepSequential,
                dimacs_output: None,
                output: Some(report_path.clone()),
            })
            .expect("checked-in sequential bench fixture check-equivalence should succeed");

            let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).expect("report should exist"))
                .expect("report should be valid json");
            assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
            assert_eq!(report["kind"], "single_step_sequential");
            assert_eq!(report["equivalent"], true);
        }
    }

    #[test]
    fn run_check_equivalence_reports_non_equivalent_sequential_bench_inputs() {
        let dir = unique_test_dir("equivalence-sequential-bench-mismatch");
        let lhs_path = dir.join("lhs.bench");
        let rhs_path = dir.join("rhs.bench");
        let report_path = dir.join("report.json");

        fs::write(
            &lhs_path,
            "INPUT(d)\nINPUT(en)\nINPUT(clk)\nq = DFF(d, clk)\nOUTPUT(q)\n",
        )
        .expect("lhs bench should write");
        fs::write(
            &rhs_path,
            "INPUT(d)\nINPUT(en)\nINPUT(clk)\nq = DFFE(d, en, clk)\nOUTPUT(q)\n",
        )
        .expect("rhs bench should write");

        run_check_equivalence(CheckEquivalenceArgs {
            lhs: lhs_path,
            lhs_format: CliNetlistInputFormat::Auto,
            rhs: rhs_path,
            rhs_format: CliNetlistInputFormat::Auto,
            kind: CliEquivalenceKind::SingleStepSequential,
            dimacs_output: None,
            output: Some(report_path.clone()),
        })
        .expect("non-equivalent sequential bench check should still run");

        let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).expect("report should exist"))
            .expect("report should be valid json");
        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["kind"], "single_step_sequential");
        assert_eq!(report["equivalent"], false);
        assert!(report["counterexample_inputs"].is_object());
        assert!(report["counterexample_states"].is_object());
    }

    #[test]
    fn run_with_diagnostics_executes_check_equivalence_with_bench_inputs() {
        let dir = unique_test_dir("run-with-diagnostics-check-equivalence-bench");
        let lhs_path = dir.join("lhs.bench");
        let rhs_path = dir.join("rhs.bench");
        let output_dir = dir.join("bundle");

        fs::write(
            &lhs_path,
            "INPUT(a)\nout = NOT(a)\nOUTPUT(out)\n",
        )
        .expect("lhs bench should write");
        fs::write(
            &rhs_path,
            "INPUT(a)\nout = NOT(a)\nOUTPUT(out)\n",
        )
        .expect("rhs bench should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CheckEquivalence,
            input: lhs_path,
            pdk: None,
            rhs: Some(rhs_path),
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("bench equivalence and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: Some(CliEquivalenceKind::Combinational),
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics check-equivalence with bench should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["invocation"]["command"], "check-equivalence");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["combinational"]));
        assert!(output_dir.join("reports").join("check-equivalence-report.json").exists());
    }

    #[test]
    fn run_with_diagnostics_reports_non_equivalent_sequential_bench_inputs() {
        let dir = unique_test_dir("run-with-diagnostics-check-equivalence-sequential-bench-mismatch");
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("synth")
            .join("tests")
            .join("fixtures")
            .join("quaigh_alignment")
            .join("bench_sequential");
        let lhs_path = fixture_dir.join("dff_dffe_mismatch_lhs.bench");
        let rhs_path = fixture_dir.join("dff_dffe_mismatch_rhs.bench");
        let output_dir = dir.join("bundle");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CheckEquivalence,
            input: lhs_path,
            pdk: None,
            rhs: Some(rhs_path),
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("sequential bench mismatch and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: Some(CliEquivalenceKind::SingleStepSequential),
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics sequential bench mismatch should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["invocation"]["command"], "check-equivalence");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["single_step_sequential"]));
        assert!(output_dir.join("reports").join("check-equivalence-report.json").exists());

        let report: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("reports").join("check-equivalence-report.json"))
                .expect("report should exist"),
        )
        .expect("report should be valid json");
        assert_eq!(report["kind"], "single_step_sequential");
        assert_eq!(report["equivalent"], false);
        assert!(report["counterexample_states"].is_object());
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
            "check-equivalence",
            "--lhs",
            "lhs.logic",
            "--lhs-format",
            "bench",
            "--rhs",
            "rhs.logic",
            "--rhs-format",
            "bench",
        ])
        .expect("check-equivalence explicit input formats should parse");

        match cli.command {
            Commands::CheckEquivalence(args) => {
                assert_eq!(args.lhs_format, CliNetlistInputFormat::Bench);
                assert_eq!(args.rhs_format, CliNetlistInputFormat::Bench);
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
            "pdk-validate",
            "--input",
            "example.pdk.json",
        ])
        .expect("pdk-validate args should parse");

        match cli.command {
            Commands::PdkValidate(args) => {
                assert_eq!(args.input, PathBuf::from("example.pdk.json"));
            }
            other => panic!("expected pdk-validate command, got {other:?}"),
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

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "solve-dimacs",
            "--input",
            "example.cnf",
            "--assumptions",
            "1,-2",
        ])
        .expect("run-with-diagnostics solve-dimacs args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::SolveDimacs);
                assert_eq!(args.input, PathBuf::from("example.cnf"));
                assert_eq!(args.assumptions, Some("1,-2".to_string()));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "compile-netlist",
            "--input",
            "example.logic",
            "--input-format",
            "bench",
        ])
        .expect("run-with-diagnostics explicit input format should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::CompileNetlist);
                assert_eq!(args.input_format, CliNetlistInputFormat::Bench);
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "pdk-validate",
            "--input",
            "example.pdk.json",
        ])
        .expect("run-with-diagnostics pdk-validate args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::PdkValidate);
                assert_eq!(args.input, PathBuf::from("example.pdk.json"));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "check-equivalence",
            "--input",
            "lhs.json",
            "--rhs",
            "rhs.json",
            "--equivalence-kind",
            "single_step_sequential",
            "--dimacs-output",
            "equivalence.cnf",
        ])
        .expect("run-with-diagnostics check-equivalence args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::CheckEquivalence);
                assert_eq!(args.input, PathBuf::from("lhs.json"));
                assert_eq!(args.rhs, Some(PathBuf::from("rhs.json")));
                assert_eq!(args.equivalence_kind, Some(CliEquivalenceKind::SingleStepSequential));
                assert_eq!(args.dimacs_output, Some(PathBuf::from("equivalence.cnf")));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "lint-input",
            "--input",
            "example.ir.json",
            "--input-kind",
            "ir",
        ])
        .expect("run-with-diagnostics lint-input args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::LintInput);
                assert_eq!(args.input, PathBuf::from("example.ir.json"));
                assert_eq!(args.input_kind, Some(CliInputKind::Ir));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "compile-netlist",
            "--input",
            "example.ir.json",
            "--pdk",
            "example.pdk.json",
        ])
        .expect("run-with-diagnostics compile-netlist args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::CompileNetlist);
                assert_eq!(args.input, PathBuf::from("example.ir.json"));
                assert_eq!(args.pdk, Some(PathBuf::from("example.pdk.json")));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "analyze-timing",
            "--input",
            "example.ir.json",
            "--pdk",
            "example.pdk.json",
        ])
        .expect("run-with-diagnostics analyze-timing args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::AnalyzeTiming);
                assert_eq!(args.input, PathBuf::from("example.ir.json"));
                assert_eq!(args.pdk, Some(PathBuf::from("example.pdk.json")));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "compile-layout",
            "--input",
            "example.ir.json",
            "--pdk",
            "example.pdk.json",
        ])
        .expect("run-with-diagnostics compile-layout args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::CompileLayout);
                assert_eq!(args.input, PathBuf::from("example.ir.json"));
                assert_eq!(args.pdk, Some(PathBuf::from("example.pdk.json")));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "simulate-file",
            "--input",
            "example.cir",
            "--mode",
            "internal_transient",
        ])
        .expect("run-with-diagnostics args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.output_dir, PathBuf::from("bundle"));
                assert_eq!(args.kind, DiagnosticsCommandKind::SimulateFile);
                assert_eq!(args.input, PathBuf::from("example.cir"));
                assert_eq!(args.mode, CliSimulationMode::InternalTransient);
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "rflux",
            "run-with-diagnostics",
            "--output-dir",
            "bundle",
            "--kind",
            "verify-layout",
            "--input",
            "example.ir.json",
            "--pdk",
            "example.pdk.json",
        ])
        .expect("run-with-diagnostics verify-layout args should parse");

        match cli.command {
            Commands::RunWithDiagnostics(args) => {
                assert_eq!(args.kind, DiagnosticsCommandKind::VerifyLayout);
                assert_eq!(args.input, PathBuf::from("example.ir.json"));
                assert_eq!(args.pdk, Some(PathBuf::from("example.pdk.json")));
            }
            other => panic!("expected run-with-diagnostics command, got {other:?}"),
        }
    }

    #[test]
    fn run_with_diagnostics_executes_simulate_file_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics");
        let input_path = dir.join("example.cir");
        let output_dir = dir.join("bundle");
        fs::write(
            &input_path,
            ".title rc_demo\nV1 in 0 PWL(0 0 1p 1m 8p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran out_rms rms V(out)\n.measure tran missing find V(out) WHEN V(in)=2m RISE=1\n.measure tran rc_delay TRIG V(in) VAL=0.5m RISE=1 TARG V(out) VAL=0.25m RISE=1\n.tran 0.5p 8p\n.end\n",
        )
        .expect("deck should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::SimulateFile,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::InternalTransient,
            external_command: None,
            notes: Some("run and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "simulate-file");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert!(manifest["execution"]["error_code"].is_null());
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["simulate_file"]));
        assert_eq!(manifest["summary"]["delay_detail_count"], 1);
        assert_eq!(manifest["summary"]["measurement_detail_count"], 1);
        assert_eq!(manifest["summary"]["measurement_warning_count"], 1);
        assert_eq!(manifest["summary"]["violation_detail_count"], 0);
        assert_eq!(manifest["structured_logs"]["event_count"], 5);
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "simulate_file");
        assert_eq!(manifest["captured_reports"][0]["report"]["delay_detail_count"], 1);
        assert_eq!(manifest["captured_reports"][0]["report"]["measurement_detail_count"], 1);
        assert_eq!(
            manifest["captured_reports"][0]["report"]["measurement_warning_count"],
            1
        );
        assert_eq!(manifest["captured_reports"][0]["report"]["violation_detail_count"], 0);
        assert!(output_dir.join("reports").join("simulate-file-report.json").exists());
        let report: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("reports").join("simulate-file-report.json"))
                .expect("report should exist"),
        )
        .expect("report should be valid json");
        assert_eq!(report["delay_details"][0]["name"], "rc_delay");
        assert_eq!(report["delay_details"][0]["from_ref"]["node"], "in");
        assert_eq!(report["delay_details"][0]["to_ref"]["node"], "out");
        assert_eq!(report["measurement_details"][0]["name"], "out_rms");
        assert_eq!(report["measurement_details"][0]["kind"], "rms");
        assert_eq!(report["measurement_details"][0]["at_ref"]["node"], "out");
        assert_eq!(report["measurement_warnings"][0]["name"], "missing");
        assert_eq!(
            report["measurement_warnings"][0]["reason"],
            "measurement_crossing_not_found"
        );

        let first_event: Value = serde_json::from_str(event_lines[0]).expect("first event should be json");
        let started_event: Value = serde_json::from_str(event_lines[2]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[3]).expect("completed event should be json");
        assert_eq!(first_event["event"], "bundle_started");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
        assert_eq!(completed_event["fields"]["delay_detail_count"], 1);
        assert_eq!(completed_event["fields"]["measurement_detail_count"], 1);
        assert_eq!(completed_event["fields"]["measurement_warning_count"], 1);
        assert_eq!(completed_event["fields"]["violation_detail_count"], 0);
    }

    #[test]
    fn simulation_report_json_includes_measurement_details() {
        let report = SimulationReport {
            backend: rflux_sim::SimulationBackend::InternalTransientCompleted,
            simulated_events: 6,
            generated_deck_lines: 8,
            generated_deck_path: None,
            waveform_path: None,
            reported_violations: 0,
            reported_worst_delay_ps: Some(1.25e-3),
            delay_details: vec![rflux_sim::SimulationDelayDetail {
                name: "rc_delay".to_string(),
                delay_ps: 1.25,
                from_ref: Some(rflux_sim::SimulationEndpointRef {
                    raw: "in".to_string(),
                    node: "in".to_string(),
                    port: None,
                }),
                to_ref: Some(rflux_sim::SimulationEndpointRef {
                    raw: "out".to_string(),
                    node: "out".to_string(),
                    port: None,
                }),
            }],
            measurement_details: vec![rflux_sim::SimulationMeasurementDetail {
                name: "out_rms".to_string(),
                kind: "rms".to_string(),
                measured_value: 8.5e-4,
                at_ref: Some(rflux_sim::SimulationEndpointRef {
                    raw: "out".to_string(),
                    node: "out".to_string(),
                    port: None,
                }),
            }],
            measurement_warnings: vec![rflux_sim::SimulationMeasurementWarning {
                name: "missing".to_string(),
                kind: "find".to_string(),
                reason: "measurement_crossing_not_found".to_string(),
                at_ref: Some(rflux_sim::SimulationEndpointRef {
                    raw: "V(in)".to_string(),
                    node: "in".to_string(),
                    port: None,
                }),
            }],
            violation_details: Vec::new(),
            external_status_code: None,
            external_result: Some("internal_transient_linear_rc".to_string()),
        };

        let report_json = simulation_report_to_json(&report);

        assert_eq!(report_json["measurement_details"][0]["name"], "out_rms");
        assert_eq!(report_json["measurement_details"][0]["kind"], "rms");
        assert_eq!(report_json["measurement_details"][0]["measured_value"], 8.5e-4);
        assert_eq!(report_json["measurement_details"][0]["at_ref"]["node"], "out");
        assert_eq!(report_json["measurement_warnings"][0]["name"], "missing");
        assert_eq!(report_json["measurement_warnings"][0]["kind"], "find");
        assert_eq!(
            report_json["measurement_warnings"][0]["reason"],
            "measurement_crossing_not_found"
        );
        assert_eq!(report_json["measurement_warnings"][0]["at_ref"]["node"], "in");
        assert_eq!(report_json["delay_details"][0]["name"], "rc_delay");
        assert_eq!(report_json["delay_details"][0]["delay_ps"], 1.25);
        assert_eq!(report_json["delay_details"][0]["from_ref"]["node"], "in");
        assert_eq!(report_json["delay_details"][0]["to_ref"]["node"], "out");
    }

    #[test]
    fn run_with_diagnostics_records_failures_in_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-failure");
        let input_path = dir.join("missing.cir");
        let output_dir = dir.join("bundle");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::SimulateFile,
            input: input_path,
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::InternalTransient,
            external_command: None,
            notes: Some("expected failure".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on failure");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-INPUT-001");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-INPUT-001]")));
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
        assert_eq!(manifest["execution"]["stderr_summary"]["line_count"], 3);
    }

    #[test]
    fn run_with_diagnostics_executes_verify_layout_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-verify-layout");
        let input_path = dir.join("example.ir.json");
        let pdk_path = dir.join("example.pdk.json");
        let output_dir = dir.join("bundle");

        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(PinRef { node: source, port: 0 }, PinRef { node: sink, port: 0 })
            .expect("source to sink should connect");
        rflux_io::write_ir_json(&input_path, &netlist).expect("ir json should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::VerifyLayout,
            input: input_path.clone(),
            pdk: Some(pdk_path.clone()),
            rhs: None,
            mode: CliSimulationMode::InternalTransient,
            external_command: None,
            notes: Some("verify and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics verify-layout should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "verify-layout");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["verify_layout"]));
        assert_eq!(manifest["summary"]["delay_detail_count"], 0);
        assert_eq!(manifest["summary"]["measurement_detail_count"], 0);
        assert_eq!(manifest["summary"]["violation_detail_count"], 0);
        assert_eq!(manifest["structured_logs"]["event_count"], 6);
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "verify_layout");
        assert_eq!(manifest["captured_reports"][0]["report"]["delay_detail_count"], 0);
        assert_eq!(manifest["captured_reports"][0]["report"]["measurement_detail_count"], 0);
        assert_eq!(manifest["captured_reports"][0]["report"]["violation_detail_count"], 0);
        assert!(output_dir.join("reports").join("verify-layout-report.json").exists());

        let started_event: Value = serde_json::from_str(event_lines[3]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[4]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
    }

    #[test]
    fn run_with_diagnostics_executes_compile_layout_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-compile-layout");
        let input_path = dir.join("example.ir.json");
        let pdk_path = dir.join("example.pdk.json");
        let output_dir = dir.join("bundle");

        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(PinRef { node: source, port: 0 }, PinRef { node: sink, port: 0 })
            .expect("source to sink should connect");
        rflux_io::write_ir_json(&input_path, &netlist).expect("ir json should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CompileLayout,
            input: input_path.clone(),
            pdk: Some(pdk_path.clone()),
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("compile and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics compile-layout should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "compile-layout");
        assert!(manifest["invocation"]["mode"].is_null());
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_layout"]));
        assert_eq!(manifest["structured_logs"]["event_count"], 6);
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "compile_layout");
        assert!(output_dir.join("reports").join("compile-layout-report.json").exists());

        let started_event: Value = serde_json::from_str(event_lines[3]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[4]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
    }

    #[test]
    fn run_with_diagnostics_executes_analyze_timing_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-analyze-timing");
        let input_path = dir.join("example.ir.json");
        let pdk_path = dir.join("example.pdk.json");
        let output_dir = dir.join("bundle");

        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(PinRef { node: source, port: 0 }, PinRef { node: sink, port: 0 })
            .expect("source to sink should connect");
        rflux_io::write_ir_json(&input_path, &netlist).expect("ir json should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::AnalyzeTiming,
            input: input_path.clone(),
            pdk: Some(pdk_path.clone()),
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("analyze and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics analyze-timing should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "analyze-timing");
        assert!(manifest["invocation"]["mode"].is_null());
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["timing_analysis"]));
        assert_eq!(manifest["structured_logs"]["event_count"], 6);
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "timing_analysis");
        assert!(output_dir.join("reports").join("analyze-timing-report.json").exists());

        let started_event: Value = serde_json::from_str(event_lines[3]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[4]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
    }

    #[test]
    fn run_with_diagnostics_executes_compile_netlist_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-compile-netlist");
        let input_path = dir.join("example.ir.json");
        let pdk_path = dir.join("example.pdk.json");
        let output_dir = dir.join("bundle");

        let mut netlist = Netlist::new();
        let source = netlist.add_node(NodeKind::Port, "source");
        let sink = netlist.add_node(NodeKind::Dff, "sink");
        netlist
            .connect(PinRef { node: source, port: 0 }, PinRef { node: sink, port: 0 })
            .expect("source to sink should connect");
        rflux_io::write_ir_json(&input_path, &netlist).expect("ir json should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CompileNetlist,
            input: input_path.clone(),
            pdk: Some(pdk_path.clone()),
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("compile netlist and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics compile-netlist should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "compile-netlist");
        assert!(manifest["invocation"]["mode"].is_null());
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_netlist"]));
        assert_eq!(manifest["structured_logs"]["event_count"], 6);
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "compile_netlist");
        assert!(output_dir.join("reports").join("compile-netlist-report.json").exists());

        let started_event: Value = serde_json::from_str(event_lines[3]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[4]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
    }

    #[test]
    fn run_with_diagnostics_executes_compile_netlist_from_bench_input() {
        let dir = unique_test_dir("run-with-diagnostics-compile-netlist-bench");
        let input_path = dir.join("example.bench");
        let pdk_path = dir.join("example.pdk.json");
        let output_dir = dir.join("bundle");

        fs::write(
            &input_path,
            "INPUT(a)\nINPUT(b)\nOUTPUT(y)\nmid = XOR(a, b)\ny = BUF(mid)\n",
        )
        .expect("bench should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CompileNetlist,
            input: input_path.clone(),
            pdk: Some(pdk_path.clone()),
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("compile bench netlist and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics compile-netlist from bench should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["invocation"]["command"], "compile-netlist");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_netlist"]));
        assert!(output_dir.join("reports").join("compile-netlist-report.json").exists());
    }

    #[test]
    fn run_with_diagnostics_executes_compile_netlist_from_explicit_bench_format() {
        let dir = unique_test_dir("run-with-diagnostics-compile-netlist-explicit-bench");
        let input_path = dir.join("example.logic");
        let output_dir = dir.join("bundle");

        fs::write(
            &input_path,
            "INPUT(a)\nOUTPUT(y)\ny = BUF(a)\n",
        )
        .expect("bench text should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CompileNetlist,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("explicit bench format compile-netlist".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Bench,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics compile-netlist from explicit bench format should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["configuration"]["input_format"], "bench");
        assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_netlist"]));
    }

    #[test]
    fn run_with_diagnostics_executes_compile_netlist_from_nand_nor_bench_input() {
        let dir = unique_test_dir("run-with-diagnostics-compile-netlist-bench-nand-nor");
        let input_path = dir.join("example.bench");
        let pdk_path = dir.join("example.pdk.json");
        let output_dir = dir.join("bundle");

        fs::write(
            &input_path,
            "INPUT(a)\nINPUT(b)\nn1 = NAND(a, b)\nn2 = NOR(a, b)\nout = XOR(n1, n2)\nOUTPUT(out)\n",
        )
        .expect("bench should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CompileNetlist,
            input: input_path.clone(),
            pdk: Some(pdk_path.clone()),
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("compile NAND/NOR bench netlist and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics compile-netlist from NAND/NOR bench should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["invocation"]["command"], "compile-netlist");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_netlist"]));
        assert!(output_dir.join("reports").join("compile-netlist-report.json").exists());
    }

    #[test]
    fn run_with_diagnostics_executes_compile_netlist_from_dffe_bench_input() {
        let dir = unique_test_dir("run-with-diagnostics-compile-netlist-bench-dffe");
        let input_path = dir.join("example.bench");
        let pdk_path = dir.join("example.pdk.json");
        let output_dir = dir.join("bundle");

        fs::write(
            &input_path,
            "INPUT(d)\nINPUT(en)\nINPUT(clk)\nq = DFFE(d, en, clk)\nOUTPUT(q)\n",
        )
        .expect("bench should write");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CompileNetlist,
            input: input_path.clone(),
            pdk: Some(pdk_path.clone()),
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("compile DFFE bench netlist and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics compile-netlist from DFFE bench should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["invocation"]["command"], "compile-netlist");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_netlist"]));
        assert!(output_dir.join("reports").join("compile-netlist-report.json").exists());
    }

    #[test]
    fn run_with_diagnostics_executes_compile_netlist_from_checked_in_bench_fixtures() {
        let dir = unique_test_dir("run-with-diagnostics-compile-netlist-bench-fixtures");
        let pdk_path = dir.join("example.pdk.json");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        for fixture_path in quaigh_alignment_bench_fixture_paths() {
            let output_dir = dir.join(
                fixture_path
                    .file_stem()
                    .expect("bench fixture should have stem"),
            );

            run_with_diagnostics(RunWithDiagnosticsArgs {
                output_dir: output_dir.clone(),
                kind: DiagnosticsCommandKind::CompileNetlist,
                input: fixture_path,
                pdk: Some(pdk_path.clone()),
                rhs: None,
                mode: CliSimulationMode::Auto,
                external_command: None,
                notes: Some("compile checked-in bench fixture and bundle".to_string()),
                assumptions: None,
                equivalence_metadata: None,
                check_ref: None,
                equivalence_kind: None,
                dimacs_output: None,
                input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
            })
            .expect("run-with-diagnostics compile-netlist from checked-in bench fixture should succeed");

            let manifest: Value = serde_json::from_str(
                &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
            )
            .expect("manifest should be valid json");

            assert_eq!(manifest["invocation"]["command"], "compile-netlist");
            assert_eq!(manifest["execution"]["status"], "succeeded");
            assert_eq!(manifest["summary"]["captured_input_count"], 2);
            assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_netlist"]));
            assert_eq!(manifest["captured_inputs"][0]["contract"]["input_kind"], "bench");
            assert_eq!(manifest["captured_inputs"][0]["contract"]["schema_format"], "bench_text");
            assert!(manifest["captured_inputs"][0]["contract"]["inspection_error"].is_null());
            assert!(output_dir.join("reports").join("compile-netlist-report.json").exists());
        }
    }

    #[test]
    fn run_with_diagnostics_executes_compile_netlist_from_checked_in_sequential_bench_fixtures() {
        let dir = unique_test_dir("run-with-diagnostics-compile-netlist-sequential-bench-fixtures");
        let pdk_path = dir.join("example.pdk.json");

        let pdk = Pdk::minimal("diag-pdk");
        write_pdk_json(&pdk_path, &pdk).expect("pdk json should write");

        for fixture_path in quaigh_alignment_sequential_bench_fixture_paths() {
            let output_dir = dir.join(
                fixture_path
                    .file_stem()
                    .expect("sequential bench fixture should have stem"),
            );

            run_with_diagnostics(RunWithDiagnosticsArgs {
                output_dir: output_dir.clone(),
                kind: DiagnosticsCommandKind::CompileNetlist,
                input: fixture_path,
                pdk: Some(pdk_path.clone()),
                rhs: None,
                mode: CliSimulationMode::Auto,
                external_command: None,
                notes: Some("compile checked-in sequential bench fixture and bundle".to_string()),
                assumptions: None,
                equivalence_metadata: None,
                check_ref: None,
                equivalence_kind: None,
                dimacs_output: None,
                input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
            })
            .expect("run-with-diagnostics compile-netlist from checked-in sequential bench fixture should succeed");

            let manifest: Value = serde_json::from_str(
                &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
            )
            .expect("manifest should be valid json");

            assert_eq!(manifest["invocation"]["command"], "compile-netlist");
            assert_eq!(manifest["execution"]["status"], "succeeded");
            assert_eq!(manifest["summary"]["captured_input_count"], 2);
            assert_eq!(manifest["summary"]["report_kinds"], json!(["compile_netlist"]));
            assert_eq!(manifest["captured_inputs"][0]["contract"]["input_kind"], "bench");
            assert_eq!(manifest["captured_inputs"][0]["contract"]["schema_format"], "bench_text");
            assert!(manifest["captured_inputs"][0]["contract"]["inspection_error"].is_null());
            assert!(output_dir.join("reports").join("compile-netlist-report.json").exists());
        }
    }

    #[test]
    fn run_with_diagnostics_executes_solve_dimacs_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-solve-dimacs");
        let input_path = dir.join("example.cnf");
        let output_dir = dir.join("bundle");
        fs::write(&input_path, "p cnf 2 2\n1 0\n2 0\n").expect("dimacs should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::SolveDimacs,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("solve and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics solve-dimacs should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "solve-dimacs");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["inspection_failure_count"], 0);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["dimacs_sat"]));
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "dimacs_sat");
        assert!(output_dir.join("reports").join("solve-dimacs-report.json").exists());

        let started_event: Value = serde_json::from_str(event_lines[2]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[3]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
    }

    #[test]
    fn run_with_diagnostics_executes_check_equivalence_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-check-equivalence");
        let lhs_path = dir.join("lhs.ir.json");
        let rhs_path = dir.join("rhs.ir.json");
        let output_dir = dir.join("bundle");
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

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CheckEquivalence,
            input: lhs_path.clone(),
            pdk: None,
            rhs: Some(rhs_path.clone()),
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("equivalence and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: Some(CliEquivalenceKind::Combinational),
            dimacs_output: Some(dimacs_path.clone()),
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics check-equivalence should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "check-equivalence");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 2);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["combinational"]));
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "combinational");
        assert!(output_dir.join("reports").join("check-equivalence-report.json").exists());
        assert!(dimacs_path.exists());

        let started_event: Value = serde_json::from_str(event_lines[3]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[4]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
    }

    #[test]
    fn run_with_diagnostics_records_check_equivalence_verify_failures_in_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-check-equivalence-failure");
        let lhs_path = dir.join("lhs.ir.json");
        let rhs_path = dir.join("rhs.ir.json");
        let output_dir = dir.join("bundle");

        let mut lhs = Netlist::new();
        let data_l = lhs.add_node(NodeKind::Port, "data");
        let clock_l = lhs.add_node(NodeKind::Port, "clock");
        let dff_l = lhs.add_node(NodeKind::Dff, "state");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(PinRef { node: data_l, port: 0 }, PinRef { node: dff_l, port: 0 })
            .expect("data->dff");
        lhs.connect(PinRef { node: clock_l, port: 0 }, PinRef { node: dff_l, port: 1 })
            .expect("clock->dff");
        lhs.connect(PinRef { node: dff_l, port: 0 }, PinRef { node: out_l, port: 0 })
            .expect("dff->out");

        let mut rhs = Netlist::new();
        let data_r = rhs.add_node(NodeKind::Port, "data");
        let clock_r = rhs.add_node(NodeKind::Port, "clock");
        let dff_r = rhs.add_node(NodeKind::Dff, "state");
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(PinRef { node: data_r, port: 0 }, PinRef { node: dff_r, port: 0 })
            .expect("data->dff");
        rhs.connect(PinRef { node: clock_r, port: 0 }, PinRef { node: dff_r, port: 1 })
            .expect("clock->dff");
        rhs.connect(PinRef { node: dff_r, port: 0 }, PinRef { node: out_r, port: 0 })
            .expect("dff->out");

        rflux_io::write_ir_json(&lhs_path, &lhs).expect("lhs should be written");
        rflux_io::write_ir_json(&rhs_path, &rhs).expect("rhs should be written");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CheckEquivalence,
            input: lhs_path.clone(),
            pdk: None,
            rhs: Some(rhs_path.clone()),
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("expected verify failure".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: Some(CliEquivalenceKind::Combinational),
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on verify failure");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-VERIFY-002");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-VERIFY-002]")));
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
    }

    #[test]
    fn run_with_diagnostics_records_check_equivalence_interface_failures_in_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-check-equivalence-interface-failure");
        let lhs_path = dir.join("lhs.ir.json");
        let rhs_path = dir.join("rhs.ir.json");
        let output_dir = dir.join("bundle");

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(PinRef { node: a_l, port: 0 }, PinRef { node: out_l, port: 0 })
            .expect("a->out");

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let out_r = rhs.add_node(NodeKind::Port, "other_out");
        rhs.connect(PinRef { node: a_r, port: 0 }, PinRef { node: out_r, port: 0 })
            .expect("a->other_out");

        rflux_io::write_ir_json(&lhs_path, &lhs).expect("lhs should be written");
        rflux_io::write_ir_json(&rhs_path, &rhs).expect("rhs should be written");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::CheckEquivalence,
            input: lhs_path.clone(),
            pdk: None,
            rhs: Some(rhs_path.clone()),
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("expected interface failure".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: Some(CliEquivalenceKind::Combinational),
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on verify interface failure");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");

        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-VERIFY-001");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-VERIFY-001]")));
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
    }

    #[test]
    fn run_with_diagnostics_executes_lint_input_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-lint-input");
        let input_path = dir.join("input.ir.json");
        let output_dir = dir.join("bundle");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "in");
        rflux_io::write_ir_json(&input_path, &netlist).expect("ir json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::LintInput,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("lint and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: Some(CliInputKind::Ir),
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics lint-input should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "lint-input");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["lint_input"]));
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "lint_input");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "rflux_ir_netlist");
        assert!(output_dir.join("reports").join("lint-input-report.json").exists());

        let started_event: Value = serde_json::from_str(event_lines[2]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[3]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
        assert_eq!(completed_event["fields"]["delay_detail_count"], 0);
        assert_eq!(completed_event["fields"]["measurement_detail_count"], 0);
        assert_eq!(completed_event["fields"]["violation_detail_count"], 0);
    }

    #[test]
    fn run_with_diagnostics_executes_bench_lint_input_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-lint-input-bench");
        let input_path = dir.join("input.bench");
        let output_dir = dir.join("bundle");
        fs::write(&input_path, "INPUT(a)\nOUTPUT(y)\ny = BUF(a)\n")
            .expect("bench input should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::LintInput,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("lint bench and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: Some(CliInputKind::Bench),
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics bench lint-input should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let report: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("reports").join("lint-input-report.json"))
                .expect("lint report should exist"),
        )
        .expect("lint report should be valid json");

        assert_eq!(manifest["invocation"]["command"], "lint-input");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["configuration"]["lint_input"]["kind"], "bench");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "quaigh_bench_subset");
        assert_eq!(report["input_kind"], "bench");
        assert_eq!(report["netlist_summary"]["node_count"], 3);
        assert_eq!(report["netlist_summary"]["edge_count"], 2);
    }

    #[test]
    fn run_with_diagnostics_executes_pdk_validate_and_writes_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-pdk-validate");
        let input_path = dir.join("input.pdk.json");
        let output_dir = dir.join("bundle");
        let pdk = Pdk::minimal("diag-validate");
        write_pdk_json(&input_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::PdkValidate,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("validate and bundle".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics pdk-validate should succeed");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "pdk-validate");
        assert_eq!(manifest["execution"]["status"], "succeeded");
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 1);
        assert_eq!(manifest["summary"]["report_kinds"], json!(["pdk_validation"]));
        assert_eq!(manifest["captured_reports"][0]["report"]["kind"], "pdk_validation");
        assert_eq!(manifest["captured_reports"][0]["report"]["schema_version"], 1);
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "rflux_pdk");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["schema_format"], "versioned_envelope");
        assert!(output_dir.join("reports").join("pdk-validate-report.json").exists());

        let started_event: Value = serde_json::from_str(event_lines[2]).expect("started event should be json");
        let completed_event: Value = serde_json::from_str(event_lines[3]).expect("completed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(completed_event["event"], "command_completed");
    }

    #[test]
    fn run_with_diagnostics_records_pdk_validate_failures_in_bundle() {
        let dir = unique_test_dir("run-with-diagnostics-pdk-validate-failure");
        let input_path = dir.join("input.pdk.json");
        let output_dir = dir.join("bundle");
        fs::write(
            &input_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "kind": "rflux_pdk",
            }))
            .expect("invalid pdk envelope should serialize"),
        )
        .expect("invalid pdk envelope should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::PdkValidate,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("expected validate failure".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: None,
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on pdk-validate failure");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-SCHEMA-002");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-SCHEMA-002]")));
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "rflux_pdk");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["schema_format"], "versioned_envelope");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["input_schema_version"], 1);
        assert!(manifest["captured_inputs"][0]["contract"]["inspection_error"].is_null());

        let failed_event: Value = serde_json::from_str(event_lines[3]).expect("failed event should be json");
        assert_eq!(failed_event["event"], "command_failed");
        assert_eq!(failed_event["fields"]["error_code"], "RFLOW-SCHEMA-002");
    }

    #[test]
    fn run_with_diagnostics_records_lint_input_invalid_envelope_failure() {
        let dir = unique_test_dir("run-with-diagnostics-lint-input-invalid-envelope");
        let input_path = dir.join("input.pdk.json");
        let output_dir = dir.join("bundle");
        fs::write(
            &input_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "kind": "rflux_pdk",
            }))
            .expect("invalid pdk envelope should serialize"),
        )
        .expect("invalid pdk envelope should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::LintInput,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("lint invalid envelope".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: Some(CliInputKind::Pdk),
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on lint-input failure");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["invocation"]["command"], "lint-input");
        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-SCHEMA-002");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-SCHEMA-002]")));
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "rflux_pdk");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["schema_format"], "versioned_envelope");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["input_schema_version"], 1);
        assert_eq!(manifest["captured_inputs"][0]["contract"]["legacy_compatibility_used"], false);
        assert!(manifest["captured_inputs"][0]["contract"]["inspection_error"].is_null());
        assert_eq!(manifest["execution"]["stderr_summary"]["line_count"], 3);

        let started_event: Value = serde_json::from_str(event_lines[2]).expect("started event should be json");
        let failed_event: Value = serde_json::from_str(event_lines[3]).expect("failed event should be json");
        assert_eq!(started_event["event"], "command_started");
        assert_eq!(failed_event["event"], "command_failed");
        assert_eq!(failed_event["fields"]["error_code"], "RFLOW-SCHEMA-002");
    }

    #[test]
    fn run_with_diagnostics_records_lint_input_kind_mismatch_failure() {
        let dir = unique_test_dir("run-with-diagnostics-lint-input-kind-mismatch");
        let input_path = dir.join("input.json");
        let output_dir = dir.join("bundle");
        let pdk = Pdk::minimal("diag-kind-mismatch");
        write_pdk_json(&input_path, &pdk).expect("pdk json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::LintInput,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("lint kind mismatch".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: Some(CliInputKind::Ir),
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on lint-input kind mismatch");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-SCHEMA-003");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-SCHEMA-003]")));
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "rflux_ir_netlist");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["schema_format"], "versioned_envelope");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["input_schema_version"], 1);
        assert!(manifest["captured_inputs"][0]["contract"]["inspection_error"].is_null());
        assert_eq!(manifest["execution"]["stderr_summary"]["line_count"], 3);

        let failed_event: Value = serde_json::from_str(event_lines[3]).expect("failed event should be json");
        assert_eq!(failed_event["event"], "command_failed");
        assert_eq!(failed_event["fields"]["error_code"], "RFLOW-SCHEMA-003");
    }

    #[test]
    fn run_with_diagnostics_records_lint_input_unsupported_schema_failure() {
        let dir = unique_test_dir("run-with-diagnostics-lint-input-unsupported-schema");
        let input_path = dir.join("input.ir.json");
        let output_dir = dir.join("bundle");
        fs::write(
            &input_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 99,
                "kind": "rflux_ir_netlist",
                "payload": Netlist::new(),
            }))
            .expect("unsupported ir json should serialize"),
        )
        .expect("unsupported ir json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::LintInput,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("lint unsupported schema".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: Some(CliInputKind::Ir),
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on unsupported schema");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-SCHEMA-001");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-SCHEMA-001]")));
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "rflux_ir_netlist");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["schema_format"], "versioned_envelope");
        assert_eq!(manifest["captured_inputs"][0]["contract"]["input_schema_version"], 99);
        assert!(manifest["captured_inputs"][0]["contract"]["inspection_error"].is_null());

        let failed_event: Value = serde_json::from_str(event_lines[3]).expect("failed event should be json");
        assert_eq!(failed_event["event"], "command_failed");
        assert_eq!(failed_event["fields"]["error_code"], "RFLOW-SCHEMA-001");
    }

    #[test]
    fn run_with_diagnostics_records_lint_input_malformed_json_failure() {
        let dir = unique_test_dir("run-with-diagnostics-lint-input-malformed-json");
        let input_path = dir.join("input.ir.json");
        let output_dir = dir.join("bundle");
        fs::write(&input_path, "not valid json").expect("malformed ir json should write");

        run_with_diagnostics(RunWithDiagnosticsArgs {
            output_dir: output_dir.clone(),
            kind: DiagnosticsCommandKind::LintInput,
            input: input_path.clone(),
            pdk: None,
            rhs: None,
            mode: CliSimulationMode::Auto,
            external_command: None,
            notes: Some("lint malformed json".to_string()),
            assumptions: None,
            equivalence_metadata: None,
            check_ref: None,
            equivalence_kind: None,
            dimacs_output: None,
            input_kind: Some(CliInputKind::Ir),
            input_format: CliNetlistInputFormat::Auto,
            rhs_format: CliNetlistInputFormat::Auto,
        })
        .expect("run-with-diagnostics should still write bundle on malformed json");

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("manifest.json")).expect("manifest should exist"),
        )
        .expect("manifest should be valid json");
        let event_log = fs::read_to_string(output_dir.join("events.jsonl"))
            .expect("event log should exist");
        let event_lines: Vec<&str> = event_log.lines().collect();

        assert_eq!(manifest["execution"]["status"], "failed");
        assert_eq!(manifest["execution"]["error_code"], "RFLOW-INPUT-002");
        assert!(manifest["execution"]["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("error[RFLOW-INPUT-002]")));
        assert_eq!(manifest["summary"]["captured_input_count"], 1);
        assert_eq!(manifest["summary"]["captured_report_count"], 0);
        assert_eq!(manifest["captured_inputs"][0]["contract"]["contract_kind"], "rflux_ir_netlist");
        assert!(manifest["captured_inputs"][0]["contract"]["schema_format"].is_null());
        assert!(manifest["captured_inputs"][0]["contract"]["input_schema_version"].is_null());
        assert!(manifest["captured_inputs"][0]["contract"]["inspection_error"]
            .as_str()
            .is_some_and(|message| message.contains("failed to parse input JSON from")));

        let failed_event: Value = serde_json::from_str(event_lines[3]).expect("failed event should be json");
        assert_eq!(failed_event["event"], "command_failed");
        assert_eq!(failed_event["fields"]["error_code"], "RFLOW-INPUT-002");
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
                "status": "ok",
                "delay_details": [
                    {"name": "critical_path", "delay_ps": 12.5}
                ],
                "measurement_details": [
                    {"name": "out_rms", "kind": "rms", "measured_value": 0.001}
                ],
                "measurement_warnings": [
                    {"name": "missing", "kind": "find", "reason": "measurement_crossing_not_found"}
                ],
                "violation_details": [
                    {"kind": "setup", "detail": "late"}
                ],
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
        assert_eq!(manifest["summary"]["delay_detail_count"], 1);
        assert_eq!(manifest["summary"]["measurement_detail_count"], 1);
        assert_eq!(manifest["summary"]["measurement_warning_count"], 1);
        assert_eq!(manifest["summary"]["violation_detail_count"], 1);
        assert_eq!(manifest["summary"]["report_inspection_failure_count"], 0);
        assert_eq!(manifest["structured_logs"]["format"], "jsonl");
        assert_eq!(manifest["structured_logs"]["event_count"], 5);
        assert!(manifest["summary"]["legacy_compatibility_inputs"]
            .as_array()
            .expect("legacy inputs should be array")
            .is_empty());
        let rflow_env_names = manifest["environment"]["present_prefixed_vars"]["RFLOW_*"]
            .as_array()
            .expect("RFLOW env list should be an array");
        assert!(rflow_env_names.iter().all(|value| {
            value
                .as_str()
                .is_some_and(|name| name.starts_with("RFLOW_"))
        }));
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
        assert_eq!(captured_reports[0]["report"]["delay_detail_count"], 1);
        assert_eq!(captured_reports[0]["report"]["measurement_detail_count"], 1);
        assert_eq!(captured_reports[0]["report"]["measurement_warning_count"], 1);
        assert_eq!(captured_reports[0]["report"]["violation_detail_count"], 1);
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
    fn diagnostics_report_snapshot_counts_simulation_details() {
        let dir = unique_test_dir("collect-diagnostics-report-detail-counts");
        let report_path = dir.join("simulate-report.json");
        emit_json(
            &json!({
                "schema_version": 1,
                "kind": "simulate_file",
                "delay_details": [
                    {"name": "critical_path", "delay_ps": 12.5}
                ],
                "measurement_details": [
                    {"name": "out_rms", "kind": "rms", "measured_value": 0.001}
                ],
                "measurement_warnings": [
                    {"name": "missing", "kind": "find", "reason": "measurement_crossing_not_found"}
                ],
                "violation_details": [
                    {"kind": "setup", "detail": "late"}
                ],
            }),
            Some(&report_path),
        )
        .expect("report should write");

        let report = diagnostics_report_snapshot(&report_path);

        assert_eq!(report["kind"], "simulate_file");
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["delay_detail_count"], 1);
        assert_eq!(report["measurement_detail_count"], 1);
        assert_eq!(report["measurement_warning_count"], 1);
        assert_eq!(report["violation_detail_count"], 1);
        assert!(report["inspection_error"].is_null());
    }

    #[test]
    fn diagnostics_report_snapshot_counts_nested_verification_simulation_details() {
        let dir = unique_test_dir("collect-diagnostics-nested-verification-report-detail-counts");
        let report_path = dir.join("verify-layout-report.json");
        emit_json(
            &json!({
                "schema_version": 1,
                "kind": "verify_layout",
                "checked_routes": 1,
                "simulation": {
                    "delay_details": [
                        {"name": "critical_path", "delay_ps": 12.5}
                    ],
                    "measurement_details": [
                        {"name": "out_rms", "kind": "rms", "measured_value": 0.001}
                    ],
                    "measurement_warnings": [
                        {"name": "missing", "kind": "find", "reason": "measurement_crossing_not_found"}
                    ],
                    "violation_details": [
                        {"kind": "setup", "detail": "late"}
                    ],
                },
            }),
            Some(&report_path),
        )
        .expect("report should write");

        let report = diagnostics_report_snapshot(&report_path);

        assert_eq!(report["kind"], "verify_layout");
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["delay_detail_count"], 1);
        assert_eq!(report["measurement_detail_count"], 1);
        assert_eq!(report["measurement_warning_count"], 1);
        assert_eq!(report["violation_detail_count"], 1);
        assert!(report["inspection_error"].is_null());
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
    fn diagnostics_contract_snapshot_recognizes_bench_inputs() {
        let dir = unique_test_dir("collect-diagnostics-contract-bench");
        let bench_path = dir.join("example.bench");
        fs::write(&bench_path, "INPUT(a)\nOUTPUT(y)\ny = BUF(a)\n").expect("bench should write");

        let bench = diagnostics_contract_snapshot("input", &bench_path);

        assert_eq!(bench["input_kind"], "bench");
        assert_eq!(bench["contract_kind"], "quaigh_bench_subset");
        assert_eq!(bench["schema_format"], "bench_text");
        assert!(bench["input_schema_version"].is_null());
        assert_eq!(bench["legacy_compatibility_used"], false);
        assert!(bench["inspection_error"].is_null());
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
                    "delay_details": [
                        {
                            "name": "critical_path",
                            "delay_ps": 12.5,
                        }
                    ],
                    "measurement_details": [
                        {
                            "name": "out_rms",
                            "kind": "rms",
                            "measured_value": 0.001,
                        }
                    ],
                    "violation_details": [
                        {
                            "kind": "setup",
                            "detail": "late",
                        }
                    ],
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
        assert_eq!(summary["delay_detail_count"], 1);
        assert_eq!(summary["measurement_detail_count"], 1);
        assert_eq!(summary["violation_detail_count"], 1);
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
        assert_eq!(report["netlist_summary"]["node_count"], 1);
        assert_eq!(report["netlist_summary"]["edge_count"], 0);
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
    fn run_lint_input_reports_bench_contract() {
        let dir = unique_test_dir("lint-input-bench");
        let input_path = dir.join("input.bench");
        let output_path = dir.join("lint-report.json");
        fs::write(&input_path, "INPUT(a)\nINPUT(b)\nOUTPUT(y)\ny = XOR(a, b)\n")
            .expect("bench input should write");

        run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Bench,
            output: Some(output_path.clone()),
        })
        .expect("bench lint-input should succeed");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("lint report should exist"),
        )
        .expect("lint report should be valid json");

        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["kind"], "lint_input");
        assert_eq!(report["input_kind"], "bench");
        assert_eq!(report["contract_kind"], "quaigh_bench_subset");
        assert_eq!(report["schema_format"], "bench_text");
        assert!(report["input_schema_version"].is_null());
        assert_eq!(report["legacy_compatibility_used"], false);
        assert_eq!(report["netlist_summary"]["node_count"], 4);
        assert_eq!(report["netlist_summary"]["edge_count"], 3);
    }

    #[test]
    fn run_lint_input_reports_versioned_pdk_contract() {
        let dir = unique_test_dir("lint-input-pdk");
        let input_path = dir.join("input.pdk.json");
        let output_path = dir.join("lint-report.json");
        let pdk = Pdk::minimal("lint-pdk");
        write_pdk_json(&input_path, &pdk).expect("pdk json should write");

        run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Pdk,
            output: Some(output_path.clone()),
        })
        .expect("lint-input should succeed for versioned pdk");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("lint report should exist"),
        )
        .expect("lint report should be valid json");

        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["kind"], "lint_input");
        assert_eq!(report["input_kind"], "pdk");
        assert_eq!(report["contract_kind"], "rflux_pdk");
        assert_eq!(report["schema_format"], "versioned_envelope");
        assert_eq!(report["input_schema_version"], 1);
        assert_eq!(report["legacy_compatibility_used"], false);
    }

    #[test]
    fn run_lint_input_reports_legacy_pdk_contract() {
        let dir = unique_test_dir("lint-input-pdk-legacy");
        let input_path = dir.join("legacy.pdk.json");
        let output_path = dir.join("lint-report.json");
        let pdk = Pdk::minimal("legacy-pdk");
        fs::write(
            &input_path,
            serde_json::to_string_pretty(&pdk).expect("legacy pdk should serialize"),
        )
        .expect("legacy pdk json should write");

        run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Pdk,
            output: Some(output_path.clone()),
        })
        .expect("legacy lint-input should succeed for pdk");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("lint report should exist"),
        )
        .expect("lint report should be valid json");

        assert_eq!(report["input_kind"], "pdk");
        assert_eq!(report["contract_kind"], "rflux_pdk");
        assert_eq!(report["schema_format"], "legacy_raw_json");
        assert!(report["input_schema_version"].is_null());
        assert_eq!(report["legacy_compatibility_used"], true);
    }

    #[test]
    fn run_pdk_validate_reports_clean_versioned_pdk() {
        let dir = unique_test_dir("pdk-validate-clean");
        let input_path = dir.join("input.pdk.json");
        let output_path = dir.join("pdk-validate-report.json");
        let pdk = Pdk::minimal("validate-clean");
        write_pdk_json(&input_path, &pdk).expect("pdk json should write");

        run_pdk_validate(PdkValidateArgs {
            input: input_path.clone(),
            output: Some(output_path.clone()),
        })
        .expect("pdk-validate should succeed");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("pdk validate report should exist"),
        )
        .expect("pdk validate report should be valid json");

        assert_eq!(report["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(report["kind"], "pdk_validation");
        assert_eq!(report["pdk_name"], "validate-clean");
        assert_eq!(report["ok"], true);
        assert_eq!(report["error_count"], 0);
        assert_eq!(report["warning_count"], 0);
        assert_eq!(report["summary"]["cell_count"], 7);
        assert_eq!(report["summary"]["cell_timing_count"], 7);
        assert_eq!(report["summary"]["interconnect_timing_count"], 2);
        assert_eq!(report["checks"]["required_cell_kinds"]["ok"], true);
        assert_eq!(report["checks"]["required_cell_timing"]["ok"], true);
        assert_eq!(report["checks"]["required_interconnect_timing"]["ok"], true);
        assert_eq!(report["checks"]["characterized_arcs"]["level"], "advisory");
        assert!(report["errors"]
            .as_array()
            .expect("errors should be array")
            .is_empty());
        assert!(report["warnings"]
            .as_array()
            .expect("warnings should be array")
            .is_empty());
    }

    #[test]
    fn run_pdk_validate_reports_structural_errors() {
        let dir = unique_test_dir("pdk-validate-errors");
        let input_path = dir.join("input.pdk.json");
        let output_path = dir.join("pdk-validate-report.json");
        let mut pdk = Pdk::minimal(" ");
        pdk.metal_layers = 0;
        pdk.ptl_forbidden_ranges.push(LengthRange {
            min_um: 10.0,
            max_um: 5.0,
        });
        write_pdk_json(&input_path, &pdk).expect("invalid pdk json should write");

        run_pdk_validate(PdkValidateArgs {
            input: input_path.clone(),
            output: Some(output_path.clone()),
        })
        .expect("pdk-validate should still emit a report for structural errors");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("pdk validate report should exist"),
        )
        .expect("pdk validate report should be valid json");

        assert_eq!(report["kind"], "pdk_validation");
        assert_eq!(report["ok"], false);
        assert_eq!(report["error_count"], 3);
        let errors = report["errors"].as_array().expect("errors should be array");
        assert!(errors.iter().any(|error| error == "pdk.name must not be empty"));
        assert!(errors
            .iter()
            .any(|error| error == "pdk.metal_layers must be greater than zero"));
        assert!(errors.iter().any(|error| error
            .as_str()
            .is_some_and(|message| message.contains("inverted range [10, 5]"))));
    }

    #[test]
    fn run_pdk_validate_reports_advisory_characterization_warnings() {
        let dir = unique_test_dir("pdk-validate-warnings");
        let input_path = dir.join("input.pdk.json");
        let output_path = dir.join("pdk-validate-report.json");
        let mut pdk = Pdk::minimal("validate-warnings");
        pdk.characterized_cell_metadata.push(NamedCharacterizationMetadata {
            cell_name: "sfq_macro".to_string(),
            metadata: CharacterizationArtifactMetadata {
                arc_delays: vec![CharacterizationArcDelay {
                    name: "unknown-sink".to_string(),
                    driver_cell_name: "sfq_macro".to_string(),
                    from_port: 0,
                    sink_cell_name: "missing_sink".to_string(),
                    to_port: 0,
                    delay_ps: 2.5,
                }],
                ..CharacterizationArtifactMetadata::default()
            },
        });
        write_pdk_json(&input_path, &pdk).expect("warning pdk json should write");

        run_pdk_validate(PdkValidateArgs {
            input: input_path.clone(),
            output: Some(output_path.clone()),
        })
        .expect("pdk-validate should still emit a report for warnings");

        let report: Value = serde_json::from_str(
            &fs::read_to_string(&output_path).expect("pdk validate report should exist"),
        )
        .expect("pdk validate report should be valid json");

        assert_eq!(report["kind"], "pdk_validation");
        assert_eq!(report["ok"], true);
        assert_eq!(report["error_count"], 0);
        assert_eq!(report["warning_count"], 1);
        assert_eq!(report["summary"]["characterized_cell_metadata_count"], 1);
        assert_eq!(report["summary"]["characterized_arc_delay_count"], 1);
        assert_eq!(report["checks"]["characterized_arcs"]["ok"], true);
        let warnings = report["warnings"].as_array().expect("warnings should be array");
        assert!(warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|message| message.contains("references unknown sink cell 'missing_sink'"))));
    }

    #[test]
    fn run_lint_input_reports_unsupported_ir_schema_version() {
        let dir = unique_test_dir("lint-input-ir-unsupported-schema");
        let input_path = dir.join("input.ir.json");
        fs::write(
            &input_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 99,
                "kind": "rflux_ir_netlist",
                "payload": Netlist::new(),
            }))
            .expect("unsupported ir json should serialize"),
        )
        .expect("unsupported ir json should write");

        let error = run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Ir,
            output: None,
        })
        .expect_err("lint-input should reject unsupported ir schema version");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-SCHEMA-001]: failed to validate IR JSON from"));
        assert!(rendered.contains("detail: unsupported rflux_ir_netlist schema version 99"));
        assert!(rendered.contains(
            "next: Regenerate the file with the current toolchain or run a compatible rflux version."
        ));
    }

    #[test]
    fn run_lint_input_reports_invalid_pdk_envelope() {
        let dir = unique_test_dir("lint-input-pdk-invalid-envelope");
        let input_path = dir.join("input.pdk.json");
        fs::write(
            &input_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "kind": "rflux_pdk",
            }))
            .expect("invalid pdk envelope should serialize"),
        )
        .expect("invalid pdk envelope should write");

        let error = run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Pdk,
            output: None,
        })
        .expect_err("lint-input should reject invalid pdk envelope");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-SCHEMA-002]: failed to validate PDK JSON from"));
        assert!(rendered.contains("detail: invalid rflux_pdk JSON envelope: missing payload"));
        assert!(rendered.contains(
            "next: Ensure the JSON envelope contains schema_version, kind, and payload."
        ));
    }

    #[test]
    fn run_lint_input_reports_malformed_ir_json() {
        let dir = unique_test_dir("lint-input-ir-malformed-json");
        let input_path = dir.join("input.ir.json");
        fs::write(&input_path, "not valid json").expect("malformed ir json should write");

        let error = run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Ir,
            output: None,
        })
        .expect_err("lint-input should reject malformed ir json");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-INPUT-002]: failed to validate IR JSON from"));
        assert!(rendered.contains("detail: json parse error:"));
        assert!(rendered.contains(
            "next: Validate the JSON syntax and field types against the current file contract."
        ));
    }

    #[test]
    fn run_lint_input_reports_schema_kind_mismatch() {
        let dir = unique_test_dir("lint-input-kind-mismatch");
        let input_path = dir.join("input.json");
        let pdk = Pdk::minimal("kind-mismatch-pdk");
        write_pdk_json(&input_path, &pdk).expect("pdk json should write");

        let error = run_lint_input(LintInputArgs {
            input: input_path.clone(),
            kind: CliInputKind::Ir,
            output: None,
        })
        .expect_err("lint-input should reject mismatched json kind");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-SCHEMA-003]: failed to validate IR JSON from"));
        assert!(rendered.contains("detail: expected rflux_ir_netlist JSON envelope, found rflux_pdk"));
        assert!(rendered.contains(
            "next: Use the correct file type for this command, or regenerate the file with the matching writer."
        ));
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

        assert!(rendered.contains("error[RFLOW-INTERNAL-001]: plain failure"));
        assert!(rendered.contains("detail: plain failure"));
        assert!(rendered.contains("next: Retry with run-with-diagnostics or collect-diagnostics and attach the bundle when reporting the issue."));
    }

    #[test]
    fn render_cli_error_uses_rflow_code_for_contextual_flow_failures() {
        let error = anyhow!("missing mapping data").context("compile-netlist failed");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-FLOW-001]: compile-netlist failed"));
        assert!(rendered.contains("detail: missing mapping data"));
        assert!(rendered.contains(
            "next: Validate the IR/PDK inputs and current SFQ mapping constraints, then retry compile-netlist."
        ));
    }

    #[test]
    fn render_cli_error_uses_rflow_code_for_simulation_missing_input() {
        let config = SimulationConfig::default();
        let error = simulate_file("missing-input.cir", &config)
            .with_context(|| "simulate-file failed for missing-input.cir")
            .expect_err("missing deck should fail");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-INPUT-001]: simulate-file failed for missing-input.cir"));
        assert!(rendered.contains("detail: io error at missing-input.cir:"));
        assert!(rendered.contains("next: Check that the deck path exists and is readable, then retry simulate-file."));
    }

    #[test]
    fn render_cli_error_uses_rflow_code_for_unsupported_simulation_syntax() {
        let dir = unique_test_dir("simulate-file-unsupported-syntax");
        fs::create_dir_all(&dir).expect("test dir should exist");
        let input_path = dir.join("unsupported.cir");
        fs::write(
            &input_path,
            ".subckt outer in out\n.subckt inner a b\nR1 a b 50\n.ends\n.ends\nX1 n1 n2 outer\n.tran 1p 10p\n.end\n",
        )
        .expect("deck should write");

        let config = SimulationConfig::default();
        let error = simulate_file(&input_path, &config)
            .with_context(|| format!("simulate-file failed for {}", input_path.display()))
            .expect_err("unsupported deck should fail");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains(&format!(
            "error[RFLOW-SIM-002]: simulate-file failed for {}",
            input_path.display()
        )));
        assert!(rendered.contains("detail: nested .subckt definition is not supported inside outer"));
        assert!(rendered.contains(
            "next: Run with --mode external_josim or simplify the deck to the currently supported internal subset."
        ));
    }

    #[test]
    fn render_cli_error_uses_rflow_code_for_unsupported_verify_sequential_semantics() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let data_l = lhs.add_node(NodeKind::Port, "data");
        let clock_l = lhs.add_node(NodeKind::Port, "clock");
        let dff_l = lhs.add_node(NodeKind::Dff, "state");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(PinRef { node: data_l, port: 0 }, PinRef { node: dff_l, port: 0 })
            .expect("data->dff");
        lhs.connect(PinRef { node: clock_l, port: 0 }, PinRef { node: dff_l, port: 1 })
            .expect("clock->dff");
        lhs.connect(PinRef { node: dff_l, port: 0 }, PinRef { node: out_l, port: 0 })
            .expect("dff->out");

        let mut rhs = Netlist::new();
        let data_r = rhs.add_node(NodeKind::Port, "data");
        let clock_r = rhs.add_node(NodeKind::Port, "clock");
        let dff_r = rhs.add_node(NodeKind::Dff, "state");
        let out_r = rhs.add_node(NodeKind::Port, "out");
        rhs.connect(PinRef { node: data_r, port: 0 }, PinRef { node: dff_r, port: 0 })
            .expect("data->dff");
        rhs.connect(PinRef { node: clock_r, port: 0 }, PinRef { node: dff_r, port: 1 })
            .expect("clock->dff");
        rhs.connect(PinRef { node: dff_r, port: 0 }, PinRef { node: out_r, port: 0 })
            .expect("dff->out");

        let error = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .context("combinational equivalence check failed")
            .expect_err("combinational equivalence should reject sequential netlists");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-VERIFY-002]: combinational equivalence check failed"));
        assert!(rendered.contains("detail: sat check does not support node"));
        assert!(rendered.contains(
            "next: Use --kind single_step_sequential for sequential netlists, or reduce the check to a combinational subset."
        ));
    }

    #[test]
    fn render_cli_error_uses_rflow_code_for_verify_interface_mismatch() {
        let verifier = Verifier::new();

        let mut lhs = Netlist::new();
        let a_l = lhs.add_node(NodeKind::Port, "a");
        let out_l = lhs.add_node(NodeKind::Port, "out");
        lhs.connect(PinRef { node: a_l, port: 0 }, PinRef { node: out_l, port: 0 })
            .expect("a->out");

        let mut rhs = Netlist::new();
        let a_r = rhs.add_node(NodeKind::Port, "a");
        let out_r = rhs.add_node(NodeKind::Port, "other_out");
        rhs.connect(PinRef { node: a_r, port: 0 }, PinRef { node: out_r, port: 0 })
            .expect("a->other_out");

        let error = verifier
            .check_boolean_equivalence(&lhs, &rhs)
            .context("combinational equivalence check failed")
            .expect_err("mismatched interfaces should reject equivalence checking");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-VERIFY-001]: combinational equivalence check failed"));
        assert!(rendered.contains("detail: sat interface mismatch: output sets differ"));
        assert!(rendered.contains(
            "next: Align the compared netlists so they expose the same named inputs, outputs, and state interface before retrying check-equivalence."
        ));
    }

    #[test]
    fn render_cli_error_uses_rflow_code_for_verify_layout_failures() {
        let error = anyhow!("layout contains unsupported crossover").context("verify-layout failed");

        let rendered = render_cli_error(&error);

        assert!(rendered.contains("error[RFLOW-VERIFY-003]: verify-layout failed"));
        assert!(rendered.contains("detail: layout contains unsupported crossover"));
        assert!(rendered.contains(
            "next: Inspect the verification report or rerun with diagnostics to identify the violated structural or simulation-backed layout checks."
        ));
    }
}

fn build_lint_input_report(input: &Path, kind: CliInputKind) -> Result<Value> {
    match kind {
        CliInputKind::Ir => lint_netlist_report(
            input,
            "ir",
            "rflux_ir_netlist",
            NetlistInputFormat::IrJson,
        ),
        CliInputKind::Bench => lint_netlist_report(
            input,
            "bench",
            "quaigh_bench_subset",
            NetlistInputFormat::Bench,
        ),
        CliInputKind::Pdk => {
            read_pdk_json(input)
                .with_context(|| format!("failed to validate PDK JSON from {}", input.display()))?;
            lint_input_report(input, "pdk", "rflux_pdk")
        }
    }
}

fn build_pdk_validate_report(input: &Path) -> Result<Value> {
    let pdk = read_pdk_json(input)
        .with_context(|| format!("failed to validate PDK JSON from {}", input.display()))?;
    let validation = pdk.validate();
    let summary = pdk_validate_summary(&pdk, validation.errors.len(), validation.warnings.len());
    let checks = pdk_validate_checks(&pdk, &validation);
    Ok(json!({
        "kind": "pdk_validation",
        "input": input.display().to_string(),
        "pdk_name": pdk.name,
        "ok": validation.is_ok(),
        "error_count": validation.errors.len(),
        "warning_count": validation.warnings.len(),
        "summary": summary,
        "checks": checks,
        "errors": validation.errors,
        "warnings": validation.warnings,
    }))
}

fn pdk_validate_summary(pdk: &Pdk, error_count: usize, warning_count: usize) -> Value {
    json!({
        "cell_count": pdk.cell_library.cells.len(),
        "cell_timing_count": pdk.cell_timing.len(),
        "named_cell_timing_count": pdk.named_cell_timing.len(),
        "characterized_cell_metadata_count": pdk.characterized_cell_metadata.len(),
        "characterized_arc_delay_count": pdk
            .characterized_cell_metadata
            .iter()
            .map(|entry| entry.metadata.arc_delays.len())
            .sum::<usize>(),
        "interconnect_timing_count": pdk.interconnect_timing.len(),
        "ptl_forbidden_range_count": pdk.ptl_forbidden_ranges.len(),
        "error_count": error_count,
        "warning_count": warning_count,
    })
}

fn pdk_validate_checks(pdk: &Pdk, validation: &rflux_tech::PdkValidationReport) -> Value {
    let required_cell_kinds = ["GenericGate", "Macro", "Splitter", "Dff", "Jtl", "Ptl", "Port"];
    let required_interconnect_kinds = ["Jtl", "Ptl"];
    let has_required_cell_kinds = required_cell_kinds.iter().all(|kind| {
        pdk.cell_library
            .cells
            .iter()
            .any(|cell| format!("{:?}", cell.kind) == *kind)
    });
    let has_required_cell_timing = required_cell_kinds.iter().all(|kind| {
        pdk.cell_timing
            .iter()
            .any(|timing| format!("{:?}", timing.kind) == *kind)
    });
    let has_required_interconnect_timing = required_interconnect_kinds.iter().all(|kind| {
        pdk.interconnect_timing
            .iter()
            .any(|model| format!("{:?}", model.kind) == *kind)
    });

    json!({
        "structural_integrity": {
            "ok": validation.is_ok(),
            "error_count": validation.errors.len(),
        },
        "required_cell_kinds": {
            "ok": has_required_cell_kinds,
            "required": required_cell_kinds,
        },
        "required_cell_timing": {
            "ok": has_required_cell_timing,
            "required": required_cell_kinds,
        },
        "required_interconnect_timing": {
            "ok": has_required_interconnect_timing,
            "required": required_interconnect_kinds,
        },
        "named_cell_timing": {
            "ok": !pdk.named_cell_timing.is_empty(),
            "count": pdk.named_cell_timing.len(),
            "level": if pdk.named_cell_timing.is_empty() { "advisory" } else { "present" },
        },
        "characterized_arcs": {
            "ok": pdk
                .characterized_cell_metadata
                .iter()
                .any(|entry| !entry.metadata.arc_delays.is_empty()),
            "metadata_count": pdk.characterized_cell_metadata.len(),
            "arc_delay_count": pdk
                .characterized_cell_metadata
                .iter()
                .map(|entry| entry.metadata.arc_delays.len())
                .sum::<usize>(),
            "level": "advisory",
        },
        "ptl_forbidden_ranges": {
            "ok": !pdk.ptl_forbidden_ranges.is_empty(),
            "count": pdk.ptl_forbidden_ranges.len(),
            "level": "advisory",
        },
    })
}
