use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationBackend {
    EventOnly,
    ExternalCompleted,
    ExternalFailed,
    ExternalUnavailable,
    InternalTransientCompleted,
    InternalTransientUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationMode {
    Auto,
    EventOnly,
    ExternalJosim,
    InternalTransient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationConfig {
    pub mode: SimulationMode,
    pub external_command: Option<String>,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            mode: SimulationMode::Auto,
            external_command: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationEndpointRef {
    pub raw: String,
    pub node: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimulationDelayDetail {
    pub name: String,
    pub delay_ps: f64,
    pub from_ref: Option<SimulationEndpointRef>,
    pub to_ref: Option<SimulationEndpointRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimulationMeasurementDetail {
    pub name: String,
    pub kind: String,
    pub measured_value: f64,
    pub at_ref: Option<SimulationEndpointRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationMeasurementWarning {
    pub name: String,
    pub kind: String,
    pub reason: String,
    pub at_ref: Option<SimulationEndpointRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationViolationDetail {
    pub kind: String,
    pub detail: String,
    pub at_ref: Option<SimulationEndpointRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimulationReport {
    pub backend: SimulationBackend,
    pub requested_mode: String,
    pub simulated_events: usize,
    pub generated_deck_lines: usize,
    pub generated_deck_path: Option<String>,
    pub waveform_path: Option<String>,
    pub waveform_format: Option<String>,
    pub external_summary_contract: Option<String>,
    pub diagnostic_code: Option<String>,
    pub reported_violations: usize,
    pub reported_worst_delay_ps: Option<f64>,
    pub delay_details: Vec<SimulationDelayDetail>,
    pub measurement_details: Vec<SimulationMeasurementDetail>,
    pub measurement_warnings: Vec<SimulationMeasurementWarning>,
    pub violation_details: Vec<SimulationViolationDetail>,
    pub external_status_code: Option<i32>,
    pub external_result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationQualityGateReport {
    pub passed: bool,
    pub status: String,
    pub required_backend: String,
    pub actual_backend: SimulationBackend,
    pub alignment_level: String,
    pub external_alignment_required: bool,
    pub external_alignment_available: bool,
    pub violation_count: usize,
    pub warning_count: usize,
    pub next_step: String,
}

impl SimulationReport {
    pub fn quality_gate(&self) -> SimulationQualityGateReport {
        simulation_quality_gate(self, false)
    }

    pub fn josim_quality_gate(&self) -> SimulationQualityGateReport {
        simulation_quality_gate(self, true)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransientAnalysis {
    pub tstep_ps: f64,
    pub tstop_ps: f64,
    pub prstart_ps: Option<f64>,
    pub prstep_ps: Option<f64>,
    pub use_initial_conditions: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedDeck {
    pub title: Option<String>,
    pub params: BTreeMap<String, f64>,
    pub transient: TransientAnalysis,
    pub element_count: usize,
    pub control_count: usize,
}

impl ParsedDeck {
    pub fn estimated_event_count(&self) -> usize {
        self.element_count.max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubcktDef {
    pins: Vec<String>,
    default_params: BTreeMap<String, String>,
    body: Vec<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SimulationError {
    #[error("missing .tran control card")]
    MissingTran,
    #[error("io error at {path}: {message}")]
    Io { path: String, message: String },
    #[error(".include requires a file-backed deck: {0}")]
    IncludeWithoutBase(String),
    #[error("invalid .subckt header: {0}")]
    InvalidSubcktHeader(String),
    #[error("duplicate .subckt definition in scope {scope}: {name}")]
    DuplicateSubcktDefinition { scope: String, name: String },
    #[error("missing .ends for subckt: {0}")]
    MissingEnds(String),
    #[error("mismatched .ends for subckt {expected}: {found}")]
    MismatchedEnds { expected: String, found: String },
    #[error("unsupported control card inside .subckt {subckt}: {line}")]
    UnsupportedSubcktControl { subckt: String, line: String },
    #[error("unknown subckt instance target: {0}")]
    UnknownSubckt(String),
    #[error("invalid subckt instance: {0}")]
    InvalidSubcktInstance(String),
    #[error("unsupported subckt instance syntax: {0}")]
    UnsupportedSubcktInstanceSyntax(String),
    #[error("invalid parameter assignment: {0}")]
    InvalidParamAssignment(String),
    #[error("invalid numeric value: {0}")]
    InvalidNumericValue(String),
    #[error("unknown parameter: {0}")]
    UnknownParameter(String),
    #[error("unsupported expression: {0}")]
    UnsupportedExpression(String),
    #[error("invalid .tran card: {0}")]
    InvalidTran(String),
    #[error("missing .lib section {section} in {path}")]
    MissingLibrarySection { path: String, section: String },
    #[error("unterminated .lib section {section} in {path}")]
    UnterminatedLibrarySection { path: String, section: String },
    #[error("mismatched .endl section in {path}: expected {expected}, found {found}")]
    MismatchedLibrarySectionEnd {
        path: String,
        expected: String,
        found: String,
    },
}

pub type ParsedSimulatorOutput = (
    Option<usize>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<usize>,
    Option<f64>,
    Vec<SimulationDelayDetail>,
    Vec<SimulationMeasurementDetail>,
    Vec<SimulationViolationDetail>,
);

pub fn parse_deck(deck: &str) -> Result<ParsedDeck, SimulationError> {
    parse_deck_expanded(deck, None)
}

pub fn parse_deck_file(path: impl AsRef<Path>) -> Result<ParsedDeck, SimulationError> {
    let expanded = expand_deck_file(path.as_ref())?;
    parse_deck_expanded(&expanded, path.as_ref().parent())
}

pub fn is_supported_external_command(command: &str) -> bool {
    is_allowed_external_command(command)
}

pub fn simulate_file(
    path: impl AsRef<Path>,
    simulation_config: &SimulationConfig,
) -> Result<SimulationReport, SimulationError> {
    let expanded = expand_deck_file(path.as_ref())?;
    let parsed = parse_deck_expanded(&expanded, path.as_ref().parent())?;
    Ok(run_generated_deck_with_base(
        &expanded,
        parsed.estimated_event_count(),
        simulation_config,
        path.as_ref().parent(),
    ))
}

fn normalize_continuation_lines(deck: &str) -> String {
    let mut normalized: Vec<String> = Vec::new();

    for raw_line in deck.lines() {
        let trimmed_start = raw_line.trim_start();
        if let Some(rest) = trimmed_start.strip_prefix('+') {
            let continuation = rest.trim_start();
            if let Some(previous) = normalized.last_mut() {
                if !previous.is_empty() && !previous.ends_with(' ') {
                    previous.push(' ');
                }
                previous.push_str(continuation);
            } else {
                normalized.push(continuation.to_string());
            }
        } else {
            normalized.push(raw_line.to_string());
        }
    }

    normalized.join("\n")
}

fn parse_deck_expanded(
    deck: &str,
    include_base_dir: Option<&Path>,
) -> Result<ParsedDeck, SimulationError> {
    let deck = normalize_continuation_lines(deck);
    let deck = flatten_subckts(&deck)?;
    let mut title = None;
    let mut params = BTreeMap::new();
    let mut transient = None;
    let mut element_count = 0usize;
    let mut control_count = 0usize;

    for raw_line in deck.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(line, ".title") {
            title = Some(rest.trim().to_string());
            control_count += 1;
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(line, ".param") {
            control_count += 1;
            parse_param_line(rest.trim(), &mut params)?;
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(line, ".include") {
            let include_path = parse_include_path(rest.trim())?;
            let Some(base_dir) = include_base_dir else {
                return Err(SimulationError::IncludeWithoutBase(include_path));
            };
            control_count += 1;
            let resolved_path = resolve_include_path(base_dir, &include_path);
            let expanded = expand_deck_file(&resolved_path)?;
            let nested = parse_deck_expanded(&expanded, resolved_path.parent())?;
            if title.is_none() {
                title = nested.title;
            }
            params.extend(nested.params);
            transient = Some(nested.transient);
            element_count += nested.element_count;
            control_count += nested.control_count;
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(line, ".lib") {
            let directive = parse_library_directive(rest.trim())?;
            let Some(base_dir) = include_base_dir else {
                return Err(SimulationError::IncludeWithoutBase(directive.include_path));
            };
            control_count += 1;
            let resolved_path = resolve_include_path(base_dir, &directive.include_path);
            let expanded = expand_library_file(&resolved_path, directive.section.as_deref())?;
            let nested = parse_deck_expanded(&expanded, resolved_path.parent())?;
            if title.is_none() {
                title = nested.title;
            }
            params.extend(nested.params);
            transient = Some(nested.transient);
            element_count += nested.element_count;
            control_count += nested.control_count;
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(line, ".tran") {
            control_count += 1;
            transient = Some(parse_tran_line(rest.trim(), &params)?);
            continue;
        }

        if line.starts_with('.') {
            control_count += 1;
            continue;
        }

        element_count += 1;
    }

    Ok(ParsedDeck {
        title,
        params,
        transient: transient.ok_or(SimulationError::MissingTran)?,
        element_count,
        control_count,
    })
}

pub fn simulate_text(
    deck: &str,
    simulation_config: &SimulationConfig,
) -> Result<SimulationReport, SimulationError> {
    let parsed = parse_deck(deck)?;
    Ok(run_generated_deck(
        deck,
        parsed.estimated_event_count(),
        simulation_config,
    ))
}

pub fn run_generated_deck(
    deck: &str,
    simulated_events: usize,
    simulation_config: &SimulationConfig,
) -> SimulationReport {
    run_generated_deck_with_base(deck, simulated_events, simulation_config, None)
}

fn simulation_mode_name(mode: &SimulationMode) -> &'static str {
    match mode {
        SimulationMode::Auto => "auto",
        SimulationMode::EventOnly => "event_only",
        SimulationMode::ExternalJosim => "external_josim",
        SimulationMode::InternalTransient => "internal_transient",
    }
}

fn run_generated_deck_with_base(
    deck: &str,
    simulated_events: usize,
    simulation_config: &SimulationConfig,
    include_base_dir: Option<&Path>,
) -> SimulationReport {
    let deck = normalize_continuation_lines(deck);
    let generated_deck_lines = deck.lines().count();
    let requested_mode = simulation_mode_name(&simulation_config.mode).to_string();

    let event_only_report = || SimulationReport {
        backend: SimulationBackend::EventOnly,
        requested_mode: requested_mode.clone(),
        simulated_events,
        generated_deck_lines,
        generated_deck_path: None,
        waveform_path: None,
        waveform_format: None,
        external_summary_contract: None,
        diagnostic_code: None,
        reported_violations: 0,
        reported_worst_delay_ps: None,
        delay_details: Vec::new(),
        measurement_details: Vec::new(),
        measurement_warnings: Vec::new(),
        violation_details: Vec::new(),
        external_status_code: None,
        external_result: None,
    };

    let internal_transient_unavailable_report = |reason: String| SimulationReport {
        backend: SimulationBackend::InternalTransientUnavailable,
        requested_mode: requested_mode.clone(),
        simulated_events,
        generated_deck_lines,
        generated_deck_path: None,
        waveform_path: None,
        waveform_format: None,
        external_summary_contract: None,
        diagnostic_code: Some(reason.clone()),
        reported_violations: 0,
        reported_worst_delay_ps: None,
        delay_details: Vec::new(),
        measurement_details: Vec::new(),
        measurement_warnings: Vec::new(),
        violation_details: Vec::new(),
        external_status_code: None,
        external_result: Some(reason),
    };

    let internal_transient_completed_report =
        |result: InternalTransientResult, waveform_path: Option<String>| {
            let external_result = result
                .option_seed
                .map(|seed| format!("internal_transient_linear_rc;seed={seed}"))
                .unwrap_or_else(|| "internal_transient_linear_rc".to_string());
            let waveform_format = if waveform_path.is_some() {
                Some("csv_v1".to_string())
            } else {
                None
            };
            SimulationReport {
                backend: SimulationBackend::InternalTransientCompleted,
                requested_mode: requested_mode.clone(),
                simulated_events: result.simulated_steps,
                generated_deck_lines,
                generated_deck_path: None,
                waveform_path,
                waveform_format,
                external_summary_contract: None,
                diagnostic_code: None,
                reported_violations: 0,
                reported_worst_delay_ps: result
                    .delay_details
                    .iter()
                    .map(|detail| detail.delay_ps)
                    .reduce(f64::max)
                    .or(Some(result.max_abs_voltage_v)),
                delay_details: result.delay_details,
                measurement_details: result.measurement_details,
                measurement_warnings: result.measurement_warnings,
                violation_details: Vec::new(),
                external_status_code: None,
                external_result: Some(external_result),
            }
        };

    let external_command = match simulation_config.mode {
        SimulationMode::Auto => simulation_config.external_command.as_deref(),
        SimulationMode::EventOnly => return event_only_report(),
        SimulationMode::ExternalJosim => {
            let Some(command) = simulation_config.external_command.as_deref() else {
                return SimulationReport {
                    backend: SimulationBackend::ExternalUnavailable,
                    requested_mode: requested_mode.clone(),
                    simulated_events,
                    generated_deck_lines,
                    generated_deck_path: None,
                    waveform_path: None,
                    waveform_format: None,
                    external_summary_contract: None,
                    diagnostic_code: Some("external_command_missing".to_string()),
                    reported_violations: 0,
                    reported_worst_delay_ps: None,
                    delay_details: Vec::new(),
                    measurement_details: Vec::new(),
                    measurement_warnings: Vec::new(),
                    violation_details: Vec::new(),
                    external_status_code: None,
                    external_result: Some("external_command_missing".to_string()),
                };
            };
            Some(command)
        }
        SimulationMode::InternalTransient => {
            return match run_internal_transient_with_base(&deck, include_base_dir) {
                Ok(result) => {
                    let waveform_path = write_internal_transient_waveform(
                        &result,
                        result.option_waveform_path.as_deref(),
                    );
                    internal_transient_completed_report(result, waveform_path)
                }
                Err(reason) => internal_transient_unavailable_report(reason),
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(command) = external_command {
            if !is_allowed_external_command(command) {
                return SimulationReport {
                    backend: SimulationBackend::ExternalUnavailable,
                    requested_mode: requested_mode.clone(),
                    simulated_events,
                    generated_deck_lines,
                    generated_deck_path: None,
                    waveform_path: None,
                    waveform_format: None,
                    external_summary_contract: None,
                    diagnostic_code: Some("external_command_not_allowed".to_string()),
                    reported_violations: 0,
                    reported_worst_delay_ps: None,
                    delay_details: Vec::new(),
                    measurement_details: Vec::new(),
                    measurement_warnings: Vec::new(),
                    violation_details: Vec::new(),
                    external_status_code: None,
                    external_result: Some("external_command_not_allowed".to_string()),
                };
            }

            use std::fs;
            use std::time::{SystemTime, UNIX_EPOCH};

            let timestamp_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default();
            let run_dir = match create_external_run_dir(
                &std::env::temp_dir(),
                std::process::id(),
                timestamp_millis,
            ) {
                Ok(run_dir) => run_dir,
                Err(_) => {
                    return SimulationReport {
                        backend: SimulationBackend::ExternalUnavailable,
                        requested_mode: requested_mode.clone(),
                        simulated_events,
                        generated_deck_lines,
                        generated_deck_path: None,
                        waveform_path: None,
                        waveform_format: None,
                        external_summary_contract: None,
                        diagnostic_code: Some("external_run_dir_create_failed".to_string()),
                        reported_violations: 0,
                        reported_worst_delay_ps: None,
                        delay_details: Vec::new(),
                        measurement_details: Vec::new(),
                        measurement_warnings: Vec::new(),
                        violation_details: Vec::new(),
                        external_status_code: None,
                        external_result: Some("external_run_dir_create_failed".to_string()),
                    };
                }
            };
            let external_translation_notes = collect_external_translation_notes(&deck);
            let prepared_deck = prepare_external_simulator_deck(&deck, include_base_dir);
            let deck_path = run_dir.join("input.sp");
            if fs::write(&deck_path, prepared_deck).is_err() {
                return SimulationReport {
                    backend: SimulationBackend::ExternalUnavailable,
                    requested_mode: requested_mode.clone(),
                    simulated_events,
                    generated_deck_lines,
                    generated_deck_path: None,
                    waveform_path: None,
                    waveform_format: None,
                    external_summary_contract: None,
                    diagnostic_code: Some("deck_write_failed".to_string()),
                    reported_violations: 0,
                    reported_worst_delay_ps: None,
                    delay_details: Vec::new(),
                    measurement_details: Vec::new(),
                    measurement_warnings: Vec::new(),
                    violation_details: Vec::new(),
                    external_status_code: None,
                    external_result: Some("deck_write_failed".to_string()),
                };
            }

            let expected_waveform_path = run_dir.join("external_output.csv");
            match build_external_simulator_command(command, &deck_path, &expected_waveform_path)
                .output()
            {
                Ok(output) => {
                    let status = output.status.code();
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let (
                        reported_events,
                        reported_result,
                        waveform_path,
                        external_summary_contract,
                        reported_violations,
                        reported_worst_delay_ps,
                        delay_details,
                        measurement_details,
                        violation_details,
                    ) = parse_simulator_output(&stdout);
                    let waveform_path = waveform_path.or_else(|| {
                        if expected_waveform_path.is_file() {
                            Some(expected_waveform_path.display().to_string())
                        } else {
                            None
                        }
                    });
                    let backend = if output.status.success() {
                        SimulationBackend::ExternalCompleted
                    } else {
                        SimulationBackend::ExternalFailed
                    };
                    let diagnostic_code = if matches!(backend, SimulationBackend::ExternalFailed) {
                        Some("external_command_failed_exit".to_string())
                    } else {
                        None
                    };
                    let external_runtime_warnings =
                        parse_external_simulator_stderr_warnings(&stderr);
                    let mut external_notes = external_translation_notes;
                    external_notes.extend(external_runtime_warnings);
                    external_notes.sort();
                    external_notes.dedup();
                    let external_result =
                        combine_external_result_with_notes(reported_result, &external_notes);
                    let waveform_source_path = waveform_path
                        .as_deref()
                        .map(Path::new)
                        .filter(|path| path.is_file())
                        .unwrap_or(&expected_waveform_path);
                    let (generated_deck_path, waveform_path) = stage_external_run_artifacts(
                        &run_dir,
                        &deck_path,
                        Some(waveform_source_path),
                        std::process::id(),
                        timestamp_millis,
                        true,
                    );
                    return SimulationReport {
                        backend,
                        requested_mode: requested_mode.clone(),
                        simulated_events: reported_events.unwrap_or(simulated_events),
                        generated_deck_lines,
                        generated_deck_path,
                        waveform_format: waveform_path.as_deref().and_then(detect_waveform_format),
                        waveform_path,
                        external_summary_contract,
                        diagnostic_code,
                        reported_violations: reported_violations.unwrap_or(violation_details.len()),
                        reported_worst_delay_ps,
                        delay_details,
                        measurement_details,
                        measurement_warnings: Vec::new(),
                        violation_details,
                        external_status_code: status,
                        external_result,
                    };
                }
                Err(_) => {
                    return SimulationReport {
                        backend: SimulationBackend::ExternalUnavailable,
                        requested_mode: requested_mode.clone(),
                        simulated_events,
                        generated_deck_lines,
                        generated_deck_path: Some(deck_path.display().to_string()),
                        waveform_path: None,
                        waveform_format: None,
                        external_summary_contract: None,
                        diagnostic_code: Some("external_command_spawn_failed".to_string()),
                        reported_violations: 0,
                        reported_worst_delay_ps: None,
                        delay_details: Vec::new(),
                        measurement_details: Vec::new(),
                        measurement_warnings: Vec::new(),
                        violation_details: Vec::new(),
                        external_status_code: None,
                        external_result: Some("external_command_spawn_failed".to_string()),
                    };
                }
            }
        }
    }

    let _ = simulation_config;
    event_only_report()
}

fn simulation_quality_gate(
    report: &SimulationReport,
    require_external_alignment: bool,
) -> SimulationQualityGateReport {
    let external_alignment_available =
        matches!(report.backend, SimulationBackend::ExternalCompleted);
    let violation_count = report.reported_violations + report.violation_details.len();
    let warning_count = report.measurement_warnings.len();
    let backend_failed = matches!(
        report.backend,
        SimulationBackend::ExternalFailed
            | SimulationBackend::ExternalUnavailable
            | SimulationBackend::InternalTransientUnavailable
    );
    let clean_measurements = violation_count == 0 && warning_count == 0;
    let passed = if require_external_alignment {
        external_alignment_available && clean_measurements
    } else {
        !backend_failed && clean_measurements
    };
    let alignment_level = if external_alignment_available {
        "josim_aligned"
    } else if matches!(
        report.backend,
        SimulationBackend::InternalTransientCompleted
    ) {
        "internal_transient"
    } else if matches!(report.backend, SimulationBackend::EventOnly) {
        "event_only"
    } else {
        "unavailable"
    };
    let status = if passed {
        "passed"
    } else if require_external_alignment && !external_alignment_available {
        "failed_external_alignment_missing"
    } else if backend_failed {
        "failed_backend"
    } else if violation_count > 0 {
        "failed_violations"
    } else {
        "failed_measurement_warnings"
    };
    let next_step = if passed {
        "Simulation quality gate passed."
    } else if require_external_alignment && !external_alignment_available {
        "Run with simulation_mode=external_josim and a working JoSIM command, then compare the reported measurements."
    } else if backend_failed {
        "Inspect simulation backend status and external_result, then rerun with a supported deck and simulator mode."
    } else if violation_count > 0 {
        "Inspect violation_details and fix the generated deck, routing, or timing constraints before signoff."
    } else {
        "Inspect measurement_warnings and add or correct required waveform measurements before signoff."
    };

    SimulationQualityGateReport {
        passed,
        status: status.to_string(),
        required_backend: if require_external_alignment {
            "external_josim".to_string()
        } else {
            "any_completed".to_string()
        },
        actual_backend: report.backend.clone(),
        alignment_level: alignment_level.to_string(),
        external_alignment_required: require_external_alignment,
        external_alignment_available,
        violation_count,
        warning_count,
        next_step: next_step.to_string(),
    }
}

fn detect_waveform_format(path: &str) -> Option<String> {
    let lowered = path.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        None
    } else if lowered.ends_with(".csv") {
        Some("csv_v1".to_string())
    } else {
        Some("external_passthrough".to_string())
    }
}

fn is_allowed_external_command(command: &str) -> bool {
    let candidate = command.trim();
    if candidate.is_empty() {
        return false;
    }

    let path = Path::new(candidate);
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    is_allowed_external_command_file_name(file_name)
}

fn is_allowed_external_command_file_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    let stem = lower
        .strip_suffix(".exe")
        .or_else(|| lower.strip_suffix(".cmd"))
        .or_else(|| lower.strip_suffix(".bat"))
        .or_else(|| lower.strip_suffix(".sh"))
        .unwrap_or(&lower);
    matches!(stem, "josim" | "josim-cli")
}

fn prepare_external_simulator_deck(deck: &str, include_base_dir: Option<&Path>) -> String {
    let flattened = flatten_subckts(deck).unwrap_or_else(|_| deck.to_string());
    let junction_models = collect_external_josephson_model_cards(&flattened);
    let mut prepared = String::new();
    for raw_line in flattened.lines() {
        let trimmed = strip_comment(raw_line).trim();
        if strip_control_card_prefix(trimmed, ".title").is_some() {
            continue;
        }
        for josephson_line in
            rewrite_external_josephson_cards(&strip_params_marker(raw_line), &junction_models)
        {
            let rewritten = strip_external_tran_uic(&rewrite_external_mutual_coupling_arguments(
                &josephson_line,
            ));
            let rewritten = rewrite_external_source_keyword_calls(&rewritten);
            prepared.push_str(&inline_external_waveform_file_source(
                &rewritten,
                include_base_dir,
            ));
            prepared.push('\n');
        }
    }
    prepared
}

fn collect_external_translation_notes(deck: &str) -> Vec<String> {
    let flattened = flatten_subckts(deck).unwrap_or_else(|_| deck.to_string());
    let junction_models = collect_external_josephson_model_cards(&flattened);
    let mut notes = Vec::<String>::new();
    for raw_line in flattened.lines() {
        collect_external_josephson_translation_notes_from_line(
            raw_line,
            &junction_models,
            &mut notes,
        );
    }
    notes.sort();
    notes.dedup();
    notes
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExternalJunctionModelCard {
    critical_current: Option<String>,
    second_harmonic_current: Option<String>,
    third_harmonic_current: Option<String>,
    fourth_harmonic_current: Option<String>,
    fifth_harmonic_current: Option<String>,
    sixth_harmonic_current: Option<String>,
    normal_resistance: Option<String>,
    junction_cap: Option<String>,
    pi_junction: Option<bool>,
    native_cpr_basis_current: Option<String>,
    native_cpr_coefficients: Option<Vec<String>>,
}

fn collect_external_josephson_model_cards(
    deck: &str,
) -> BTreeMap<String, ExternalJunctionModelCard> {
    let mut models = BTreeMap::<String, ExternalJunctionModelCard>::new();
    for raw_line in deck.lines() {
        let trimmed = strip_comment(raw_line).trim();
        if !trimmed.to_ascii_lowercase().starts_with(".model") {
            continue;
        }
        let Some((name, card)) = parse_external_josephson_model_card(trimmed) else {
            continue;
        };
        models.insert(name, card);
    }
    models
}

fn split_junction_model_argument_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::<String>::new();
    let mut current = String::new();
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    for ch in text.chars() {
        match ch {
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            '=' if brace_depth == 0 && paren_depth == 0 => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                tokens.push("=".to_string());
            }
            ',' | ' ' | '\t' | '\r' | '\n' if brace_depth == 0 && paren_depth == 0 => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn split_top_level_comma_values(text: &str) -> Vec<String> {
    let mut values = Vec::<String>::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    for ch in text.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if paren_depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    values.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        values.push(trimmed.to_string());
    }
    values
}

fn parse_cpr_coefficients(value: &str) -> Option<Vec<String>> {
    let trimmed = value.trim();
    let body = trimmed.strip_prefix('{')?.strip_suffix('}')?;
    let coefficients = split_top_level_comma_values(body);
    if coefficients.is_empty() {
        return None;
    }
    Some(coefficients)
}

fn scale_external_harmonic_from_cpr(coefficient: &str, basis: &str) -> Option<String> {
    let trimmed = coefficient.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        if value.abs() < 1.0e-12 {
            return None;
        }
        if (value - 1.0).abs() < 1.0e-12 {
            return Some(basis.to_string());
        }
    }
    Some(format!("({trimmed})*({basis})"))
}

fn parse_external_josephson_model_card(line: &str) -> Option<(String, ExternalJunctionModelCard)> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 3 || !tokens[0].eq_ignore_ascii_case(".model") {
        return None;
    }

    let model_name = tokens[1].to_ascii_lowercase();
    let body = tokens[2..].join(" ");
    let lower_body = body.to_ascii_lowercase();
    let argument_text = if lower_body.starts_with("jj(") {
        let close_index = body.rfind(')')?;
        body[3..close_index].to_string()
    } else if lower_body == "jj" || lower_body.starts_with("jj ") {
        body[2..].trim().to_string()
    } else {
        return None;
    };

    let raw_tokens = split_junction_model_argument_tokens(&argument_text);
    let collapsed =
        collapse_spaced_assignments(&raw_tokens.iter().map(String::as_str).collect::<Vec<_>>());
    let mut card = ExternalJunctionModelCard {
        critical_current: None,
        second_harmonic_current: None,
        third_harmonic_current: None,
        fourth_harmonic_current: None,
        fifth_harmonic_current: None,
        sixth_harmonic_current: None,
        normal_resistance: None,
        junction_cap: None,
        pi_junction: None,
        native_cpr_basis_current: None,
        native_cpr_coefficients: None,
    };
    let mut cpr_coefficients = None::<Vec<String>>;
    for token in collapsed {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("icrit") || name.eq_ignore_ascii_case("ic") {
            card.critical_current = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit2")
            || name.eq_ignore_ascii_case("ic2")
            || name.eq_ignore_ascii_case("cp2")
        {
            card.second_harmonic_current = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit3")
            || name.eq_ignore_ascii_case("ic3")
            || name.eq_ignore_ascii_case("cp3")
        {
            card.third_harmonic_current = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit4")
            || name.eq_ignore_ascii_case("ic4")
            || name.eq_ignore_ascii_case("cp4")
        {
            card.fourth_harmonic_current = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit5")
            || name.eq_ignore_ascii_case("ic5")
            || name.eq_ignore_ascii_case("cp5")
        {
            card.fifth_harmonic_current = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit6")
            || name.eq_ignore_ascii_case("ic6")
            || name.eq_ignore_ascii_case("cp6")
        {
            card.sixth_harmonic_current = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("rn") {
            card.normal_resistance = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("pi") {
            card.pi_junction = parse_external_junction_pi_flag(value);
            continue;
        }
        if name.eq_ignore_ascii_case("cpr") {
            cpr_coefficients = parse_cpr_coefficients(value);
            continue;
        }
        if name.eq_ignore_ascii_case("cap") || name.eq_ignore_ascii_case("cj") {
            card.junction_cap = Some(value.to_string());
        }
    }
    if let (Some(basis_current), Some(coefficients)) = (&card.critical_current, cpr_coefficients) {
        let basis_current = basis_current.clone();
        card.native_cpr_basis_current = Some(basis_current.clone());
        card.native_cpr_coefficients = Some(coefficients.clone());
        if let Some(current) = scale_external_harmonic_from_cpr(
            coefficients.first().map(String::as_str).unwrap_or("1"),
            &basis_current,
        ) {
            card.critical_current = Some(current);
        } else {
            card.critical_current = None;
        }
        if card.second_harmonic_current.is_none() {
            card.second_harmonic_current = coefficients.get(1).and_then(|coefficient| {
                scale_external_harmonic_from_cpr(coefficient, &basis_current)
            });
        }
        if card.third_harmonic_current.is_none() {
            card.third_harmonic_current = coefficients.get(2).and_then(|coefficient| {
                scale_external_harmonic_from_cpr(coefficient, &basis_current)
            });
        }
        if card.fourth_harmonic_current.is_none() {
            card.fourth_harmonic_current = coefficients.get(3).and_then(|coefficient| {
                scale_external_harmonic_from_cpr(coefficient, &basis_current)
            });
        }
        if card.fifth_harmonic_current.is_none() {
            card.fifth_harmonic_current = coefficients.get(4).and_then(|coefficient| {
                scale_external_harmonic_from_cpr(coefficient, &basis_current)
            });
        }
        if card.sixth_harmonic_current.is_none() {
            card.sixth_harmonic_current = coefficients.get(5).and_then(|coefficient| {
                scale_external_harmonic_from_cpr(coefficient, &basis_current)
            });
        }
    }
    Some((model_name, card))
}

fn parse_external_junction_pi_flag(value: &str) -> Option<bool> {
    let trimmed = value.trim().trim_end_matches([',', ')']);
    if trimmed.eq_ignore_ascii_case("true")
        || trimmed.eq_ignore_ascii_case("yes")
        || trimmed.eq_ignore_ascii_case("on")
    {
        return Some(true);
    }
    if trimmed.eq_ignore_ascii_case("false")
        || trimmed.eq_ignore_ascii_case("no")
        || trimmed.eq_ignore_ascii_case("off")
    {
        return Some(false);
    }
    let value = trimmed.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if (value - rounded).abs() > 1.0e-9 {
        return None;
    }
    match rounded as i64 {
        0 => Some(false),
        _ => Some(true),
    }
}

fn apply_external_pi_to_critical_current(
    critical_current: Option<String>,
    pi_junction: Option<bool>,
) -> Option<String> {
    let critical_current = critical_current?;
    if !pi_junction.unwrap_or(false) {
        return Some(critical_current);
    }

    let trimmed = critical_current.trim();
    if let Some(rest) = trimmed.strip_prefix('-') {
        return Some(rest.trim().to_string());
    }
    Some(format!("-{trimmed}"))
}

fn build_external_junction_harmonic_terms(
    critical_current: Option<String>,
    second_harmonic_current: Option<String>,
    third_harmonic_current: Option<String>,
    fourth_harmonic_current: Option<String>,
    fifth_harmonic_current: Option<String>,
    sixth_harmonic_current: Option<String>,
    pi_junction: bool,
) -> (Option<String>, Option<String>) {
    let basis_harmonic = if critical_current.is_some() {
        Some(1)
    } else if second_harmonic_current.is_some() {
        Some(2)
    } else if third_harmonic_current.is_some() {
        Some(3)
    } else if fourth_harmonic_current.is_some() {
        Some(4)
    } else if fifth_harmonic_current.is_some() {
        Some(5)
    } else if sixth_harmonic_current.is_some() {
        Some(6)
    } else {
        None
    };
    let Some(basis_harmonic) = basis_harmonic else {
        return (None, None);
    };

    let basis_current = match basis_harmonic {
        1 => critical_current.clone().unwrap(),
        2 => second_harmonic_current.clone().unwrap(),
        3 => third_harmonic_current.clone().unwrap(),
        4 => fourth_harmonic_current.clone().unwrap(),
        5 => fifth_harmonic_current.clone().unwrap(),
        6 => sixth_harmonic_current.clone().unwrap(),
        _ => unreachable!(),
    };
    let basis_sign_negative = pi_junction && basis_harmonic % 2 == 1;
    let signed_basis_current = if basis_sign_negative {
        apply_external_pi_to_critical_current(Some(basis_current.clone()), Some(true)).unwrap()
    } else {
        basis_current.clone()
    };

    let max_harmonic = if sixth_harmonic_current.is_some() {
        6
    } else if fifth_harmonic_current.is_some() {
        5
    } else if fourth_harmonic_current.is_some() {
        4
    } else if third_harmonic_current.is_some() {
        3
    } else if second_harmonic_current.is_some() {
        2
    } else {
        1
    };
    if max_harmonic == 1 && basis_harmonic == 1 {
        return (Some(signed_basis_current), None);
    }

    let harmonic_currents = [
        critical_current.as_deref(),
        second_harmonic_current.as_deref(),
        third_harmonic_current.as_deref(),
        fourth_harmonic_current.as_deref(),
        fifth_harmonic_current.as_deref(),
        sixth_harmonic_current.as_deref(),
    ];
    let coefficients = (1..=max_harmonic)
        .map(|harmonic| {
            if harmonic == basis_harmonic {
                return "1".to_string();
            }
            let Some(current) = harmonic_currents[harmonic - 1] else {
                return "0".to_string();
            };
            let harmonic_sign_negative = pi_junction && harmonic % 2 == 1;
            let coefficient_negative = basis_sign_negative ^ harmonic_sign_negative;
            if coefficient_negative {
                format!("-{current}/{basis_current}")
            } else {
                format!("{current}/{basis_current}")
            }
        })
        .collect::<Vec<_>>();

    (
        Some(signed_basis_current),
        Some(format!("cpr={{{}}}", coefficients.join(","))),
    )
}

fn parse_external_runtime_harmonic_assignment(
    name: &str,
    value: &str,
    second_harmonic_current: &mut Option<String>,
    third_harmonic_current: &mut Option<String>,
    fourth_harmonic_current: &mut Option<String>,
    fifth_harmonic_current: &mut Option<String>,
    sixth_harmonic_current: &mut Option<String>,
) -> bool {
    if name.eq_ignore_ascii_case("icrit2")
        || name.eq_ignore_ascii_case("ic2")
        || name.eq_ignore_ascii_case("cp2")
    {
        *second_harmonic_current = Some(value.to_string());
        return true;
    }
    if name.eq_ignore_ascii_case("icrit3")
        || name.eq_ignore_ascii_case("ic3")
        || name.eq_ignore_ascii_case("cp3")
    {
        *third_harmonic_current = Some(value.to_string());
        return true;
    }
    if name.eq_ignore_ascii_case("icrit4")
        || name.eq_ignore_ascii_case("ic4")
        || name.eq_ignore_ascii_case("cp4")
    {
        *fourth_harmonic_current = Some(value.to_string());
        return true;
    }
    if name.eq_ignore_ascii_case("icrit5")
        || name.eq_ignore_ascii_case("ic5")
        || name.eq_ignore_ascii_case("cp5")
    {
        *fifth_harmonic_current = Some(value.to_string());
        return true;
    }
    if name.eq_ignore_ascii_case("icrit6")
        || name.eq_ignore_ascii_case("ic6")
        || name.eq_ignore_ascii_case("cp6")
    {
        *sixth_harmonic_current = Some(value.to_string());
        return true;
    }
    false
}

fn collect_external_josephson_translation_notes_from_line(
    line: &str,
    junction_models: &BTreeMap<String, ExternalJunctionModelCard>,
    notes: &mut Vec<String>,
) {
    let trimmed = strip_comment(line).trim();
    if trimmed.is_empty() {
        return;
    }

    if trimmed.to_ascii_lowercase().starts_with(".model") {
        let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 3 {
            return;
        }
        if !tokens[2].eq_ignore_ascii_case("jj")
            && !tokens[2].to_ascii_lowercase().starts_with("jj(")
        {
            return;
        }
        let normalized = trimmed.replace(',', " ");
        for token in collapse_spaced_assignments(&normalized.split_whitespace().collect::<Vec<_>>())
        {
            let Some((name, _)) = token.split_once('=') else {
                continue;
            };
            if name.eq_ignore_ascii_case("pi") {
                let value = token.split_once('=').map(|(_, value)| value).unwrap_or("");
                if parse_external_junction_pi_flag(value).is_none() {
                    notes.push(
                        "external_josim_translation_warning:jj_pi_model_unsupported".to_string(),
                    );
                }
            }
        }
        return;
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4 {
        return;
    }
    let Some(first) = tokens.first() else {
        return;
    };
    if !first.starts_with('J') && !first.starts_with('j') {
        return;
    }

    let normalized = normalized_junction_assignment_tokens(&tokens[3..]);
    let mut referenced_model = None::<String>;
    let mut saw_override = false;
    for (index, token) in normalized.iter().enumerate() {
        if !token.contains('=') {
            if index == 0 {
                referenced_model = Some(token.to_ascii_lowercase());
            }
            continue;
        }
        let Some((name, _)) = token.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("model") || name.eq_ignore_ascii_case("modelname") {
            referenced_model = Some(
                token
                    .split_once('=')
                    .map(|(_, value)| value.to_ascii_lowercase())
                    .unwrap_or_default(),
            );
            continue;
        }
        if name.eq_ignore_ascii_case("pi") {
            let value = token.split_once('=').map(|(_, value)| value).unwrap_or("");
            if parse_external_junction_pi_flag(value).is_none() {
                notes.push(
                    "external_josim_translation_warning:jj_pi_instance_unsupported".to_string(),
                );
            }
            continue;
        }
        if name.eq_ignore_ascii_case("icrit") || name.eq_ignore_ascii_case("ic") {
            continue;
        }
        if name.eq_ignore_ascii_case("icrit2")
            || name.eq_ignore_ascii_case("ic2")
            || name.eq_ignore_ascii_case("cp2")
        {
            continue;
        }
        if referenced_model.is_some()
            && (name.eq_ignore_ascii_case("icrit")
                || name.eq_ignore_ascii_case("ic")
                || name.eq_ignore_ascii_case("rn")
                || name.eq_ignore_ascii_case("cj")
                || name.eq_ignore_ascii_case("cap"))
        {
            saw_override = true;
        }
    }
    if referenced_model.is_some() && saw_override {
        let rewritten = rewrite_external_josephson_inline_parameters(trimmed, junction_models);
        if rewritten.len() == 1
            && rewritten[0] == rewrite_external_josephson_element_prefix(trimmed)
        {
            notes.push(
                "external_josim_translation_warning:jj_model_override_unsupported".to_string(),
            );
        }
    }
}

fn combine_external_result_with_notes(
    external_result: Option<String>,
    notes: &[String],
) -> Option<String> {
    if notes.is_empty() {
        return external_result;
    }
    let note_text = notes.join(";");
    match external_result {
        Some(result) if !result.is_empty() => Some(format!("{result};{note_text}")),
        _ => Some(note_text),
    }
}

fn parse_external_simulator_stderr_warnings(stderr: &str) -> Vec<String> {
    let mut warnings = Vec::<String>::new();
    for line in stderr.lines() {
        let trimmed = line.trim();
        let lowercase = trimmed.to_ascii_lowercase();
        if let Some(parameter) = lowercase.strip_prefix("the parameter:") {
            let parameter = parameter
                .trim()
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .to_ascii_lowercase();
            if !parameter.is_empty() {
                warnings.push(format!(
                    "external_josim_runtime_warning:unknown_model_parameter:{parameter}"
                ));
            }
        }
    }
    warnings.sort();
    warnings.dedup();
    warnings
}

fn rewrite_external_josephson_cards(
    line: &str,
    junction_models: &BTreeMap<String, ExternalJunctionModelCard>,
) -> Vec<String> {
    let model_rewritten = rewrite_external_josephson_model_card(line);
    rewrite_external_josephson_inline_parameters(&model_rewritten, junction_models)
}

fn rewrite_external_josephson_model_card(line: &str) -> String {
    let trimmed = strip_comment(line).trim();
    if !trimmed.to_ascii_lowercase().starts_with(".model") {
        return line.to_string();
    }
    let Some((model_name, card)) = parse_external_josephson_model_card(trimmed) else {
        return line.to_string();
    };
    build_external_josephson_model_card_line(&model_name, &card)
}

fn build_external_josephson_model_card_line(
    model_name: &str,
    card: &ExternalJunctionModelCard,
) -> String {
    let pi_junction = card.pi_junction.unwrap_or(false);
    if !pi_junction {
        if let (Some(native_basis_current), Some(native_cpr_coefficients)) = (
            &card.native_cpr_basis_current,
            &card.native_cpr_coefficients,
        ) {
            let mut arguments = vec![format!("icrit={native_basis_current}")];
            if let Some(normal_resistance) = &card.normal_resistance {
                arguments.push(format!("rn={normal_resistance}"));
            }
            if let Some(junction_cap) = &card.junction_cap {
                arguments.push(format!("cap={junction_cap}"));
            }
            arguments.push(format!("cpr={{{}}}", native_cpr_coefficients.join(",")));
            return format!(".model {model_name} jj({})", arguments.join(" "));
        }
    }
    let (harmonic_basis_current, harmonic_cpr) = build_external_junction_harmonic_terms(
        card.critical_current.clone(),
        card.second_harmonic_current.clone(),
        card.third_harmonic_current.clone(),
        card.fourth_harmonic_current.clone(),
        card.fifth_harmonic_current.clone(),
        card.sixth_harmonic_current.clone(),
        pi_junction,
    );
    let critical_current = harmonic_basis_current;
    let mut arguments = Vec::<String>::new();
    if let Some(critical_current) = critical_current {
        arguments.push(format!("icrit={critical_current}"));
    }
    if let Some(normal_resistance) = &card.normal_resistance {
        arguments.push(format!("rn={normal_resistance}"));
    }
    if let Some(junction_cap) = &card.junction_cap {
        arguments.push(format!("cap={junction_cap}"));
    }
    if let Some(harmonic_cpr) = harmonic_cpr {
        arguments.push(harmonic_cpr);
    }
    if arguments.is_empty() {
        return format!(".model {model_name} jj()");
    }
    format!(".model {model_name} jj({})", arguments.join(" "))
}

fn rewrite_external_josephson_inline_parameters(
    line: &str,
    junction_models: &BTreeMap<String, ExternalJunctionModelCard>,
) -> Vec<String> {
    let trimmed = strip_comment(line).trim();
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4 {
        return vec![rewrite_external_josephson_element_prefix(line)];
    }
    let Some(first) = tokens.first() else {
        return vec![line.to_string()];
    };
    if !first.starts_with('J') && !first.starts_with('j') {
        return vec![line.to_string()];
    }

    let remainder = &tokens[3..];
    let normalized = normalized_junction_assignment_tokens(remainder);
    if normalized.is_empty() {
        return vec![rewrite_external_josephson_element_prefix(line)];
    }

    let mut referenced_model = None::<String>;
    let mut supported_inline_parameters = Vec::<String>::new();
    let mut instance_pi_junction = None::<bool>;
    let mut instance_second_harmonic = None::<String>;
    let mut instance_third_harmonic = None::<String>;
    let mut instance_fourth_harmonic = None::<String>;
    let mut instance_fifth_harmonic = None::<String>;
    let mut instance_sixth_harmonic = None::<String>;
    let mut instance_native_cpr_coefficients = None::<Vec<String>>;
    for (index, token) in normalized.iter().enumerate() {
        if let Some((name, value)) = token.split_once('=') {
            if name.eq_ignore_ascii_case("model") || name.eq_ignore_ascii_case("modelname") {
                referenced_model = Some(value.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("icrit") || name.eq_ignore_ascii_case("ic") {
                supported_inline_parameters.push(format!("icrit={value}"));
                continue;
            }
            if parse_external_runtime_harmonic_assignment(
                name,
                value,
                &mut instance_second_harmonic,
                &mut instance_third_harmonic,
                &mut instance_fourth_harmonic,
                &mut instance_fifth_harmonic,
                &mut instance_sixth_harmonic,
            ) {
                continue;
            }
            if name.eq_ignore_ascii_case("rn") {
                supported_inline_parameters.push(format!("rn={value}"));
                continue;
            }
            if name.eq_ignore_ascii_case("cj") || name.eq_ignore_ascii_case("cap") {
                supported_inline_parameters.push(format!("cap={value}"));
                continue;
            }
            if name.eq_ignore_ascii_case("pi") {
                instance_pi_junction = parse_external_junction_pi_flag(value);
                continue;
            }
            if name.eq_ignore_ascii_case("cpr") {
                instance_native_cpr_coefficients = parse_cpr_coefficients(value);
            }
            continue;
        }
        if index == 0 {
            referenced_model = Some(token.to_string());
        }
    }

    if let Some(model_name) = referenced_model {
        if supported_inline_parameters.is_empty()
            && instance_pi_junction.is_none()
            && instance_second_harmonic.is_none()
            && instance_third_harmonic.is_none()
            && instance_fourth_harmonic.is_none()
            && instance_fifth_harmonic.is_none()
            && instance_sixth_harmonic.is_none()
            && instance_native_cpr_coefficients.is_none()
        {
            let mut instance_name = (*first).to_string();
            instance_name.replace_range(..1, "B");
            return vec![format!(
                "{instance_name} {} {} {model_name}",
                tokens[1], tokens[2]
            )];
        }
        let normalized_model_name = model_name.to_ascii_lowercase();
        if let Some(model_defaults) = junction_models.get(&normalized_model_name) {
            let merged_model_name = build_external_josephson_model_name(first);
            let merged_model_line = build_external_merged_josephson_model_line(
                &merged_model_name,
                model_defaults,
                &supported_inline_parameters,
                instance_pi_junction,
                instance_second_harmonic.as_deref(),
                instance_third_harmonic.as_deref(),
                instance_fourth_harmonic.as_deref(),
                instance_fifth_harmonic.as_deref(),
                instance_sixth_harmonic.as_deref(),
                instance_native_cpr_coefficients.as_deref(),
            );
            if let Some(merged_model_line) = merged_model_line {
                let mut instance_name = (*first).to_string();
                instance_name.replace_range(..1, "B");
                return vec![
                    merged_model_line,
                    format!(
                        "{instance_name} {} {} {merged_model_name}",
                        tokens[1], tokens[2]
                    ),
                ];
            }
        }
        return vec![rewrite_external_josephson_element_prefix(line)];
    }

    if supported_inline_parameters.is_empty()
        && instance_pi_junction.is_none()
        && instance_second_harmonic.is_none()
        && instance_third_harmonic.is_none()
        && instance_fourth_harmonic.is_none()
        && instance_fifth_harmonic.is_none()
        && instance_sixth_harmonic.is_none()
        && instance_native_cpr_coefficients.is_none()
    {
        return vec![rewrite_external_josephson_element_prefix(line)];
    }

    let model_name = build_external_josephson_model_name(first);
    let mut instance_name = (*first).to_string();
    instance_name.replace_range(..1, "B");
    let empty_defaults = ExternalJunctionModelCard {
        critical_current: None,
        second_harmonic_current: None,
        third_harmonic_current: None,
        fourth_harmonic_current: None,
        fifth_harmonic_current: None,
        sixth_harmonic_current: None,
        normal_resistance: None,
        junction_cap: None,
        pi_junction: None,
        native_cpr_basis_current: None,
        native_cpr_coefficients: None,
    };
    let Some(model_line) = build_external_merged_josephson_model_line(
        &model_name,
        &empty_defaults,
        &supported_inline_parameters,
        instance_pi_junction,
        instance_second_harmonic.as_deref(),
        instance_third_harmonic.as_deref(),
        instance_fourth_harmonic.as_deref(),
        instance_fifth_harmonic.as_deref(),
        instance_sixth_harmonic.as_deref(),
        instance_native_cpr_coefficients.as_deref(),
    ) else {
        return vec![rewrite_external_josephson_element_prefix(line)];
    };
    vec![
        model_line,
        format!("{instance_name} {} {} {model_name}", tokens[1], tokens[2]),
    ]
}

fn build_external_josephson_model_name(instance_name: &str) -> String {
    let sanitized = instance_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("rflux_auto_{sanitized}")
}

fn build_external_merged_josephson_model_line(
    model_name: &str,
    model_defaults: &ExternalJunctionModelCard,
    overrides: &[String],
    pi_override: Option<bool>,
    second_harmonic_override: Option<&str>,
    third_harmonic_override: Option<&str>,
    fourth_harmonic_override: Option<&str>,
    fifth_harmonic_override: Option<&str>,
    sixth_harmonic_override: Option<&str>,
    native_cpr_override: Option<&[String]>,
) -> Option<String> {
    let mut critical_current = model_defaults.critical_current.clone();
    let mut second_harmonic_current = model_defaults.second_harmonic_current.clone();
    let mut third_harmonic_current = model_defaults.third_harmonic_current.clone();
    let mut fourth_harmonic_current = model_defaults.fourth_harmonic_current.clone();
    let mut fifth_harmonic_current = model_defaults.fifth_harmonic_current.clone();
    let mut sixth_harmonic_current = model_defaults.sixth_harmonic_current.clone();
    let mut normal_resistance = model_defaults.normal_resistance.clone();
    let mut junction_cap = model_defaults.junction_cap.clone();
    let mut native_cpr_basis_current = model_defaults.native_cpr_basis_current.clone();
    let mut native_cpr_coefficients = model_defaults.native_cpr_coefficients.clone();
    for token in overrides {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("icrit") || name.eq_ignore_ascii_case("ic") {
            critical_current = Some(value.to_string());
            if native_cpr_coefficients.is_some() {
                native_cpr_basis_current = Some(value.to_string());
            }
            continue;
        }
        if parse_external_runtime_harmonic_assignment(
            name,
            value,
            &mut second_harmonic_current,
            &mut third_harmonic_current,
            &mut fourth_harmonic_current,
            &mut fifth_harmonic_current,
            &mut sixth_harmonic_current,
        ) {
            native_cpr_basis_current = None;
            native_cpr_coefficients = None;
            continue;
        }
        if name.eq_ignore_ascii_case("rn") {
            normal_resistance = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("cap") || name.eq_ignore_ascii_case("cj") {
            junction_cap = Some(value.to_string());
        }
    }
    if let Some(second_harmonic_override) = second_harmonic_override {
        second_harmonic_current = Some(second_harmonic_override.to_string());
        native_cpr_basis_current = None;
        native_cpr_coefficients = None;
    }
    if let Some(third_harmonic_override) = third_harmonic_override {
        third_harmonic_current = Some(third_harmonic_override.to_string());
        native_cpr_basis_current = None;
        native_cpr_coefficients = None;
    }
    if let Some(fourth_harmonic_override) = fourth_harmonic_override {
        fourth_harmonic_current = Some(fourth_harmonic_override.to_string());
        native_cpr_basis_current = None;
        native_cpr_coefficients = None;
    }
    if let Some(fifth_harmonic_override) = fifth_harmonic_override {
        fifth_harmonic_current = Some(fifth_harmonic_override.to_string());
        native_cpr_basis_current = None;
        native_cpr_coefficients = None;
    }
    if let Some(sixth_harmonic_override) = sixth_harmonic_override {
        sixth_harmonic_current = Some(sixth_harmonic_override.to_string());
        native_cpr_basis_current = None;
        native_cpr_coefficients = None;
    }
    if let Some(native_cpr_override) = native_cpr_override {
        native_cpr_basis_current = overrides
            .iter()
            .find_map(|token| {
                let (name, value) = token.split_once('=')?;
                if name.eq_ignore_ascii_case("icrit") || name.eq_ignore_ascii_case("ic") {
                    Some(value.to_string())
                } else {
                    None
                }
            })
            .or_else(|| model_defaults.native_cpr_basis_current.clone())
            .or_else(|| critical_current.clone());
        native_cpr_coefficients = Some(native_cpr_override.to_vec());
        second_harmonic_current = None;
        third_harmonic_current = None;
        fourth_harmonic_current = None;
        fifth_harmonic_current = None;
        sixth_harmonic_current = None;
    }
    let pi_junction = pi_override.or(model_defaults.pi_junction).unwrap_or(false);
    if pi_override.is_some() {
        native_cpr_basis_current = None;
        native_cpr_coefficients = None;
    }
    let line = build_external_josephson_model_card_line(
        model_name,
        &ExternalJunctionModelCard {
            critical_current,
            second_harmonic_current,
            third_harmonic_current,
            fourth_harmonic_current,
            fifth_harmonic_current,
            sixth_harmonic_current,
            normal_resistance,
            junction_cap,
            pi_junction: Some(pi_junction),
            native_cpr_basis_current,
            native_cpr_coefficients,
        },
    );
    if line.ends_with("jj()") {
        return None;
    }
    Some(line)
}

fn strip_params_marker(line: &str) -> String {
    let mut rewritten = line.to_string();
    loop {
        let lowercase = rewritten.to_ascii_lowercase();
        let Some(index) = lowercase.find(" params:") else {
            break;
        };
        rewritten.replace_range(index..index + " params:".len(), " ");
    }
    rewritten
}

fn rewrite_external_josephson_element_prefix(line: &str) -> String {
    let trimmed = strip_comment(line).trim();
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4 {
        return line.to_string();
    }
    let Some(first) = tokens.first() else {
        return line.to_string();
    };
    if !first.starts_with('J') && !first.starts_with('j') {
        return line.to_string();
    }

    let mut rewritten = (*first).to_string();
    rewritten.replace_range(..1, "B");
    if tokens.len() == 1 {
        return rewritten;
    }
    format!("{} {}", rewritten, tokens[1..].join(" "))
}

fn rewrite_external_mutual_coupling_arguments(line: &str) -> String {
    let trimmed = strip_comment(line).trim();
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4 {
        return line.to_string();
    }
    let Some(first) = tokens.first() else {
        return line.to_string();
    };
    if !first.starts_with('K') && !first.starts_with('k') {
        return line.to_string();
    }

    let remainder = &tokens[3..];
    if remainder.len() == 1 {
        if let Some(value) = remainder[0].strip_prefix("coupling=") {
            return format!("{} {} {} {}", tokens[0], tokens[1], tokens[2], value);
        }
        return line.to_string();
    }
    if remainder.len() >= 2 && remainder[0].eq_ignore_ascii_case("coupling") {
        if remainder[1] == "=" && remainder.len() >= 3 {
            let value = remainder[2];
            if !value.is_empty() {
                return format!("{} {} {} {}", tokens[0], tokens[1], tokens[2], value);
            }
        }
        let value = remainder[1].trim_start_matches('=');
        if !value.is_empty() {
            return format!("{} {} {} {}", tokens[0], tokens[1], tokens[2], value);
        }
    }
    line.to_string()
}

fn rewrite_external_source_keyword_calls(line: &str) -> String {
    let trimmed = strip_comment(line).trim();
    if trimmed.is_empty() {
        return line.to_string();
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4 {
        return line.to_string();
    }
    let Some(prefix) = tokens[0].chars().next() else {
        return line.to_string();
    };
    if !matches!(prefix, 'V' | 'v' | 'I' | 'i') {
        return line.to_string();
    }

    let descriptor = tokens[3..].join(" ");
    let Some(rewritten_descriptor) = rewrite_external_source_descriptor(&descriptor) else {
        return line.to_string();
    };
    format!(
        "{} {} {} {}",
        tokens[0], tokens[1], tokens[2], rewritten_descriptor
    )
}

fn rewrite_external_source_descriptor(descriptor: &str) -> Option<String> {
    if let Some(args) = parse_source_call_arguments(descriptor, "pulse") {
        return rewrite_external_pulse_arguments(args).map(|args| format!("PULSE({args})"));
    }
    if let Some(args) = parse_source_call_arguments(descriptor, "exp") {
        return rewrite_external_exp_arguments(args).map(|args| format!("EXP({args})"));
    }
    if let Some(args) = parse_source_call_arguments(descriptor, "sin") {
        return rewrite_external_sin_arguments(args).map(|args| format!("SIN({args})"));
    }
    None
}

fn rewrite_external_pulse_arguments(args: &str) -> Option<String> {
    let values = split_source_arguments(args);
    let collapsed = collapse_spaced_assignments(&values);
    if !collapsed.iter().any(|value| value.contains('=')) {
        return None;
    }

    let mut low = None::<String>;
    let mut high = None::<String>;
    let mut delay = None::<String>;
    let mut rise = None::<String>;
    let mut fall = None::<String>;
    let mut width = None::<String>;
    let mut period = None::<String>;
    let mut cycles = None::<String>;
    for value in collapsed {
        let (name, expr) = value.split_once('=')?;
        if name.eq_ignore_ascii_case("v1") || name.eq_ignore_ascii_case("low") {
            low = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("v2") || name.eq_ignore_ascii_case("high") {
            high = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("td") || name.eq_ignore_ascii_case("delay") {
            delay = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("tr") || name.eq_ignore_ascii_case("rise") {
            rise = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("tf") || name.eq_ignore_ascii_case("fall") {
            fall = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("pw") || name.eq_ignore_ascii_case("width") {
            width = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("per") || name.eq_ignore_ascii_case("period") {
            period = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("ncycles") || name.eq_ignore_ascii_case("cycles") {
            cycles = Some(expr.to_string());
            continue;
        }
        return None;
    }

    let mut fields = vec![low?, high?, delay?, rise?, fall?, width?];
    if let Some(period) = period {
        fields.push(period);
    }
    if let Some(cycles) = cycles {
        if fields.len() == 6 {
            return None;
        }
        fields.push(cycles);
    }
    Some(fields.join(" "))
}

fn rewrite_external_exp_arguments(args: &str) -> Option<String> {
    let values = split_source_arguments(args);
    let collapsed = collapse_spaced_assignments(&values);
    if !collapsed.iter().any(|value| value.contains('=')) {
        return None;
    }

    let mut initial = None::<String>;
    let mut pulsed = None::<String>;
    let mut rise_delay = None::<String>;
    let mut rise_tau = None::<String>;
    let mut fall_delay = None::<String>;
    let mut fall_tau = None::<String>;
    for value in collapsed {
        let (name, expr) = value.split_once('=')?;
        if name.eq_ignore_ascii_case("v1")
            || name.eq_ignore_ascii_case("initial")
            || name.eq_ignore_ascii_case("low")
        {
            initial = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("v2")
            || name.eq_ignore_ascii_case("pulsed")
            || name.eq_ignore_ascii_case("high")
        {
            pulsed = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("td1")
            || name.eq_ignore_ascii_case("rise_delay")
            || name.eq_ignore_ascii_case("delay1")
        {
            rise_delay = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("tau1")
            || name.eq_ignore_ascii_case("rise_tau")
            || name.eq_ignore_ascii_case("tau_rise")
        {
            rise_tau = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("td2")
            || name.eq_ignore_ascii_case("fall_delay")
            || name.eq_ignore_ascii_case("delay2")
        {
            fall_delay = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("tau2")
            || name.eq_ignore_ascii_case("fall_tau")
            || name.eq_ignore_ascii_case("tau_fall")
        {
            fall_tau = Some(expr.to_string());
            continue;
        }
        return None;
    }

    Some(
        [
            initial?,
            pulsed?,
            rise_delay?,
            rise_tau?,
            fall_delay?,
            fall_tau?,
        ]
        .join(" "),
    )
}

fn rewrite_external_sin_arguments(args: &str) -> Option<String> {
    let values = split_source_arguments(args);
    let collapsed = collapse_spaced_assignments(&values);
    if !collapsed.iter().any(|value| value.contains('=')) {
        return None;
    }

    let mut offset = None::<String>;
    let mut amplitude = None::<String>;
    let mut frequency = None::<String>;
    let mut delay = None::<String>;
    let mut damping = None::<String>;
    let mut phase = None::<String>;
    for value in collapsed {
        let (name, expr) = value.split_once('=')?;
        if name.eq_ignore_ascii_case("vo") || name.eq_ignore_ascii_case("offset") {
            offset = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("va") || name.eq_ignore_ascii_case("amplitude") {
            amplitude = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("freq")
            || name.eq_ignore_ascii_case("frequency")
            || name.eq_ignore_ascii_case("f")
        {
            frequency = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("td") || name.eq_ignore_ascii_case("delay") {
            delay = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("theta")
            || name.eq_ignore_ascii_case("damp")
            || name.eq_ignore_ascii_case("damping")
        {
            damping = Some(expr.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("phase")
            || name.eq_ignore_ascii_case("phase_deg")
            || name.eq_ignore_ascii_case("phi")
        {
            phase = Some(expr.to_string());
            continue;
        }
        return None;
    }

    let mut fields = vec![offset?, amplitude?, frequency?];
    if let Some(delay) = delay {
        fields.push(delay);
    }
    if let Some(damping) = damping {
        if fields.len() == 3 {
            fields.push("0".to_string());
        }
        fields.push(damping);
    }
    if let Some(phase) = phase {
        if fields.len() == 3 {
            fields.push("0".to_string());
        }
        fields.push(phase);
    }
    Some(fields.join(" "))
}

fn strip_external_tran_uic(line: &str) -> String {
    let trimmed = strip_comment(line).trim();
    if !trimmed.to_ascii_lowercase().starts_with(".tran") {
        return line.to_string();
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let filtered = tokens
        .into_iter()
        .filter(|token| !token.eq_ignore_ascii_case("uic"))
        .collect::<Vec<_>>();
    filtered.join(" ")
}

fn inline_external_waveform_file_source(line: &str, include_base_dir: Option<&Path>) -> String {
    let trimmed = strip_comment(line).trim();
    if trimmed.is_empty() || !trimmed.to_ascii_lowercase().contains("pwl") {
        return line.to_string();
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4 {
        return line.to_string();
    }
    let Some(prefix) = tokens[0].chars().next() else {
        return line.to_string();
    };
    if !matches!(prefix, 'V' | 'v' | 'I' | 'i') {
        return line.to_string();
    }

    let descriptor = tokens[3..].join(" ");
    let Some(args) = parse_source_call_arguments(&descriptor, "pwl") else {
        return line.to_string();
    };
    let values = split_source_arguments(args);
    let Some(raw_path) = parse_waveform_file_source_argument(&values) else {
        return line.to_string();
    };

    let resolved_path = resolve_external_waveform_source_path(include_base_dir, &raw_path);
    let Ok(points) = parse_pwl_points_from_file(&resolved_path, &BTreeMap::new()) else {
        return line.to_string();
    };
    let inline_pwl = format_inline_pwl_points(&points);
    format!(
        "{} {} {} PWL({})",
        tokens[0], tokens[1], tokens[2], inline_pwl
    )
}

fn resolve_external_waveform_source_path(
    include_base_dir: Option<&Path>,
    raw_path: &str,
) -> String {
    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        return raw_path.to_string();
    }
    if candidate.is_file() {
        return raw_path.to_string();
    }
    let resolved = resolve_waveform_source_path(include_base_dir, raw_path);
    if Path::new(&resolved).is_file() {
        return resolved;
    }
    raw_path.to_string()
}

fn format_inline_pwl_points(points: &[(f64, f64)]) -> String {
    let mut fields = Vec::with_capacity(points.len() * 2);
    for (time_s, value) in points {
        fields.push(format!("{time_s:.12e}"));
        fields.push(format!("{value:.12e}"));
    }
    fields.join(" ")
}

fn build_external_simulator_command(
    command: &str,
    deck_path: &Path,
    waveform_path: &Path,
) -> std::process::Command {
    let mut child = std::process::Command::new(command);
    child
        .arg("-a")
        .arg("0")
        .arg("-o")
        .arg(waveform_path)
        .arg(deck_path);
    let env_names = std::env::vars_os().map(|(name, _)| name);
    let _ = apply_external_env_sanitization(&mut child, env_names);
    child
}

fn apply_external_env_sanitization<I>(
    command: &mut std::process::Command,
    env_names: I,
) -> Vec<String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut removed = Vec::new();
    for name in env_names {
        if should_strip_external_env_var(&name) {
            command.env_remove(&name);
            removed.push(name.to_string_lossy().into_owned());
        }
    }
    removed.sort();
    removed.dedup();
    removed
}

fn should_strip_external_env_var(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.starts_with("RFLOW_") || name.starts_with("JOSIM_")
}

fn create_external_run_dir(
    base_temp_dir: &Path,
    process_id: u32,
    timestamp_millis: u128,
) -> Result<PathBuf, std::io::Error> {
    let run_dir = base_temp_dir.join(format!("rflux-ext-{}-{}", process_id, timestamp_millis));
    fs::create_dir(&run_dir)?;
    Ok(run_dir)
}

fn stage_external_run_artifacts(
    run_dir: &Path,
    deck_path: &Path,
    waveform_source_path: Option<&Path>,
    process_id: u32,
    timestamp_millis: u128,
    cleanup_run_dir: bool,
) -> (Option<String>, Option<String>) {
    let staged_deck_path = std::env::temp_dir().join(format!(
        "rflux-ext-{}-{}-input.sp",
        process_id, timestamp_millis
    ));
    let staged_waveform_path = std::env::temp_dir().join(format!(
        "rflux-ext-{}-{}-external_output.csv",
        process_id, timestamp_millis
    ));
    let deck_copied = fs::copy(deck_path, &staged_deck_path).is_ok();
    let waveform_copied = waveform_source_path
        .filter(|path| path.is_file())
        .map(|source_path| fs::copy(source_path, &staged_waveform_path).is_ok())
        .unwrap_or(false);
    if cleanup_run_dir && deck_copied && waveform_copied {
        let _ = fs::remove_dir_all(run_dir);
    }
    let generated_deck_path = Some(if deck_copied {
        staged_deck_path.display().to_string()
    } else {
        deck_path.display().to_string()
    });
    let waveform_path = waveform_source_path.map(|path| {
        if waveform_copied {
            staged_waveform_path.display().to_string()
        } else {
            path.display().to_string()
        }
    });
    (generated_deck_path, waveform_path)
}

fn strip_comment(line: &str) -> &str {
    match line.find('*') {
        Some(0) => "",
        Some(index) => &line[..index],
        None => line,
    }
}

fn parse_include_path(value: &str) -> Result<String, SimulationError> {
    let raw = value.trim();
    if raw.is_empty() {
        return Err(SimulationError::IncludeWithoutBase(String::new()));
    }
    if let Some(inner) = raw
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
    {
        return Ok(inner.trim().to_string());
    }
    Ok(raw.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LibraryDirective {
    include_path: String,
    section: Option<String>,
}

fn parse_library_directive(value: &str) -> Result<LibraryDirective, SimulationError> {
    let raw = value.trim();
    if raw.is_empty() {
        return Err(SimulationError::IncludeWithoutBase(String::new()));
    }

    if let Some(quote) = raw.chars().next().filter(|ch| *ch == '"' || *ch == '\'') {
        let tail = &raw[1..];
        let Some(closing_index) = tail.find(quote) else {
            return Err(SimulationError::IncludeWithoutBase(raw.to_string()));
        };
        let include_path = tail[..closing_index].trim().to_string();
        let section = parse_library_section_from_text(&tail[closing_index + 1..]);
        return Ok(LibraryDirective {
            include_path,
            section,
        });
    }

    let normalized = raw.replace(',', " ");
    let mut tokens = normalized.split_whitespace();
    let include_path = tokens.next().unwrap_or_default().trim().to_string();
    Ok(LibraryDirective {
        include_path,
        section: parse_library_section_from_text(&tokens.collect::<Vec<_>>().join(" ")),
    })
}

fn normalize_library_section_token(token: &str) -> String {
    let trimmed = token.trim().trim_matches(',');
    let trimmed = trimmed.split(';').next().unwrap_or_default().trim();
    strip_wrapping_quotes(trimmed)
}

fn parse_library_section_from_text(text: &str) -> Option<String> {
    let normalized = text.replace(',', " ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index];
        let normalized_token = normalize_library_section_token(token);
        if normalized_token.is_empty() {
            index += 1;
            continue;
        }

        if token == "=" {
            index += 1;
            continue;
        }

        if let Some((key, value)) = token.split_once('=') {
            if key.eq_ignore_ascii_case("section") || key.eq_ignore_ascii_case("sec") {
                let normalized_value = normalize_library_section_token(value);
                if !normalized_value.is_empty() {
                    return Some(normalized_value);
                }
                if let Some(next) = tokens.get(index + 1) {
                    let normalized_next = normalize_library_section_token(next);
                    if !normalized_next.is_empty() {
                        return Some(normalized_next);
                    }
                }
                return None;
            }
            return Some(normalized_token);
        }

        if token.eq_ignore_ascii_case("section") || token.eq_ignore_ascii_case("sec") {
            if let Some(next) = tokens.get(index + 1) {
                if *next == "=" {
                    if let Some(value) = tokens.get(index + 2) {
                        let normalized_value = normalize_library_section_token(value);
                        return (!normalized_value.is_empty()).then_some(normalized_value);
                    }
                    return None;
                }
                if let Some(value) = next.strip_prefix('=') {
                    let normalized_value = normalize_library_section_token(value);
                    if !normalized_value.is_empty() {
                        return Some(normalized_value);
                    }
                    if let Some(fallback) = tokens.get(index + 2) {
                        let normalized_fallback = normalize_library_section_token(fallback);
                        return (!normalized_fallback.is_empty()).then_some(normalized_fallback);
                    }
                    return None;
                }
                let normalized_value = normalize_library_section_token(next);
                return (!normalized_value.is_empty()).then_some(normalized_value);
            }
            return None;
        }

        return Some(normalized_token);
    }
    None
}

fn resolve_include_path(base_dir: &Path, include_path: &str) -> PathBuf {
    let candidate = Path::new(include_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    }
}

fn expand_deck_file(path: &Path) -> Result<String, SimulationError> {
    let deck = fs::read_to_string(path).map_err(|err| SimulationError::Io {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    expand_deck_text(&deck, path.parent())
}

fn extract_library_section_text(
    deck: &str,
    section: &str,
    source_path: &Path,
) -> Result<String, SimulationError> {
    let mut extracted = String::new();
    let mut collecting = false;
    let mut found_section = false;
    let mut depth = 0usize;

    for raw_line in deck.lines() {
        let trimmed = strip_comment(raw_line).trim();
        if let Some(rest) = strip_control_card_prefix(trimmed, ".lib") {
            if collecting {
                depth += 1;
                extracted.push_str(raw_line);
                extracted.push('\n');
                continue;
            }
            let current_section = rest.split_whitespace().collect::<Vec<_>>().join(" ");
            let current_section =
                parse_library_section_from_text(&current_section).unwrap_or_default();
            if current_section.eq_ignore_ascii_case(section) {
                collecting = true;
                found_section = true;
                depth = 1;
            }
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(trimmed, ".endl") {
            if collecting {
                let end_section = parse_library_section_from_text(rest).unwrap_or_default();
                if depth == 1 {
                    if !end_section.is_empty() && !end_section.eq_ignore_ascii_case(section) {
                        return Err(SimulationError::MismatchedLibrarySectionEnd {
                            path: source_path.display().to_string(),
                            expected: section.to_string(),
                            found: end_section,
                        });
                    }
                    return Ok(extracted);
                }
                depth -= 1;
                extracted.push_str(raw_line);
                extracted.push('\n');
            }
            continue;
        }

        if collecting {
            extracted.push_str(raw_line);
            extracted.push('\n');
        }
    }

    let path = source_path.display().to_string();
    if collecting {
        return Err(SimulationError::UnterminatedLibrarySection {
            path,
            section: section.to_string(),
        });
    }
    if !found_section {
        return Err(SimulationError::MissingLibrarySection {
            path,
            section: section.to_string(),
        });
    }
    Ok(extracted)
}

fn expand_library_file(path: &Path, section: Option<&str>) -> Result<String, SimulationError> {
    let deck = fs::read_to_string(path).map_err(|err| SimulationError::Io {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    if let Some(section) = section {
        let section_text = extract_library_section_text(&deck, section, path)?;
        return expand_deck_text(&section_text, path.parent());
    }
    expand_deck_text(&deck, path.parent())
}

fn expand_deck_text(deck: &str, base_dir: Option<&Path>) -> Result<String, SimulationError> {
    let mut expanded = String::new();
    for raw_line in deck.lines() {
        let trimmed = strip_comment(raw_line).trim();
        if let Some(rest) = strip_control_card_prefix(trimmed, ".include") {
            let include_path = parse_include_path(rest.trim())?;
            let Some(base_dir) = base_dir else {
                return Err(SimulationError::IncludeWithoutBase(include_path));
            };
            let resolved_path = resolve_include_path(base_dir, &include_path);
            expanded.push_str(&expand_deck_file(&resolved_path)?);
            if !expanded.ends_with('\n') {
                expanded.push('\n');
            }
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(trimmed, ".lib") {
            let directive = parse_library_directive(rest.trim())?;
            let Some(base_dir) = base_dir else {
                return Err(SimulationError::IncludeWithoutBase(directive.include_path));
            };
            let resolved_path = resolve_include_path(base_dir, &directive.include_path);
            expanded.push_str(&expand_library_file(
                &resolved_path,
                directive.section.as_deref(),
            )?);
            if !expanded.ends_with('\n') {
                expanded.push('\n');
            }
            continue;
        }

        let normalized_line = rewrite_relative_waveform_source_paths(raw_line, base_dir);
        expanded.push_str(&normalized_line);
        expanded.push('\n');
    }
    Ok(expanded)
}

fn rewrite_relative_waveform_source_paths(raw_line: &str, base_dir: Option<&Path>) -> String {
    let Some(base_dir) = base_dir else {
        return raw_line.to_string();
    };
    let lowercase = raw_line.to_ascii_lowercase();
    if !lowercase.contains("pwl") {
        return raw_line.to_string();
    }

    let mut rewritten = raw_line.to_string();
    for marker in ["file=", "path="] {
        let search = rewritten.to_ascii_lowercase();
        let Some(marker_index) = search.find(marker) else {
            continue;
        };
        let value_start = marker_index + marker.len();
        let value_slice = &rewritten[value_start..];
        let Some((raw_value, consumed_len)) = extract_waveform_path_token(value_slice) else {
            continue;
        };
        let stripped = strip_wrapping_quotes(raw_value);
        let resolved = resolve_waveform_source_path(Some(base_dir), &stripped);
        if resolved == stripped {
            continue;
        }
        let replacement = if raw_value.starts_with('"') {
            format!("\"{resolved}\"")
        } else if raw_value.starts_with('\'') {
            format!("'{resolved}'")
        } else {
            resolved
        };
        rewritten.replace_range(value_start..value_start + consumed_len, &replacement);
    }
    rewritten
}

fn extract_waveform_path_token(text: &str) -> Option<(&str, usize)> {
    let trimmed_start = text.len() - text.trim_start().len();
    let text = &text[trimmed_start..];
    if text.is_empty() {
        return None;
    }
    let first = text.chars().next()?;
    if first == '"' || first == '\'' {
        let closing = text[1..].find(first)? + 2;
        return Some((&text[..closing], trimmed_start + closing));
    }
    let end = text
        .find(|ch: char| ch == ',' || ch == ')' || ch.is_ascii_whitespace())
        .unwrap_or(text.len());
    Some((&text[..end], trimmed_start + end))
}

fn flatten_subckts(deck: &str) -> Result<String, SimulationError> {
    const ROOT_SCOPE: &str = "";

    let mut defs = BTreeMap::<String, SubcktDef>::new();
    let mut top_level_lines = Vec::<String>::new();
    let mut scope_symbols = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut scope_parent = BTreeMap::<String, String>::new();
    let mut def_stack = Vec::<SubcktFrame>::new();
    scope_symbols.entry(ROOT_SCOPE.to_string()).or_default();

    for raw_line in deck.lines() {
        let line = strip_comment(raw_line).trim();

        if let Some(rest) = strip_control_card_prefix(line, ".subckt") {
            let (name, def) = parse_subckt_header(rest.trim())?;
            let parent_scope = def_stack
                .last()
                .map(|frame| frame.scoped_name.as_str())
                .unwrap_or(ROOT_SCOPE);
            let scoped_name = scoped_subckt_name(parent_scope, &name);
            let symbols = scope_symbols.entry(parent_scope.to_string()).or_default();
            if symbols.contains_key(&name) {
                return Err(SimulationError::DuplicateSubcktDefinition {
                    scope: display_scope_name(parent_scope),
                    name,
                });
            }
            symbols.insert(name.clone(), scoped_name.clone());
            scope_symbols.entry(scoped_name.clone()).or_default();
            scope_parent.insert(scoped_name.clone(), parent_scope.to_string());
            def_stack.push(SubcktFrame {
                local_name: name,
                scoped_name,
                def,
            });
            continue;
        }

        if let Some(rest) = strip_control_card_prefix(line, ".ends") {
            if let Some(frame) = def_stack.pop() {
                let end_name = rest.trim();
                if !end_name.is_empty() && !end_name.eq_ignore_ascii_case(&frame.local_name) {
                    return Err(SimulationError::MismatchedEnds {
                        expected: frame.local_name,
                        found: end_name.to_string(),
                    });
                }
                defs.insert(frame.scoped_name, frame.def);
                continue;
            }
        }

        if let Some(frame) = def_stack.last_mut() {
            if line.starts_with('.') {
                return Err(SimulationError::UnsupportedSubcktControl {
                    subckt: frame.local_name.clone(),
                    line: line.to_string(),
                });
            }
            frame.def.body.push(raw_line.to_string());
            continue;
        }

        top_level_lines.push(raw_line.to_string());
    }

    if let Some(frame) = def_stack.last() {
        return Err(SimulationError::MissingEnds(frame.local_name.clone()));
    }

    expand_subckt_lines(
        &top_level_lines,
        &defs,
        &scope_symbols,
        &scope_parent,
        ROOT_SCOPE,
        None,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubcktFrame {
    local_name: String,
    scoped_name: String,
    def: SubcktDef,
}

fn scoped_subckt_name(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}__{}", scope, name)
    }
}

fn display_scope_name(scope: &str) -> String {
    if scope.is_empty() {
        "<top-level>".to_string()
    } else {
        scope.to_string()
    }
}

fn resolve_subckt_name(
    local_name: &str,
    declaration_scope: &str,
    scope_symbols: &BTreeMap<String, BTreeMap<String, String>>,
    scope_parent: &BTreeMap<String, String>,
) -> Option<String> {
    let mut scope = declaration_scope.to_string();
    loop {
        if let Some(symbols) = scope_symbols.get(&scope) {
            if let Some(scoped_name) = symbols.get(local_name) {
                return Some(scoped_name.clone());
            }
        }
        if scope.is_empty() {
            break;
        }
        let Some(parent) = scope_parent.get(&scope) else {
            break;
        };
        scope = parent.clone();
    }
    None
}

fn parse_subckt_header(header: &str) -> Result<(String, SubcktDef), SimulationError> {
    let tokens = header.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(SimulationError::InvalidSubcktHeader(header.to_string()));
    }

    let name = tokens[0].to_string();
    let mut pins = Vec::new();
    let mut default_params = BTreeMap::new();
    let mut in_params = false;
    for token in &tokens[1..] {
        if token.eq_ignore_ascii_case("params:") {
            in_params = true;
            continue;
        }
        if in_params || token.contains('=') {
            let (param, value) = token
                .split_once('=')
                .ok_or_else(|| SimulationError::InvalidSubcktHeader(header.to_string()))?;
            default_params.insert(param.trim().to_ascii_lowercase(), value.trim().to_string());
        } else {
            pins.push((*token).to_string());
        }
    }

    Ok((
        name,
        SubcktDef {
            pins,
            default_params,
            body: Vec::new(),
        },
    ))
}

fn expand_subckt_lines(
    lines: &[String],
    defs: &BTreeMap<String, SubcktDef>,
    scope_symbols: &BTreeMap<String, BTreeMap<String, String>>,
    scope_parent: &BTreeMap<String, String>,
    declaration_scope: &str,
    instance_prefix: Option<&str>,
) -> Result<String, SimulationError> {
    let mut expanded = String::new();
    for raw_line in lines {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            expanded.push_str(raw_line);
            expanded.push('\n');
            continue;
        }

        if starts_with_instance(line) {
            let instance =
                parse_subckt_instance(line, defs, scope_symbols, scope_parent, declaration_scope)?;
            let nested_prefix = match instance_prefix {
                Some(prefix) => format!("{}__{}", prefix, instance.instance_name),
                None => instance.instance_name.clone(),
            };
            let def = defs
                .get(&instance.subckt_key)
                .ok_or_else(|| SimulationError::UnknownSubckt(instance.subckt_name.clone()))?;

            let mut body_lines = Vec::with_capacity(def.body.len());
            for body_line in &def.body {
                body_lines.push(rewrite_subckt_body_line(
                    body_line,
                    &nested_prefix,
                    &instance.node_map,
                    &instance.param_map,
                ));
            }
            expanded.push_str(&expand_subckt_lines(
                &body_lines,
                defs,
                scope_symbols,
                scope_parent,
                &instance.subckt_key,
                Some(&nested_prefix),
            )?);
            continue;
        }

        let line_to_emit = if let Some(prefix) = instance_prefix {
            prefix_instance_name(raw_line, prefix)
        } else {
            raw_line.clone()
        };
        expanded.push_str(&line_to_emit);
        expanded.push('\n');
    }

    Ok(expanded)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSubcktInstance {
    instance_name: String,
    subckt_name: String,
    subckt_key: String,
    node_map: BTreeMap<String, String>,
    param_map: BTreeMap<String, String>,
}

fn starts_with_instance(line: &str) -> bool {
    line.as_bytes()
        .first()
        .map(|byte| matches!(*byte, b'X' | b'x'))
        .unwrap_or(false)
}

fn parse_subckt_instance(
    line: &str,
    defs: &BTreeMap<String, SubcktDef>,
    scope_symbols: &BTreeMap<String, BTreeMap<String, String>>,
    scope_parent: &BTreeMap<String, String>,
    declaration_scope: &str,
) -> Result<ParsedSubcktInstance, SimulationError> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 3 {
        return Err(SimulationError::InvalidSubcktInstance(line.to_string()));
    }

    let params_marker_index = tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case("params:"));
    let first_assignment_index = tokens
        .iter()
        .position(|token| token.contains('='))
        .unwrap_or(tokens.len());
    let split_index = params_marker_index.unwrap_or(first_assignment_index);
    let Some(subckt_index) = tokens[1..split_index]
        .iter()
        .rposition(|token| {
            resolve_subckt_name(token, declaration_scope, scope_symbols, scope_parent).is_some()
        })
        .map(|index| index + 1)
    else {
        let subckt_index = split_index.saturating_sub(1);
        if subckt_index < 2 {
            return Err(SimulationError::InvalidSubcktInstance(line.to_string()));
        }
        return Err(SimulationError::UnknownSubckt(
            tokens[subckt_index].to_string(),
        ));
    };
    if subckt_index < 2 {
        return Err(SimulationError::InvalidSubcktInstance(line.to_string()));
    }
    if subckt_index != split_index.saturating_sub(1) {
        return Err(SimulationError::UnsupportedSubcktInstanceSyntax(
            line.to_string(),
        ));
    }

    let instance_name = tokens[0].to_string();
    let subckt_name = tokens[subckt_index].to_string();
    let subckt_key =
        resolve_subckt_name(&subckt_name, declaration_scope, scope_symbols, scope_parent)
            .ok_or_else(|| SimulationError::UnknownSubckt(subckt_name.clone()))?;
    let def = defs
        .get(&subckt_key)
        .ok_or_else(|| SimulationError::UnknownSubckt(subckt_name.clone()))?;
    let nodes = &tokens[1..subckt_index];
    if nodes.len() != def.pins.len() {
        return Err(SimulationError::InvalidSubcktInstance(line.to_string()));
    }

    let mut node_map = BTreeMap::new();
    for (pin, node) in def.pins.iter().zip(nodes.iter()) {
        node_map.insert(pin.clone(), (*node).to_string());
    }

    let mut param_map = def.default_params.clone();
    for token in &tokens[split_index..] {
        if token.eq_ignore_ascii_case("params:") {
            continue;
        }
        let (param, value) = token
            .split_once('=')
            .ok_or_else(|| SimulationError::UnsupportedSubcktInstanceSyntax(line.to_string()))?;
        param_map.insert(param.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    Ok(ParsedSubcktInstance {
        instance_name,
        subckt_name,
        subckt_key,
        node_map,
        param_map,
    })
}

fn rewrite_subckt_body_line(
    raw_line: &str,
    prefix: &str,
    node_map: &BTreeMap<String, String>,
    param_map: &BTreeMap<String, String>,
) -> String {
    let is_mutual_coupling = raw_line
        .split_whitespace()
        .next()
        .and_then(|token| token.chars().next())
        .map(|ch| matches!(ch, 'K' | 'k'))
        .unwrap_or(false);
    let mut rewritten_tokens = Vec::new();
    for (index, token) in raw_line.split_whitespace().enumerate() {
        let mut rewritten = rewrite_token(token, node_map, param_map);
        if is_mutual_coupling && matches!(index, 1 | 2) {
            rewritten = scoped_instance_name(&rewritten, prefix);
        }
        rewritten_tokens.push(rewritten);
    }
    rewritten_tokens.join(" ")
}

fn rewrite_token(
    token: &str,
    node_map: &BTreeMap<String, String>,
    param_map: &BTreeMap<String, String>,
) -> String {
    if let Some(node) = node_map.get(token) {
        return node.clone();
    }
    if let Some(param) = param_map.get(&token.to_ascii_lowercase()) {
        return param.clone();
    }
    if token.starts_with('{') && token.ends_with('}') {
        let inner = &token[1..token.len() - 1];
        if let Some(param) = param_map.get(&inner.trim().to_ascii_lowercase()) {
            return param.clone();
        }
    }
    if let Some((name, value)) = token.split_once('=') {
        let rewritten_value = rewrite_token(value, node_map, param_map);
        return format!("{name}={rewritten_value}");
    }
    token.to_string()
}

#[derive(Debug, Clone, PartialEq)]
struct InternalTransientResult {
    simulated_steps: usize,
    captured_steps: usize,
    max_abs_voltage_v: f64,
    delay_details: Vec<SimulationDelayDetail>,
    measurement_details: Vec<SimulationMeasurementDetail>,
    measurement_warnings: Vec<SimulationMeasurementWarning>,
    option_seed: Option<u64>,
    option_waveform_path: Option<String>,
    node_names: Vec<String>,
    final_node_voltages: Vec<f64>,
    captured_samples: Vec<InternalTransientSample>,
}

#[derive(Debug, Clone, PartialEq)]
struct InternalTransientSample {
    time_ps: f64,
    node_voltages: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalMeasurementKind {
    Max,
    Min,
    PeakToPeak,
    Average,
    Rms,
    Final,
    Find,
}

impl InternalMeasurementKind {
    fn as_str(self) -> &'static str {
        match self {
            InternalMeasurementKind::Max => "max",
            InternalMeasurementKind::Min => "min",
            InternalMeasurementKind::PeakToPeak => "peak_to_peak",
            InternalMeasurementKind::Average => "average",
            InternalMeasurementKind::Rms => "rms",
            InternalMeasurementKind::Final => "final",
            InternalMeasurementKind::Find => "find",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct InternalVoltageProbe {
    raw: String,
    pos_name: String,
    neg_name: Option<String>,
    pos: Option<usize>,
    neg: Option<usize>,
}

impl InternalVoltageProbe {
    fn node_label(&self) -> String {
        if let Some(neg_name) = &self.neg_name {
            format!("{},{}", self.pos_name, neg_name)
        } else {
            self.pos_name.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct InternalMeasurement {
    name: String,
    kind: InternalMeasurementKind,
    probe: InternalVoltageProbe,
    from_ps: Option<f64>,
    to_ps: Option<f64>,
    at_ps: Option<f64>,
    when: Option<InternalDelayEndpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalDelayCrossingDirection {
    Rise,
    Fall,
    Cross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalDelayCrossingOrdinal {
    Index(usize),
    Last,
}

#[derive(Debug, Clone, PartialEq)]
struct InternalDelayEndpoint {
    probe: InternalVoltageProbe,
    threshold_v: f64,
    direction: InternalDelayCrossingDirection,
    ordinal: InternalDelayCrossingOrdinal,
    td_ps: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
struct InternalDelayMeasurement {
    name: String,
    trigger: InternalDelayEndpoint,
    target: InternalDelayEndpoint,
}

#[derive(Debug, Clone, PartialEq)]
enum InternalMeasurementCard {
    Scalar(InternalMeasurement),
    Delay(InternalDelayMeasurement),
}

#[derive(Debug, Clone, PartialEq)]
struct InternalTransientNetlist {
    transient: TransientAnalysis,
    option_seed: Option<u64>,
    option_waveform_path: Option<String>,
    option_reltol: Option<f64>,
    option_abstol: Option<f64>,
    option_max_iterations: Option<usize>,
    option_noise_sigma: Option<f64>,
    option_temperature_k: Option<f64>,
    option_nominal_temperature_k: Option<f64>,
    node_names: Vec<String>,
    initial_node_voltages: Vec<f64>,
    startup_node_voltages: Vec<f64>,
    elements: Vec<InternalElement>,
    mutual_couplings: Vec<InternalMutualCoupling>,
    measurements: Vec<InternalMeasurement>,
    delay_measurements: Vec<InternalDelayMeasurement>,
    auxiliary_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct InternalMutualCoupling {
    branch_a: usize,
    branch_b: usize,
    mutual_h: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct InternalNonlinearSolveConfig {
    max_iterations: usize,
    residual_tolerance: f64,
    absolute_tolerance: f64,
    relative_tolerance: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct InternalOptionCard {
    seed: Option<u64>,
    waveform_path: Option<String>,
    reltol: Option<f64>,
    abstol: Option<f64>,
    max_iterations: Option<usize>,
    noise_sigma: Option<f64>,
    temperature_k: Option<f64>,
    nominal_temperature_k: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct InternalJunctionModelCard {
    critical_current_a: Option<f64>,
    second_harmonic_current_a: Option<f64>,
    third_harmonic_current_a: Option<f64>,
    fourth_harmonic_current_a: Option<f64>,
    fifth_harmonic_current_a: Option<f64>,
    sixth_harmonic_current_a: Option<f64>,
    normal_resistance_ohm: Option<f64>,
    junction_cap_f: Option<f64>,
    pi_junction: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
enum InternalElement {
    Resistor {
        pos: Option<usize>,
        neg: Option<usize>,
        resistance_ohm: f64,
    },
    Capacitor {
        pos: Option<usize>,
        neg: Option<usize>,
        capacitance_f: f64,
    },
    Inductor {
        pos: Option<usize>,
        neg: Option<usize>,
        inductance_h: f64,
        branch_index: usize,
    },
    CurrentSource {
        pos: Option<usize>,
        neg: Option<usize>,
        source: InternalSourceSpec,
    },
    VoltageSource {
        pos: Option<usize>,
        neg: Option<usize>,
        source: InternalSourceSpec,
        branch_index: usize,
    },
    TransmissionLineResistive {
        pos_a: Option<usize>,
        neg_a: Option<usize>,
        pos_b: Option<usize>,
        neg_b: Option<usize>,
        impedance_ohm: f64,
        delay_s: f64,
        attenuation: f64,
    },
    JosephsonJunction {
        pos: Option<usize>,
        neg: Option<usize>,
        critical_current_a: f64,
        second_harmonic_current_a: f64,
        third_harmonic_current_a: f64,
        fourth_harmonic_current_a: f64,
        fifth_harmonic_current_a: f64,
        sixth_harmonic_current_a: f64,
        normal_resistance_ohm: f64,
        junction_cap_f: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum InternalSourceSpec {
    Dc(f64),
    Pulse {
        low: f64,
        high: f64,
        delay_s: f64,
        rise_s: f64,
        fall_s: f64,
        width_s: f64,
        period_s: Option<f64>,
        cycle_count: Option<usize>,
    },
    Exp {
        initial: f64,
        pulsed: f64,
        rise_delay_s: f64,
        rise_tau_s: f64,
        fall_delay_s: f64,
        fall_tau_s: f64,
    },
    Pwl(Vec<(f64, f64)>),
    Sin {
        offset: f64,
        amplitude: f64,
        frequency_hz: f64,
        delay_s: f64,
        damping_hz: f64,
        phase_rad: f64,
    },
}

#[allow(dead_code)]
fn run_internal_transient(deck: &str) -> Result<InternalTransientResult, String> {
    run_internal_transient_with_base(deck, None)
}

fn run_internal_transient_with_base(
    deck: &str,
    include_base_dir: Option<&Path>,
) -> Result<InternalTransientResult, String> {
    let flattened = flatten_subckts(deck).map_err(|err| err.to_string())?;
    let parsed =
        parse_deck_expanded(&flattened, include_base_dir).map_err(|err| err.to_string())?;
    let netlist =
        parse_internal_transient_netlist_with_base(&flattened, &parsed, include_base_dir)?;

    if netlist.node_names.is_empty() {
        return Ok(InternalTransientResult {
            simulated_steps: 0,
            captured_steps: 1,
            max_abs_voltage_v: 0.0,
            delay_details: Vec::new(),
            measurement_details: Vec::new(),
            measurement_warnings: Vec::new(),
            option_seed: netlist.option_seed,
            option_waveform_path: netlist.option_waveform_path,
            node_names: Vec::new(),
            final_node_voltages: Vec::new(),
            captured_samples: Vec::new(),
        });
    }

    let node_count = netlist.node_names.len();
    let time_step_s = (netlist.transient.tstep_ps * 1.0e-12).max(f64::EPSILON);
    let total_steps = ((netlist.transient.tstop_ps.max(0.0)
        / netlist.transient.tstep_ps.max(f64::EPSILON))
    .floor() as usize)
        .max(1);
    let capture_start_ps = netlist.transient.prstart_ps.unwrap_or(0.0);
    let capture_step_ps = netlist
        .transient
        .prstep_ps
        .unwrap_or(netlist.transient.tstep_ps)
        .max(f64::EPSILON);

    let mut previous_solution = vec![0.0; node_count + netlist.auxiliary_count];
    previous_solution[..node_count].copy_from_slice(&netlist.startup_node_voltages);
    if netlist.transient.use_initial_conditions {
        previous_solution[..node_count].copy_from_slice(&netlist.initial_node_voltages);
    }
    let mut max_abs_voltage_v: f64 = 0.0;
    let mut captured_steps = usize::from(capture_start_ps <= 0.0);
    let mut captured_samples = Vec::new();
    let mut solution_history = std::collections::VecDeque::<(f64, Vec<f64>)>::new();

    if capture_start_ps <= 0.0 {
        captured_samples.push(InternalTransientSample {
            time_ps: 0.0,
            node_voltages: previous_solution[..node_count].to_vec(),
        });
    }
    solution_history.push_back((0.0, previous_solution.clone()));

    for step in 1..=total_steps {
        let current_time_ps = (step as f64) * netlist.transient.tstep_ps;
        let step_start_time_s = ((step - 1) as f64) * time_step_s;
        let segment_boundaries =
            collect_netlist_breakpoints_within_step(&netlist, step_start_time_s, time_step_s);
        for boundary_window in segment_boundaries.windows(2) {
            let segment_start_s = boundary_window[0];
            let segment_end_s = boundary_window[1];
            let segment_dt_s = (segment_end_s - segment_start_s).max(f64::EPSILON);
            previous_solution = advance_internal_transient_step(
                &netlist,
                &previous_solution,
                segment_dt_s,
                segment_start_s,
                &solution_history,
            )?;
            solution_history.push_back((segment_end_s, previous_solution.clone()));
            while solution_history.len() > 128 {
                solution_history.pop_front();
            }
        }
        for voltage in &previous_solution[..node_count] {
            max_abs_voltage_v = max_abs_voltage_v.max(voltage.abs());
        }
        if current_time_ps + 1.0e-9 >= capture_start_ps {
            let capture_index = ((current_time_ps - capture_start_ps) / capture_step_ps).round();
            if capture_index >= 0.0 {
                let expected_time_ps = capture_start_ps + capture_index * capture_step_ps;
                if (current_time_ps - expected_time_ps).abs()
                    <= netlist.transient.tstep_ps * 0.5 + 1.0e-9
                {
                    captured_steps += 1;
                    captured_samples.push(InternalTransientSample {
                        time_ps: current_time_ps,
                        node_voltages: previous_solution[..node_count].to_vec(),
                    });
                }
            }
        }
    }

    let (delay_details, mut measurement_warnings) =
        evaluate_internal_delay_measurements(&netlist, &captured_samples);
    let (measurement_details, scalar_measurement_warnings) =
        evaluate_internal_measurements(&netlist, &captured_samples);
    measurement_warnings.extend(scalar_measurement_warnings);

    Ok(InternalTransientResult {
        simulated_steps: total_steps,
        captured_steps: captured_steps.max(1),
        max_abs_voltage_v,
        delay_details,
        measurement_details,
        measurement_warnings,
        option_seed: netlist.option_seed,
        option_waveform_path: netlist.option_waveform_path,
        node_names: netlist.node_names,
        final_node_voltages: previous_solution[..node_count].to_vec(),
        captured_samples,
    })
}

#[allow(dead_code)]
fn parse_internal_transient_netlist(
    deck: &str,
    parsed: &ParsedDeck,
) -> Result<InternalTransientNetlist, String> {
    parse_internal_transient_netlist_with_base(deck, parsed, None)
}

fn parse_internal_transient_netlist_with_base(
    deck: &str,
    parsed: &ParsedDeck,
    include_base_dir: Option<&Path>,
) -> Result<InternalTransientNetlist, String> {
    let mut node_indices = BTreeMap::<String, usize>::new();
    let mut node_names = Vec::<String>::new();
    let mut elements = Vec::<InternalElement>::new();
    let mut inductor_branches = BTreeMap::<String, (usize, f64)>::new();
    let mut deferred_mutual_couplings = Vec::<(String, String, String)>::new();
    let mut junction_models = BTreeMap::<String, InternalJunctionModelCard>::new();
    let mut option_seed = None::<u64>;
    let mut option_waveform_path = None::<String>;
    let mut option_reltol = None::<f64>;
    let mut option_abstol = None::<f64>;
    let mut option_max_iterations = None::<usize>;
    let mut option_noise_sigma = None::<f64>;
    let mut option_temperature_k = None::<f64>;
    let mut option_nominal_temperature_k = None::<f64>;
    let mut deferred_measurements = Vec::<InternalMeasurement>::new();
    let mut deferred_delay_measurements = Vec::<InternalDelayMeasurement>::new();
    let mut auxiliary_count = 0usize;

    for raw_line in deck.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let option_rest = strip_control_card_prefix(line, ".options")
            .or_else(|| strip_control_card_prefix(line, ".option"));
        if let Some(rest) = option_rest {
            let option_card = parse_internal_option_card(rest.trim(), parsed)?;
            if let Some(seed) = option_card.seed {
                option_seed = Some(seed);
            }
            if let Some(path) = option_card.waveform_path {
                option_waveform_path = Some(path);
            }
            if let Some(reltol) = option_card.reltol {
                option_reltol = Some(reltol);
            }
            if let Some(abstol) = option_card.abstol {
                option_abstol = Some(abstol);
            }
            if let Some(max_iterations) = option_card.max_iterations {
                option_max_iterations = Some(max_iterations);
            }
            if let Some(noise_sigma) = option_card.noise_sigma {
                option_noise_sigma = Some(noise_sigma);
            }
            if let Some(temperature_k) = option_card.temperature_k {
                option_temperature_k = Some(temperature_k);
            }
            if let Some(nominal_temperature_k) = option_card.nominal_temperature_k {
                option_nominal_temperature_k = Some(nominal_temperature_k);
            }
            continue;
        }
        if let Some(rest) = strip_control_card_prefix(line, ".model") {
            if let Some((name, model_card)) =
                parse_internal_junction_model_card(rest, &parsed.params)?
            {
                junction_models.insert(name, model_card);
            }
            continue;
        }
        if let Some(rest) = strip_control_card_prefix(line, ".measure")
            .or_else(|| strip_control_card_prefix(line, ".meas"))
        {
            match parse_internal_measurement_card(rest, &parsed.params)? {
                Some(InternalMeasurementCard::Scalar(measurement)) => {
                    deferred_measurements.push(measurement);
                }
                Some(InternalMeasurementCard::Delay(measurement)) => {
                    deferred_delay_measurements.push(measurement);
                }
                None => {}
            }
            continue;
        }
        if line.starts_with('.') {
            continue;
        }
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            continue;
        }

        let prefix = tokens[0]
            .chars()
            .next()
            .ok_or_else(|| "internal_transient_invalid_element".to_string())?
            .to_ascii_uppercase();
        match prefix {
            'R' => {
                let resistance_ohm = parse_two_terminal_passive_value(
                    &tokens,
                    &parsed.params,
                    &["r", "res", "resistance"],
                    "resistor",
                )?
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if resistance_ohm <= 0.0 {
                    return Err(format!("internal_transient_invalid_resistance:{line}"));
                }
                elements.push(InternalElement::Resistor {
                    pos: intern_node(tokens[1], &mut node_indices, &mut node_names),
                    neg: intern_node(tokens[2], &mut node_indices, &mut node_names),
                    resistance_ohm,
                });
            }
            'C' => {
                let capacitance_f = parse_two_terminal_passive_value(
                    &tokens,
                    &parsed.params,
                    &["c", "cap", "capacitance"],
                    "capacitor",
                )?
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if capacitance_f < 0.0 {
                    return Err(format!("internal_transient_invalid_capacitance:{line}"));
                }
                elements.push(InternalElement::Capacitor {
                    pos: intern_node(tokens[1], &mut node_indices, &mut node_names),
                    neg: intern_node(tokens[2], &mut node_indices, &mut node_names),
                    capacitance_f,
                });
            }
            'L' => {
                let inductor_name = tokens[0].to_ascii_lowercase();
                let inductance_h = parse_two_terminal_passive_value(
                    &tokens,
                    &parsed.params,
                    &["l", "ind", "inductance"],
                    "inductor",
                )?
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if inductance_h <= 0.0 {
                    return Err(format!("internal_transient_invalid_inductance:{line}"));
                }
                elements.push(InternalElement::Inductor {
                    pos: intern_node(tokens[1], &mut node_indices, &mut node_names),
                    neg: intern_node(tokens[2], &mut node_indices, &mut node_names),
                    inductance_h,
                    branch_index: auxiliary_count,
                });
                inductor_branches.insert(inductor_name, (auxiliary_count, inductance_h));
                auxiliary_count += 1;
            }
            'K' => {
                if tokens.len() < 4 {
                    return Err(format!(
                        "internal_transient_unsupported_mutual_syntax:{line}"
                    ));
                }
                deferred_mutual_couplings.push((
                    tokens[1].to_ascii_lowercase(),
                    tokens[2].to_ascii_lowercase(),
                    parse_mutual_coupling_expression(tokens[3..].to_vec())?,
                ));
            }
            'T' => {
                let (impedance_ohm, delay_s, attenuation) =
                    parse_transmission_line_parameters(&tokens, &parsed.params)?;
                elements.push(InternalElement::TransmissionLineResistive {
                    pos_a: intern_node(tokens[1], &mut node_indices, &mut node_names),
                    neg_a: intern_node(tokens[2], &mut node_indices, &mut node_names),
                    pos_b: intern_node(tokens[3], &mut node_indices, &mut node_names),
                    neg_b: intern_node(tokens[4], &mut node_indices, &mut node_names),
                    impedance_ohm,
                    delay_s,
                    attenuation,
                });
            }
            'J' => {
                if tokens.len() < 4 {
                    return Err(format!("internal_transient_unsupported_element:{prefix}"));
                }
                let junction_tokens = normalized_junction_assignment_tokens(&tokens[3..]);
                let junction_model =
                    resolve_internal_junction_model_defaults(&tokens[3..], &junction_models);
                let has_assignment = junction_tokens.iter().any(|token| token.contains('='));
                if junction_model.is_none() && !has_assignment {
                    return Err(format!("internal_transient_unsupported_element:{prefix}"));
                }
                let (
                    critical_current_a,
                    second_harmonic_current_a,
                    third_harmonic_current_a,
                    fourth_harmonic_current_a,
                    fifth_harmonic_current_a,
                    sixth_harmonic_current_a,
                    normal_resistance_ohm,
                    junction_cap_f,
                ) = parse_internal_junction_parameters(
                    &tokens[3..],
                    &parsed.params,
                    junction_model,
                )?;
                elements.push(InternalElement::JosephsonJunction {
                    pos: intern_node(tokens[1], &mut node_indices, &mut node_names),
                    neg: intern_node(tokens[2], &mut node_indices, &mut node_names),
                    critical_current_a,
                    second_harmonic_current_a,
                    third_harmonic_current_a,
                    fourth_harmonic_current_a,
                    fifth_harmonic_current_a,
                    sixth_harmonic_current_a,
                    normal_resistance_ohm,
                    junction_cap_f,
                });
            }
            'I' => {
                let source = parse_internal_source_spec(
                    &tokens,
                    &parsed.params,
                    "current",
                    include_base_dir,
                )?;
                elements.push(InternalElement::CurrentSource {
                    pos: intern_node(tokens[1], &mut node_indices, &mut node_names),
                    neg: intern_node(tokens[2], &mut node_indices, &mut node_names),
                    source,
                });
            }
            'V' => {
                let source = parse_internal_source_spec(
                    &tokens,
                    &parsed.params,
                    "voltage",
                    include_base_dir,
                )?;
                elements.push(InternalElement::VoltageSource {
                    pos: intern_node(tokens[1], &mut node_indices, &mut node_names),
                    neg: intern_node(tokens[2], &mut node_indices, &mut node_names),
                    source,
                    branch_index: auxiliary_count,
                });
                auxiliary_count += 1;
            }
            _ => return Err(format!("internal_transient_unsupported_element:{prefix}")),
        }
    }

    let mut mutual_couplings = Vec::with_capacity(deferred_mutual_couplings.len());
    for (inductor_a_name, inductor_b_name, coupling_expr) in deferred_mutual_couplings {
        let coupling = evaluate_expression(&coupling_expr, &parsed.params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        if !coupling.is_finite() || coupling.abs() > 1.0 {
            return Err(format!(
                "internal_transient_invalid_mutual_coupling:{coupling_expr}"
            ));
        }
        let Some((branch_a, inductance_a_h)) = inductor_branches.get(&inductor_a_name).copied()
        else {
            return Err(format!(
                "internal_transient_unknown_mutual_inductor:{inductor_a_name}"
            ));
        };
        let Some((branch_b, inductance_b_h)) = inductor_branches.get(&inductor_b_name).copied()
        else {
            return Err(format!(
                "internal_transient_unknown_mutual_inductor:{inductor_b_name}"
            ));
        };
        if branch_a == branch_b {
            return Err(format!(
                "internal_transient_invalid_mutual_inductor_pair:{inductor_a_name}"
            ));
        }
        let mutual_h = coupling * (inductance_a_h * inductance_b_h).sqrt();
        mutual_couplings.push(InternalMutualCoupling {
            branch_a,
            branch_b,
            mutual_h,
        });
    }

    let mut initial_node_voltages = vec![0.0; node_names.len()];
    let mut startup_node_voltages = vec![0.0; node_names.len()];
    let mut measurements = Vec::with_capacity(deferred_measurements.len());
    for mut measurement in deferred_measurements {
        resolve_internal_voltage_probe(&mut measurement.probe, &node_indices);
        if let Some(when) = &mut measurement.when {
            resolve_internal_voltage_probe(&mut when.probe, &node_indices);
        }
        measurements.push(measurement);
    }
    let mut delay_measurements = Vec::with_capacity(deferred_delay_measurements.len());
    for mut measurement in deferred_delay_measurements {
        resolve_internal_voltage_probe(&mut measurement.trigger.probe, &node_indices);
        resolve_internal_voltage_probe(&mut measurement.target.probe, &node_indices);
        delay_measurements.push(measurement);
    }
    for raw_line in deck.lines() {
        let line = strip_comment(raw_line).trim();
        let Some(rest) = strip_control_card_prefix(line, ".ic") else {
            if let Some(rest) = strip_control_card_prefix(line, ".nodeset") {
                parse_node_voltage_assignments(
                    rest.trim(),
                    parsed,
                    &node_indices,
                    &mut startup_node_voltages,
                    "nodeset",
                )?;
            }
            continue;
        };
        parse_node_voltage_assignments(
            rest.trim(),
            parsed,
            &node_indices,
            &mut initial_node_voltages,
            "ic",
        )?;
    }

    Ok(InternalTransientNetlist {
        transient: parsed.transient.clone(),
        option_seed,
        option_waveform_path,
        option_reltol,
        option_abstol,
        option_max_iterations,
        option_noise_sigma,
        option_temperature_k,
        option_nominal_temperature_k,
        node_names,
        initial_node_voltages,
        startup_node_voltages,
        elements,
        mutual_couplings,
        measurements,
        delay_measurements,
        auxiliary_count,
    })
}

fn parse_internal_option_card(
    line: &str,
    parsed: &ParsedDeck,
) -> Result<InternalOptionCard, String> {
    if line.is_empty() {
        return Ok(InternalOptionCard {
            seed: None,
            waveform_path: None,
            reltol: None,
            abstol: None,
            max_iterations: None,
            noise_sigma: None,
            temperature_k: None,
            nominal_temperature_k: None,
        });
    }

    let normalized = line.replace(',', " ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let collapsed_tokens = collapse_spaced_assignments(&tokens);
    let mut assignments = Vec::<String>::new();
    let mut index = 0usize;
    while index < collapsed_tokens.len() {
        let token = collapsed_tokens[index].as_str();
        if token.contains('=') {
            assignments.push(token.to_string());
            index += 1;
            continue;
        }
        if index + 1 < collapsed_tokens.len() {
            let value_token = collapsed_tokens[index + 1].as_str();
            if !value_token.contains('=') {
                assignments.push(format!("{}={}", token, value_token));
                index += 2;
                continue;
            }
        }
        assignments.push(token.to_string());
        index += 1;
    }
    let mut seed = None::<u64>;
    let mut waveform_path = None::<String>;
    let mut reltol = None::<f64>;
    let mut abstol = None::<f64>;
    let mut max_iterations = None::<usize>;
    let mut noise_sigma = None::<f64>;
    let mut temperature_k = None::<f64>;
    let mut nominal_temperature_k = None::<f64>;
    for assignment in assignments {
        let Some((name, value_expr)) = assignment.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("seed") {
            let value = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_seed".to_string())?;
            if !value.is_finite() || value < 0.0 {
                return Err("internal_transient_invalid_option_seed".to_string());
            }
            let rounded = value.round();
            if (value - rounded).abs() > 1.0e-9 || rounded > (u64::MAX as f64) {
                return Err("internal_transient_invalid_option_seed".to_string());
            }
            seed = Some(rounded as u64);
            continue;
        }

        if name.eq_ignore_ascii_case("csvout")
            || name.eq_ignore_ascii_case("waveform")
            || name.eq_ignore_ascii_case("waveform_path")
            || name.eq_ignore_ascii_case("raw_file")
        {
            let value = strip_wrapping_quotes(value_expr.trim());
            if value.is_empty() {
                return Err("internal_transient_invalid_option_waveform_path".to_string());
            }
            waveform_path = Some(value);
            continue;
        }

        if name.eq_ignore_ascii_case("reltol")
            || name.eq_ignore_ascii_case("rel")
            || name.eq_ignore_ascii_case("relerr")
        {
            let value = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_reltol".to_string())?;
            if !value.is_finite() || value <= 0.0 {
                return Err("internal_transient_invalid_option_reltol".to_string());
            }
            reltol = Some(value);
            continue;
        }

        if name.eq_ignore_ascii_case("abstol")
            || name.eq_ignore_ascii_case("abs")
            || name.eq_ignore_ascii_case("abserr")
        {
            let value = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_abstol".to_string())?;
            if !value.is_finite() || value <= 0.0 {
                return Err("internal_transient_invalid_option_abstol".to_string());
            }
            abstol = Some(value);
            continue;
        }

        if name.eq_ignore_ascii_case("itl4")
            || name.eq_ignore_ascii_case("itl")
            || name.eq_ignore_ascii_case("itl1")
            || name.eq_ignore_ascii_case("maxiter")
            || name.eq_ignore_ascii_case("maxiters")
        {
            let value = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_max_iterations".to_string())?;
            if !value.is_finite() || value <= 0.0 {
                return Err("internal_transient_invalid_option_max_iterations".to_string());
            }
            let rounded = value.round();
            if (value - rounded).abs() > 1.0e-9 || rounded > (usize::MAX as f64) {
                return Err("internal_transient_invalid_option_max_iterations".to_string());
            }
            max_iterations = Some(rounded as usize);
            continue;
        }

        if name.eq_ignore_ascii_case("tnoise")
            || name.eq_ignore_ascii_case("noise")
            || name.eq_ignore_ascii_case("noisesigma")
            || name.eq_ignore_ascii_case("noise_sigma")
            || name.eq_ignore_ascii_case("sigma")
        {
            let value = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_noise".to_string())?;
            if !value.is_finite() || value < 0.0 {
                return Err("internal_transient_invalid_option_noise".to_string());
            }
            noise_sigma = Some(value);
            continue;
        }

        if name.eq_ignore_ascii_case("temp")
            || name.eq_ignore_ascii_case("temperature")
            || name.eq_ignore_ascii_case("temperature_c")
            || name.eq_ignore_ascii_case("tempc")
        {
            let value_c = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_temp".to_string())?;
            let value_k = value_c + 273.15;
            if !value_k.is_finite() || value_k <= 0.0 {
                return Err("internal_transient_invalid_option_temp".to_string());
            }
            temperature_k = Some(value_k);
            continue;
        }

        if name.eq_ignore_ascii_case("temp_k") || name.eq_ignore_ascii_case("temperature_k") {
            let value_k = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_temp".to_string())?;
            if !value_k.is_finite() || value_k <= 0.0 {
                return Err("internal_transient_invalid_option_temp".to_string());
            }
            temperature_k = Some(value_k);
            continue;
        }

        if name.eq_ignore_ascii_case("tnom")
            || name.eq_ignore_ascii_case("nomtemp")
            || name.eq_ignore_ascii_case("nominal_temperature")
            || name.eq_ignore_ascii_case("nominal_temperature_c")
            || name.eq_ignore_ascii_case("tnomc")
        {
            let value_c = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_tnom".to_string())?;
            let value_k = value_c + 273.15;
            if !value_k.is_finite() || value_k <= 0.0 {
                return Err("internal_transient_invalid_option_tnom".to_string());
            }
            nominal_temperature_k = Some(value_k);
            continue;
        }

        if name.eq_ignore_ascii_case("tnom_k")
            || name.eq_ignore_ascii_case("nomtemp_k")
            || name.eq_ignore_ascii_case("nominal_temperature_k")
        {
            let value_k = evaluate_expression(value_expr.trim(), &parsed.params)
                .map_err(|_| "internal_transient_invalid_option_tnom".to_string())?;
            if !value_k.is_finite() || value_k <= 0.0 {
                return Err("internal_transient_invalid_option_tnom".to_string());
            }
            nominal_temperature_k = Some(value_k);
            continue;
        }
    }

    Ok(InternalOptionCard {
        seed,
        waveform_path,
        reltol,
        abstol,
        max_iterations,
        noise_sigma,
        temperature_k,
        nominal_temperature_k,
    })
}

fn strip_control_card_prefix<'a>(line: &'a str, card: &str) -> Option<&'a str> {
    if line.len() < card.len() {
        return None;
    }
    if !line[..card.len()].eq_ignore_ascii_case(card) {
        return None;
    }
    let rest = &line[card.len()..];
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(rest.trim_start())
    } else {
        None
    }
}

fn parse_node_voltage_assignments(
    line: &str,
    parsed: &ParsedDeck,
    node_indices: &BTreeMap<String, usize>,
    node_voltages: &mut [f64],
    card_name: &str,
) -> Result<(), String> {
    if line.is_empty() {
        return Err(format!("internal_transient_invalid_{card_name}"));
    }
    let normalized = line.replace(',', " ");
    let raw_tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let assignments = collapse_spaced_assignments(&raw_tokens);
    for assignment in assignments {
        let Some((target, value_expr)) = assignment.split_once('=') else {
            return Err(format!(
                "internal_transient_invalid_{card_name}:{}",
                assignment
            ));
        };
        let Some(node_name) = target
            .strip_prefix("v(")
            .or_else(|| target.strip_prefix("V("))
            .and_then(|inner| inner.strip_suffix(')'))
        else {
            return Err(format!(
                "internal_transient_invalid_{card_name}:{}",
                assignment
            ));
        };
        if is_ground_node(node_name) {
            continue;
        }
        let key = node_name.trim().to_ascii_lowercase();
        let Some(index) = node_indices.get(&key) else {
            return Err(format!(
                "internal_transient_unknown_{card_name}_node:{node_name}"
            ));
        };
        let value = evaluate_expression(value_expr.trim(), &parsed.params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        node_voltages[*index] = value;
    }
    Ok(())
}

fn parse_mutual_coupling_expression(tokens: Vec<&str>) -> Result<String, String> {
    if tokens.is_empty() {
        return Err("internal_transient_invalid_mutual_coupling".to_string());
    }

    let collapsed = collapse_spaced_assignments(&tokens);
    if collapsed.len() == 2 {
        let name = collapsed[0].trim();
        let value = collapsed[1].trim();
        if (name.eq_ignore_ascii_case("k") || name.eq_ignore_ascii_case("coupling"))
            && !value.is_empty()
        {
            return Ok(value.to_string());
        }
    }

    let normalized = collapsed.join("").replace(char::is_whitespace, "");
    if normalized.is_empty() {
        return Err("internal_transient_invalid_mutual_coupling".to_string());
    }
    if let Some((name, value)) = normalized.split_once('=') {
        if !name.eq_ignore_ascii_case("k") && !name.eq_ignore_ascii_case("coupling") {
            return Err("internal_transient_invalid_mutual_coupling".to_string());
        }
        if value.is_empty() {
            return Err("internal_transient_invalid_mutual_coupling".to_string());
        }
        return Ok(value.to_string());
    }
    Ok(normalized)
}

fn parse_two_terminal_passive_value(
    tokens: &[&str],
    params: &BTreeMap<String, f64>,
    accepted_names: &[&str],
    kind: &str,
) -> Result<Result<f64, SimulationError>, String> {
    if tokens.len() < 4 {
        return Err(format!(
            "internal_transient_unsupported_{kind}_syntax:{}",
            tokens.join(" ")
        ));
    }

    let raw_value_tokens = tokens[3..].to_vec();
    let mut value_expr = None::<String>;
    let mut index = 0usize;
    while index < raw_value_tokens.len() {
        let token = raw_value_tokens[index];
        if let Some((name, expr)) = token.split_once('=') {
            if !accepted_names
                .iter()
                .any(|accepted| name.eq_ignore_ascii_case(accepted))
            {
                return Err(format!(
                    "internal_transient_unsupported_{kind}_syntax:{}",
                    tokens.join(" ")
                ));
            }
            if value_expr.replace(expr.to_string()).is_some() {
                return Err(format!(
                    "internal_transient_unsupported_{kind}_syntax:{}",
                    tokens.join(" ")
                ));
            }
            index += 1;
            continue;
        }

        if index + 2 < raw_value_tokens.len()
            && raw_value_tokens[index + 1] == "="
            && accepted_names
                .iter()
                .any(|accepted| token.eq_ignore_ascii_case(accepted))
        {
            if value_expr
                .replace(raw_value_tokens[index + 2].to_string())
                .is_some()
            {
                return Err(format!(
                    "internal_transient_unsupported_{kind}_syntax:{}",
                    tokens.join(" ")
                ));
            }
            index += 3;
            continue;
        }

        if index + 1 < raw_value_tokens.len()
            && accepted_names
                .iter()
                .any(|accepted| token.eq_ignore_ascii_case(accepted))
        {
            if value_expr
                .replace(raw_value_tokens[index + 1].to_string())
                .is_some()
            {
                return Err(format!(
                    "internal_transient_unsupported_{kind}_syntax:{}",
                    tokens.join(" ")
                ));
            }
            index += 2;
            continue;
        }

        if value_expr.replace(token.to_string()).is_some() {
            return Err(format!(
                "internal_transient_unsupported_{kind}_syntax:{}",
                tokens.join(" ")
            ));
        }
        index += 1;
    }

    let Some(value_expr) = value_expr else {
        return Err(format!(
            "internal_transient_unsupported_{kind}_syntax:{}",
            tokens.join(" ")
        ));
    };

    Ok(evaluate_expression(&value_expr, params))
}

fn parse_transmission_line_parameters(
    tokens: &[&str],
    params: &BTreeMap<String, f64>,
) -> Result<(f64, f64, f64), String> {
    if tokens.len() < 6 {
        return Err(format!(
            "internal_transient_unsupported_transmission_syntax:{}",
            tokens.join(" ")
        ));
    }
    let mut z0_expr = None::<String>;
    let mut td_expr = None::<String>;
    let mut loss_expr = None::<String>;
    let mut alpha_expr = None::<String>;
    let normalized = tokens[5..].join(" ").replace(',', " ");
    let raw_tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < raw_tokens.len() {
        let token = raw_tokens[index];
        if let Some((name, value)) = token.split_once('=') {
            if name.eq_ignore_ascii_case("z0") || name.eq_ignore_ascii_case("zo") {
                z0_expr = Some(value.to_string());
                index += 1;
                continue;
            }
            if name.eq_ignore_ascii_case("td")
                || name.eq_ignore_ascii_case("delay")
                || name.eq_ignore_ascii_case("tau")
            {
                td_expr = Some(value.to_string());
                index += 1;
                continue;
            }
            if name.eq_ignore_ascii_case("loss") || name.eq_ignore_ascii_case("atten") {
                loss_expr = Some(value.to_string());
                index += 1;
                continue;
            }
            if name.eq_ignore_ascii_case("alpha") {
                alpha_expr = Some(value.to_string());
                index += 1;
                continue;
            }
            return Err("internal_transient_invalid_transmission_parameter".to_string());
        }
        if index + 2 < raw_tokens.len() && raw_tokens[index + 1] == "=" {
            let name = token;
            let value = raw_tokens[index + 2];
            if name.eq_ignore_ascii_case("z0") || name.eq_ignore_ascii_case("zo") {
                z0_expr = Some(value.to_string());
                index += 3;
                continue;
            }
            if name.eq_ignore_ascii_case("td")
                || name.eq_ignore_ascii_case("delay")
                || name.eq_ignore_ascii_case("tau")
            {
                td_expr = Some(value.to_string());
                index += 3;
                continue;
            }
            if name.eq_ignore_ascii_case("loss") || name.eq_ignore_ascii_case("atten") {
                loss_expr = Some(value.to_string());
                index += 3;
                continue;
            }
            if name.eq_ignore_ascii_case("alpha") {
                alpha_expr = Some(value.to_string());
                index += 3;
                continue;
            }
            return Err("internal_transient_invalid_transmission_parameter".to_string());
        }
        if index + 1 < raw_tokens.len() {
            let name = token;
            let value = raw_tokens[index + 1];
            if name.eq_ignore_ascii_case("z0") || name.eq_ignore_ascii_case("zo") {
                z0_expr = Some(value.to_string());
                index += 2;
                continue;
            }
            if name.eq_ignore_ascii_case("td")
                || name.eq_ignore_ascii_case("delay")
                || name.eq_ignore_ascii_case("tau")
            {
                td_expr = Some(value.to_string());
                index += 2;
                continue;
            }
            if name.eq_ignore_ascii_case("loss") || name.eq_ignore_ascii_case("atten") {
                loss_expr = Some(value.to_string());
                index += 2;
                continue;
            }
            if name.eq_ignore_ascii_case("alpha") {
                alpha_expr = Some(value.to_string());
                index += 2;
                continue;
            }
        }
        if z0_expr.is_none() {
            z0_expr = Some(token.to_string());
            index += 1;
            continue;
        }
        if td_expr.is_none() {
            td_expr = Some(token.to_string());
            index += 1;
            continue;
        }
        if loss_expr.is_none() {
            loss_expr = Some(token.to_string());
            index += 1;
            continue;
        }
        return Err("internal_transient_unsupported_transmission_syntax".to_string());
    }

    let z0_expr =
        z0_expr.ok_or_else(|| "internal_transient_invalid_transmission_impedance".to_string())?;
    let impedance_ohm = evaluate_expression(&z0_expr, params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    if !impedance_ohm.is_finite() || impedance_ohm <= 0.0 {
        return Err("internal_transient_invalid_transmission_impedance".to_string());
    }

    let delay_s = if let Some(delay_expr) = td_expr {
        evaluate_expression(&delay_expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else {
        0.0
    };
    if !delay_s.is_finite() || delay_s < 0.0 {
        return Err("internal_transient_invalid_transmission_delay".to_string());
    }
    if loss_expr.is_some() && alpha_expr.is_some() {
        return Err("internal_transient_invalid_transmission_parameter".to_string());
    }

    let attenuation = if let Some(loss_expr) = loss_expr {
        let loss = evaluate_expression(&loss_expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        if !loss.is_finite() || !(0.0..=1.0).contains(&loss) {
            return Err("internal_transient_invalid_transmission_loss".to_string());
        }
        1.0 - loss
    } else if let Some(alpha_expr) = alpha_expr {
        let alpha = evaluate_expression(&alpha_expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        if !alpha.is_finite() || alpha < 0.0 {
            return Err("internal_transient_invalid_transmission_alpha".to_string());
        }
        (-alpha * delay_s).exp()
    } else {
        1.0
    };

    Ok((impedance_ohm, delay_s, attenuation))
}

fn parse_internal_junction_parameters(
    tokens: &[&str],
    params: &BTreeMap<String, f64>,
    model_defaults: Option<InternalJunctionModelCard>,
) -> Result<(f64, f64, f64, f64, f64, f64, f64, f64), String> {
    let mut critical_current_expr = None::<String>;
    let mut second_harmonic_current_expr = None::<String>;
    let mut third_harmonic_current_expr = None::<String>;
    let mut fourth_harmonic_current_expr = None::<String>;
    let mut fifth_harmonic_current_expr = None::<String>;
    let mut sixth_harmonic_current_expr = None::<String>;
    let mut normal_resistance_expr = None::<String>;
    let mut junction_cap_expr = None::<String>;
    let mut pi_expr = None::<String>;
    let mut cpr_coefficients = None::<Vec<String>>;

    for token in normalized_junction_assignment_tokens(tokens) {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("icrit") || name.eq_ignore_ascii_case("ic") {
            critical_current_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit2")
            || name.eq_ignore_ascii_case("ic2")
            || name.eq_ignore_ascii_case("cp2")
        {
            second_harmonic_current_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit3")
            || name.eq_ignore_ascii_case("ic3")
            || name.eq_ignore_ascii_case("cp3")
        {
            third_harmonic_current_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit4")
            || name.eq_ignore_ascii_case("ic4")
            || name.eq_ignore_ascii_case("cp4")
        {
            fourth_harmonic_current_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit5")
            || name.eq_ignore_ascii_case("ic5")
            || name.eq_ignore_ascii_case("cp5")
        {
            fifth_harmonic_current_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("icrit6")
            || name.eq_ignore_ascii_case("ic6")
            || name.eq_ignore_ascii_case("cp6")
        {
            sixth_harmonic_current_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("rn") {
            normal_resistance_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("cj") || name.eq_ignore_ascii_case("cap") {
            junction_cap_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("pi") {
            pi_expr = Some(value.to_string());
            continue;
        }
        if name.eq_ignore_ascii_case("cpr") {
            cpr_coefficients = parse_cpr_coefficients(value);
            continue;
        }
    }

    let evaluated_cpr_coefficients = if let Some(coefficients) = cpr_coefficients {
        let mut evaluated = Vec::<f64>::with_capacity(coefficients.len());
        for coefficient in coefficients {
            evaluated.push(
                evaluate_expression(&coefficient, params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
        }
        Some(evaluated)
    } else {
        None
    };

    let critical_current_basis_a = if let Some(expr) = critical_current_expr {
        evaluate_expression(&expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else {
        model_defaults
            .and_then(|defaults| defaults.critical_current_a)
            .or_else(|| {
                (second_harmonic_current_expr.is_some()
                    || third_harmonic_current_expr.is_some()
                    || fourth_harmonic_current_expr.is_some()
                    || fifth_harmonic_current_expr.is_some()
                    || sixth_harmonic_current_expr.is_some())
                .then_some(0.0)
            })
            .ok_or_else(|| "internal_transient_invalid_junction_icrit".to_string())?
    };
    let second_harmonic_current_a = if let Some(expr) = second_harmonic_current_expr {
        evaluate_expression(&expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else if let Some(coefficients) = &evaluated_cpr_coefficients {
        coefficients.get(1).copied().unwrap_or(0.0) * critical_current_basis_a
    } else {
        model_defaults
            .and_then(|defaults| defaults.second_harmonic_current_a)
            .unwrap_or(0.0)
    };
    if !second_harmonic_current_a.is_finite() {
        return Err("internal_transient_invalid_junction_icrit2".to_string());
    }

    let third_harmonic_current_a = if let Some(expr) = third_harmonic_current_expr {
        evaluate_expression(&expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else if let Some(coefficients) = &evaluated_cpr_coefficients {
        coefficients.get(2).copied().unwrap_or(0.0) * critical_current_basis_a
    } else {
        model_defaults
            .and_then(|defaults| defaults.third_harmonic_current_a)
            .unwrap_or(0.0)
    };
    if !third_harmonic_current_a.is_finite() {
        return Err("internal_transient_invalid_junction_icrit3".to_string());
    }

    let fourth_harmonic_current_a = if let Some(expr) = fourth_harmonic_current_expr {
        evaluate_expression(&expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else if let Some(coefficients) = &evaluated_cpr_coefficients {
        coefficients.get(3).copied().unwrap_or(0.0) * critical_current_basis_a
    } else {
        model_defaults
            .and_then(|defaults| defaults.fourth_harmonic_current_a)
            .unwrap_or(0.0)
    };
    if !fourth_harmonic_current_a.is_finite() {
        return Err("internal_transient_invalid_junction_icrit4".to_string());
    }

    let fifth_harmonic_current_a = if let Some(expr) = fifth_harmonic_current_expr {
        evaluate_expression(&expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else if let Some(coefficients) = &evaluated_cpr_coefficients {
        coefficients.get(4).copied().unwrap_or(0.0) * critical_current_basis_a
    } else {
        model_defaults
            .and_then(|defaults| defaults.fifth_harmonic_current_a)
            .unwrap_or(0.0)
    };
    if !fifth_harmonic_current_a.is_finite() {
        return Err("internal_transient_invalid_junction_icrit5".to_string());
    }

    let sixth_harmonic_current_a = if let Some(expr) = sixth_harmonic_current_expr {
        evaluate_expression(&expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else if let Some(coefficients) = &evaluated_cpr_coefficients {
        coefficients.get(5).copied().unwrap_or(0.0) * critical_current_basis_a
    } else {
        model_defaults
            .and_then(|defaults| defaults.sixth_harmonic_current_a)
            .unwrap_or(0.0)
    };
    if !sixth_harmonic_current_a.is_finite() {
        return Err("internal_transient_invalid_junction_icrit6".to_string());
    }

    let critical_current_raw_a = if let Some(coefficients) = &evaluated_cpr_coefficients {
        coefficients.first().copied().unwrap_or(1.0) * critical_current_basis_a
    } else {
        critical_current_basis_a
    };
    let normal_resistance_ohm = if let Some(expr) = normal_resistance_expr {
        evaluate_expression(&expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else {
        model_defaults
            .and_then(|defaults| defaults.normal_resistance_ohm)
            .ok_or_else(|| "internal_transient_invalid_junction_rn".to_string())?
    };
    let junction_cap_f = if let Some(cap_expr) = junction_cap_expr {
        evaluate_expression(&cap_expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else {
        model_defaults
            .and_then(|defaults| defaults.junction_cap_f)
            .unwrap_or(0.0)
    };

    if !critical_current_raw_a.is_finite() || critical_current_raw_a < 0.0 {
        return Err("internal_transient_invalid_junction_icrit".to_string());
    }
    if !normal_resistance_ohm.is_finite() || normal_resistance_ohm <= 0.0 {
        return Err("internal_transient_invalid_junction_rn".to_string());
    }
    if !junction_cap_f.is_finite() || junction_cap_f < 0.0 {
        return Err("internal_transient_invalid_junction_cj".to_string());
    }
    let pi_junction = if let Some(expr) = pi_expr {
        parse_internal_junction_pi_flag(&expr, params)?
    } else {
        model_defaults
            .and_then(|defaults| defaults.pi_junction)
            .unwrap_or(false)
    };
    let critical_current_a = if pi_junction {
        -critical_current_raw_a
    } else {
        critical_current_raw_a
    };
    let third_harmonic_current_a = if pi_junction {
        -third_harmonic_current_a
    } else {
        third_harmonic_current_a
    };
    let fifth_harmonic_current_a = if pi_junction {
        -fifth_harmonic_current_a
    } else {
        fifth_harmonic_current_a
    };

    Ok((
        critical_current_a,
        second_harmonic_current_a,
        third_harmonic_current_a,
        fourth_harmonic_current_a,
        fifth_harmonic_current_a,
        sixth_harmonic_current_a,
        normal_resistance_ohm,
        junction_cap_f,
    ))
}

fn parse_internal_junction_pi_flag(
    value_expr: &str,
    params: &BTreeMap<String, f64>,
) -> Result<bool, String> {
    let trimmed = value_expr.trim();
    if trimmed.eq_ignore_ascii_case("true")
        || trimmed.eq_ignore_ascii_case("yes")
        || trimmed.eq_ignore_ascii_case("on")
    {
        return Ok(true);
    }
    if trimmed.eq_ignore_ascii_case("false")
        || trimmed.eq_ignore_ascii_case("no")
        || trimmed.eq_ignore_ascii_case("off")
    {
        return Ok(false);
    }

    let value = if let Ok(literal) = trimmed.parse::<f64>() {
        literal
    } else {
        evaluate_expression(trimmed, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    };
    if !value.is_finite() {
        return Err("internal_transient_invalid_junction_pi".to_string());
    }
    let rounded = value.round();
    if (value - rounded).abs() > 1.0e-9 {
        return Err("internal_transient_invalid_junction_pi".to_string());
    }
    match rounded as i64 {
        0 => Ok(false),
        _ => Ok(true),
    }
}

fn parse_internal_junction_model_card(
    rest: &str,
    params: &BTreeMap<String, f64>,
) -> Result<Option<(String, InternalJunctionModelCard)>, String> {
    let raw_tokens = rest.split_whitespace().collect::<Vec<_>>();
    if raw_tokens.len() < 2 {
        return Ok(None);
    }

    let model_name = raw_tokens[0].to_ascii_lowercase();
    let model_body = raw_tokens[1..].join(" ");
    let lower_body = model_body.to_ascii_lowercase();

    let argument_text = if lower_body.starts_with("jj(") {
        let Some(close_index) = model_body.rfind(')') else {
            return Err("internal_transient_invalid_junction_model".to_string());
        };
        model_body[3..close_index].to_string()
    } else if lower_body == "jj" || lower_body.starts_with("jj ") {
        model_body[2..].trim().to_string()
    } else {
        return Ok(None);
    };

    if argument_text.is_empty() {
        return Ok(Some((
            model_name,
            InternalJunctionModelCard {
                critical_current_a: None,
                second_harmonic_current_a: None,
                third_harmonic_current_a: None,
                fourth_harmonic_current_a: None,
                fifth_harmonic_current_a: None,
                sixth_harmonic_current_a: None,
                normal_resistance_ohm: None,
                junction_cap_f: None,
                pi_junction: None,
            },
        )));
    }

    let raw_tokens = split_junction_model_argument_tokens(&argument_text);
    let collapsed =
        normalized_name_value_tokens(&raw_tokens.iter().map(String::as_str).collect::<Vec<_>>());

    let mut critical_current_basis_a = None::<f64>;
    let mut second_harmonic_current_a = None::<f64>;
    let mut third_harmonic_current_a = None::<f64>;
    let mut fourth_harmonic_current_a = None::<f64>;
    let mut fifth_harmonic_current_a = None::<f64>;
    let mut sixth_harmonic_current_a = None::<f64>;
    let mut normal_resistance_ohm = None::<f64>;
    let mut junction_cap_f = None::<f64>;
    let mut pi_junction = None::<bool>;
    let mut cpr_coefficients = None::<Vec<String>>;
    for token in collapsed {
        let Some((name, value_expr)) = token.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("icrit") || name.eq_ignore_ascii_case("ic") {
            critical_current_basis_a = Some(
                evaluate_expression(value_expr.trim(), params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
            continue;
        }
        if name.eq_ignore_ascii_case("icrit2")
            || name.eq_ignore_ascii_case("ic2")
            || name.eq_ignore_ascii_case("cp2")
        {
            second_harmonic_current_a = Some(
                evaluate_expression(value_expr.trim(), params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
            continue;
        }
        if name.eq_ignore_ascii_case("icrit3")
            || name.eq_ignore_ascii_case("ic3")
            || name.eq_ignore_ascii_case("cp3")
        {
            third_harmonic_current_a = Some(
                evaluate_expression(value_expr.trim(), params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
            continue;
        }
        if name.eq_ignore_ascii_case("icrit4")
            || name.eq_ignore_ascii_case("ic4")
            || name.eq_ignore_ascii_case("cp4")
        {
            fourth_harmonic_current_a = Some(
                evaluate_expression(value_expr.trim(), params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
            continue;
        }
        if name.eq_ignore_ascii_case("icrit5")
            || name.eq_ignore_ascii_case("ic5")
            || name.eq_ignore_ascii_case("cp5")
        {
            fifth_harmonic_current_a = Some(
                evaluate_expression(value_expr.trim(), params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
            continue;
        }
        if name.eq_ignore_ascii_case("rn") {
            normal_resistance_ohm = Some(
                evaluate_expression(value_expr.trim(), params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
            continue;
        }
        if name.eq_ignore_ascii_case("cj") || name.eq_ignore_ascii_case("cap") {
            junction_cap_f = Some(
                evaluate_expression(value_expr.trim(), params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?,
            );
            continue;
        }
        if name.eq_ignore_ascii_case("pi") {
            pi_junction = Some(parse_internal_junction_pi_flag(value_expr.trim(), params)?);
            continue;
        }
        if name.eq_ignore_ascii_case("cpr") {
            cpr_coefficients = parse_cpr_coefficients(value_expr.trim());
            continue;
        }
    }

    let critical_current_a = if let (Some(basis_current_a), Some(coefficients)) =
        (critical_current_basis_a, cpr_coefficients)
    {
        let first_coefficient = coefficients.first().map(String::as_str).unwrap_or("1");
        let first_value = evaluate_expression(first_coefficient, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let critical_current_a = if first_value.abs() < 1.0e-12 {
            None
        } else {
            Some(basis_current_a * first_value)
        };
        if second_harmonic_current_a.is_none() {
            if let Some(second_coefficient) = coefficients.get(1) {
                let second_value = evaluate_expression(second_coefficient, params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if second_value.abs() >= 1.0e-12 {
                    second_harmonic_current_a = Some(basis_current_a * second_value);
                }
            }
        }
        if third_harmonic_current_a.is_none() {
            if let Some(third_coefficient) = coefficients.get(2) {
                let third_value = evaluate_expression(third_coefficient, params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if third_value.abs() >= 1.0e-12 {
                    third_harmonic_current_a = Some(basis_current_a * third_value);
                }
            }
        }
        if fourth_harmonic_current_a.is_none() {
            if let Some(fourth_coefficient) = coefficients.get(3) {
                let fourth_value = evaluate_expression(fourth_coefficient, params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if fourth_value.abs() >= 1.0e-12 {
                    fourth_harmonic_current_a = Some(basis_current_a * fourth_value);
                }
            }
        }
        if fifth_harmonic_current_a.is_none() {
            if let Some(fifth_coefficient) = coefficients.get(4) {
                let fifth_value = evaluate_expression(fifth_coefficient, params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if fifth_value.abs() >= 1.0e-12 {
                    fifth_harmonic_current_a = Some(basis_current_a * fifth_value);
                }
            }
        }
        if sixth_harmonic_current_a.is_none() {
            if let Some(sixth_coefficient) = coefficients.get(5) {
                let sixth_value = evaluate_expression(sixth_coefficient, params)
                    .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
                if sixth_value.abs() >= 1.0e-12 {
                    sixth_harmonic_current_a = Some(basis_current_a * sixth_value);
                }
            }
        }
        critical_current_a
    } else {
        critical_current_basis_a
    };

    if critical_current_a.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return Err("internal_transient_invalid_junction_icrit".to_string());
    }
    if second_harmonic_current_a.is_some_and(|value| !value.is_finite()) {
        return Err("internal_transient_invalid_junction_icrit2".to_string());
    }
    if third_harmonic_current_a.is_some_and(|value| !value.is_finite()) {
        return Err("internal_transient_invalid_junction_icrit3".to_string());
    }
    if fourth_harmonic_current_a.is_some_and(|value| !value.is_finite()) {
        return Err("internal_transient_invalid_junction_icrit4".to_string());
    }
    if fifth_harmonic_current_a.is_some_and(|value| !value.is_finite()) {
        return Err("internal_transient_invalid_junction_icrit5".to_string());
    }
    if sixth_harmonic_current_a.is_some_and(|value| !value.is_finite()) {
        return Err("internal_transient_invalid_junction_icrit6".to_string());
    }
    if normal_resistance_ohm.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err("internal_transient_invalid_junction_rn".to_string());
    }
    if junction_cap_f.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return Err("internal_transient_invalid_junction_cj".to_string());
    }

    Ok(Some((
        model_name,
        InternalJunctionModelCard {
            critical_current_a,
            second_harmonic_current_a,
            third_harmonic_current_a,
            fourth_harmonic_current_a,
            fifth_harmonic_current_a,
            sixth_harmonic_current_a,
            normal_resistance_ohm,
            junction_cap_f,
            pi_junction,
        },
    )))
}

fn resolve_internal_junction_model_defaults(
    tokens: &[&str],
    junction_models: &BTreeMap<String, InternalJunctionModelCard>,
) -> Option<InternalJunctionModelCard> {
    let collapsed = normalized_junction_assignment_tokens(tokens);

    if let Some(first) = collapsed.first() {
        if !first.contains('=') {
            let model_name = first.trim().to_ascii_lowercase();
            if let Some(model) = junction_models.get(&model_name) {
                return Some(*model);
            }
        }
    }

    for token in collapsed {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("model") || name.eq_ignore_ascii_case("modelname") {
            let model_name = value.trim().to_ascii_lowercase();
            if let Some(model) = junction_models.get(&model_name) {
                return Some(*model);
            }
        }
    }

    None
}

fn normalized_name_value_tokens(tokens: &[&str]) -> Vec<String> {
    let collapsed = collapse_spaced_assignments(tokens);
    let mut normalized = Vec::with_capacity(collapsed.len());
    let mut index = 0usize;
    while index < collapsed.len() {
        let token = collapsed[index].as_str();
        if token.contains('=') {
            normalized.push(token.to_string());
            index += 1;
            continue;
        }
        if index + 1 < collapsed.len() && !collapsed[index + 1].contains('=') {
            normalized.push(format!("{}={}", token, collapsed[index + 1]));
            index += 2;
            continue;
        }
        normalized.push(token.to_string());
        index += 1;
    }
    normalized
}

fn normalized_junction_assignment_tokens(tokens: &[&str]) -> Vec<String> {
    let joined = tokens.join(" ");
    let raw_token_strings = split_junction_model_argument_tokens(&joined);
    let raw_tokens = raw_token_strings
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let collapsed = collapse_spaced_assignments(&raw_tokens);
    let mut normalized_tokens = Vec::with_capacity(collapsed.len());
    let mut index = 0usize;
    while index < collapsed.len() {
        let token = collapsed[index].as_str();
        if token.contains('=') {
            normalized_tokens.push(token.to_string());
            index += 1;
            continue;
        }
        if index == 0 && !is_junction_assignment_name(token) {
            normalized_tokens.push(token.to_string());
            index += 1;
            continue;
        }
        if index + 1 < collapsed.len() && !collapsed[index + 1].contains('=') {
            normalized_tokens.push(format!("{}={}", token, collapsed[index + 1]));
            index += 2;
            continue;
        }
        normalized_tokens.push(token.to_string());
        index += 1;
    }
    normalized_tokens
}

fn is_junction_assignment_name(token: &str) -> bool {
    token.eq_ignore_ascii_case("model")
        || token.eq_ignore_ascii_case("modelname")
        || token.eq_ignore_ascii_case("icrit")
        || token.eq_ignore_ascii_case("ic")
        || token.eq_ignore_ascii_case("icrit2")
        || token.eq_ignore_ascii_case("ic2")
        || token.eq_ignore_ascii_case("cp2")
        || token.eq_ignore_ascii_case("icrit3")
        || token.eq_ignore_ascii_case("ic3")
        || token.eq_ignore_ascii_case("cp3")
        || token.eq_ignore_ascii_case("icrit4")
        || token.eq_ignore_ascii_case("ic4")
        || token.eq_ignore_ascii_case("cp4")
        || token.eq_ignore_ascii_case("cpr")
        || token.eq_ignore_ascii_case("rn")
        || token.eq_ignore_ascii_case("cj")
        || token.eq_ignore_ascii_case("cap")
        || token.eq_ignore_ascii_case("pi")
}

fn collapse_spaced_assignments(tokens: &[&str]) -> Vec<String> {
    let mut collapsed = Vec::with_capacity(tokens.len());
    let mut index = 0usize;
    while index < tokens.len() {
        if index + 2 < tokens.len() && tokens[index + 1] == "=" {
            collapsed.push(format!("{}={}", tokens[index], tokens[index + 2]));
            index += 3;
            continue;
        }
        collapsed.push(tokens[index].to_string());
        index += 1;
    }
    collapsed
}

fn parse_internal_source_spec(
    tokens: &[&str],
    params: &BTreeMap<String, f64>,
    kind: &str,
    include_base_dir: Option<&Path>,
) -> Result<InternalSourceSpec, String> {
    if tokens.len() < 4 {
        return Err(format!("internal_transient_invalid_{kind}_source"));
    }
    let descriptor = tokens[3..].join(" ");
    if let Some(args) = parse_source_call_arguments(&descriptor, "pulse") {
        let values = split_source_arguments(args);
        let (low, high, delay_s, rise_s, fall_s, width_s, period_s, cycle_count) =
            parse_pulse_source_arguments(&values, params, kind, tokens)?;
        if cycle_count.is_some() && period_s.is_none() {
            return Err(format!("internal_transient_invalid_{kind}_source"));
        }
        return Ok(InternalSourceSpec::Pulse {
            low,
            high,
            delay_s,
            rise_s,
            fall_s,
            width_s,
            period_s,
            cycle_count,
        });
    }
    if let Some(args) = parse_source_call_arguments(&descriptor, "pwl") {
        let values = split_source_arguments(args);
        if let Some(path) = parse_waveform_file_source_argument(&values) {
            let resolved_path = resolve_waveform_source_path(include_base_dir, &path);
            let points = parse_pwl_points_from_file(&resolved_path, params)?;
            if points.len() < 2 {
                return Err(format!("internal_transient_invalid_{kind}_source"));
            }
            return Ok(InternalSourceSpec::Pwl(points));
        }
        if values.len() < 4 || values.len() % 2 != 0 {
            return Err(format!(
                "internal_transient_unsupported_{kind}_source:{}",
                tokens.join(" ")
            ));
        }
        let mut points = Vec::with_capacity(values.len() / 2);
        for (index, pair) in values.chunks(2).enumerate() {
            let time_s = evaluate_expression(pair[0], params)
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
            let value = evaluate_expression(pair[1], params)
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
            points.push((time_s, index, value));
        }
        points.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.1.cmp(&right.1))
        });
        return Ok(InternalSourceSpec::Pwl(
            points
                .into_iter()
                .map(|(time_s, _, value)| (time_s, value))
                .collect(),
        ));
    }
    if let Some(args) = parse_source_call_arguments(&descriptor, "exp") {
        let values = split_source_arguments(args);
        let (initial, pulsed, rise_delay_s, rise_tau_s, fall_delay_s, fall_tau_s) =
            parse_exp_source_arguments(&values, params, kind, tokens)?;
        if rise_tau_s <= 0.0 || fall_tau_s <= 0.0 {
            return Err(format!("internal_transient_invalid_{kind}_source"));
        }
        return Ok(InternalSourceSpec::Exp {
            initial,
            pulsed,
            rise_delay_s,
            rise_tau_s,
            fall_delay_s,
            fall_tau_s,
        });
    }
    if let Some(args) = parse_source_call_arguments(&descriptor, "sin") {
        let values = split_source_arguments(args);
        let (offset, amplitude, frequency_hz, delay_s, damping_hz, phase_rad) =
            parse_sin_source_arguments(&values, params, kind, tokens)?;
        if frequency_hz <= 0.0 || damping_hz < 0.0 {
            return Err(format!("internal_transient_invalid_{kind}_source"));
        }
        return Ok(InternalSourceSpec::Sin {
            offset,
            amplitude,
            frequency_hz,
            delay_s,
            damping_hz,
            phase_rad,
        });
    }

    if let Some(value_expr) = parse_internal_dc_source_value(tokens) {
        let value = evaluate_expression(&value_expr, params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        return Ok(InternalSourceSpec::Dc(value));
    }

    Err(format!(
        "internal_transient_unsupported_{kind}_source:{}",
        tokens.join(" ")
    ))
}

fn split_source_arguments(args: &str) -> Vec<&str> {
    args.split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect()
}

fn parse_waveform_file_source_argument(values: &[&str]) -> Option<String> {
    let collapsed = collapse_spaced_assignments(values);
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.len() == 1 {
        let (name, value) = collapsed[0].split_once('=')?;
        if name.eq_ignore_ascii_case("file") || name.eq_ignore_ascii_case("path") {
            return Some(strip_wrapping_quotes(value));
        }
        return None;
    }
    if collapsed.len() == 2
        && (collapsed[0].eq_ignore_ascii_case("file") || collapsed[0].eq_ignore_ascii_case("path"))
    {
        return Some(strip_wrapping_quotes(&collapsed[1]));
    }
    None
}

fn resolve_waveform_source_path(include_base_dir: Option<&Path>, raw_path: &str) -> String {
    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        return raw_path.to_string();
    }
    if let Some(base_dir) = include_base_dir {
        if candidate.starts_with(base_dir) {
            return raw_path.to_string();
        }
        return resolve_include_path(base_dir, raw_path)
            .display()
            .to_string();
    }
    raw_path.to_string()
}

fn strip_wrapping_quotes(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() >= 2 {
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

fn parse_pwl_points_from_file(
    path: &str,
    params: &BTreeMap<String, f64>,
) -> Result<Vec<(f64, f64)>, String> {
    let content = fs::read_to_string(path)
        .map_err(|_| format!("internal_transient_waveform_file_read_failed:{path}"))?;
    let mut points = Vec::new();
    for (line_index, raw_line) in content.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let fields = line
            .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if fields.len() < 2 {
            return Err(format!(
                "internal_transient_invalid_waveform_file_line:{}:{}",
                path,
                line_index + 1
            ));
        }
        let time_s = evaluate_expression(fields[0], params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let value = evaluate_expression(fields[1], params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        points.push((time_s, line_index, value));
    }
    points.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.1.cmp(&right.1))
    });
    Ok(points
        .into_iter()
        .map(|(time_s, _, value)| (time_s, value))
        .collect())
}

fn parse_source_call_arguments<'a>(descriptor: &'a str, function_name: &str) -> Option<&'a str> {
    let descriptor = descriptor.trim();
    let name = descriptor.get(..function_name.len())?;
    if !name.eq_ignore_ascii_case(function_name) {
        return None;
    }
    let remainder = descriptor[function_name.len()..].trim_start();
    remainder.strip_prefix('(')?.strip_suffix(')')
}

fn parse_internal_dc_source_value(tokens: &[&str]) -> Option<String> {
    let descriptor_tokens = tokens.get(3..)?;
    let collapsed = collapse_spaced_assignments(descriptor_tokens);
    let collapsed_refs = collapsed.iter().map(String::as_str).collect::<Vec<_>>();
    match collapsed_refs.as_slice() {
        [dc_value] if dc_value.len() > 3 && dc_value[..3].eq_ignore_ascii_case("dc=") => {
            Some(dc_value[3..].to_string())
        }
        [value] => Some((*value).to_string()),
        [dc, value] if dc.eq_ignore_ascii_case("dc") => Some((*value).to_string()),
        [value, ac, _] if ac.eq_ignore_ascii_case("ac") => Some((*value).to_string()),
        [dc, value, ac, _] if dc.eq_ignore_ascii_case("dc") && ac.eq_ignore_ascii_case("ac") => {
            Some((*value).to_string())
        }
        [dc_value, ac, _]
            if dc_value.len() > 3
                && dc_value[..3].eq_ignore_ascii_case("dc=")
                && ac.eq_ignore_ascii_case("ac") =>
        {
            Some(dc_value[3..].to_string())
        }
        [dc_value, ac_value]
            if dc_value.len() > 3
                && dc_value[..3].eq_ignore_ascii_case("dc=")
                && ac_value.len() > 3
                && ac_value[..3].eq_ignore_ascii_case("ac=") =>
        {
            Some(dc_value[3..].to_string())
        }
        _ => None,
    }
}

fn parse_pulse_source_arguments(
    values: &[&str],
    params: &BTreeMap<String, f64>,
    kind: &str,
    tokens: &[&str],
) -> Result<(f64, f64, f64, f64, f64, f64, Option<f64>, Option<usize>), String> {
    let collapsed = collapse_spaced_assignments(values);
    if collapsed.iter().all(|value| value.contains('=')) {
        let mut low_expr = None::<String>;
        let mut high_expr = None::<String>;
        let mut delay_expr = None::<String>;
        let mut rise_expr = None::<String>;
        let mut fall_expr = None::<String>;
        let mut width_expr = None::<String>;
        let mut period_expr = None::<String>;
        let mut cycle_count = None::<usize>;
        for value in collapsed {
            let Some((name, expr)) = value.split_once('=') else {
                return Err(format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                ));
            };
            if name.eq_ignore_ascii_case("v1") || name.eq_ignore_ascii_case("low") {
                low_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("v2") || name.eq_ignore_ascii_case("high") {
                high_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("td") || name.eq_ignore_ascii_case("delay") {
                delay_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("tr") || name.eq_ignore_ascii_case("rise") {
                rise_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("tf") || name.eq_ignore_ascii_case("fall") {
                fall_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("pw") || name.eq_ignore_ascii_case("width") {
                width_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("per") || name.eq_ignore_ascii_case("period") {
                period_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("ncycles") || name.eq_ignore_ascii_case("cycles") {
                cycle_count = Some(parse_pulse_cycle_count(&value, params)?);
                continue;
            }
            return Err(format!(
                "internal_transient_unsupported_{kind}_source:{}",
                tokens.join(" ")
            ));
        }

        let low = evaluate_expression(
            &low_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let high = evaluate_expression(
            &high_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let delay_s = evaluate_expression(
            &delay_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let rise_s = evaluate_expression(
            &rise_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let fall_s = evaluate_expression(
            &fall_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let width_s = evaluate_expression(
            &width_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let period_s = if let Some(period_expr) = period_expr {
            let period_s = evaluate_expression(&period_expr, params)
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
            if period_s <= 0.0 {
                return Err(format!("internal_transient_invalid_{kind}_source"));
            }
            Some(period_s)
        } else {
            None
        };
        return Ok((
            low,
            high,
            delay_s,
            rise_s,
            fall_s,
            width_s,
            period_s,
            cycle_count,
        ));
    }

    if collapsed.len() < 6 {
        return Err(format!(
            "internal_transient_unsupported_{kind}_source:{}",
            tokens.join(" ")
        ));
    }
    let low = evaluate_expression(collapsed[0].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let high = evaluate_expression(collapsed[1].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let delay_s = evaluate_expression(collapsed[2].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let rise_s = evaluate_expression(collapsed[3].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let fall_s = evaluate_expression(collapsed[4].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let width_s = evaluate_expression(collapsed[5].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let period_s = if collapsed.len() >= 7 {
        let period_s = evaluate_expression(collapsed[6].as_str(), params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        if period_s <= 0.0 {
            return Err(format!("internal_transient_invalid_{kind}_source"));
        }
        Some(period_s)
    } else {
        None
    };
    let cycle_count = if collapsed.len() >= 8 {
        Some(parse_pulse_cycle_count(&collapsed[7..].join(""), params)?)
    } else {
        None
    };
    Ok((
        low,
        high,
        delay_s,
        rise_s,
        fall_s,
        width_s,
        period_s,
        cycle_count,
    ))
}

fn parse_exp_source_arguments(
    values: &[&str],
    params: &BTreeMap<String, f64>,
    kind: &str,
    tokens: &[&str],
) -> Result<(f64, f64, f64, f64, f64, f64), String> {
    let collapsed = collapse_spaced_assignments(values);
    if collapsed.iter().any(|value| value.contains('=')) {
        let mut initial_expr = None::<String>;
        let mut pulsed_expr = None::<String>;
        let mut rise_delay_expr = None::<String>;
        let mut rise_tau_expr = None::<String>;
        let mut fall_delay_expr = None::<String>;
        let mut fall_tau_expr = None::<String>;
        for value in collapsed {
            let Some((name, expr)) = value.split_once('=') else {
                return Err(format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                ));
            };
            if name.eq_ignore_ascii_case("v1")
                || name.eq_ignore_ascii_case("initial")
                || name.eq_ignore_ascii_case("low")
            {
                initial_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("v2")
                || name.eq_ignore_ascii_case("pulsed")
                || name.eq_ignore_ascii_case("high")
            {
                pulsed_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("td1")
                || name.eq_ignore_ascii_case("rise_delay")
                || name.eq_ignore_ascii_case("delay1")
            {
                rise_delay_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("tau1")
                || name.eq_ignore_ascii_case("rise_tau")
                || name.eq_ignore_ascii_case("tau_rise")
            {
                rise_tau_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("td2")
                || name.eq_ignore_ascii_case("fall_delay")
                || name.eq_ignore_ascii_case("delay2")
            {
                fall_delay_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("tau2")
                || name.eq_ignore_ascii_case("fall_tau")
                || name.eq_ignore_ascii_case("tau_fall")
            {
                fall_tau_expr = Some(expr.to_string());
                continue;
            }
            return Err(format!(
                "internal_transient_unsupported_{kind}_source:{}",
                tokens.join(" ")
            ));
        }

        let initial = evaluate_expression(
            &initial_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let pulsed = evaluate_expression(
            &pulsed_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let rise_delay_s = evaluate_expression(
            &rise_delay_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let rise_tau_s = evaluate_expression(
            &rise_tau_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let fall_delay_s = evaluate_expression(
            &fall_delay_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let fall_tau_s = evaluate_expression(
            &fall_tau_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        return Ok((
            initial,
            pulsed,
            rise_delay_s,
            rise_tau_s,
            fall_delay_s,
            fall_tau_s,
        ));
    }

    if collapsed.len() != 6 {
        return Err(format!(
            "internal_transient_unsupported_{kind}_source:{}",
            tokens.join(" ")
        ));
    }
    let initial = evaluate_expression(collapsed[0].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let pulsed = evaluate_expression(collapsed[1].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let rise_delay_s = evaluate_expression(collapsed[2].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let rise_tau_s = evaluate_expression(collapsed[3].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let fall_delay_s = evaluate_expression(collapsed[4].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let fall_tau_s = evaluate_expression(collapsed[5].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    Ok((
        initial,
        pulsed,
        rise_delay_s,
        rise_tau_s,
        fall_delay_s,
        fall_tau_s,
    ))
}

fn parse_sin_source_arguments(
    values: &[&str],
    params: &BTreeMap<String, f64>,
    kind: &str,
    tokens: &[&str],
) -> Result<(f64, f64, f64, f64, f64, f64), String> {
    let collapsed = collapse_spaced_assignments(values);
    if collapsed.iter().any(|value| value.contains('=')) {
        let mut offset_expr = None::<String>;
        let mut amplitude_expr = None::<String>;
        let mut frequency_expr = None::<String>;
        let mut delay_expr = None::<String>;
        let mut damping_expr = None::<String>;
        let mut phase_expr = None::<String>;
        for value in collapsed {
            let Some((name, expr)) = value.split_once('=') else {
                return Err(format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                ));
            };
            if name.eq_ignore_ascii_case("vo") || name.eq_ignore_ascii_case("offset") {
                offset_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("va") || name.eq_ignore_ascii_case("amplitude") {
                amplitude_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("freq")
                || name.eq_ignore_ascii_case("frequency")
                || name.eq_ignore_ascii_case("f")
            {
                frequency_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("td") || name.eq_ignore_ascii_case("delay") {
                delay_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("theta")
                || name.eq_ignore_ascii_case("damp")
                || name.eq_ignore_ascii_case("damping")
            {
                damping_expr = Some(expr.to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("phase")
                || name.eq_ignore_ascii_case("phase_deg")
                || name.eq_ignore_ascii_case("phi")
            {
                phase_expr = Some(expr.to_string());
                continue;
            }
            return Err(format!(
                "internal_transient_unsupported_{kind}_source:{}",
                tokens.join(" ")
            ));
        }

        let offset = evaluate_expression(
            &offset_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let amplitude = evaluate_expression(
            &amplitude_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let frequency_hz = evaluate_expression(
            &frequency_expr.ok_or_else(|| {
                format!(
                    "internal_transient_unsupported_{kind}_source:{}",
                    tokens.join(" ")
                )
            })?,
            params,
        )
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
        let delay_s = if let Some(delay_expr) = delay_expr {
            evaluate_expression(&delay_expr, params)
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
        } else {
            0.0
        };
        let damping_hz = if let Some(damping_expr) = damping_expr {
            evaluate_expression(&damping_expr, params)
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
        } else {
            0.0
        };
        let phase_rad = if let Some(phase_expr) = phase_expr {
            evaluate_expression(&phase_expr, params)
                .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
                .to_radians()
        } else {
            0.0
        };
        return Ok((
            offset,
            amplitude,
            frequency_hz,
            delay_s,
            damping_hz,
            phase_rad,
        ));
    }

    if !(3..=6).contains(&collapsed.len()) {
        return Err(format!(
            "internal_transient_unsupported_{kind}_source:{}",
            tokens.join(" ")
        ));
    }
    let offset = evaluate_expression(collapsed[0].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let amplitude = evaluate_expression(collapsed[1].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let frequency_hz = evaluate_expression(collapsed[2].as_str(), params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let delay_s = if collapsed.len() >= 4 {
        evaluate_expression(collapsed[3].as_str(), params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else {
        0.0
    };
    let damping_hz = if collapsed.len() == 6 {
        evaluate_expression(collapsed[4].as_str(), params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
    } else {
        0.0
    };
    let phase_rad = if collapsed.len() == 5 {
        evaluate_expression(collapsed[4].as_str(), params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
            .to_radians()
    } else if collapsed.len() == 6 {
        evaluate_expression(collapsed[5].as_str(), params)
            .map_err(|err| format!("internal_transient_invalid_value:{err}"))?
            .to_radians()
    } else {
        0.0
    };
    Ok((
        offset,
        amplitude,
        frequency_hz,
        delay_s,
        damping_hz,
        phase_rad,
    ))
}

fn parse_pulse_cycle_count(token: &str, params: &BTreeMap<String, f64>) -> Result<usize, String> {
    let value_token = if let Some((name, value)) = token.split_once('=') {
        if !name.trim().eq_ignore_ascii_case("ncycles")
            && !name.trim().eq_ignore_ascii_case("cycles")
        {
            return Err("internal_transient_invalid_pulse_source".to_string());
        }
        value.trim()
    } else {
        token.trim()
    };
    let cycle_value = evaluate_expression(value_token, params)
        .map_err(|err| format!("internal_transient_invalid_value:{err}"))?;
    let cycle_count = cycle_value.round();
    if cycle_count <= 0.0 || (cycle_count - cycle_value).abs() > 1.0e-9 {
        return Err("internal_transient_invalid_pulse_source".to_string());
    }
    Ok(cycle_count as usize)
}

fn parse_internal_measurement_card(
    rest: &str,
    params: &BTreeMap<String, f64>,
) -> Result<Option<InternalMeasurementCard>, String> {
    let raw_tokens = rest.split_whitespace().collect::<Vec<_>>();
    let tokens = collapse_spaced_assignments(&raw_tokens);
    if tokens.len() < 4 {
        return Ok(None);
    }
    let mut index = 0usize;
    if tokens[index].eq_ignore_ascii_case("tran") {
        index += 1;
    }
    if tokens.len().saturating_sub(index) < 3 {
        return Ok(None);
    }
    let name = tokens[index].to_string();
    let kind_token = tokens[index + 1].to_ascii_lowercase();
    if kind_token == "trig" {
        return parse_internal_delay_measurement_card(name, &tokens[index + 1..], params).map(Some);
    }
    let kind = match kind_token.as_str() {
        "max" => InternalMeasurementKind::Max,
        "min" => InternalMeasurementKind::Min,
        "pp" | "peak_to_peak" | "peak-to-peak" => InternalMeasurementKind::PeakToPeak,
        "avg" | "average" => InternalMeasurementKind::Average,
        "rms" => InternalMeasurementKind::Rms,
        "final" | "last" => InternalMeasurementKind::Final,
        "find" => InternalMeasurementKind::Find,
        _ => return Ok(None),
    };
    let mut measurement_tokens = tokens[index + 2..].to_vec();
    let when = if let Some(when_index) = measurement_tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case("when"))
    {
        let when_tokens = measurement_tokens.split_off(when_index + 1);
        measurement_tokens.truncate(when_index);
        Some(parse_internal_delay_endpoint(&when_tokens, params)?)
    } else {
        None
    };
    let mut expression_tokens = Vec::new();
    let mut from_ps = None::<f64>;
    let mut to_ps = None::<f64>;
    let mut at_ps = None::<f64>;
    for token in &measurement_tokens {
        if let Some((key, value)) = token.split_once('=') {
            if key.eq_ignore_ascii_case("from") {
                from_ps = Some(resolve_time_ps(value, params).map_err(|err| err.to_string())?);
                continue;
            }
            if key.eq_ignore_ascii_case("to") {
                to_ps = Some(resolve_time_ps(value, params).map_err(|err| err.to_string())?);
                continue;
            }
            if key.eq_ignore_ascii_case("at") {
                at_ps = Some(resolve_time_ps(value, params).map_err(|err| err.to_string())?);
                continue;
            }
        }
        expression_tokens.push(token.as_str());
    }
    if kind == InternalMeasurementKind::Find && at_ps.is_none() && when.is_none() {
        return Err("internal_transient_invalid_measurement_find".to_string());
    }
    if let (Some(from_ps), Some(to_ps)) = (from_ps, to_ps) {
        if to_ps < from_ps {
            return Err("internal_transient_invalid_measurement_window".to_string());
        }
    }
    let expression = expression_tokens.join("");
    let Some(probe) = parse_measurement_voltage_probe(&expression) else {
        return Ok(None);
    };
    Ok(Some(InternalMeasurementCard::Scalar(InternalMeasurement {
        name,
        kind,
        probe,
        from_ps,
        to_ps,
        at_ps,
        when,
    })))
}

fn parse_measurement_voltage_probe(expression: &str) -> Option<InternalVoltageProbe> {
    let trimmed = expression.trim();
    if trimmed.len() < 4 {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("v(") || !trimmed.ends_with(')') {
        return None;
    }
    let inner = trimmed[2..trimmed.len() - 1].trim();
    if inner.is_empty() {
        return None;
    }
    let parts = inner.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 2 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let pos_name = parts[0].to_ascii_lowercase();
    let neg_name = parts
        .get(1)
        .filter(|name| !is_ground_node(name))
        .map(|name| name.to_ascii_lowercase());
    Some(InternalVoltageProbe {
        raw: format!("V({inner})"),
        pos_name,
        neg_name,
        pos: None,
        neg: None,
    })
}

fn parse_internal_delay_measurement_card(
    name: String,
    tokens: &[String],
    params: &BTreeMap<String, f64>,
) -> Result<InternalMeasurementCard, String> {
    let Some(target_index) = tokens.iter().position(|token| {
        token.eq_ignore_ascii_case("targ") || token.eq_ignore_ascii_case("target")
    }) else {
        return Err("internal_transient_invalid_measurement_delay".to_string());
    };
    if target_index == 0 {
        return Err("internal_transient_invalid_measurement_delay".to_string());
    }
    let trigger = parse_internal_delay_endpoint(&tokens[1..target_index], params)?;
    let target = parse_internal_delay_endpoint(&tokens[target_index + 1..], params)?;
    Ok(InternalMeasurementCard::Delay(InternalDelayMeasurement {
        name,
        trigger,
        target,
    }))
}

fn parse_internal_delay_endpoint(
    tokens: &[String],
    params: &BTreeMap<String, f64>,
) -> Result<InternalDelayEndpoint, String> {
    if tokens.is_empty() {
        return Err("internal_transient_invalid_measurement_delay".to_string());
    }
    let (probe_token, inline_threshold) =
        if let Some((probe_token, value)) = tokens[0].split_once('=') {
            (
                probe_token,
                Some(evaluate_expression(value, params).map_err(|err| err.to_string())?),
            )
        } else {
            (tokens[0].as_str(), None)
        };
    let Some(probe) = parse_measurement_voltage_probe(probe_token) else {
        return Err("internal_transient_invalid_measurement_delay".to_string());
    };
    let mut threshold_v = inline_threshold;
    let mut direction = None::<InternalDelayCrossingDirection>;
    let mut ordinal = None::<InternalDelayCrossingOrdinal>;
    let mut td_ps = None::<f64>;
    for token in &tokens[1..] {
        let Some((key, value)) = token.split_once('=') else {
            return Err("internal_transient_invalid_measurement_delay".to_string());
        };
        if key.eq_ignore_ascii_case("val") || key.eq_ignore_ascii_case("value") {
            threshold_v = Some(evaluate_expression(value, params).map_err(|err| err.to_string())?);
            continue;
        }
        if key.eq_ignore_ascii_case("rise") {
            direction = Some(InternalDelayCrossingDirection::Rise);
            ordinal = Some(parse_measurement_ordinal(value)?);
            continue;
        }
        if key.eq_ignore_ascii_case("fall") {
            direction = Some(InternalDelayCrossingDirection::Fall);
            ordinal = Some(parse_measurement_ordinal(value)?);
            continue;
        }
        if key.eq_ignore_ascii_case("cross") {
            direction = Some(InternalDelayCrossingDirection::Cross);
            ordinal = Some(parse_measurement_ordinal(value)?);
            continue;
        }
        if key.eq_ignore_ascii_case("td") || key.eq_ignore_ascii_case("delay") {
            td_ps = Some(resolve_time_ps(value, params).map_err(|err| err.to_string())?);
            continue;
        }
        return Err("internal_transient_invalid_measurement_delay".to_string());
    }
    Ok(InternalDelayEndpoint {
        probe,
        threshold_v: threshold_v
            .ok_or_else(|| "internal_transient_invalid_measurement_delay".to_string())?,
        direction: direction.unwrap_or(InternalDelayCrossingDirection::Cross),
        ordinal: ordinal.unwrap_or(InternalDelayCrossingOrdinal::Index(1)),
        td_ps,
    })
}

fn parse_measurement_ordinal(value: &str) -> Result<InternalDelayCrossingOrdinal, String> {
    if value.eq_ignore_ascii_case("last") {
        return Ok(InternalDelayCrossingOrdinal::Last);
    }
    let parsed = value
        .trim()
        .parse::<usize>()
        .map_err(|_| "internal_transient_invalid_measurement_delay".to_string())?;
    if parsed == 0 {
        return Err("internal_transient_invalid_measurement_delay".to_string());
    }
    Ok(InternalDelayCrossingOrdinal::Index(parsed))
}

fn resolve_internal_voltage_probe(
    probe: &mut InternalVoltageProbe,
    node_indices: &BTreeMap<String, usize>,
) {
    probe.pos = node_indices.get(&probe.pos_name).copied();
    probe.neg = probe
        .neg_name
        .as_ref()
        .and_then(|name| node_indices.get(name).copied());
}

fn sample_internal_voltage_probe(
    sample: &InternalTransientSample,
    probe: &InternalVoltageProbe,
) -> Option<f64> {
    let pos = *sample.node_voltages.get(probe.pos?)?;
    let neg = match probe.neg {
        Some(index) => *sample.node_voltages.get(index)?,
        None => 0.0,
    };
    Some(pos - neg)
}

fn sample_internal_voltage_probe_at_time(
    samples: &[InternalTransientSample],
    probe: &InternalVoltageProbe,
    time_ps: f64,
) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    if time_ps <= samples[0].time_ps {
        return sample_internal_voltage_probe(&samples[0], probe);
    }
    for window in samples.windows(2) {
        let previous = &window[0];
        let current = &window[1];
        if time_ps > current.time_ps {
            continue;
        }
        let previous_value = sample_internal_voltage_probe(previous, probe)?;
        let current_value = sample_internal_voltage_probe(current, probe)?;
        let duration_ps = (current.time_ps - previous.time_ps).max(f64::EPSILON);
        let ratio = ((time_ps - previous.time_ps) / duration_ps).clamp(0.0, 1.0);
        return Some(previous_value + ratio * (current_value - previous_value));
    }
    samples
        .last()
        .and_then(|sample| sample_internal_voltage_probe(sample, probe))
}

fn voltage_probe_endpoint_ref(probe: &InternalVoltageProbe) -> SimulationEndpointRef {
    SimulationEndpointRef {
        raw: probe.raw.clone(),
        node: probe.node_label(),
        port: None,
    }
}

fn evaluate_internal_delay_measurements(
    netlist: &InternalTransientNetlist,
    samples: &[InternalTransientSample],
) -> (
    Vec<SimulationDelayDetail>,
    Vec<SimulationMeasurementWarning>,
) {
    let mut details = Vec::new();
    let mut warnings = Vec::new();
    for measurement in &netlist.delay_measurements {
        let Some(trigger_time_ps) = find_internal_delay_crossing(samples, &measurement.trigger)
        else {
            warnings.push(internal_measurement_warning(
                &measurement.name,
                "delay",
                "measurement_trigger_crossing_not_found",
                &measurement.trigger.probe,
            ));
            continue;
        };
        let Some(target_time_ps) = find_internal_delay_crossing(samples, &measurement.target)
        else {
            warnings.push(internal_measurement_warning(
                &measurement.name,
                "delay",
                "measurement_target_crossing_not_found",
                &measurement.target.probe,
            ));
            continue;
        };
        let delay_ps = target_time_ps - trigger_time_ps;
        if delay_ps.is_finite() {
            details.push(SimulationDelayDetail {
                name: measurement.name.clone(),
                delay_ps,
                from_ref: Some(voltage_probe_endpoint_ref(&measurement.trigger.probe)),
                to_ref: Some(voltage_probe_endpoint_ref(&measurement.target.probe)),
            });
        } else {
            warnings.push(internal_measurement_warning(
                &measurement.name,
                "delay",
                "measurement_non_finite",
                &measurement.target.probe,
            ));
        }
    }
    (details, warnings)
}

fn find_internal_delay_crossing(
    samples: &[InternalTransientSample],
    endpoint: &InternalDelayEndpoint,
) -> Option<f64> {
    let mut crossings = Vec::new();
    let start_ps = endpoint.td_ps.unwrap_or(f64::NEG_INFINITY);
    for window in samples.windows(2) {
        let previous = &window[0];
        let current = &window[1];
        if current.time_ps + 1.0e-9 < start_ps {
            continue;
        }
        let previous_value = sample_internal_voltage_probe(previous, &endpoint.probe)?;
        let current_value = sample_internal_voltage_probe(current, &endpoint.probe)?;
        if !delay_crossing_matches(
            endpoint.direction,
            previous_value,
            current_value,
            endpoint.threshold_v,
        ) {
            continue;
        }
        let delta = current_value - previous_value;
        let ratio = if delta.abs() <= f64::EPSILON {
            0.0
        } else {
            ((endpoint.threshold_v - previous_value) / delta).clamp(0.0, 1.0)
        };
        let crossing_time_ps = previous.time_ps + ratio * (current.time_ps - previous.time_ps);
        if crossing_time_ps + 1.0e-9 < start_ps {
            continue;
        }
        crossings.push(crossing_time_ps);
    }
    match endpoint.ordinal {
        InternalDelayCrossingOrdinal::Index(index) => crossings.get(index - 1).copied(),
        InternalDelayCrossingOrdinal::Last => crossings.last().copied(),
    }
}

fn delay_crossing_matches(
    direction: InternalDelayCrossingDirection,
    previous_value: f64,
    current_value: f64,
    threshold_v: f64,
) -> bool {
    match direction {
        InternalDelayCrossingDirection::Rise => {
            previous_value < threshold_v && current_value >= threshold_v
        }
        InternalDelayCrossingDirection::Fall => {
            previous_value > threshold_v && current_value <= threshold_v
        }
        InternalDelayCrossingDirection::Cross => {
            (previous_value < threshold_v && current_value >= threshold_v)
                || (previous_value > threshold_v && current_value <= threshold_v)
        }
    }
}

fn evaluate_internal_measurements(
    netlist: &InternalTransientNetlist,
    samples: &[InternalTransientSample],
) -> (
    Vec<SimulationMeasurementDetail>,
    Vec<SimulationMeasurementWarning>,
) {
    let mut details = Vec::new();
    let mut warnings = Vec::new();
    for measurement in &netlist.measurements {
        let kind = measurement.kind.as_str();
        if measurement.probe.pos.is_none()
            || measurement.probe.neg_name.is_some() && measurement.probe.neg.is_none()
        {
            warnings.push(internal_measurement_warning(
                &measurement.name,
                kind,
                "measurement_probe_unavailable",
                &measurement.probe,
            ));
            continue;
        }
        let value = if measurement.kind == InternalMeasurementKind::Find {
            if let Some(when) = &measurement.when {
                if when.probe.pos.is_none()
                    || when.probe.neg_name.is_some() && when.probe.neg.is_none()
                {
                    warnings.push(internal_measurement_warning(
                        &measurement.name,
                        kind,
                        "measurement_when_probe_unavailable",
                        &when.probe,
                    ));
                    continue;
                }
                let Some(time_ps) = find_internal_delay_crossing(samples, when) else {
                    warnings.push(internal_measurement_warning(
                        &measurement.name,
                        kind,
                        "measurement_crossing_not_found",
                        &when.probe,
                    ));
                    continue;
                };
                sample_internal_voltage_probe_at_time(samples, &measurement.probe, time_ps)
            } else {
                let Some(at_ps) = measurement.at_ps else {
                    warnings.push(internal_measurement_warning(
                        &measurement.name,
                        kind,
                        "measurement_at_missing",
                        &measurement.probe,
                    ));
                    continue;
                };
                sample_internal_voltage_probe_at_time(samples, &measurement.probe, at_ps)
            }
        } else {
            None
        };
        if let Some(value) = value {
            if value.is_finite() {
                details.push(SimulationMeasurementDetail {
                    name: measurement.name.clone(),
                    kind: kind.to_string(),
                    measured_value: value,
                    at_ref: Some(voltage_probe_endpoint_ref(&measurement.probe)),
                });
            } else {
                warnings.push(internal_measurement_warning(
                    &measurement.name,
                    kind,
                    "measurement_non_finite",
                    &measurement.probe,
                ));
            }
            continue;
        } else if measurement.kind == InternalMeasurementKind::Find {
            warnings.push(internal_measurement_warning(
                &measurement.name,
                kind,
                "measurement_sample_unavailable",
                &measurement.probe,
            ));
            continue;
        }
        let values = samples
            .iter()
            .filter(|sample| {
                measurement
                    .from_ps
                    .is_none_or(|from_ps| sample.time_ps + 1.0e-9 >= from_ps)
                    && measurement
                        .to_ps
                        .is_none_or(|to_ps| sample.time_ps <= to_ps + 1.0e-9)
            })
            .filter_map(|sample| sample_internal_voltage_probe(sample, &measurement.probe))
            .collect::<Vec<_>>();
        if values.is_empty() {
            warnings.push(internal_measurement_warning(
                &measurement.name,
                kind,
                "measurement_window_empty",
                &measurement.probe,
            ));
            continue;
        }
        let value = match measurement.kind {
            InternalMeasurementKind::Max => {
                values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            }
            InternalMeasurementKind::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
            InternalMeasurementKind::PeakToPeak => {
                let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let min = values.iter().copied().fold(f64::INFINITY, f64::min);
                max - min
            }
            InternalMeasurementKind::Average => values.iter().sum::<f64>() / values.len() as f64,
            InternalMeasurementKind::Rms => {
                (values.iter().map(|value| value * value).sum::<f64>() / values.len() as f64).sqrt()
            }
            InternalMeasurementKind::Final => values.last().copied().unwrap_or(0.0),
            InternalMeasurementKind::Find => continue,
        };
        if value.is_finite() {
            details.push(SimulationMeasurementDetail {
                name: measurement.name.clone(),
                kind: kind.to_string(),
                measured_value: value,
                at_ref: Some(voltage_probe_endpoint_ref(&measurement.probe)),
            });
        } else {
            warnings.push(internal_measurement_warning(
                &measurement.name,
                kind,
                "measurement_non_finite",
                &measurement.probe,
            ));
        }
    }
    (details, warnings)
}

fn internal_measurement_warning(
    name: &str,
    kind: &str,
    reason: &str,
    probe: &InternalVoltageProbe,
) -> SimulationMeasurementWarning {
    SimulationMeasurementWarning {
        name: name.to_string(),
        kind: kind.to_string(),
        reason: reason.to_string(),
        at_ref: Some(voltage_probe_endpoint_ref(probe)),
    }
}

fn internal_source_value_at_time(source: &InternalSourceSpec, time_s: f64) -> f64 {
    match source {
        InternalSourceSpec::Dc(value) => *value,
        InternalSourceSpec::Pulse {
            low,
            high,
            delay_s,
            rise_s,
            fall_s,
            width_s,
            period_s,
            cycle_count,
        } => {
            if time_s < *delay_s {
                return *low;
            }
            let local_time = if let Some(period_s) = period_s {
                let elapsed_s = time_s - delay_s;
                if let Some(cycle_count) = cycle_count {
                    let active_duration_s = (*cycle_count as f64) * *period_s;
                    if elapsed_s >= active_duration_s {
                        return *low;
                    }
                }
                elapsed_s.rem_euclid(*period_s)
            } else {
                time_s - delay_s
            };
            let high_delta = high - low;
            let rise_s = (*rise_s).max(f64::EPSILON);
            let fall_s = (*fall_s).max(f64::EPSILON);
            if local_time < rise_s {
                return low + high_delta * (local_time / rise_s);
            }
            if local_time < rise_s + *width_s {
                return *high;
            }
            if local_time < rise_s + *width_s + fall_s {
                let fall_time = local_time - rise_s - *width_s;
                return high - high_delta * (fall_time / fall_s);
            }
            *low
        }
        InternalSourceSpec::Exp {
            initial,
            pulsed,
            rise_delay_s,
            rise_tau_s,
            fall_delay_s,
            fall_tau_s,
        } => {
            if time_s < *rise_delay_s {
                return *initial;
            }
            let rise_component =
                (pulsed - initial) * (1.0 - (-(time_s - rise_delay_s) / rise_tau_s).exp());
            if time_s < *fall_delay_s {
                return initial + rise_component;
            }
            let fall_component =
                (initial - pulsed) * (1.0 - (-(time_s - fall_delay_s) / fall_tau_s).exp());
            initial + rise_component + fall_component
        }
        InternalSourceSpec::Pwl(points) => {
            if points.is_empty() {
                return 0.0;
            }
            if time_s < points[0].0 {
                return points[0].1;
            }
            let same_time_tolerance = f64::EPSILON * 16.0;
            let mut last_exact_value = None;
            for (point_time_s, point_value) in points {
                if (*point_time_s - time_s).abs() <= same_time_tolerance {
                    last_exact_value = Some(*point_value);
                }
            }
            if let Some(value) = last_exact_value {
                return value;
            }
            for window in points.windows(2) {
                let (start_t, start_v) = window[0];
                let (end_t, end_v) = window[1];
                if time_s < end_t {
                    let duration = (end_t - start_t).max(f64::EPSILON);
                    let alpha = ((time_s - start_t) / duration).clamp(0.0, 1.0);
                    return start_v + alpha * (end_v - start_v);
                }
            }
            points.last().map(|(_, value)| *value).unwrap_or(0.0)
        }
        InternalSourceSpec::Sin {
            offset,
            amplitude,
            frequency_hz,
            delay_s,
            damping_hz,
            phase_rad,
        } => {
            if time_s < *delay_s {
                return *offset;
            }
            let elapsed_s = time_s - delay_s;
            let phase = phase_rad + 2.0 * std::f64::consts::PI * frequency_hz * elapsed_s;
            let damping = (-damping_hz * elapsed_s).exp();
            offset + amplitude * damping * phase.sin()
        }
    }
}

fn internal_source_step_value_at_time(source: &InternalSourceSpec, time_s: f64) -> f64 {
    match source {
        InternalSourceSpec::Pwl(points) => {
            let same_time_tolerance = f64::EPSILON * 16.0;
            let mut exact_match_count = 0usize;
            let mut last_before_value = None;
            for (point_time_s, point_value) in points {
                if *point_time_s < time_s - same_time_tolerance {
                    last_before_value = Some(*point_value);
                    continue;
                }
                if (*point_time_s - time_s).abs() <= same_time_tolerance {
                    exact_match_count += 1;
                    continue;
                }
                break;
            }
            if exact_match_count >= 2 {
                if let Some(value) = last_before_value {
                    return value;
                }
            }
            internal_source_value_at_time(source, time_s)
        }
        _ => internal_source_value_at_time(source, time_s),
    }
}

fn advance_internal_transient_step(
    netlist: &InternalTransientNetlist,
    previous_solution: &[f64],
    time_step_s: f64,
    step_start_time_s: f64,
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
) -> Result<Vec<f64>, String> {
    advance_internal_transient_step_with_limits(
        netlist,
        previous_solution,
        time_step_s,
        step_start_time_s,
        64,
        1.0,
        solution_history,
    )
}

fn advance_internal_transient_step_with_limits(
    netlist: &InternalTransientNetlist,
    previous_solution: &[f64],
    time_step_s: f64,
    step_start_time_s: f64,
    max_substeps: usize,
    tolerance: f64,
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
) -> Result<Vec<f64>, String> {
    let mut substep_count = internal_substep_count(netlist, time_step_s, step_start_time_s);
    let max_substeps = max_substeps.max(substep_count.max(1));
    let tolerance = tolerance.max(0.0);

    loop {
        let coarse = solve_internal_transient_step_sequence(
            netlist,
            previous_solution,
            time_step_s,
            step_start_time_s,
            substep_count,
            solution_history,
        )?;
        let refined_substeps = (substep_count.saturating_mul(2)).min(max_substeps);
        let refined = solve_internal_transient_step_sequence(
            netlist,
            previous_solution,
            time_step_s,
            step_start_time_s,
            refined_substeps,
            solution_history,
        )?;
        let error_norm = solution_error_norm(&coarse, &refined);

        if refined_substeps == substep_count {
            if error_norm <= tolerance {
                return Ok(refined);
            }
            return Err(format!(
                "internal_transient_timestep_not_converged:t_ps={:.6}:dt_ps={:.6}:substeps={refined_substeps}:error={error_norm:.6e}:tol={tolerance:.6e}",
                step_start_time_s * 1.0e12,
                time_step_s * 1.0e12,
            ));
        }
        if error_norm <= tolerance {
            return Ok(refined);
        }
        if refined_substeps >= max_substeps {
            return Err(format!(
                "internal_transient_timestep_not_converged:t_ps={:.6}:dt_ps={:.6}:substeps={refined_substeps}:error={error_norm:.6e}:tol={tolerance:.6e}",
                step_start_time_s * 1.0e12,
                time_step_s * 1.0e12,
            ));
        }
        substep_count = refined_substeps;
    }
}

fn solve_internal_transient_step_sequence(
    netlist: &InternalTransientNetlist,
    previous_solution: &[f64],
    time_step_s: f64,
    step_start_time_s: f64,
    substep_count: usize,
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
) -> Result<Vec<f64>, String> {
    let substep_count = substep_count.max(1);
    let mut state = previous_solution.to_vec();
    let boundaries =
        substep_boundaries_with_breakpoints(netlist, step_start_time_s, time_step_s, substep_count);
    for window in boundaries.windows(2) {
        let substep_start_s = window[0];
        let current_time_s = window[1];
        let substep_size_s = (current_time_s - substep_start_s).max(f64::EPSILON);
        state = solve_internal_transient_step(
            netlist,
            &state,
            substep_size_s,
            current_time_s,
            solution_history,
        )?;
    }
    Ok(state)
}

fn substep_boundaries_with_breakpoints(
    netlist: &InternalTransientNetlist,
    step_start_time_s: f64,
    time_step_s: f64,
    substep_count: usize,
) -> Vec<f64> {
    let step_end_time_s = step_start_time_s + time_step_s;
    let mut breakpoints = vec![step_start_time_s, step_end_time_s];
    breakpoints.extend(collect_transmission_breakpoints_within_step(
        netlist,
        step_start_time_s,
        time_step_s,
    ));

    for element in &netlist.elements {
        let source = match element {
            InternalElement::CurrentSource { source, .. } => Some(source),
            InternalElement::VoltageSource { source, .. } => Some(source),
            _ => None,
        };
        let Some(source) = source else {
            continue;
        };
        breakpoints.extend(collect_source_breakpoints_within_step(
            source,
            step_start_time_s,
            time_step_s,
        ));
    }

    sort_and_dedup_breakpoints(&mut breakpoints);

    let mut boundaries = Vec::with_capacity(substep_count.max(breakpoints.len()));
    boundaries.push(step_start_time_s);
    for window in breakpoints.windows(2) {
        let interval_start_s = window[0];
        let interval_end_s = window[1];
        let interval_s = (interval_end_s - interval_start_s).max(0.0);
        if interval_s <= 0.0 {
            continue;
        }
        let segments = ((interval_s / time_step_s) * substep_count as f64).ceil() as usize;
        let segments = segments.max(1);
        for segment in 1..=segments {
            let alpha = segment as f64 / segments as f64;
            boundaries.push(interval_start_s + interval_s * alpha);
        }
    }
    sort_and_dedup_breakpoints(&mut boundaries);
    boundaries
}

fn sort_and_dedup_breakpoints(times: &mut Vec<f64>) {
    times.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|left, right| (*left - *right).abs() <= f64::EPSILON * 16.0);
}

fn collect_netlist_breakpoints_within_step(
    netlist: &InternalTransientNetlist,
    step_start_time_s: f64,
    time_step_s: f64,
) -> Vec<f64> {
    let step_end_time_s = step_start_time_s + time_step_s;
    let mut breakpoints = vec![step_start_time_s, step_end_time_s];
    breakpoints.extend(collect_transmission_breakpoints_within_step(
        netlist,
        step_start_time_s,
        time_step_s,
    ));
    for element in &netlist.elements {
        let source = match element {
            InternalElement::CurrentSource { source, .. } => Some(source),
            InternalElement::VoltageSource { source, .. } => Some(source),
            _ => None,
        };
        let Some(source) = source else {
            continue;
        };
        breakpoints.extend(collect_source_breakpoints_within_step(
            source,
            step_start_time_s,
            time_step_s,
        ));
    }
    sort_and_dedup_breakpoints(&mut breakpoints);
    breakpoints
}

fn collect_transmission_breakpoints_within_step(
    netlist: &InternalTransientNetlist,
    step_start_time_s: f64,
    time_step_s: f64,
) -> Vec<f64> {
    let step_end_time_s = step_start_time_s + time_step_s;
    let mut breakpoints = Vec::new();
    for element in &netlist.elements {
        let InternalElement::TransmissionLineResistive {
            pos_a,
            neg_a,
            pos_b,
            neg_b,
            delay_s,
            ..
        } = element
        else {
            continue;
        };
        if *delay_s > step_start_time_s
            && *delay_s < step_end_time_s
            && transmission_line_has_target_endpoints(netlist, [*pos_a, *neg_a], [*pos_b, *neg_b])
        {
            breakpoints.push(*delay_s);
        }
    }
    for source_element in &netlist.elements {
        let (source_nodes, source) = match source_element {
            InternalElement::CurrentSource { pos, neg, source } => ([*pos, *neg], source),
            InternalElement::VoltageSource {
                pos, neg, source, ..
            } => ([*pos, *neg], source),
            _ => continue,
        };
        breakpoints.extend(collect_propagated_source_breakpoints_within_step(
            netlist,
            source,
            source_nodes,
            step_start_time_s,
            time_step_s,
        ));
    }
    breakpoints
}

fn collect_propagated_source_breakpoints_within_step(
    netlist: &InternalTransientNetlist,
    source: &InternalSourceSpec,
    source_nodes: [Option<usize>; 2],
    step_start_time_s: f64,
    time_step_s: f64,
) -> Vec<f64> {
    let mut breakpoints = Vec::new();
    let source_node_pair = canonicalize_node_pair(source_nodes);
    let mut frontier = std::collections::VecDeque::from([(
        source_nodes,
        0.0_f64,
        Vec::<usize>::new(),
        vec![source_node_pair],
    )]);

    while let Some((frontier_nodes, accumulated_delay_s, used_transmissions, visited_nodes)) =
        frontier.pop_front()
    {
        for (line_index, element) in netlist.elements.iter().enumerate() {
            let InternalElement::TransmissionLineResistive {
                pos_a,
                neg_a,
                pos_b,
                neg_b,
                delay_s,
                ..
            } = element
            else {
                continue;
            };

            if used_transmissions.contains(&line_index) {
                continue;
            }

            let side_a = [*pos_a, *neg_a];
            let side_b = [*pos_b, *neg_b];
            let next_nodes = if node_pairs_are_adjacent(frontier_nodes, side_a) {
                side_b
            } else if node_pairs_are_adjacent(frontier_nodes, side_b) {
                side_a
            } else {
                continue;
            };

            let propagated_delay_s = accumulated_delay_s + *delay_s;
            let canonical_next_nodes = canonicalize_node_pair(next_nodes);
            if visited_nodes.contains(&canonical_next_nodes) {
                continue;
            }

            if transmission_node_pair_is_breakpoint_target(netlist, canonical_next_nodes) {
                let shifted_step_start_s = step_start_time_s - propagated_delay_s;
                let shifted_breakpoints = collect_source_breakpoints_within_step(
                    source,
                    shifted_step_start_s,
                    time_step_s,
                );
                for breakpoint_s in shifted_breakpoints {
                    let delayed_breakpoint_s = breakpoint_s + propagated_delay_s;
                    if delayed_breakpoint_s > step_start_time_s
                        && delayed_breakpoint_s < step_start_time_s + time_step_s
                    {
                        breakpoints.push(delayed_breakpoint_s);
                    }
                }
            }

            let mut next_used_transmissions = used_transmissions.clone();
            next_used_transmissions.push(line_index);
            let mut next_visited_nodes = visited_nodes.clone();
            next_visited_nodes.push(canonical_next_nodes);
            frontier.push_back((
                next_nodes,
                propagated_delay_s,
                next_used_transmissions,
                next_visited_nodes,
            ));
        }
    }
    breakpoints
}

fn node_pairs_share_signal_node(left: [Option<usize>; 2], right: [Option<usize>; 2]) -> bool {
    left.iter().flatten().any(|left_node| {
        right
            .iter()
            .flatten()
            .any(|right_node| left_node == right_node)
    })
}

fn node_pairs_are_adjacent(left: [Option<usize>; 2], right: [Option<usize>; 2]) -> bool {
    canonicalize_node_pair(left) == canonicalize_node_pair(right)
        || node_pairs_share_signal_node(left, right)
}

fn canonicalize_node_pair(nodes: [Option<usize>; 2]) -> (Option<usize>, Option<usize>) {
    let mut ordered = [nodes[0], nodes[1]];
    ordered.sort();
    (ordered[0], ordered[1])
}

fn transmission_node_pair_is_breakpoint_target(
    netlist: &InternalTransientNetlist,
    node_pair: (Option<usize>, Option<usize>),
) -> bool {
    let node_array = [node_pair.0, node_pair.1];

    for element in &netlist.elements {
        match element {
            InternalElement::TransmissionLineResistive { .. } => continue,
            InternalElement::Resistor { pos, neg, .. }
            | InternalElement::Capacitor { pos, neg, .. }
            | InternalElement::CurrentSource { pos, neg, .. }
            | InternalElement::JosephsonJunction { pos, neg, .. } => {
                if canonicalize_node_pair([*pos, *neg]) == node_pair
                    || node_pairs_share_signal_node(node_array, [*pos, *neg])
                {
                    return true;
                }
            }
            InternalElement::Inductor { pos, neg, .. }
            | InternalElement::VoltageSource { pos, neg, .. } => {
                if canonicalize_node_pair([*pos, *neg]) == node_pair
                    || node_pairs_share_signal_node(node_array, [*pos, *neg])
                {
                    return true;
                }
            }
        }
    }

    false
}

fn transmission_line_has_target_endpoints(
    netlist: &InternalTransientNetlist,
    left_nodes: [Option<usize>; 2],
    right_nodes: [Option<usize>; 2],
) -> bool {
    transmission_node_pair_is_breakpoint_target(netlist, canonicalize_node_pair(left_nodes))
        && transmission_node_pair_is_breakpoint_target(netlist, canonicalize_node_pair(right_nodes))
}

fn solution_error_norm(coarse: &[f64], refined: &[f64]) -> f64 {
    solution_error_norm_with_tolerances(coarse, refined, 1.0e-5, 2.0e-2)
}

fn solution_error_norm_with_tolerances(
    coarse: &[f64],
    refined: &[f64],
    absolute_tolerance: f64,
    relative_tolerance: f64,
) -> f64 {
    coarse
        .iter()
        .zip(refined.iter())
        .map(|(left, right)| {
            let scale = absolute_tolerance + relative_tolerance * left.abs().max(right.abs());
            (left - right).abs() / scale.max(f64::EPSILON)
        })
        .fold(0.0_f64, f64::max)
}

fn internal_substep_count(
    netlist: &InternalTransientNetlist,
    time_step_s: f64,
    step_start_time_s: f64,
) -> usize {
    let mut suggested = 1usize;

    if netlist
        .elements
        .iter()
        .any(|element| matches!(element, InternalElement::Inductor { .. }))
    {
        suggested = suggested.max(4);
    }

    for element in &netlist.elements {
        if let InternalElement::TransmissionLineResistive { delay_s, .. } = element {
            if *delay_s > 0.0 {
                let delay_substeps = (time_step_s / delay_s.max(f64::EPSILON)).ceil() as usize;
                suggested = suggested.max(delay_substeps.clamp(1, 16));
            }
        }
    }

    for element in &netlist.elements {
        let source = match element {
            InternalElement::CurrentSource { source, .. } => Some(source),
            InternalElement::VoltageSource { source, .. } => Some(source),
            _ => None,
        };
        let Some(source) = source else {
            continue;
        };
        let characteristic_s = match source {
            InternalSourceSpec::Dc(_) => None,
            InternalSourceSpec::Pulse {
                rise_s,
                fall_s,
                width_s,
                ..
            } => Some((*rise_s).max((*fall_s).max(*width_s)).max(f64::EPSILON)),
            InternalSourceSpec::Exp {
                rise_tau_s,
                fall_tau_s,
                rise_delay_s,
                fall_delay_s,
                ..
            } => {
                let delay_gap_s = (fall_delay_s - rise_delay_s).abs();
                Some(
                    (*rise_tau_s)
                        .min(*fall_tau_s)
                        .min(delay_gap_s.max(f64::EPSILON))
                        .max(f64::EPSILON),
                )
            }
            InternalSourceSpec::Pwl(points) => points
                .windows(2)
                .map(|window| (window[1].0 - window[0].0).abs())
                .filter(|delta| *delta > 0.0)
                .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)),
            InternalSourceSpec::Sin {
                frequency_hz,
                damping_hz,
                ..
            } => {
                let oscillation_s = (1.0 / frequency_hz).abs() / 16.0;
                let damping_s = if *damping_hz > 0.0 {
                    1.0 / damping_hz.abs()
                } else {
                    f64::INFINITY
                };
                Some(oscillation_s.min(damping_s / 8.0).max(f64::EPSILON))
            }
        };
        let Some(characteristic_s) = characteristic_s else {
            continue;
        };
        let source_substeps = (time_step_s / characteristic_s).ceil() as usize;
        suggested = suggested.max(source_substeps.clamp(1, 16));

        if let Some(event_segment_s) =
            source_event_segment_within_step(source, step_start_time_s, time_step_s)
        {
            let event_substeps = ((time_step_s / event_segment_s).ceil() as usize).clamp(1, 16);
            suggested = suggested.max(event_substeps);
        }
    }

    suggested.clamp(1, 16)
}

fn source_event_segment_within_step(
    source: &InternalSourceSpec,
    step_start_time_s: f64,
    time_step_s: f64,
) -> Option<f64> {
    let step_end_time_s = step_start_time_s + time_step_s;
    let mut breakpoints = vec![step_start_time_s, step_end_time_s];
    breakpoints.extend(collect_source_breakpoints_within_step(
        source,
        step_start_time_s,
        time_step_s,
    ));
    sort_and_dedup_breakpoints(&mut breakpoints);

    if breakpoints.len() <= 2 {
        return None;
    }
    breakpoints
        .windows(2)
        .map(|window| (window[1] - window[0]).abs())
        .filter(|segment_s| *segment_s > 0.0)
        .min_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
        .map(|segment_s| segment_s.max(f64::EPSILON))
}

fn collect_source_breakpoints_within_step(
    source: &InternalSourceSpec,
    step_start_time_s: f64,
    time_step_s: f64,
) -> Vec<f64> {
    let step_end_time_s = step_start_time_s + time_step_s;
    let mut breakpoints = Vec::new();

    match source {
        InternalSourceSpec::Dc(_) => return breakpoints,
        InternalSourceSpec::Sin { delay_s, .. } => {
            if *delay_s > step_start_time_s && *delay_s < step_end_time_s {
                breakpoints.push(*delay_s);
            }
            return breakpoints;
        }
        InternalSourceSpec::Pulse {
            delay_s,
            rise_s,
            fall_s,
            width_s,
            period_s,
            cycle_count,
            ..
        } => {
            let local_edges = [
                *delay_s,
                *delay_s + *rise_s,
                *delay_s + *rise_s + *width_s,
                *delay_s + *rise_s + *width_s + *fall_s,
            ];
            if let Some(period_s) = period_s {
                let period_s = (*period_s).max(f64::EPSILON);
                let start_cycle = ((step_start_time_s - local_edges[3]) / period_s).floor() as i64;
                let end_cycle = ((step_end_time_s - local_edges[0]) / period_s).ceil() as i64;
                let min_cycle = if cycle_count.is_some() {
                    0
                } else {
                    start_cycle
                };
                let max_cycle = if let Some(cycle_count) = cycle_count {
                    (*cycle_count as i64).saturating_sub(1)
                } else {
                    end_cycle
                };
                let loop_start = start_cycle.max(min_cycle);
                let loop_end = end_cycle.min(max_cycle);
                if loop_start <= loop_end {
                    for cycle in loop_start..=loop_end {
                        let cycle_offset = (cycle as f64) * period_s;
                        for edge in local_edges {
                            let time_s = edge + cycle_offset;
                            if time_s > step_start_time_s && time_s < step_end_time_s {
                                breakpoints.push(time_s);
                            }
                        }
                    }
                }
                if let Some(cycle_count) = cycle_count {
                    let sequence_end_s = *delay_s + (*cycle_count as f64) * period_s;
                    if sequence_end_s > step_start_time_s && sequence_end_s < step_end_time_s {
                        breakpoints.push(sequence_end_s);
                    }
                }
            } else {
                for edge in local_edges {
                    if edge > step_start_time_s && edge < step_end_time_s {
                        breakpoints.push(edge);
                    }
                }
            }
        }
        InternalSourceSpec::Exp {
            rise_delay_s,
            fall_delay_s,
            ..
        } => {
            for edge in [*rise_delay_s, *fall_delay_s] {
                if edge > step_start_time_s && edge < step_end_time_s {
                    breakpoints.push(edge);
                }
            }
        }
        InternalSourceSpec::Pwl(points) => {
            for (time_s, _) in points {
                if *time_s > step_start_time_s && *time_s < step_end_time_s {
                    breakpoints.push(*time_s);
                }
            }
        }
    }

    breakpoints
}

fn intern_node(
    token: &str,
    node_indices: &mut BTreeMap<String, usize>,
    node_names: &mut Vec<String>,
) -> Option<usize> {
    if is_ground_node(token) {
        return None;
    }
    let key = token.to_ascii_lowercase();
    if let Some(index) = node_indices.get(&key) {
        return Some(*index);
    }
    let index = node_names.len();
    node_indices.insert(key, index);
    node_names.push(token.to_string());
    Some(index)
}

fn is_ground_node(token: &str) -> bool {
    token == "0" || token.eq_ignore_ascii_case("gnd")
}

fn solve_internal_transient_step(
    netlist: &InternalTransientNetlist,
    previous_solution: &[f64],
    time_step_s: f64,
    current_time_s: f64,
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
) -> Result<Vec<f64>, String> {
    let nonlinear_config = internal_nonlinear_solve_config(netlist);
    let requires_iteration = netlist_requires_nonlinear_iteration(netlist);
    let mut iterate_solution = previous_solution.to_vec();

    for iteration in 0..nonlinear_config.max_iterations {
        let (matrix, rhs) = assemble_internal_transient_system(
            netlist,
            previous_solution,
            time_step_s,
            current_time_s,
            &iterate_solution,
            solution_history,
        )?;
        let candidate = solve_dense_linear_system(matrix, rhs)?;
        let residual = solution_error_norm_with_tolerances(
            &iterate_solution,
            &candidate,
            nonlinear_config.absolute_tolerance,
            nonlinear_config.relative_tolerance,
        );
        iterate_solution = candidate;

        if !requires_iteration || residual <= nonlinear_config.residual_tolerance {
            return Ok(iterate_solution);
        }
        if iteration + 1 >= nonlinear_config.max_iterations {
            return Err(format!(
                "internal_transient_newton_not_converged:iters={}:residual={residual:.6e}:tol={:.6e}",
                nonlinear_config.max_iterations,
                nonlinear_config.residual_tolerance
            ));
        }
    }
    Err("internal_transient_newton_not_converged:iters=0".to_string())
}

fn internal_nonlinear_solve_config(
    netlist: &InternalTransientNetlist,
) -> InternalNonlinearSolveConfig {
    InternalNonlinearSolveConfig {
        max_iterations: netlist.option_max_iterations.unwrap_or(8),
        residual_tolerance: 1.0e-3,
        absolute_tolerance: netlist.option_abstol.unwrap_or(1.0e-5),
        relative_tolerance: netlist.option_reltol.unwrap_or(2.0e-2),
    }
}

fn netlist_requires_nonlinear_iteration(netlist: &InternalTransientNetlist) -> bool {
    netlist.elements.iter().any(|element| match element {
        InternalElement::JosephsonJunction { .. } => true,
        InternalElement::TransmissionLineResistive { delay_s, .. } => *delay_s > 0.0,
        _ => false,
    })
}

fn assemble_internal_transient_system(
    netlist: &InternalTransientNetlist,
    previous_solution: &[f64],
    time_step_s: f64,
    current_time_s: f64,
    iterate_solution: &[f64],
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
) -> Result<(Vec<Vec<f64>>, Vec<f64>), String> {
    let node_count = netlist.node_names.len();
    let matrix_size = node_count + netlist.auxiliary_count;
    if matrix_size == 0 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut matrix = vec![vec![0.0; matrix_size]; matrix_size];
    let mut rhs = vec![0.0; matrix_size];

    for (element_index, element) in netlist.elements.iter().enumerate() {
        match *element {
            InternalElement::Resistor {
                pos,
                neg,
                resistance_ohm,
            } => {
                stamp_conductance(&mut matrix, pos, neg, 1.0 / resistance_ohm);
                let resistor_noise_a = internal_resistor_noise_value(
                    netlist,
                    current_time_s,
                    time_step_s,
                    element_index as u64,
                    resistance_ohm,
                );
                stamp_current_injection(&mut rhs, pos, neg, resistor_noise_a);
            }
            InternalElement::Capacitor {
                pos,
                neg,
                capacitance_f,
            } => {
                let conductance = capacitance_f / time_step_s;
                stamp_conductance(&mut matrix, pos, neg, conductance);
                let previous_delta =
                    node_voltage(previous_solution, pos) - node_voltage(previous_solution, neg);
                stamp_current_injection(&mut rhs, pos, neg, conductance * previous_delta);
            }
            InternalElement::Inductor {
                pos,
                neg,
                inductance_h,
                branch_index,
            } => {
                let current_row = node_count + branch_index;
                let branch_current = previous_solution.get(current_row).copied().unwrap_or(0.0);
                if let Some(pos) = pos {
                    matrix[pos][current_row] += 1.0;
                    matrix[current_row][pos] += 1.0;
                }
                if let Some(neg) = neg {
                    matrix[neg][current_row] -= 1.0;
                    matrix[current_row][neg] -= 1.0;
                }
                matrix[current_row][current_row] -= inductance_h / time_step_s;
                rhs[current_row] -= (inductance_h / time_step_s) * branch_current;
            }
            InternalElement::CurrentSource {
                pos,
                neg,
                ref source,
            } => {
                let source_index = element_index as u64;
                let current_a = internal_source_step_value_at_time(source, current_time_s)
                    + internal_source_noise_value(
                        netlist,
                        current_time_s,
                        time_step_s,
                        source_index,
                        false,
                    );
                stamp_current_injection(&mut rhs, pos, neg, -current_a);
            }
            InternalElement::VoltageSource {
                pos,
                neg,
                ref source,
                branch_index,
            } => {
                let current_row = node_count + branch_index;
                if let Some(pos) = pos {
                    matrix[pos][current_row] += 1.0;
                    matrix[current_row][pos] += 1.0;
                }
                if let Some(neg) = neg {
                    matrix[neg][current_row] -= 1.0;
                    matrix[current_row][neg] -= 1.0;
                }
                let source_index = element_index as u64;
                rhs[current_row] += internal_source_step_value_at_time(source, current_time_s)
                    + internal_source_noise_value(
                        netlist,
                        current_time_s,
                        time_step_s,
                        source_index,
                        true,
                    );
            }
            InternalElement::TransmissionLineResistive {
                pos_a,
                neg_a,
                pos_b,
                neg_b,
                impedance_ohm,
                delay_s,
                attenuation,
            } => {
                let conductance = 1.0 / impedance_ohm;
                if delay_s <= 0.0 {
                    if attenuation >= 1.0 - 1.0e-12 {
                        stamp_conductance(&mut matrix, pos_a, pos_b, conductance);
                        stamp_conductance(&mut matrix, neg_a, neg_b, conductance);
                    } else {
                        let delayed_port_a = attenuation
                            * (node_voltage(iterate_solution, pos_a)
                                - node_voltage(iterate_solution, neg_a));
                        let delayed_port_b = attenuation
                            * (node_voltage(iterate_solution, pos_b)
                                - node_voltage(iterate_solution, neg_b));
                        stamp_conductance(&mut matrix, pos_a, neg_a, conductance);
                        stamp_conductance(&mut matrix, pos_b, neg_b, conductance);
                        stamp_current_injection(
                            &mut rhs,
                            pos_a,
                            neg_a,
                            conductance * delayed_port_b,
                        );
                        stamp_current_injection(
                            &mut rhs,
                            pos_b,
                            neg_b,
                            conductance * delayed_port_a,
                        );
                    }
                } else {
                    let delayed_port_a = attenuation
                        * delayed_port_voltage_delta(
                            solution_history,
                            pos_a,
                            neg_a,
                            previous_solution,
                            iterate_solution,
                            current_time_s,
                            delay_s,
                            time_step_s,
                        );
                    let delayed_port_b = attenuation
                        * delayed_port_voltage_delta(
                            solution_history,
                            pos_b,
                            neg_b,
                            previous_solution,
                            iterate_solution,
                            current_time_s,
                            delay_s,
                            time_step_s,
                        );
                    stamp_conductance(&mut matrix, pos_a, neg_a, conductance);
                    stamp_conductance(&mut matrix, pos_b, neg_b, conductance);
                    stamp_current_injection(&mut rhs, pos_a, neg_a, conductance * delayed_port_b);
                    stamp_current_injection(&mut rhs, pos_b, neg_b, conductance * delayed_port_a);
                }
            }
            InternalElement::JosephsonJunction { .. } => {}
        }
    }

    for coupling in &netlist.mutual_couplings {
        let row_a = node_count + coupling.branch_a;
        let row_b = node_count + coupling.branch_b;
        let branch_current_a = previous_solution.get(row_a).copied().unwrap_or(0.0);
        let branch_current_b = previous_solution.get(row_b).copied().unwrap_or(0.0);
        let coupling_coeff = coupling.mutual_h / time_step_s;
        matrix[row_a][row_b] -= coupling_coeff;
        matrix[row_b][row_a] -= coupling_coeff;
        rhs[row_a] -= coupling_coeff * branch_current_b;
        rhs[row_b] -= coupling_coeff * branch_current_a;
    }

    stamp_internal_nonlinear_terms(
        netlist,
        previous_solution,
        iterate_solution,
        current_time_s,
        solution_history,
        time_step_s,
        &mut matrix,
        &mut rhs,
    );

    Ok((matrix, rhs))
}

fn internal_source_noise_value(
    netlist: &InternalTransientNetlist,
    current_time_s: f64,
    time_step_s: f64,
    source_index: u64,
    is_voltage_source: bool,
) -> f64 {
    let sigma = netlist.option_noise_sigma.unwrap_or(0.0);
    let seed = match netlist.option_seed {
        Some(seed) if sigma > 0.0 => seed,
        _ => return 0.0,
    };
    let normalized_step = if time_step_s > 0.0 {
        (current_time_s / time_step_s).round()
    } else {
        0.0
    };
    let step_index = if normalized_step.is_finite() {
        normalized_step as i64
    } else {
        0
    };
    let source_tag = if is_voltage_source {
        0xA5A5_5A5A_u64
    } else {
        0x5A5A_A5A5_u64
    };
    let temperature_scale = internal_noise_temperature_scale(netlist);
    sigma
        * temperature_scale
        * internal_unit_noise_sample(seed, step_index, source_index ^ source_tag)
}

fn internal_resistor_noise_value(
    netlist: &InternalTransientNetlist,
    current_time_s: f64,
    time_step_s: f64,
    resistor_index: u64,
    resistance_ohm: f64,
) -> f64 {
    let sigma = netlist.option_noise_sigma.unwrap_or(0.0);
    let seed = match netlist.option_seed {
        Some(seed) if sigma > 0.0 => seed,
        _ => return 0.0,
    };
    let normalized_step = if time_step_s > 0.0 {
        (current_time_s / time_step_s).round()
    } else {
        0.0
    };
    let step_index = if normalized_step.is_finite() {
        normalized_step as i64
    } else {
        0
    };
    let resistance_scale = 1.0 / resistance_ohm.max(f64::EPSILON).sqrt();
    let temperature_scale = internal_noise_temperature_scale(netlist);
    sigma
        * temperature_scale
        * resistance_scale
        * internal_unit_noise_sample(seed, step_index, resistor_index ^ 0xC3C3_3C3C_u64)
}

fn internal_noise_temperature_scale(netlist: &InternalTransientNetlist) -> f64 {
    let temperature_k = netlist.option_temperature_k.unwrap_or(300.15);
    let nominal_temperature_k = netlist.option_nominal_temperature_k.unwrap_or(300.15);
    (temperature_k / nominal_temperature_k).sqrt()
}

fn internal_unit_noise_sample(seed: u64, step_index: i64, salt: u64) -> f64 {
    let mut state =
        seed ^ salt.rotate_left(13) ^ (step_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    state = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    state ^= state >> 31;
    let unit = (state as f64) / (u64::MAX as f64);
    2.0 * unit - 1.0
}

fn stamp_internal_nonlinear_terms(
    netlist: &InternalTransientNetlist,
    previous_solution: &[f64],
    iterate_solution: &[f64],
    current_time_s: f64,
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
    time_step_s: f64,
    matrix: &mut [Vec<f64>],
    rhs: &mut [f64],
) {
    const PHI0_WEBER: f64 = 2.067_833_848e-15;
    let josephson_gain = 2.0 * std::f64::consts::PI / PHI0_WEBER;

    for element in &netlist.elements {
        let InternalElement::JosephsonJunction {
            pos,
            neg,
            critical_current_a,
            second_harmonic_current_a,
            third_harmonic_current_a,
            fourth_harmonic_current_a,
            fifth_harmonic_current_a,
            sixth_harmonic_current_a,
            normal_resistance_ohm,
            junction_cap_f,
        } = *element
        else {
            continue;
        };

        let v_iter = node_voltage(iterate_solution, pos) - node_voltage(iterate_solution, neg);
        let v_prev = node_voltage(previous_solution, pos) - node_voltage(previous_solution, neg);
        let phase_prev = josephson_phase_from_history(
            solution_history,
            pos,
            neg,
            current_time_s,
            josephson_gain,
        );
        let phase_iter = phase_prev + josephson_gain * time_step_s * v_iter;
        let shunt_g = 1.0 / normal_resistance_ohm;
        let cap_g = junction_cap_f / time_step_s;
        let nonlinear_g = (critical_current_a * phase_iter.cos()
            + 2.0 * second_harmonic_current_a * (2.0 * phase_iter).cos()
            + 3.0 * third_harmonic_current_a * (3.0 * phase_iter).cos()
            + 4.0 * fourth_harmonic_current_a * (4.0 * phase_iter).cos()
            + 5.0 * fifth_harmonic_current_a * (5.0 * phase_iter).cos()
            + 6.0 * sixth_harmonic_current_a * (6.0 * phase_iter).cos())
            * josephson_gain
            * time_step_s;
        let total_g = shunt_g + cap_g + nonlinear_g;
        let current = shunt_g * v_iter
            + cap_g * (v_iter - v_prev)
            + critical_current_a * phase_iter.sin()
            + second_harmonic_current_a * (2.0 * phase_iter).sin()
            + third_harmonic_current_a * (3.0 * phase_iter).sin()
            + fourth_harmonic_current_a * (4.0 * phase_iter).sin()
            + fifth_harmonic_current_a * (5.0 * phase_iter).sin()
            + sixth_harmonic_current_a * (6.0 * phase_iter).sin();
        let current_eq = current - total_g * v_iter;

        stamp_conductance(matrix, pos, neg, total_g);
        stamp_current_injection(rhs, pos, neg, -current_eq);
    }
}

fn josephson_phase_from_history(
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
    pos: Option<usize>,
    neg: Option<usize>,
    current_time_s: f64,
    josephson_gain: f64,
) -> f64 {
    if solution_history.is_empty() {
        return 0.0;
    }

    let mut samples = solution_history
        .iter()
        .map(|(time_s, solution)| {
            (
                *time_s,
                node_voltage(solution, pos) - node_voltage(solution, neg),
            )
        })
        .collect::<Vec<_>>();
    samples.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut phase = 0.0;
    let mut last_time_s = samples[0].0;
    let mut last_voltage_v = samples[0].1;
    for (time_s, voltage_v) in samples.into_iter().skip(1) {
        let dt_s = (time_s - last_time_s).max(0.0);
        phase += josephson_gain * dt_s * 0.5 * (last_voltage_v + voltage_v);
        last_time_s = time_s;
        last_voltage_v = voltage_v;
    }

    if current_time_s > last_time_s {
        let dt_s = current_time_s - last_time_s;
        phase += josephson_gain * dt_s * last_voltage_v;
    }
    phase
}

fn delayed_port_voltage_delta(
    solution_history: &std::collections::VecDeque<(f64, Vec<f64>)>,
    pos: Option<usize>,
    neg: Option<usize>,
    previous_solution: &[f64],
    iterate_solution: &[f64],
    current_time_s: f64,
    delay_s: f64,
    time_step_s: f64,
) -> f64 {
    if delay_s <= 0.0 {
        return node_voltage(iterate_solution, pos) - node_voltage(iterate_solution, neg);
    }
    let current_delta = node_voltage(iterate_solution, pos) - node_voltage(iterate_solution, neg);
    let target_time_s = (current_time_s - delay_s).max(0.0);

    let mut samples = solution_history
        .iter()
        .map(|(time_s, solution)| {
            (
                *time_s,
                node_voltage(solution, pos) - node_voltage(solution, neg),
            )
        })
        .collect::<Vec<_>>();
    samples.push((current_time_s, current_delta));
    samples.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if target_time_s <= samples[0].0 {
        return samples[0].1;
    }

    for window in samples.windows(2) {
        let (left_time_s, left_value) = window[0];
        let (right_time_s, right_value) = window[1];
        if target_time_s <= right_time_s {
            let span_s = (right_time_s - left_time_s).max(time_step_s.max(f64::EPSILON));
            let alpha = ((target_time_s - left_time_s) / span_s).clamp(0.0, 1.0);
            return left_value + alpha * (right_value - left_value);
        }
    }

    node_voltage(previous_solution, pos) - node_voltage(previous_solution, neg)
}

fn stamp_conductance(
    matrix: &mut [Vec<f64>],
    pos: Option<usize>,
    neg: Option<usize>,
    conductance: f64,
) {
    if let Some(pos) = pos {
        matrix[pos][pos] += conductance;
    }
    if let Some(neg) = neg {
        matrix[neg][neg] += conductance;
    }
    if let (Some(pos), Some(neg)) = (pos, neg) {
        matrix[pos][neg] -= conductance;
        matrix[neg][pos] -= conductance;
    }
}

fn stamp_current_injection(
    rhs: &mut [f64],
    pos: Option<usize>,
    neg: Option<usize>,
    current_a: f64,
) {
    if let Some(pos) = pos {
        rhs[pos] += current_a;
    }
    if let Some(neg) = neg {
        rhs[neg] -= current_a;
    }
}

fn node_voltage(voltages: &[f64], node: Option<usize>) -> f64 {
    node.and_then(|index| voltages.get(index).copied())
        .unwrap_or(0.0)
}

fn solve_dense_linear_system(
    mut matrix: Vec<Vec<f64>>,
    mut rhs: Vec<f64>,
) -> Result<Vec<f64>, String> {
    let size = rhs.len();
    for pivot_index in 0..size {
        let mut pivot_row = pivot_index;
        let mut pivot_value = matrix[pivot_index][pivot_index].abs();
        for row in (pivot_index + 1)..size {
            let candidate = matrix[row][pivot_index].abs();
            if candidate > pivot_value {
                pivot_row = row;
                pivot_value = candidate;
            }
        }
        if pivot_value <= 1.0e-18 {
            return Err("internal_transient_singular_matrix".to_string());
        }
        if pivot_row != pivot_index {
            matrix.swap(pivot_row, pivot_index);
            rhs.swap(pivot_row, pivot_index);
        }

        let pivot = matrix[pivot_index][pivot_index];
        for row in (pivot_index + 1)..size {
            let factor = matrix[row][pivot_index] / pivot;
            if factor == 0.0 {
                continue;
            }
            for col in pivot_index..size {
                matrix[row][col] -= factor * matrix[pivot_index][col];
            }
            rhs[row] -= factor * rhs[pivot_index];
        }
    }

    let mut solution = vec![0.0; size];
    for row in (0..size).rev() {
        let mut value = rhs[row];
        for col in (row + 1)..size {
            value -= matrix[row][col] * solution[col];
        }
        solution[row] = value / matrix[row][row];
    }
    Ok(solution)
}

fn write_internal_transient_waveform(
    result: &InternalTransientResult,
    requested_path: Option<&str>,
) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (result, requested_path);
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};

        if result.captured_samples.is_empty() {
            return None;
        }

        let waveform_path = if let Some(path) = requested_path {
            PathBuf::from(path)
        } else {
            std::env::temp_dir().join(format!(
                "rflux-internal-{}-{}.csv",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_millis())
                    .unwrap_or_default()
            ))
        };

        let mut csv = String::from("time_ps");
        for node_name in &result.node_names {
            csv.push(',');
            csv.push_str(node_name);
        }
        csv.push('\n');

        for sample in &result.captured_samples {
            csv.push_str(&sample.time_ps.to_string());
            for voltage in &sample.node_voltages {
                csv.push(',');
                csv.push_str(&voltage.to_string());
            }
            csv.push('\n');
        }

        if let Some(parent) = waveform_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }

        fs::write(&waveform_path, csv)
            .ok()
            .map(|_| waveform_path.display().to_string())
    }
}

fn prefix_instance_name(raw_line: &str, prefix: &str) -> String {
    let mut tokens = raw_line
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if tokens.is_empty() || tokens[0].starts_with('.') {
        return raw_line.to_string();
    }
    tokens[0] = scoped_instance_name(&tokens[0], prefix);
    tokens.join(" ")
}

fn scoped_instance_name(name: &str, prefix: &str) -> String {
    if name.is_empty() {
        return prefix.to_string();
    }
    let (head, tail) = name.split_at(1);
    format!("{head}{prefix}__{tail}")
}

fn parse_param_line(line: &str, params: &mut BTreeMap<String, f64>) -> Result<(), SimulationError> {
    let normalized = line.replace(',', " ");
    let raw_tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let assignments = collapse_spaced_assignments(&raw_tokens);
    for assignment in assignments {
        let (name, expr) = assignment
            .split_once('=')
            .ok_or_else(|| SimulationError::InvalidParamAssignment(assignment.to_string()))?;
        let key = name.trim().to_ascii_lowercase();
        if key.is_empty() {
            return Err(SimulationError::InvalidParamAssignment(
                assignment.to_string(),
            ));
        }
        let value = evaluate_expression(expr.trim(), params)?;
        params.insert(key, value);
    }
    Ok(())
}

fn parse_tran_line(
    line: &str,
    params: &BTreeMap<String, f64>,
) -> Result<TransientAnalysis, SimulationError> {
    let normalized = line.replace(',', " ");
    let raw_tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let tokens = collapse_spaced_assignments(&raw_tokens);
    if tokens.len() < 2 {
        return Err(SimulationError::InvalidTran(line.to_string()));
    }

    let use_initial_conditions = tokens.iter().any(|token| token.eq_ignore_ascii_case("uic"));
    let time_tokens = tokens
        .iter()
        .filter(|token| !token.eq_ignore_ascii_case("uic"))
        .map(|token| token.as_str())
        .collect::<Vec<_>>();

    if time_tokens.iter().any(|token| token.contains('=')) {
        let mut tstep_ps = None;
        let mut tstop_ps = None;
        let mut prstart_ps = None;
        let mut prstep_ps = None;
        for token in &time_tokens {
            let Some((name, value_expr)) = token.split_once('=') else {
                return Err(SimulationError::InvalidTran(line.to_string()));
            };
            let value_ps = resolve_time_ps(value_expr.trim(), params)?;
            if name.eq_ignore_ascii_case("tstep") || name.eq_ignore_ascii_case("step") {
                tstep_ps = Some(value_ps);
                continue;
            }
            if name.eq_ignore_ascii_case("tstop") || name.eq_ignore_ascii_case("stop") {
                tstop_ps = Some(value_ps);
                continue;
            }
            if name.eq_ignore_ascii_case("prstart") || name.eq_ignore_ascii_case("tstart") {
                prstart_ps = Some(value_ps);
                continue;
            }
            if name.eq_ignore_ascii_case("prstep") || name.eq_ignore_ascii_case("tprint") {
                prstep_ps = Some(value_ps);
                continue;
            }
            return Err(SimulationError::InvalidTran(line.to_string()));
        }

        return Ok(TransientAnalysis {
            tstep_ps: tstep_ps.ok_or_else(|| SimulationError::InvalidTran(line.to_string()))?,
            tstop_ps: tstop_ps.ok_or_else(|| SimulationError::InvalidTran(line.to_string()))?,
            prstart_ps,
            prstep_ps,
            use_initial_conditions,
        });
    }

    if time_tokens.len() < 2 || time_tokens.len() > 4 {
        return Err(SimulationError::InvalidTran(line.to_string()));
    }

    let tstep_ps = resolve_time_ps(time_tokens[0], params)?;
    let tstop_ps = resolve_time_ps(time_tokens[1], params)?;
    let prstart_ps = if time_tokens.len() >= 3 {
        Some(resolve_time_ps(time_tokens[2], params)?)
    } else {
        None
    };
    let prstep_ps = if time_tokens.len() >= 4 {
        Some(resolve_time_ps(time_tokens[3], params)?)
    } else {
        None
    };

    Ok(TransientAnalysis {
        tstep_ps,
        tstop_ps,
        prstart_ps,
        prstep_ps,
        use_initial_conditions,
    })
}

fn resolve_time_ps(token: &str, params: &BTreeMap<String, f64>) -> Result<f64, SimulationError> {
    let value_seconds = evaluate_expression(token, params)?;
    Ok(value_seconds * 1.0e12)
}

fn evaluate_expression(expr: &str, params: &BTreeMap<String, f64>) -> Result<f64, SimulationError> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Err(SimulationError::UnsupportedExpression(expr.to_string()));
    }

    for operator in ['+', '-', '*', '/'] {
        if let Some(index) = find_binary_operator(expr, operator) {
            let left = evaluate_expression(&expr[..index], params)?;
            let right = evaluate_expression(&expr[index + 1..], params)?;
            return match operator {
                '+' => Ok(left + right),
                '-' => Ok(left - right),
                '*' => Ok(left * right),
                '/' => Ok(left / right),
                _ => unreachable!(),
            };
        }
    }

    resolve_atom(expr, params)
}

fn find_binary_operator(expr: &str, operator: char) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in expr.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth != 0 {
            continue;
        }
        if ch == operator {
            // Treat 1e-6 / 1E+3 as numeric literals rather than subtraction/addition.
            if matches!(operator, '+' | '-') {
                let prev = expr[..index].chars().next_back();
                if matches!(prev, Some('e' | 'E')) {
                    continue;
                }
            }
            return Some(index);
        }
    }
    None
}

fn resolve_atom(token: &str, params: &BTreeMap<String, f64>) -> Result<f64, SimulationError> {
    let token = token.trim();
    if token.starts_with('(') && token.ends_with(')') && token.len() >= 2 {
        return evaluate_expression(&token[1..token.len() - 1], params);
    }
    if let Some(param_name) = token
        .strip_prefix('{')
        .and_then(|inner| inner.strip_suffix('}'))
    {
        return params
            .get(&param_name.trim().to_ascii_lowercase())
            .copied()
            .ok_or_else(|| SimulationError::UnknownParameter(param_name.trim().to_string()));
    }
    if let Some(value) = params.get(&token.to_ascii_lowercase()) {
        return Ok(*value);
    }
    parse_engineering_number(token)
}

fn parse_engineering_number(token: &str) -> Result<f64, SimulationError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(SimulationError::InvalidNumericValue(token.to_string()));
    }

    let split_at = token
        .char_indices()
        .find(|(_, ch)| !matches!(ch, '0'..='9' | '.' | '+' | '-' | 'e' | 'E'))
        .map(|(index, _)| index)
        .unwrap_or(token.len());
    let (number_part, suffix_part) = token.split_at(split_at);
    let base = number_part
        .parse::<f64>()
        .map_err(|_| SimulationError::InvalidNumericValue(token.to_string()))?;
    let suffix = suffix_part.trim().to_ascii_lowercase();
    let scale = match suffix.as_str() {
        "" => 1.0,
        "t" => 1.0e12,
        "g" => 1.0e9,
        "meg" => 1.0e6,
        "k" => 1.0e3,
        "m" => 1.0e-3,
        "u" => 1.0e-6,
        "n" => 1.0e-9,
        "p" => 1.0e-12,
        "f" => 1.0e-15,
        _ => return Err(SimulationError::InvalidNumericValue(token.to_string())),
    };
    Ok(base * scale)
}

#[must_use]
fn write_internal_transient_vcd(
    result: &InternalTransientResult,
    requested_path: Option<&str>,
) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (result, requested_path);
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        if result.captured_samples.is_empty() || result.node_names.is_empty() {
            return None;
        }

        let vcd_path = if let Some(path) = requested_path {
            PathBuf::from(path)
        } else {
            std::env::temp_dir().join(format!(
                "rflux-internal-{}-{}.vcd",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_millis())
                    .unwrap_or_default()
            ))
        };

        // VCD header
        let mut vcd = String::from("$timescale 1 ps $end\n");
        vcd.push_str("$scope module top $end\n");

        // Assign single-char identifiers
        let mut identifiers: Vec<(String, char)> = Vec::new();
        for (i, name) in result.node_names.iter().enumerate() {
            let id = char::from_u32(33 + (i % 94) as u32).unwrap_or('!');
            identifiers.push((name.clone(), id));
            vcd.push_str(&format!("$var wire 1 {id} {name} $end\n"));
        }

        vcd.push_str("$upscope $end\n");
        vcd.push_str("$enddefinitions $end\n");
        vcd.push_str("$dumpvars\n");

        // Initial values (all 0)
        for (_, id) in &identifiers {
            vcd.push_str(&format!("0{id}\n"));
        }
        vcd.push_str("$end\n");

        // Track previous values to avoid redundant transitions
        let mut previous: Vec<bool> = vec![false; result.node_names.len()];

        // Convert analog voltages to digital with threshold
        for sample in &result.captured_samples {
            vcd.push_str(&format!("#{}\n", sample.time_ps as u64));
            for (node_index, voltage) in sample.node_voltages.iter().enumerate() {
                if node_index >= result.node_names.len() {
                    break;
                }
                let digital = *voltage > 1.25; // ~0.5 * 2.5V VDD threshold
                if digital != previous[node_index] {
                    let (_, id) = &identifiers[node_index];
                    if digital {
                        vcd.push_str(&format!("1{id}\n"));
                    } else {
                        vcd.push_str(&format!("0{id}\n"));
                    }
                    previous[node_index] = digital;
                }
            }
        }

        if let Some(parent) = vcd_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }

        fs::write(&vcd_path, vcd)
            .ok()
            .map(|()| vcd_path.display().to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::{
        apply_external_env_sanitization, create_external_run_dir, is_allowed_external_command,
        parse_deck, parse_deck_file, run_generated_deck, should_strip_external_env_var,
        simulate_file, simulate_text, ParsedDeck, SimulationBackend, SimulationConfig,
        SimulationMode, SimulationReport,
    };
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn event_only_mode_ignores_external_command() {
        let report = run_generated_deck(
            ".tran 1p 10p\n.end\n",
            5,
            &SimulationConfig {
                mode: SimulationMode::EventOnly,
                external_command: Some("__missing__".to_string()),
            },
        );

        assert_eq!(report.backend, SimulationBackend::EventOnly);
        assert_eq!(report.external_result, None);
        let gate = report.quality_gate();
        assert!(gate.passed);
        assert_eq!(gate.status, "passed");
        assert_eq!(gate.alignment_level, "event_only");
        assert!(!gate.external_alignment_required);
        assert!(!gate.external_alignment_available);
        let josim_gate = report.josim_quality_gate();
        assert!(!josim_gate.passed);
        assert_eq!(josim_gate.status, "failed_external_alignment_missing");
        assert!(josim_gate.external_alignment_required);
    }

    #[test]
    fn external_mode_rejects_non_allowlisted_command() {
        let report = run_generated_deck(
            ".tran 1p 10p\n.end\n",
            5,
            &SimulationConfig {
                mode: SimulationMode::ExternalJosim,
                external_command: Some("python".to_string()),
            },
        );

        assert_eq!(report.backend, SimulationBackend::ExternalUnavailable);
        assert_eq!(
            report.external_result.as_deref(),
            Some("external_command_not_allowed")
        );
        let gate = report.quality_gate();
        assert!(!gate.passed);
        assert_eq!(gate.status, "failed_backend");
        assert_eq!(gate.alignment_level, "unavailable");
    }

    #[test]
    fn auto_mode_rejects_non_allowlisted_command() {
        let report = run_generated_deck(
            ".tran 1p 10p\n.end\n",
            5,
            &SimulationConfig {
                mode: SimulationMode::Auto,
                external_command: Some("cmd.exe".to_string()),
            },
        );

        assert_eq!(report.backend, SimulationBackend::ExternalUnavailable);
        assert_eq!(
            report.external_result.as_deref(),
            Some("external_command_not_allowed")
        );
    }

    #[test]
    fn external_completed_report_passes_josim_quality_gate() {
        let report = SimulationReport {
            backend: SimulationBackend::ExternalCompleted,
            requested_mode: "external_josim".to_string(),
            simulated_events: 3,
            generated_deck_lines: 2,
            generated_deck_path: Some("input.sp".to_string()),
            waveform_path: Some("wave.csv".to_string()),
            waveform_format: Some("csv_v1".to_string()),
            external_summary_contract: Some("sim_v1".to_string()),
            diagnostic_code: None,
            reported_violations: 0,
            reported_worst_delay_ps: Some(8.0),
            delay_details: Vec::new(),
            measurement_details: Vec::new(),
            measurement_warnings: Vec::new(),
            violation_details: Vec::new(),
            external_status_code: Some(0),
            external_result: Some("ok".to_string()),
        };

        let gate = report.quality_gate();
        assert!(gate.passed);
        assert_eq!(gate.alignment_level, "josim_aligned");
        assert!(gate.external_alignment_available);
        let josim_gate = report.josim_quality_gate();
        assert!(josim_gate.passed);
        assert_eq!(josim_gate.required_backend, "external_josim");
    }

    #[test]
    fn external_completed_report_fails_gate_on_reported_violations() {
        let report = SimulationReport {
            backend: SimulationBackend::ExternalCompleted,
            requested_mode: "external_josim".to_string(),
            simulated_events: 3,
            generated_deck_lines: 2,
            generated_deck_path: Some("input.sp".to_string()),
            waveform_path: Some("wave.csv".to_string()),
            waveform_format: Some("csv_v1".to_string()),
            external_summary_contract: Some("sim_v1".to_string()),
            diagnostic_code: None,
            reported_violations: 1,
            reported_worst_delay_ps: Some(8.0),
            delay_details: Vec::new(),
            measurement_details: Vec::new(),
            measurement_warnings: Vec::new(),
            violation_details: Vec::new(),
            external_status_code: Some(0),
            external_result: Some("ok".to_string()),
        };

        let gate = report.josim_quality_gate();
        assert!(!gate.passed);
        assert_eq!(gate.status, "failed_violations");
        assert_eq!(gate.violation_count, 1);
    }

    #[test]
    fn external_mode_reports_spawn_failure_after_deck_write() {
        let missing_command = std::env::temp_dir()
            .join("rflux-missing-josim-command")
            .join(if cfg!(windows) { "josim.exe" } else { "josim" });
        let report = run_generated_deck(
            ".tran 1p 10p\n.end\n",
            5,
            &SimulationConfig {
                mode: SimulationMode::ExternalJosim,
                external_command: Some(missing_command.display().to_string()),
            },
        );

        assert_eq!(report.backend, SimulationBackend::ExternalUnavailable);
        assert!(report.generated_deck_path.is_some());
        assert_eq!(
            report.external_result.as_deref(),
            Some("external_command_spawn_failed")
        );
    }

    #[test]
    fn allowlist_accepts_known_josim_command_names_and_paths() {
        assert!(is_allowed_external_command("josim"));
        assert!(is_allowed_external_command("josim.exe"));
        assert!(is_allowed_external_command("josim-cli"));
        assert!(is_allowed_external_command("josim-cli.exe"));
        assert!(is_allowed_external_command("josim.cmd"));
        assert!(is_allowed_external_command("josim.bat"));
        assert!(is_allowed_external_command("josim.sh"));
        assert!(is_allowed_external_command(r"C:\tools\josim.exe"));
        assert!(is_allowed_external_command(r"C:\tools\josim.cmd"));
        assert!(is_allowed_external_command(
            r"C:\tools\JoSIM-v2.7-windows-x64\bin\josim-cli.exe"
        ));
        assert!(is_allowed_external_command("subdir/josim"));
        assert!(!is_allowed_external_command("python"));
        assert!(!is_allowed_external_command(r"C:\tools\python.exe"));
        assert!(!is_allowed_external_command("mock-sim.cmd"));
        assert!(!is_allowed_external_command(r"C:\tools\cmd.exe"));
    }

    #[test]
    fn external_env_sanitization_targets_rflow_and_josim_prefixes() {
        assert!(should_strip_external_env_var(OsStr::new(
            "RFLOW_JOSIM_COMMAND"
        )));
        assert!(should_strip_external_env_var(OsStr::new(
            "JOSIM_LICENSE_FILE"
        )));
        assert!(!should_strip_external_env_var(OsStr::new("PATH")));
        assert!(!should_strip_external_env_var(OsStr::new("HOME")));
    }

    #[test]
    fn external_command_builder_requests_explicit_waveform_output_path() {
        let command = super::build_external_simulator_command(
            "josim",
            std::path::Path::new(r"C:\temp\run\input.sp"),
            std::path::Path::new(r"C:\temp\run\external_output.csv"),
        );

        let rendered = format!("{command:?}");
        assert!(rendered.contains("-a"));
        assert!(rendered.contains("\"0\""));
        assert!(rendered.contains("-o"));
        assert!(rendered.contains("external_output.csv"));
        assert!(rendered.contains("input.sp"));
    }

    #[test]
    fn stage_external_run_artifacts_copies_outputs_and_cleans_run_dir() {
        let base_dir = unique_test_dir("external-stage-artifacts");
        let run_dir = base_dir.join("rflux-ext-1234-5678");
        fs::create_dir_all(&run_dir).unwrap();
        let deck_path = run_dir.join("input.sp");
        let waveform_path = run_dir.join("external_output.csv");
        fs::write(&deck_path, ".tran 1p 1p\n.end\n").unwrap();
        fs::write(&waveform_path, "time,voltage\n0,0\n").unwrap();

        let (generated_deck_path, staged_waveform_path) = super::stage_external_run_artifacts(
            &run_dir,
            &deck_path,
            Some(&waveform_path),
            1234,
            5678,
            true,
        );

        let staged_deck_path = Path::new(generated_deck_path.as_deref().unwrap());
        let staged_waveform_path = Path::new(staged_waveform_path.as_deref().unwrap());

        assert!(staged_deck_path.is_file());
        assert!(staged_waveform_path.is_file());
        assert!(!run_dir.exists());
        assert!(staged_deck_path
            .to_string_lossy()
            .contains("rflux-ext-1234-5678-input.sp"));
        assert!(staged_waveform_path
            .to_string_lossy()
            .contains("rflux-ext-1234-5678-external_output.csv"));

        let _ = fs::remove_dir_all(&base_dir);
        let _ = fs::remove_file(staged_deck_path);
        let _ = fs::remove_file(staged_waveform_path);
    }

    #[test]
    fn stage_external_run_artifacts_can_retain_run_dir_for_failure_review() {
        let base_dir = unique_test_dir("external-stage-retain");
        let run_dir = base_dir.join("rflux-ext-2234-6678");
        fs::create_dir_all(&run_dir).unwrap();
        let deck_path = run_dir.join("input.sp");
        let waveform_path = run_dir.join("external_output.csv");
        fs::write(&deck_path, ".tran 1p 1p\n.end\n").unwrap();
        fs::write(&waveform_path, "time,voltage\n0,0\n").unwrap();

        let (generated_deck_path, staged_waveform_path) = super::stage_external_run_artifacts(
            &run_dir,
            &deck_path,
            Some(&waveform_path),
            2234,
            6678,
            false,
        );

        let staged_deck_path = Path::new(generated_deck_path.as_deref().unwrap());
        let staged_waveform_path = Path::new(staged_waveform_path.as_deref().unwrap());

        assert!(staged_deck_path.is_file());
        assert!(staged_waveform_path.is_file());
        assert!(run_dir.exists());

        let _ = fs::remove_dir_all(&base_dir);
        let _ = fs::remove_file(staged_deck_path);
        let _ = fs::remove_file(staged_waveform_path);
    }

    #[test]
    fn prepare_external_simulator_deck_strips_title_card() {
        let prepared = super::prepare_external_simulator_deck(
            ".title demo\nR1 in out 1\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(!prepared.to_ascii_lowercase().contains(".title"));
        assert!(prepared.contains("R1 in out 1"));
        assert!(prepared.contains(".tran 1p 5p"));
    }

    #[test]
    fn prepare_external_simulator_deck_strips_params_markers() {
        let prepared = super::prepare_external_simulator_deck(
            ".subckt child out params: rval=10\nX1 out child params: rval=10\n.end\n",
            None,
        );

        assert!(!prepared.to_ascii_lowercase().contains("params:"));
        assert!(prepared.contains(".subckt child out  rval=10"));
        assert!(prepared.contains("X1 out child  rval=10"));
    }

    #[test]
    fn prepare_external_simulator_deck_flattens_parametrized_subckt_instances() {
        let prepared = super::prepare_external_simulator_deck(
            ".subckt child out params: rval=10\nR1 out 0 rval\n.ends\nX1 n1 child params: rval=12\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(!prepared.to_ascii_lowercase().contains(".subckt"));
        assert!(!prepared.contains("X1 n1 child"));
        assert!(prepared.contains("12"));
    }

    #[test]
    fn prepare_external_simulator_deck_inlines_file_driven_pwl_sources() {
        let dir = unique_test_dir("external-inline-pwl");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("wave.txt"), "0 0\n1p 0.2m\n3p 0\n").unwrap();

        let prepared = super::prepare_external_simulator_deck(
            "V1 in 0 PWL(file=\"wave.txt\")\n.tran 1p 5p\n.end\n",
            Some(&dir),
        );

        assert!(prepared.contains("PWL(0.000000000000e0"));
        assert!(!prepared.to_ascii_lowercase().contains("file="));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_keyword_exp_source_to_positional_form() {
        let prepared = super::prepare_external_simulator_deck(
            "V1 in 0 EXP(v1=0 v2=1m td1=1p tau1=0.5p td2=4p tau2=0.5p)\n.tran 1p 6p\n.end\n",
            None,
        );

        assert!(prepared.contains("V1 in 0 EXP(0 1m 1p 0.5p 4p 0.5p)"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_keyword_pulse_source_to_positional_form() {
        let prepared = super::prepare_external_simulator_deck(
            "V1 in 0 PULSE(v1 = 0 v2 = 1m td = 1p tr = 0.2p tf = 0.2p pw = 2p per = 4p ncycles = 2)\n.tran 0.5p 10p\n.end\n",
            None,
        );

        assert!(prepared.contains("V1 in 0 PULSE(0 1m 1p 0.2p 0.2p 2p 4p 2)"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_keyword_sin_source_to_positional_form() {
        let prepared = super::prepare_external_simulator_deck(
            "V1 in 0 SIN(vo=0 va=1m freq=100g td=0 theta=300g phi=90)\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains("V1 in 0 SIN(0 1m 100g 0 300g 90)"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_josephson_prefix_for_josim() {
        let prepared =
            super::prepare_external_simulator_deck("J1 n1 0 jjmod\n.tran 1p 5p\n.end\n", None);

        assert!(prepared.contains("B1 n1 0 jjmod"));
        assert!(!prepared.contains("J1 n1 0 jjmod"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_inline_junction_parameters_into_model_card() {
        let prepared = super::prepare_external_simulator_deck(
            "J1 n1 0 icrit=0.5m rn=20 cj=0.5p\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model rflux_auto_j1 jj(icrit=0.5m rn=20 cap=0.5p)"));
        assert!(prepared.contains("B1 n1 0 rflux_auto_j1"));
        assert!(!prepared.contains("B1 n1 0 icrit=0.5m rn=20 cj=0.5p"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_junction_model_cj_alias_for_josim() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p)"));
        assert!(!prepared.contains("cj=0.5p"));
        assert!(prepared.contains("B1 n1 0 jjmod"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_junction_model_keyword_reference_for_josim() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nJ1 n1 0 model=jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains("B1 n1 0 jjmod"));
        assert!(!prepared.contains("B1 n1 0 model=jjmod"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_junction_model_pi_into_negative_icrit() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p pi=1 icrit2=0.2m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=-0.5m rn=20 cap=0.5p cpr={1,-0.2m/0.5m})"));
        assert!(!prepared.to_ascii_lowercase().contains("pi=1"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit2=0.2m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_nonzero_numeric_pi_flag_into_negative_icrit() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p pi=-2)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=-0.5m rn=20 cap=0.5p)"));
        assert!(!prepared.to_ascii_lowercase().contains("pi=-2"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_second_harmonic_into_cpr() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p icrit2=0.2m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p cpr={1,0.2m/0.5m})"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit2=0.2m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_third_harmonic_into_cpr() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p icrit3=0.05m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(
            prepared.contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p cpr={1,0,0.05m/0.5m})")
        );
        assert!(!prepared.to_ascii_lowercase().contains("icrit3=0.05m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_fourth_harmonic_into_cpr() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p icrit4=0.01m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(
            prepared.contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p cpr={1,0,0,0.01m/0.5m})")
        );
        assert!(!prepared.to_ascii_lowercase().contains("icrit4=0.01m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_fifth_harmonic_into_cpr() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p icrit5=0.005m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared
            .contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p cpr={1,0,0,0,0.005m/0.5m})"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit5=0.005m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_sixth_harmonic_into_cpr() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p icrit6=0.001m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared
            .contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p cpr={1,0,0,0,0,0.001m/0.5m})"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit6=0.001m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_pure_third_harmonic_model_into_cpr_basis() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(rn=20 cj=0.5p icrit3=0.05m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=0.05m rn=20 cap=0.5p cpr={0,0,1})"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit3=0.05m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_pure_second_harmonic_model_into_cpr_basis() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(rn=20 cj=0.5p icrit2=0.2m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=0.2m rn=20 cap=0.5p cpr={0,1})"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit2=0.2m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_pure_fourth_harmonic_model_into_cpr_basis() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(rn=20 cj=0.5p icrit4=0.01m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=0.01m rn=20 cap=0.5p cpr={0,0,0,1})"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit4=0.01m"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_pure_fifth_harmonic_model_into_cpr_basis() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(rn=20 cj=0.5p icrit5=0.005m)\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model jjmod jj(icrit=0.005m rn=20 cap=0.5p cpr={0,0,0,0,1})"));
        assert!(!prepared.to_ascii_lowercase().contains("icrit5=0.005m"));
    }

    #[test]
    fn prepare_external_simulator_deck_preserves_native_cpr_model_card() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p cpr={1,0.2,0.05,0.01})\nJ1 n1 0 jjmod\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains("cpr={"));
        assert!(!prepared.to_ascii_lowercase().contains("cpr={1,0.2,0.05,0.01}" ) || prepared.contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p cpr={1,0.2,0.05,0.01})") || prepared.contains(".model jjmod jj(icrit=0.5m rn=20 cap=0.5p cpr={1,(0.2)*(0.5m)/(0.5m),(0.05)*(0.5m)/(0.5m),(0.01)*(0.5m)/(0.5m)})"));
    }

    #[test]
    fn prepare_external_simulator_deck_preserves_native_cpr_instance_override() {
        let prepared = super::prepare_external_simulator_deck(
            "J1 n1 0 icrit=0.5m rn=20 cj=0.5p cpr={1,0.2,0.05,0.01}\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared
            .contains(".model rflux_auto_j1 jj(icrit=0.5m rn=20 cap=0.5p cpr={1,0.2,0.05,0.01})"));
        assert!(prepared.contains("B1 n1 0 rflux_auto_j1"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_pure_second_harmonic_instance_override_into_synthetic_model(
    ) {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(rn=20 cj=0.5p)\nJ1 n1 0 model=jjmod ic2=0.1m\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model rflux_auto_j1 jj(icrit=0.1m rn=20 cap=0.5p cpr={0,1})"));
        assert!(prepared.contains("B1 n1 0 rflux_auto_j1"));
    }

    #[test]
    fn prepare_external_simulator_deck_merges_instance_pi_override_into_synthetic_model() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nJ1 n1 0 model=jjmod pi=1\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model rflux_auto_j1 jj(icrit=-0.5m rn=20 cap=0.5p)"));
        assert!(prepared.contains("B1 n1 0 rflux_auto_j1"));
    }

    #[test]
    fn prepare_external_simulator_deck_instance_pi_override_replaces_model_pi_default() {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p pi=1)\nJ1 n1 0 model=jjmod pi=0\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model rflux_auto_j1 jj(icrit=0.5m rn=20 cap=0.5p)"));
        assert!(prepared.contains("B1 n1 0 rflux_auto_j1"));
    }

    #[test]
    fn prepare_external_simulator_deck_merges_supported_junction_model_override_into_synthetic_model(
    ) {
        let prepared = super::prepare_external_simulator_deck(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nJ1 n1 0 model=jjmod rn=25\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains(".model rflux_auto_j1 jj(icrit=0.5m rn=25 cap=0.5p)"));
        assert!(prepared.contains("B1 n1 0 rflux_auto_j1"));
        assert!(!prepared.contains("B1 n1 0 model=jjmod rn=25"));
    }

    #[test]
    fn collect_external_translation_notes_reports_only_remaining_unsupported_junction_features() {
        let notes = super::collect_external_translation_notes(
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p pi=1 icrit2=0.2m)\nJ1 n1 0 model=jjmod rn=25 pi=1\n.tran 1p 5p\n.end\n",
        );

        assert!(!notes.contains(
            &"external_josim_translation_warning:jj_second_harmonic_model_unsupported".to_string()
        ));
        assert!(!notes.contains(
            &"external_josim_translation_warning:jj_second_harmonic_instance_unsupported"
                .to_string()
        ));
        assert!(!notes
            .contains(&"external_josim_translation_warning:jj_pi_model_unsupported".to_string()));
        assert!(!notes.contains(
            &"external_josim_translation_warning:jj_pi_instance_unsupported".to_string()
        ));
        assert!(!notes.contains(
            &"external_josim_translation_warning:jj_model_override_unsupported".to_string()
        ));
    }

    #[test]
    fn collect_external_translation_notes_accepts_pure_second_harmonic_without_primary_icrit() {
        let notes = super::collect_external_translation_notes(
            ".model jjmod jj(icrit2=0.2m rn=20)\nJ1 n1 0 ic2=0.1m rn=25\n.tran 1p 5p\n.end\n",
        );

        assert!(!notes.contains(
            &"external_josim_translation_warning:jj_second_harmonic_model_unsupported".to_string()
        ));
        assert!(!notes.contains(
            &"external_josim_translation_warning:jj_second_harmonic_instance_unsupported"
                .to_string()
        ));
    }

    #[test]
    fn combine_external_result_with_notes_appends_translation_warnings() {
        let combined = super::combine_external_result_with_notes(
            Some("ok".to_string()),
            &["external_josim_translation_warning:jj_model_override_unsupported".to_string()],
        );

        assert_eq!(
            combined.as_deref(),
            Some("ok;external_josim_translation_warning:jj_model_override_unsupported")
        );
    }

    #[test]
    fn parse_external_simulator_stderr_warnings_reports_unknown_model_parameter() {
        let stderr = "W: Model\nUnknown model parameter specified.\nThe parameter:  ICRIT2 \nContinuing with default model parameters.\n";

        let warnings = super::parse_external_simulator_stderr_warnings(stderr);

        assert_eq!(
            warnings,
            vec!["external_josim_runtime_warning:unknown_model_parameter:icrit2".to_string()]
        );
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_mutual_coupling_keyword_for_josim() {
        let prepared = super::prepare_external_simulator_deck(
            "K1 L1 L2 coupling=0.9\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains("K1 L1 L2 0.9"));
        assert!(!prepared.contains("coupling=0.9"));
    }

    #[test]
    fn prepare_external_simulator_deck_rewrites_spaced_mutual_coupling_keyword_for_josim() {
        let prepared = super::prepare_external_simulator_deck(
            "K1 L1 L2 coupling = 0.9\n.tran 1p 5p\n.end\n",
            None,
        );

        assert!(prepared.contains("K1 L1 L2 0.9"));
        assert!(!prepared.contains("coupling = 0.9"));
    }

    #[test]
    fn prepare_external_simulator_deck_strips_tran_uic_for_josim() {
        let prepared = super::prepare_external_simulator_deck(".tran 0.5p 10p uic\n.end\n", None);

        assert!(prepared.contains(".tran 0.5p 10p"));
        assert!(!prepared.to_ascii_lowercase().contains("uic"));
    }

    #[test]
    fn external_env_sanitization_reports_removed_names() {
        let mut command = Command::new("josim");
        let removed = apply_external_env_sanitization(
            &mut command,
            vec![
                OsString::from("PATH"),
                OsString::from("RFLOW_JOSIM_COMMAND"),
                OsString::from("JOSIM_LICENSE_FILE"),
                OsString::from("RFLOW_JOSIM_COMMAND"),
            ],
        );

        assert_eq!(
            removed,
            vec![
                "JOSIM_LICENSE_FILE".to_string(),
                "RFLOW_JOSIM_COMMAND".to_string(),
            ]
        );
    }

    #[test]
    fn external_run_dir_is_created_as_dedicated_subdirectory() {
        let base_dir = std::env::temp_dir().join(format!(
            "rflux-sim-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default()
        ));
        fs::create_dir(&base_dir).unwrap();

        let run_dir = create_external_run_dir(&base_dir, 1234, 5678).unwrap();

        assert!(run_dir.exists());
        assert!(run_dir.is_dir());
        assert_eq!(
            run_dir.file_name().and_then(|name| name.to_str()),
            Some("rflux-ext-1234-5678")
        );

        fs::remove_dir_all(&base_dir).unwrap();
    }

    #[test]
    fn internal_transient_mode_reports_unavailable() {
        let report = run_generated_deck(
            ".tran 1p 10p\n.end\n",
            5,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        );

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert_eq!(report.simulated_events, 0);
    }

    #[test]
    fn parse_deck_resolves_params_and_tran_times() {
        let parsed = parse_deck(
            "* demo\n.title demo\n.param tstep=0.5p tstop=20p scale=2\nV1 in 0 DC 1m\n.tran {tstep} tstop {tstep} tstop/scale\n.end\n",
        )
        .unwrap();

        assert_eq!(
            parsed,
            ParsedDeck {
                title: Some("demo".to_string()),
                params: [
                    ("scale".to_string(), 2.0),
                    ("tstep".to_string(), 0.5e-12),
                    ("tstop".to_string(), 20.0e-12),
                ]
                .into_iter()
                .collect(),
                transient: super::TransientAnalysis {
                    tstep_ps: 0.5,
                    tstop_ps: 20.0,
                    prstart_ps: Some(0.5),
                    prstep_ps: Some(10.0),
                    use_initial_conditions: false,
                },
                element_count: 1,
                control_count: 4,
            }
        );
    }

    #[test]
    fn parse_deck_supports_scientific_notation_params() {
        let parsed = parse_deck(
            ".title demo\n.param tstep=1e-12 tstop=5e-12\nR1 n1 0 50\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 5.0);
    }

    #[test]
    fn parse_deck_supports_spaced_and_comma_param_assignments() {
        let parsed = parse_deck(
            ".title demo\n.param tstep = 1p, tstop=5p, scale = 2\nR1 n1 0 50\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 5.0);
        assert_eq!(parsed.params.get("scale").copied(), Some(2.0));
    }

    #[test]
    fn parse_deck_supports_spice_continuation_lines() {
        let parsed = parse_deck(
            ".title demo\n.param tstep=0.5p\n+ tstop=20p, scale=2\nR1 n1 0 50\n.tran 1p\n+ 10p uic\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.title.as_deref(), Some("demo"));
        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
        assert_eq!(parsed.params.get("tstep").copied(), Some(0.5e-12));
        assert_eq!(parsed.params.get("tstop").copied(), Some(20.0e-12));
        assert_eq!(parsed.params.get("scale").copied(), Some(2.0));
        assert!(parsed.transient.use_initial_conditions);
        assert_eq!(parsed.element_count, 1);
    }

    #[test]
    fn parse_deck_rejects_invalid_spaced_param_assignment() {
        let err =
            parse_deck(".title demo\n.param tstep =\nR1 n1 0 50\n.tran 1p 5p\n.end\n").unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::InvalidParamAssignment("tstep".to_string())
        );
    }

    #[test]
    fn parse_deck_supports_tran_uic_flag() {
        let parsed = parse_deck(".title demo\nR1 n1 0 50\n.tran 1p 10p 2p 2p uic\n.end\n").unwrap();

        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
        assert_eq!(parsed.transient.prstart_ps, Some(2.0));
        assert_eq!(parsed.transient.prstep_ps, Some(2.0));
        assert!(parsed.transient.use_initial_conditions);
    }

    #[test]
    fn parse_deck_supports_keyword_tran_fields() {
        let parsed = parse_deck(
            ".title demo\nR1 n1 0 50\n.tran tstop=10p tstep=1p prstart=2p prstep=2p\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
        assert_eq!(parsed.transient.prstart_ps, Some(2.0));
        assert_eq!(parsed.transient.prstep_ps, Some(2.0));
        assert!(!parsed.transient.use_initial_conditions);
    }

    #[test]
    fn parse_deck_supports_keyword_tran_fields_with_spaced_equals_and_uic() {
        let parsed = parse_deck(
            ".title demo\nR1 n1 0 50\n.tran tstep = 1p tstop = 10p tstart = 2p tprint = 2p uic\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
        assert_eq!(parsed.transient.prstart_ps, Some(2.0));
        assert_eq!(parsed.transient.prstep_ps, Some(2.0));
        assert!(parsed.transient.use_initial_conditions);
    }

    #[test]
    fn parse_deck_accepts_uppercase_control_cards() {
        let parsed = parse_deck(
            ".TITLE demo\n.PARAM tstep=1p, tstop=5p\nR1 n1 0 50\n.TRAN tstep tstop\n.END\n",
        )
        .unwrap();

        assert_eq!(parsed.title.as_deref(), Some("demo"));
        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 5.0);
    }

    #[test]
    fn parse_deck_rejects_mixed_keyword_and_positional_tran_fields() {
        let err = parse_deck(".title demo\nR1 n1 0 50\n.tran tstep=1p 10p\n.end\n").unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::InvalidTran("tstep=1p 10p".to_string())
        );
    }

    #[test]
    fn internal_transient_applies_ic_when_uic_is_present() {
        let result = super::run_internal_transient(
            ".title demo\n.ic V(out)=1\nR1 out 0 1\nC1 out 0 1p\n.tran 1p 2p uic\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!((result.captured_samples[0].node_voltages[out_index] - 1.0).abs() < 1.0e-9);
        assert!(result.final_node_voltages[out_index] < 1.0);
    }

    #[test]
    fn internal_transient_applies_ic_with_spaced_assignment_syntax() {
        let result = super::run_internal_transient(
            ".title demo\n.ic V(out) = 1\nR1 out 0 1\nC1 out 0 1p\n.tran 1p 2p uic\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!((result.captured_samples[0].node_voltages[out_index] - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn internal_transient_uses_nodeset_as_startup_hint_without_uic() {
        let result = super::run_internal_transient(
            ".title demo\n.nodeset V(out)=1\nR1 out 0 1\nC1 out 0 1p\n.tran 1p 2p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!((result.captured_samples[0].node_voltages[out_index] - 1.0).abs() < 1.0e-9);
        assert!(result.final_node_voltages[out_index] < 1.0);
    }

    #[test]
    fn internal_transient_supports_nodeset_with_spaced_and_comma_assignments() {
        let result = super::run_internal_transient(
            ".title demo\n.nodeset V(out) = 1, V(in) = 0.5\nV1 in 0 DC 0\nR1 in out 1\nC1 out 0 1p\n.tran 1p 2p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let in_index = result
            .node_names
            .iter()
            .position(|name| name == "in")
            .unwrap();
        assert!((result.captured_samples[0].node_voltages[out_index] - 1.0).abs() < 1.0e-9);
        assert!((result.captured_samples[0].node_voltages[in_index] - 0.5).abs() < 1.0e-9);
    }

    #[test]
    fn internal_transient_prefers_ic_over_nodeset_when_uic_is_present() {
        let result = super::run_internal_transient(
            ".title demo\n.nodeset V(out)=1\n.ic V(out)=2\nR1 out 0 1\nC1 out 0 1p\n.tran 1p 2p uic\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!((result.captured_samples[0].node_voltages[out_index] - 2.0).abs() < 1.0e-9);
    }

    #[test]
    fn simulate_text_uses_direct_deck_api() {
        let report = simulate_text(
            ".title demo\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.tran tstep tstop\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::EventOnly,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(report.backend, SimulationBackend::EventOnly);
        assert_eq!(report.simulated_events, 1);
        assert_eq!(report.generated_deck_lines, 5);
    }

    #[test]
    fn parse_deck_flattens_subckt_with_param_override() {
        let parsed = parse_deck(
            ".subckt stage in out rval=50\nR1 in out rval\n.ends\nX1 n1 n2 stage rval=75\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
    }

    #[test]
    fn parse_deck_supports_params_marker_in_subckt_and_instance() {
        let parsed = parse_deck(
            ".subckt stage in out params: rval=50\nR1 in out rval\n.ends\nX1 n1 n2 stage params: rval=75\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
    }

    #[test]
    fn parse_deck_supports_nested_subckt_param_passthrough() {
        let deck = ".subckt leaf in out params: rval=50\nR1 in out rval\n.ends\n".to_string()
            + ".subckt stage in out params: stage_r=75\n"
            + "Xleaf in out leaf params: rval=stage_r\n"
            + ".ends\n"
            + "X1 n1 n2 stage params: stage_r=90\n"
            + ".tran 1p 10p\n"
            + ".end\n";
        let parsed = parse_deck(&deck).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
    }

    #[test]
    fn parse_deck_supports_nested_subckt_definition() {
        let parsed = parse_deck(
            ".subckt outer in out\n.subckt inner a b\nR1 a b 50\n.ends inner\nXinner in out inner\n.ends outer\nX1 n1 n2 outer\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 1.0);
        assert_eq!(parsed.transient.tstop_ps, 10.0);
    }

    #[test]
    fn parse_deck_resolves_shadowed_nested_subckt_name() {
        let flattened = super::flatten_subckts(
            ".subckt leaf a b\nRtop a b 100\n.ends\n.subckt outer in out\n.subckt leaf a b\nRinner a b 50\n.ends\nXlocal in out leaf\n.ends\nX1 n1 n2 outer\nX2 n3 n4 leaf\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        assert!(flattened.contains("X1__Xlocal"));
        assert!(flattened.contains("50"));
        assert!(flattened.contains("100"));
    }

    #[test]
    fn parse_deck_rejects_duplicate_subckt_name_in_same_scope() {
        let err = parse_deck(
            ".subckt stage in out\nR1 in out 50\n.ends\n.subckt stage in out\nR2 in out 75\n.ends\nX1 n1 n2 stage\n.tran 1p 10p\n.end\n",
        )
        .unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::DuplicateSubcktDefinition {
                scope: "<top-level>".to_string(),
                name: "stage".to_string(),
            }
        );
    }

    #[test]
    fn parse_deck_reports_extra_tokens_after_subckt_name() {
        let err = parse_deck(
            ".subckt stage in out rval=50\nR1 in out rval\n.ends\nX1 n1 n2 stage extra rval=75\n.tran 1p 10p\n.end\n",
        )
        .unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::UnsupportedSubcktInstanceSyntax(
                "X1 n1 n2 stage extra rval=75".to_string()
            )
        );
    }

    #[test]
    fn parse_deck_file_resolves_include() {
        let dir = unique_test_dir("include-parse");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(&inc_path, ".param tstep=0.5p tstop=20p\nR1 n1 0 50\n").unwrap();
        fs::write(
            &top_path,
            ".title demo\n.include \"defs.inc\"\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.title.as_deref(), Some("demo"));
        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive() {
        let dir = unique_test_dir("lib-parse");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".title demo\n.lib \"defs.inc\" TT\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.title.as_deref(), Some("demo"));
        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_reports_missing_lib_section() {
        let dir = unique_test_dir("lib-missing-section");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\n.endl TT\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" FF\n.tran 1p 10p\n.end\n").unwrap();

        let err = parse_deck_file(&top_path).unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::MissingLibrarySection {
                path: inc_path.display().to_string(),
                section: "FF".to_string(),
            }
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_reports_unterminated_lib_section() {
        let dir = unique_test_dir("lib-unterminated-section");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(&inc_path, ".lib TT\n.param tstep=0.5p tstop=20p\n").unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran 1p 10p\n.end\n").unwrap();

        let err = parse_deck_file(&top_path).unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::UnterminatedLibrarySection {
                path: inc_path.display().to_string(),
                section: "TT".to_string(),
            }
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_single_quote_and_case_insensitive_section() {
        let dir = unique_test_dir("lib-parse-case-quote");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib 'defs.inc' tt\n.tran tstep tstop\n.end\n").unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_reports_mismatched_lib_section_end_name() {
        let dir = unique_test_dir("lib-mismatched-endl");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\n.endl FF\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran 1p 10p\n.end\n").unwrap();

        let err = parse_deck_file(&top_path).unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::MismatchedLibrarySectionEnd {
                path: inc_path.display().to_string(),
                expected: "TT".to_string(),
                found: "FF".to_string(),
            }
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_quoted_section_tokens() {
        let dir = unique_test_dir("lib-parse-quoted-sections");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib \"TT\"\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl \"TT\"\n.lib \"FF\"\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl \"FF\"\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\" \"TT\"\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_reports_mismatched_lib_section_end_name_with_quoted_token() {
        let dir = unique_test_dir("lib-mismatched-endl-quoted");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\n.endl \"FF\"\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran 1p 10p\n.end\n").unwrap();

        let err = parse_deck_file(&top_path).unwrap_err();

        assert_eq!(
            err,
            super::SimulationError::MismatchedLibrarySectionEnd {
                path: inc_path.display().to_string(),
                expected: "TT".to_string(),
                found: "FF".to_string(),
            }
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_comma_separated_section() {
        let dir = unique_test_dir("lib-parse-comma-section");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\", TT\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_section_with_comma_delimited_lib_and_endl_tokens() {
        let dir = unique_test_dir("lib-parse-comma-delimited-section-tokens");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT,\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT,\n.lib FF,\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF,\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran tstep tstop\n.end\n").unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_inline_semicolon_comment_tokens() {
        let dir = unique_test_dir("lib-parse-semicolon-inline-comment");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT;body\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT;body-end\n.lib FF;body\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF;body-end\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\" TT;select\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_section_equals_keyword() {
        let dir = unique_test_dir("lib-parse-section-equals-keyword");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\" section=TT\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_block_with_section_equals_headers() {
        let dir = unique_test_dir("lib-parse-section-equals-headers");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib section=TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl section=TT\n.lib section=FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl section=FF\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran tstep tstop\n.end\n").unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_section_spaced_equals_keyword() {
        let dir = unique_test_dir("lib-parse-section-spaced-equals-keyword");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\" section = TT\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_block_with_section_spaced_equals_headers() {
        let dir = unique_test_dir("lib-parse-section-spaced-equals-headers");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib section = TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl section = TT\n.lib section = FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl section = FF\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran tstep tstop\n.end\n").unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_sec_spaced_equals_keyword() {
        let dir = unique_test_dir("lib-parse-sec-spaced-equals-keyword");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\" sec = TT\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_section_equals_and_spaced_value() {
        let dir = unique_test_dir("lib-parse-section-equals-spaced-value");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\" section= TT\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_directive_with_sec_equals_attached_value() {
        let dir = unique_test_dir("lib-parse-sec-equals-attached-value");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl TT\n.lib FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl FF\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".lib \"defs.inc\" sec =TT\n.tran tstep tstop\n.end\n",
        )
        .unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_block_with_sec_equals_headers() {
        let dir = unique_test_dir("lib-parse-sec-equals-headers");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib sec=TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl sec=TT\n.lib sec=FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl sec=FF\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran tstep tstop\n.end\n").unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_block_with_section_equals_and_spaced_value_headers() {
        let dir = unique_test_dir("lib-parse-section-equals-spaced-value-headers");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib section= TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl section= TT\n.lib section= FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl section= FF\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran tstep tstop\n.end\n").unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_deck_file_resolves_lib_block_with_sec_equals_attached_value_headers() {
        let dir = unique_test_dir("lib-parse-sec-equals-attached-value-headers");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".lib sec =TT\n.param tstep=0.5p tstop=20p\nR1 n1 0 50\n.endl sec =TT\n.lib sec =FF\n.param tstep=1p tstop=10p\nR2 n2 0 75\n.endl sec =FF\n",
        )
        .unwrap();
        fs::write(&top_path, ".lib \"defs.inc\" TT\n.tran tstep tstop\n.end\n").unwrap();

        let parsed = parse_deck_file(&top_path).unwrap();

        assert_eq!(parsed.element_count, 1);
        assert_eq!(parsed.transient.tstep_ps, 0.5);
        assert_eq!(parsed.transient.tstop_ps, 20.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_uses_included_content() {
        let dir = unique_test_dir("include-sim");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(&inc_path, ".param tstep=0.5p tstop=20p\nR1 n1 0 50\n").unwrap();
        fs::write(&top_path, ".include defs.inc\n.tran tstep tstop\n.end\n").unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::EventOnly,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(report.backend, SimulationBackend::EventOnly);
        assert_eq!(report.simulated_events, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_subckt() {
        let dir = unique_test_dir("include-subckt");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".subckt stage in out rval=50\nR1 in out rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 n1 n2 stage rval=75\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::EventOnly,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(report.backend, SimulationBackend::EventOnly);
        assert_eq!(report.simulated_events, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_nested_subckt_definition() {
        let dir = unique_test_dir("include-nested-subckt");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".subckt outer in out\n.subckt inner a b\nR1 a b 50\n.ends inner\nXinner in out inner\n.ends outer\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 n1 n2 outer\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::EventOnly,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(report.backend, SimulationBackend::EventOnly);
        assert_eq!(report.simulated_events, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_subckt_with_params_marker() {
        let dir = unique_test_dir("include-subckt-params-marker");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".subckt stage in out params: rval=50\nR1 in out rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 n1 n2 stage params: rval=75\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::EventOnly,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(report.backend, SimulationBackend::EventOnly);
        assert_eq!(report.simulated_events, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_junction_subckt_with_model_card() {
        let dir = unique_test_dir("include-junction-subckt-model-card");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt jj_stage in out params: rdrive=10\nR1 in n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nX1 in 0 jj_stage params: rdrive=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_file_driven_source_junction_subckt() {
        let dir = unique_test_dir("include-source-pwl-junction-subckt");
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("wave.txt"),
            "0 0\n1p 0.8m\n3p 1.8m\n4p 1.8m\n6p 0\n8p 0\n",
        )
        .unwrap();
        fs::write(
            &inc_path,
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt source_pwl_jj_stage out params: rdrive=10\nV1 in 0 PWL(file=\"wave.txt\")\nR1 in n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 0 source_pwl_jj_stage params: rdrive=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_delayed_t_junction_subckt() {
        let dir = unique_test_dir("include-delay-junction-subckt");
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &inc_path,
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt delay_jj_stage out params: z0=50 td=3p rdrive=10\nV1 src 0 PULSE(0,2m,0,1p,1p,2p,8p)\nT1 src 0 mid 0 z0=z0 td=td\nR1 mid n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 0 delay_jj_stage params: z0=50 td=3p rdrive=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_chained_include_delayed_t_junction_subckt() {
        let dir = unique_test_dir("chained-include-delay-junction-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt delay_jj_leaf out params: z0=50 td=3p rdrive=10\nV1 src 0 PULSE(0,2m,0,1p,1p,2p,8p)\nT1 src 0 mid 0 z0=z0 td=td\nR1 mid n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt delay_jj_stage out params: stage_z0=50 stage_td=3p stage_r=10\nXleaf out delay_jj_leaf params: z0=stage_z0 td=stage_td rdrive=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 0 delay_jj_stage params: stage_z0=50 stage_td=3p stage_r=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_included_source_bearing_mutual_inductance_junction_subckt() {
        let dir = unique_test_dir("include-source-mutual-inductance-junction-subckt");
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &inc_path,
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt source_coupled_jj_stage tap params: coupling=0.9 lval=1p rval=1\nV1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out n1 rval\nJ1 n1 0 jjmod\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 tap source_coupled_jj_stage params: coupling=0.9 lval=1p rval=1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_file_driven_source_mutual_inductance_junction_subckt() {
        let dir = unique_test_dir("include-source-pwl-mutual-inductance-junction-subckt");
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        let wave_path = dir.join("wave.txt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &wave_path,
            "0 0\n1p 0.2m\n2p 0.5m\n3p 0.8m\n5p 0.8m\n6p 0.4m\n8p 0\n",
        )
        .unwrap();
        fs::write(
            &inc_path,
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt source_pwl_coupled_jj_stage tap params: coupling=0.9 lval=1p rval=10\nV1 in 0 PWL(file=\"wave.txt\")\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out n1 rval\nJ1 n1 0 jjmod\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 tap source_pwl_coupled_jj_stage params: coupling=0.9 lval=1p rval=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_delayed_file_driven_source_mutual_inductance_junction_subckt(
    ) {
        let dir = unique_test_dir("include-delay-source-pwl-mutual-inductance-junction-subckt");
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        let wave_path = dir.join("wave.txt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &wave_path,
            "0 0\n1p 0.2m\n2p 0.5m\n3p 0.8m\n5p 0.8m\n6p 0.4m\n8p 0\n",
        )
        .unwrap();
        fs::write(
            &inc_path,
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt delay_source_pwl_coupled_jj_stage tap params: z0=50 td=3p coupling=0.9 lval=1p rval=10\nV1 src 0 PWL(file=\"wave.txt\")\nT1 src 0 mid 0 z0=z0 td=td\nK1 L1 L2 coupling=coupling\nL1 mid out lval\nR1 out n1 rval\nJ1 n1 0 jjmod\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 tap delay_source_pwl_coupled_jj_stage params: z0=50 td=3p coupling=0.9 lval=1p rval=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_chained_include_file_driven_source_mutual_inductance_junction_subckt()
    {
        let dir = unique_test_dir("chained-include-source-pwl-mutual-inductance-junction-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("wave.txt"),
            "0 0\n1p 0.2m\n2p 0.5m\n3p 0.8m\n5p 0.8m\n6p 0.4m\n8p 0\n",
        )
        .unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt source_pwl_coupled_jj_leaf out tap params: coupling=0.9 lval=1p rval=10\nV1 in 0 PWL(file=\"wave.txt\")\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out n1 rval\nJ1 n1 0 jjmod\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt source_pwl_coupled_jj_stage out tap params: stage_k=0.9 stage_l=1p stage_r=10\nXleaf out tap source_pwl_coupled_jj_leaf params: coupling=stage_k lval=stage_l rval=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 0 tap source_pwl_coupled_jj_stage params: stage_k=0.9 stage_l=1p stage_r=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_chained_include_delayed_file_driven_source_mutual_inductance_junction_subckt(
    ) {
        let dir =
            unique_test_dir("chained-include-delay-source-pwl-mutual-inductance-junction-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("wave.txt"),
            "0 0\n1p 0.2m\n2p 0.5m\n3p 0.8m\n5p 0.8m\n6p 0.4m\n8p 0\n",
        )
        .unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt delay_source_pwl_coupled_jj_leaf out tap params: z0=50 td=3p coupling=0.9 lval=1p rval=10\nV1 src 0 PWL(file=\"wave.txt\")\nT1 src 0 mid 0 z0=z0 td=td\nK1 L1 L2 coupling=coupling\nL1 mid out lval\nR1 out n1 rval\nJ1 n1 0 jjmod\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt delay_source_pwl_coupled_jj_stage out tap params: stage_z0=50 stage_td=3p stage_k=0.9 stage_l=1p stage_r=10\nXleaf out tap delay_source_pwl_coupled_jj_leaf params: z0=stage_z0 td=stage_td coupling=stage_k lval=stage_l rval=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 0 tap delay_source_pwl_coupled_jj_stage params: stage_z0=50 stage_td=3p stage_k=0.9 stage_l=1p stage_r=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_chained_include_source_bearing_mutual_inductance_junction_subckt() {
        let dir = unique_test_dir("chained-include-source-mutual-inductance-junction-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt source_coupled_jj_leaf out tap params: coupling=0.9 lval=1p rval=1\nV1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out n1 rval\nJ1 n1 0 jjmod\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt source_coupled_jj_stage out tap params: stage_k=0.9 stage_l=1p stage_r=1\nXleaf out tap source_coupled_jj_leaf params: coupling=stage_k lval=stage_l rval=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 0 tap source_coupled_jj_stage params: stage_k=0.9 stage_l=1p stage_r=1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_junction_subckt_with_model_card() {
        let dir = unique_test_dir("chained-include-junction-subckt-model-card");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt jj_leaf in out params: rdrive=10\nR1 in n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt jj_stage in out params: stage_r=10\nXleaf in out jj_leaf params: rdrive=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nX1 in 0 jj_stage params: stage_r=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_source_bearing_junction_subckt() {
        let dir = unique_test_dir("chained-include-source-junction-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt source_jj_leaf out params: rdrive=10\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt source_jj_stage out params: stage_r=10\nXleaf out source_jj_leaf params: rdrive=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 0 source_jj_stage params: stage_r=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_file_driven_source_junction_subckt() {
        let dir = unique_test_dir("chained-include-source-pwl-junction-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf_wave.txt"),
            "0 0\n1p 0.8m\n3p 1.8m\n4p 1.8m\n6p 0\n8p 0\n",
        )
        .unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt source_pwl_jj_leaf out params: rdrive=10\nV1 in 0 PWL(file=\"leaf_wave.txt\")\nR1 in n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt source_pwl_jj_stage out params: stage_r=10\nXleaf out source_pwl_jj_leaf params: rdrive=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 0 source_pwl_jj_stage params: stage_r=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_delayed_t_with_file_driven_source_junction_subckt() {
        let dir = unique_test_dir("chained-include-delay-source-pwl-junction-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf_wave.txt"),
            "0 0\n1p 0.8m\n3p 1.8m\n4p 1.8m\n6p 0\n8p 0\n",
        )
        .unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\n\n.subckt delay_source_jj_leaf out params: z0=50 td=3p rdrive=10\nV1 src 0 PWL(file=\"leaf_wave.txt\")\nT1 src 0 mid 0 z0=z0 td=td\nR1 mid n1 rdrive\nJ1 n1 out jjmod\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt delay_source_jj_stage out params: stage_z0=50 stage_td=3p stage_r=10\nXleaf out delay_source_jj_leaf params: z0=stage_z0 td=stage_td rdrive=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 0 delay_source_jj_stage params: stage_z0=50 stage_td=3p stage_r=10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_sixth_harmonic_junction() {
        let dir = unique_test_dir("sixth-harmonic-junction");
        fs::create_dir_all(&dir).unwrap();
        let deck_path = dir.join("deck.cir");
        fs::write(
            &deck_path,
            ".title demo\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 out icrit=0.5m ic6=0.02m rn=20 cj=0.5p\nR2 out 0 10\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &deck_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_subckt_with_mutual_inductance() {
        let dir = unique_test_dir("include-subckt-mutual-inductance");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".subckt coupled_stage in out tap params: coupling=0.9 lval=1p rval=1\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out 0 rval\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nV1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\nX1 in out tap coupled_stage params: coupling=0.9 lval=1p rval=1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_file_supports_included_source_bearing_subckt_with_mutual_inductance() {
        let dir = unique_test_dir("include-source-subckt-mutual-inductance");
        fs::create_dir_all(&dir).unwrap();
        let inc_path = dir.join("defs.inc");
        let top_path = dir.join("top.cir");
        fs::write(
            &inc_path,
            ".subckt source_coupled_stage out tap params: coupling=0.9 lval=1p rval=1\nV1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out 0 rval\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            &top_path,
            ".include defs.inc\nX1 out tap source_coupled_stage params: coupling=0.9 lval=1p rval=1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            &top_path,
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.waveform_path.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_text_internal_transient_completes_for_passive_source_only_deck() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert_eq!(report.simulated_events, 5);
        assert_eq!(report.reported_worst_delay_ps, Some(0.001));
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_text_internal_transient_accepts_dc_equals_source_form() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC=1m AC=0\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.reported_worst_delay_ps, Some(0.001));
    }

    #[test]
    fn simulate_text_internal_transient_accepts_dc_spaced_equals_source_form() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC = 1m AC = 0\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.reported_worst_delay_ps, Some(0.001));
    }

    #[test]
    fn simulate_text_internal_transient_accepts_keyword_resistor_value() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC 1m\nR1 in out resistance=50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.reported_worst_delay_ps, Some(0.001));
    }

    #[test]
    fn simulate_text_internal_transient_accepts_keyword_capacitor_value() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 capacitance = 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.reported_worst_delay_ps, Some(0.001));
    }

    #[test]
    fn simulate_text_internal_transient_accepts_keyword_inductor_value() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC 1m\nL1 in out inductance = 1p\nR1 out 0 50\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert!(report.reported_worst_delay_ps.is_some());
    }

    #[test]
    fn simulate_text_internal_transient_accepts_space_separated_keyword_resistor_value() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC 1m\nR1 in out resistance 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.reported_worst_delay_ps, Some(0.001));
    }

    #[test]
    fn simulate_text_internal_transient_accepts_space_separated_keyword_capacitor_value() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 capacitance 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.reported_worst_delay_ps, Some(0.001));
    }

    #[test]
    fn simulate_text_internal_transient_accepts_space_separated_keyword_inductor_value() {
        let report = simulate_text(
            ".title demo\nV1 in 0 DC 1m\nL1 in out inductance 1p\nR1 out 0 50\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert!(report.reported_worst_delay_ps.is_some());
    }

    #[test]
    fn simulate_text_internal_transient_reports_option_seed() {
        let report = simulate_text(
            ".title demo\n.param base_seed=123\n.option seed = {base_seed}\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=123")
        );
    }

    #[test]
    fn simulate_text_internal_transient_reports_option_seed_from_comma_options() {
        let report = simulate_text(
            ".title demo\n.option method=trap, seed=456, reltol=1e-3\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=456")
        );
    }

    #[test]
    fn simulate_text_internal_transient_keeps_seed_across_other_option_lines() {
        let report = simulate_text(
            ".title demo\n.option seed=321\n.option method=gear\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=321")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_reltol_and_abstol_options() {
        let report = simulate_text(
            ".title demo\n.option method=trap, reltol=1e-4, abstol = 1e-6\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_reltol_and_abstol_aliases() {
        let report = simulate_text(
            ".title demo\n.option rel=1e-4, abserr = 1e-6\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_space_separated_option_pairs() {
        let report = simulate_text(
            ".title demo\n.option reltol 1e-4 abstol 1e-6 itl4 12 seed 123\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=123")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_option_reltol() {
        let report = simulate_text(
            ".title demo\n.option reltol=0\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_reltol")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_option_abstol() {
        let report = simulate_text(
            ".title demo\n.option abstol=-1m\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_abstol")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_option_abstol_alias() {
        let report = simulate_text(
            ".title demo\n.option abs=0\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_abstol")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_space_separated_option_pair() {
        let report = simulate_text(
            ".title demo\n.option reltol 0\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_reltol")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_option_tnoise() {
        let report = simulate_text(
            ".title demo\n.option seed=123 tnoise=1u\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=123")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_option_tnoise() {
        let report = simulate_text(
            ".title demo\n.option tnoise=-1u\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_noise")
        );
    }

    #[test]
    fn internal_transient_noise_is_reproducible_with_same_seed() {
        let deck =
            ".title demo\n.option seed=42 tnoise=1m\nV1 in 0 DC 1\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n";
        let first = super::run_internal_transient(deck).unwrap();
        let second = super::run_internal_transient(deck).unwrap();

        assert_eq!(first.final_node_voltages, second.final_node_voltages);
    }

    #[test]
    fn internal_transient_noise_differs_for_different_seed() {
        let deck_a =
            ".title demo\n.option seed=100 tnoise=1m\nV1 in 0 DC 1\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n";
        let deck_b =
            ".title demo\n.option seed=101 tnoise=1m\nV1 in 0 DC 1\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n";
        let result_a = super::run_internal_transient(deck_a).unwrap();
        let result_b = super::run_internal_transient(deck_b).unwrap();

        let out_index = result_a
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(
            (result_a.final_node_voltages[out_index] - result_b.final_node_voltages[out_index])
                .abs()
                > 1.0e-12
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_option_temp_and_tnom() {
        let report = simulate_text(
            ".title demo\n.option seed=77 tnoise=1u temp=60 tnom=27\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=77")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_option_aliases_for_noise_and_temperature() {
        let report = simulate_text(
            ".title demo\n.option seed=77 noise_sigma=1u temperature=60 nominal_temperature=27\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=77")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_kelvin_option_temperature_aliases() {
        let report = simulate_text(
            ".title demo\n.option seed=77 sigma=1u temperature_k=333.15 nominal_temperature_k=300.15\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=77")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_celsius_option_temperature_aliases() {
        let report = simulate_text(
            ".title demo\n.option seed=77 sigma=1u tempc=60 tnomc=27\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=77")
        );
    }

    #[test]
    fn simulate_text_internal_transient_writes_waveform_to_option_csvout_path() {
        let dir = unique_test_dir("option-csvout");
        fs::create_dir_all(&dir).unwrap();
        let target_path = dir.join("wave.csv");
        let report = simulate_text(
            &format!(
                ".title demo\n.option csvout=\"{}\"\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
                target_path.display()
            ),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.waveform_path.as_deref(),
            Some(target_path.display().to_string().as_str())
        );
        let content = fs::read_to_string(&target_path).unwrap();
        assert!(content.starts_with("time_ps"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_text_internal_transient_rejects_empty_option_csvout_path() {
        let report = simulate_text(
            ".title demo\n.option csvout=\"\"\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_waveform_path")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_option_temp() {
        let report = simulate_text(
            ".title demo\n.option temp=-300\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_temp")
        );
    }

    #[test]
    fn internal_transient_noise_scales_with_temperature_ratio() {
        let baseline =
            super::run_internal_transient(
                ".title demo\n.option seed=17 tnoise=1m temp=27 tnom=27\nV1 in 0 DC 1\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
            )
            .unwrap();
        let hotter =
            super::run_internal_transient(
                ".title demo\n.option seed=17 tnoise=1m temp=327 tnom=27\nV1 in 0 DC 1\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
            )
            .unwrap();
        let noiseless =
            super::run_internal_transient(
                ".title demo\n.option seed=17\nV1 in 0 DC 1\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
            )
            .unwrap();

        let out_index = baseline
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let baseline_delta = (baseline.final_node_voltages[out_index]
            - noiseless.final_node_voltages[out_index])
            .abs();
        let hotter_delta = (hotter.final_node_voltages[out_index]
            - noiseless.final_node_voltages[out_index])
            .abs();
        assert!(hotter_delta > baseline_delta);
    }

    #[test]
    fn internal_transient_resistor_noise_is_reproducible_with_same_seed() {
        let deck =
            ".title demo\n.option seed=23 tnoise=1u\nR1 out 0 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n";
        let first = super::run_internal_transient(deck).unwrap();
        let second = super::run_internal_transient(deck).unwrap();

        assert_eq!(first.final_node_voltages, second.final_node_voltages);
    }

    #[test]
    fn internal_transient_resistor_noise_changes_solution_vs_no_noise() {
        let noisy = super::run_internal_transient(
            ".title demo\n.option seed=23 tnoise=1u\nR1 out 0 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();
        let noiseless = super::run_internal_transient(
            ".title demo\n.option seed=23\nR1 out 0 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();

        let out_index = noisy
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(
            (noisy.final_node_voltages[out_index] - noiseless.final_node_voltages[out_index]).abs()
                > 1.0e-12
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_option_itl4_and_maxiters() {
        let report = simulate_text(
            ".title demo\n.option itl4=12, maxiters = 10\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_option_itl_aliases() {
        let report = simulate_text(
            ".title demo\n.option itl=11, itl1=10\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
    }

    #[test]
    fn simulate_text_internal_transient_accepts_options_card_name() {
        let report = simulate_text(
            ".title demo\n.options reltol=1e-4, abstol=1e-6, maxiters=9\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
    }

    #[test]
    fn simulate_text_internal_transient_reports_measurement_details() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PULSE(0,1m,0,1p,1p,2p,6p)\nR1 in out 1\nC1 out 0 1p\n.measure tran out_peak max V(out)\n.measure tran out_pp pp V(out)\n.measure tran out_rms rms V(out)\n.measure tran out_final final V(out)\n.tran 1p 6p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert!(report.delay_details.is_empty());
        assert_eq!(report.measurement_details.len(), 4);
        assert_eq!(report.measurement_details[0].name, "out_peak");
        assert_eq!(report.measurement_details[0].kind, "max");
        assert_eq!(
            report.measurement_details[0]
                .at_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("out")
        );
        assert!(report.measurement_details[0].measured_value > 0.0);
        assert_eq!(report.measurement_details[1].name, "out_pp");
        assert_eq!(report.measurement_details[1].kind, "peak_to_peak");
        assert!(
            report.measurement_details[1].measured_value
                >= report.measurement_details[0].measured_value
        );
        assert_eq!(report.measurement_details[2].name, "out_rms");
        assert_eq!(report.measurement_details[2].kind, "rms");
        assert!(report.measurement_details[2].measured_value > 0.0);
        assert_eq!(report.measurement_details[3].name, "out_final");
        assert_eq!(report.measurement_details[3].kind, "final");
        assert!(report.measurement_details[3].measured_value.is_finite());
    }

    #[test]
    fn simulate_text_internal_transient_measurement_details_honor_time_windows() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 3p 1m 6p 0)\nR1 in out 1\nC1 out 0 1p\n.measure tran early_final final V(out) FROM = 0p TO=3p\n.measure tran full_final final V(out)\n.tran 1p 6p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.measurement_details.len(), 2);
        assert_eq!(report.measurement_details[0].name, "early_final");
        assert_eq!(report.measurement_details[0].kind, "final");
        assert_eq!(report.measurement_details[1].name, "full_final");
        assert_eq!(report.measurement_details[1].kind, "final");
        assert!(
            report.measurement_details[0].measured_value
                > report.measurement_details[1].measured_value
        );
    }

    #[test]
    fn simulate_text_internal_transient_measurement_details_support_differential_voltage() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 6p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran diff_final final V(in,out)\n.tran 0.5p 6p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.measurement_details.len(), 1);
        assert_eq!(report.measurement_details[0].name, "diff_final");
        assert_eq!(
            report.measurement_details[0]
                .at_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("in,out")
        );
        assert!(report.measurement_details[0].measured_value.is_finite());
    }

    #[test]
    fn simulate_text_internal_transient_measurement_details_support_find_at() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 6p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran out_at find V(out) AT=2.5p\n.measure tran diff_at find V(in,out) AT=2.5p\n.tran 0.5p 6p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.measurement_details.len(), 2);
        assert_eq!(report.measurement_details[0].name, "out_at");
        assert_eq!(report.measurement_details[0].kind, "find");
        assert!(report.measurement_details[0].measured_value > 0.0);
        assert_eq!(report.measurement_details[1].name, "diff_at");
        assert_eq!(
            report.measurement_details[1]
                .at_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("in,out")
        );
        assert!(report.measurement_details[1].measured_value.is_finite());
    }

    #[test]
    fn simulate_text_internal_transient_measurement_details_support_find_when() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 6p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran out_when find V(out) WHEN V(in)=0.5m RISE=1\n.measure tran diff_when find V(in,out) WHEN V(in,out)=0.2m RISE=1\n.tran 0.5p 6p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.measurement_details.len(), 2);
        assert_eq!(report.measurement_details[0].name, "out_when");
        assert_eq!(report.measurement_details[0].kind, "find");
        assert!(report.measurement_details[0].measured_value.is_finite());
        assert_eq!(report.measurement_details[1].name, "diff_when");
        assert_eq!(
            report.measurement_details[1]
                .at_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("in,out")
        );
        assert!(report.measurement_details[1].measured_value.is_finite());
    }

    #[test]
    fn simulate_text_internal_transient_reports_measurement_warnings() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 6p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran missing find V(out) WHEN V(in)=2m RISE=1\n.tran 0.5p 6p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert!(report.measurement_details.is_empty());
        assert_eq!(report.measurement_warnings.len(), 1);
        assert_eq!(report.measurement_warnings[0].name, "missing");
        assert_eq!(report.measurement_warnings[0].kind, "find");
        assert_eq!(
            report.measurement_warnings[0].reason,
            "measurement_crossing_not_found"
        );
        assert_eq!(
            report.measurement_warnings[0]
                .at_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("in")
        );
    }

    #[test]
    fn simulate_text_internal_transient_reports_delay_measurement_details() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 8p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran rc_delay TRIG V(in)=0.5m RISE=1 TARG V(out)=0.25m RISE=1\n.tran 0.5p 8p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.delay_details.len(), 1);
        assert_eq!(report.delay_details[0].name, "rc_delay");
        assert_eq!(
            report.delay_details[0]
                .from_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("in")
        );
        assert_eq!(
            report.delay_details[0]
                .to_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("out")
        );
        assert!(report.delay_details[0].delay_ps > 0.0);
        assert_eq!(
            report.reported_worst_delay_ps,
            Some(report.delay_details[0].delay_ps)
        );
    }

    #[test]
    fn simulate_text_internal_transient_delay_measurements_support_differential_voltage() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 8p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran diff_delay TRIG V(in,out) VAL=0.2m RISE=1 TARG V(out) VAL=0.25m RISE=1\n.tran 0.5p 8p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.delay_details.len(), 1);
        assert_eq!(report.delay_details[0].name, "diff_delay");
        assert_eq!(
            report.delay_details[0]
                .from_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("in,out")
        );
        assert_eq!(
            report.delay_details[0]
                .to_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("out")
        );
        assert!(report.delay_details[0].delay_ps > 0.0);
    }

    #[test]
    fn simulate_text_internal_transient_delay_measurements_support_fall_cross_and_td() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 3p 1m 4p 0 8p 0)\nR1 in out 1\nC1 out 0 1p\n.measure tran fall_delay TRIG V(in) VAL=0.5m FALL=1 TARG V(out) VAL=0.25m FALL=1\n.measure tran gated_cross TRIG V(in) VAL=0.5m CROSS=1 TD=2p TARGET V(out) VAL=0.25m FALL=1 TD=2p\n.tran 0.5p 8p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.delay_details.len(), 2);
        assert_eq!(report.delay_details[0].name, "fall_delay");
        assert!(report.delay_details[0].delay_ps > 0.0);
        assert_eq!(report.delay_details[1].name, "gated_cross");
        assert!(report.delay_details[1].delay_ps > 0.0);
    }

    #[test]
    fn simulate_text_internal_transient_delay_measurements_support_last_crossing() {
        let report = simulate_text(
            ".title demo\nV1 in 0 PWL(0 0 1p 1m 2p 1m 3p 0 8p 0 9p 1m 14p 1m)\nR1 in out 1\nC1 out 0 1p\n.measure tran last_rise_delay TRIG V(in) VAL=0.5m RISE=LAST TARG V(out) VAL=0.25m RISE=LAST\n.tran 0.5p 14p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(report.delay_details.len(), 1);
        assert_eq!(report.delay_details[0].name, "last_rise_delay");
        assert!(report.delay_details[0].delay_ps > 0.0);
    }

    #[test]
    fn simulate_text_internal_transient_accepts_uppercase_option_cards() {
        let report = simulate_text(
            ".title demo\n.OPTION seed=42\n.OPTIONS RELTOL 1e-4 ABSTOL 1e-6\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc;seed=42")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_option_maxiters() {
        let report = simulate_text(
            ".title demo\n.option maxiters=0\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_max_iterations")
        );
    }

    #[test]
    fn simulate_text_internal_transient_keeps_maxiters_across_other_option_lines() {
        let report = simulate_text(
            ".title demo\n.option maxiters=9\n.option method=gear\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientCompleted
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
    }

    #[test]
    fn simulate_text_internal_transient_rejects_invalid_option_seed() {
        let report = simulate_text(
            ".title demo\n.option seed=-1\nV1 in 0 DC 1m\nR1 in out 50\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_invalid_option_seed")
        );
    }

    #[test]
    fn simulate_text_internal_transient_stays_unavailable_for_unsupported_elements() {
        let report = simulate_text(
            ".title demo\nJ1 n1 n2 model\n.tran 1p 5p\n.end\n",
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.backend,
            SimulationBackend::InternalTransientUnavailable
        );
        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_unsupported_element:J")
        );
    }

    #[test]
    fn internal_transient_solver_tracks_rc_charge() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 5);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let out_voltage = result.final_node_voltages[out_index];
        let expected = 1.0 - (-5.0_f64).exp();
        assert!((out_voltage - expected).abs() < 1.0e-2);
        assert!((result.max_abs_voltage_v - 1.0).abs() < 1.0e-9);
        assert_eq!(result.captured_steps, 6);
        assert_eq!(result.captured_samples.len(), 6);
    }

    #[test]
    fn internal_transient_supports_pulse_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,1,0,1p,1p,2p,6p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 6);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples.len() >= 2);
        let peak_out_voltage = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[out_index])
            .fold(0.0_f64, f64::max);
        assert!(peak_out_voltage > 0.4);
        assert!(result.final_node_voltages[out_index] < peak_out_voltage);
    }

    #[test]
    fn internal_transient_supports_one_shot_pulse_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,1,0,1p,1p,2p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let peak_out_voltage = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[out_index])
            .fold(0.0_f64, f64::max);
        assert!(peak_out_voltage > 0.4);
        assert!(result.final_node_voltages[out_index] < peak_out_voltage);
    }

    #[test]
    fn internal_transient_supports_finite_cycle_pulse_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,1,0,1p,1p,2p,4p,2)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let peak_out_voltage = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[out_index])
            .fold(0.0_f64, f64::max);
        assert!(peak_out_voltage > 0.4);
        assert!(result.final_node_voltages[out_index] < 0.2);
    }

    #[test]
    fn internal_transient_supports_finite_cycle_pulse_with_ncycles_keyword() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,1,0,1p,1p,2p,4p,ncycles=2)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.final_node_voltages[out_index] < 0.2);
    }

    #[test]
    fn internal_transient_supports_finite_cycle_pulse_with_spaced_ncycles_keyword() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,1,0,1p,1p,2p,4p,ncycles = 2)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.final_node_voltages[out_index] < 0.2);
    }

    #[test]
    fn internal_transient_supports_keyword_pulse_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(v1=0,v2=1,td=0,tr=1p,tf=1p,pw=2p,per=6p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let peak_out_voltage = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[out_index])
            .fold(0.0_f64, f64::max);
        assert!(peak_out_voltage > 0.4);
        assert!(result.final_node_voltages[out_index] < peak_out_voltage);
    }

    #[test]
    fn internal_transient_supports_keyword_pulse_with_spaced_equals_and_ncycles() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(v1 = 0 v2 = 1 td = 0 tr = 1p tf = 1p pw = 2p per = 4p ncycles = 2)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.final_node_voltages[out_index] < 0.2);
    }

    #[test]
    fn internal_transient_supports_keyword_pulse_with_cycles_alias() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(low=0 high=1 delay=0 rise=1p fall=1p width=2p period=4p cycles=2)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 10p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.final_node_voltages[out_index] < 0.2);
    }

    #[test]
    fn internal_transient_supports_pwl_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PWL(0,0,2p,1,4p,0)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 5);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.0);
        assert!(
            result.captured_samples[3].node_voltages[out_index]
                > result.captured_samples[4].node_voltages[out_index]
        );
    }

    #[test]
    fn internal_transient_supports_whitespace_separated_pwl_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PWL(0 0 2p 1 4p 0)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 5);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.0);
        assert!(
            result.captured_samples[3].node_voltages[out_index]
                > result.captured_samples[4].node_voltages[out_index]
        );
    }

    #[test]
    fn internal_transient_supports_file_driven_pwl_voltage_sources() {
        let dir = unique_test_dir("pwl-file-source");
        fs::create_dir_all(&dir).unwrap();
        let waveform_path = dir.join("wave.txt");
        fs::write(&waveform_path, "0 0\n2p 1\n4p 0\n").unwrap();
        let deck = format!(
            ".title demo\nV1 in 0 PWL(file=\"{}\")\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
            waveform_path.display()
        );

        let result = super::run_internal_transient(&deck).unwrap();

        assert_eq!(result.simulated_steps, 5);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.0);
        assert!(
            result.captured_samples[3].node_voltages[out_index]
                > result.captured_samples[4].node_voltages[out_index]
        );
    }

    #[test]
    fn simulate_file_resolves_relative_file_driven_pwl_sources_from_deck_directory() {
        let dir = unique_test_dir("pwl-file-source-relative");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("wave.txt"), "0 0\n2p 1\n4p 0\n").unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\nV1 in 0 PWL(file=\"wave.txt\")\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_resolves_relative_pwl_paths_from_included_file_directory() {
        let dir = unique_test_dir("pwl-file-source-in-include-dir");
        let include_dir = dir.join("cells");
        fs::create_dir_all(&include_dir).unwrap();
        fs::write(include_dir.join("driver_wave.txt"), "0 0\n2p 1\n4p 0\n").unwrap();
        fs::write(
            include_dir.join("driver.inc"),
            ".subckt driver out\nV1 src 0 PWL(file=\"driver_wave.txt\")\nR1 src out 1\nC1 out 0 1p\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"cells/driver.inc\"\nX1 out driver\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_included_pwl_source_through_delayed_t_subckt() {
        let dir = unique_test_dir("pwl-file-source-with-delay-in-include-dir");
        let include_dir = dir.join("cells");
        fs::create_dir_all(&include_dir).unwrap();
        fs::write(
            include_dir.join("driver_wave.txt"),
            "0 0\n1p 1\n3p 1\n4p 0\n8p 0\n",
        )
        .unwrap();
        fs::write(
            include_dir.join("driver.inc"),
            ".subckt delay_driver out params: z0=50 td=3p rval=50\nV1 src 0 PWL(file=\"driver_wave.txt\")\nT1 src 0 out 0 z0=z0 td=td\nR1 out 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"cells/driver.inc\"\nX1 out delay_driver params: z0=50 td=3p rval=50\n.tran 0.5p 10p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_delayed_t_subckt() {
        let dir = unique_test_dir("chained-include-delay-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".subckt delay_leaf in out params: z0=50 td=3p rval=50\nT1 in 0 out 0 z0=z0 td=td\nR1 out 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt delay_stage in out params: stage_z0=50 stage_td=3p stage_r=50\nXleaf in out delay_leaf params: z0=stage_z0 td=stage_td rval=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nV1 in 0 PULSE(0 1m 0 0.2p 0.2p 2p 6p)\nX1 in out delay_stage params: stage_z0=50 stage_td=3p stage_r=50\n.tran 0.5p 10p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_delayed_t_with_file_driven_source() {
        let dir = unique_test_dir("chained-include-delay-source-pwl-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("leaf_wave.txt"), "0 0\n1p 1\n3p 1\n4p 0\n8p 0\n").unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".subckt delay_source_leaf out params: z0=50 td=3p rval=50\nV1 src 0 PWL(file=\"leaf_wave.txt\")\nT1 src 0 out 0 z0=z0 td=td\nR1 out 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt delay_source_stage out params: stage_z0=50 stage_td=3p stage_r=50\nXleaf out delay_source_leaf params: z0=stage_z0 td=stage_td rval=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 out delay_source_stage params: stage_z0=50 stage_td=3p stage_r=50\n.tran 0.5p 10p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_mutual_inductance_subckt() {
        let dir = unique_test_dir("chained-include-mutual-inductance-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".subckt coupled_leaf in out tap params: coupling=0.9 lval=1p rval=1\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out 0 rval\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt coupled_stage in out tap params: stage_k=0.9 stage_l=1p stage_r=1\nXleaf in out tap coupled_leaf params: coupling=stage_k lval=stage_l rval=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nV1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\nX1 in out tap coupled_stage params: stage_k=0.9 stage_l=1p stage_r=1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn simulate_file_supports_chained_include_source_bearing_mutual_inductance_subckt() {
        let dir = unique_test_dir("chained-include-source-mutual-inductance-subckt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("leaf.inc"),
            ".subckt source_coupled_leaf out tap params: coupling=0.9 lval=1p rval=1\nV1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)\nK1 L1 L2 coupling=coupling\nL1 in out lval\nR1 out 0 rval\nL2 tap 0 lval\nR2 tap 0 rval\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("defs.inc"),
            ".include \"leaf.inc\"\n\n.subckt source_coupled_stage out tap params: stage_k=0.9 stage_l=1p stage_r=1\nXleaf out tap source_coupled_leaf params: coupling=stage_k lval=stage_l rval=stage_r\n.ends\n",
        )
        .unwrap();
        fs::write(
            dir.join("deck.cir"),
            ".title demo\n.include \"defs.inc\"\nX1 out tap source_coupled_stage params: stage_k=0.9 stage_l=1p stage_r=1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let report = simulate_file(
            dir.join("deck.cir"),
            &SimulationConfig {
                mode: SimulationMode::InternalTransient,
                external_command: None,
            },
        )
        .unwrap();

        assert_eq!(
            report.external_result.as_deref(),
            Some("internal_transient_linear_rc")
        );
        assert!(report.simulated_events > 0);
        assert!(report.waveform_path.is_some());
    }

    #[test]
    fn internal_transient_reports_missing_pwl_waveform_file() {
        let err = super::run_internal_transient(
            ".title demo\nV1 in 0 PWL(file=\"missing-waveform-source.txt\")\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap_err();

        assert!(err.contains("internal_transient_waveform_file_read_failed"));
    }

    #[test]
    fn resolve_waveform_source_path_does_not_duplicate_pre_rewritten_relative_paths() {
        let base_dir = std::path::Path::new("python/tests/benchmarks/phase6");
        let raw_path = "python/tests/benchmarks/phase6/include_source_pwl_jj_wave.txt";

        let resolved = super::resolve_waveform_source_path(Some(base_dir), raw_path);

        assert_eq!(resolved, raw_path);
    }

    #[test]
    fn internal_transient_supports_pwl_repeated_time_breakpoints_as_steps() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PWL(0,0,2p,0,2p,1,4p,1)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[2].node_voltages[out_index] < 0.1);
        assert!(result.captured_samples[3].node_voltages[out_index] > 0.1);
    }

    #[test]
    fn internal_source_value_uses_last_repeated_pwl_value_at_exact_breakpoint() {
        let value = super::internal_source_value_at_time(
            &super::InternalSourceSpec::Pwl(vec![
                (0.0, 0.0),
                (2.0e-12, 0.0),
                (2.0e-12, 1.0),
                (4.0e-12, 1.0),
            ]),
            2.0e-12,
        );

        assert!((value - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn internal_source_step_value_uses_left_limit_at_repeated_pwl_breakpoint() {
        let value = super::internal_source_step_value_at_time(
            &super::InternalSourceSpec::Pwl(vec![
                (0.0, 0.0),
                (2.0e-12, 0.0),
                (2.0e-12, 1.0),
                (4.0e-12, 1.0),
            ]),
            2.0e-12,
        );

        assert!(value.abs() < 1.0e-12);
    }

    #[test]
    fn internal_source_value_respects_finite_cycle_pulse_end() {
        let source = super::InternalSourceSpec::Pulse {
            low: 0.0,
            high: 1.0,
            delay_s: 0.0,
            rise_s: 1.0e-12,
            fall_s: 1.0e-12,
            width_s: 2.0e-12,
            period_s: Some(4.0e-12),
            cycle_count: Some(2),
        };

        let value = super::internal_source_value_at_time(&source, 8.0e-12);
        assert!(value.abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_supports_mixed_case_function_source_names() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 sIn(0 1 100g 0 90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.5);
    }

    #[test]
    fn internal_transient_supports_function_source_name_followed_by_space_before_paren() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN (0 1 100g 0 90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.5);
    }

    #[test]
    fn internal_transient_supports_exp_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 EXP(0,1,1p,0.5p,4p,0.5p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 6);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[2].node_voltages[out_index] > 0.1);
        assert!(
            result.captured_samples[4].node_voltages[out_index]
                > result.captured_samples[5].node_voltages[out_index]
        );
    }

    #[test]
    fn internal_transient_supports_keyword_exp_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 EXP(v1=0,v2=1,td1=1p,tau1=0.5p,td2=4p,tau2=0.5p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[2].node_voltages[out_index] > 0.1);
        assert!(
            result.captured_samples[4].node_voltages[out_index]
                > result.captured_samples[5].node_voltages[out_index]
        );
    }

    #[test]
    fn internal_transient_supports_keyword_exp_with_spaced_equals() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 EXP(v1 = 0 v2 = 1 td1 = 1p tau1 = 0.5p td2 = 4p tau2 = 0.5p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[2].node_voltages[out_index] > 0.1);
    }

    #[test]
    fn internal_transient_supports_keyword_exp_with_low_high_aliases() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 EXP(low=0,high=1,rise_delay=1p,rise_tau=0.5p,fall_delay=4p,fall_tau=0.5p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[2].node_voltages[out_index] > 0.1);
        assert!(
            result.captured_samples[4].node_voltages[out_index]
                > result.captured_samples[5].node_voltages[out_index]
        );
    }

    #[test]
    fn internal_transient_supports_keyword_exp_with_delay_and_tau_aliases() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 EXP(low=0,high=1,delay1=1p,tau_rise=0.5p,delay2=4p,tau_fall=0.5p)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[2].node_voltages[out_index] > 0.1);
        assert!(
            result.captured_samples[4].node_voltages[out_index]
                > result.captured_samples[5].node_voltages[out_index]
        );
    }

    #[test]
    fn internal_transient_supports_inductors() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nL1 in out 1p\nR1 out 0 1\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 4);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.0);
        assert!(result.final_node_voltages[out_index] > 0.5);
    }

    #[test]
    fn internal_transient_supports_mutual_inductance_with_forward_declared_k() {
        let result = super::run_internal_transient(
            ".title demo\nK1 L1 L2 0.9\nV1 in 0 PULSE(0,1,0,1p,1p,2p,8p)\nL1 in out 1p\nR1 out 0 1\nL2 tap 0 1p\nR2 tap 0 1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let tap_index = result
            .node_names
            .iter()
            .position(|name| name == "tap")
            .unwrap();
        let coupled_peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[tap_index].abs())
            .fold(0.0_f64, f64::max);
        assert!(coupled_peak > 0.01);
    }

    #[test]
    fn internal_transient_supports_mutual_inductance_keyword_syntax() {
        let result = super::run_internal_transient(
            ".title demo\nK1 L1 L2 coupling = 0.9\nV1 in 0 PULSE(0,1,0,1p,1p,2p,8p)\nL1 in out 1p\nR1 out 0 1\nL2 tap 0 1p\nR2 tap 0 1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let tap_index = result
            .node_names
            .iter()
            .position(|name| name == "tap")
            .unwrap();
        let coupled_peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[tap_index].abs())
            .fold(0.0_f64, f64::max);
        assert!(coupled_peak > 0.01);
    }

    #[test]
    fn internal_transient_supports_space_separated_mutual_inductance_keyword() {
        let result = super::run_internal_transient(
            ".title demo\nK1 L1 L2 coupling 0.9\nV1 in 0 PULSE(0,1,0,1p,1p,2p,8p)\nL1 in out 1p\nR1 out 0 1\nL2 tap 0 1p\nR2 tap 0 1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let tap_index = result
            .node_names
            .iter()
            .position(|name| name == "tap")
            .unwrap();
        let coupled_peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[tap_index].abs())
            .fold(0.0_f64, f64::max);
        assert!(coupled_peak > 0.01);
    }

    #[test]
    fn internal_transient_supports_space_separated_mutual_inductance_k_keyword() {
        let result = super::run_internal_transient(
            ".title demo\nK1 L1 L2 k 0.9\nV1 in 0 PULSE(0,1,0,1p,1p,2p,8p)\nL1 in out 1p\nR1 out 0 1\nL2 tap 0 1p\nR2 tap 0 1\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let tap_index = result
            .node_names
            .iter()
            .position(|name| name == "tap")
            .unwrap();
        let coupled_peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[tap_index].abs())
            .fold(0.0_f64, f64::max);
        assert!(coupled_peak > 0.01);
    }

    #[test]
    fn internal_transient_supports_zero_delay_transmission_line_subset() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 50 0\nR1 out 0 50\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let out_voltage = result.final_node_voltages[out_index];
        assert!(out_voltage > 0.45);
        assert!(out_voltage < 0.55);
    }

    #[test]
    fn internal_transient_supports_finite_delay_transmission_line_subset() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=1p\nR1 out 0 50\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let out_voltage = result.final_node_voltages[out_index];
        assert!(out_voltage > 0.3);
    }

    #[test]
    fn internal_transient_supports_space_separated_transmission_line_keywords() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0 50 td 1p\nR1 out 0 50\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let out_voltage = result.final_node_voltages[out_index];
        assert!(out_voltage > 0.3);
    }

    #[test]
    fn internal_transient_transmission_line_loss_reduces_output_amplitude() {
        let baseline = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=3p\nR1 out 0 50\n.tran 1p 6p\n.end\n",
        )
        .unwrap();
        let lossy = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=3p loss=0.3\nR1 out 0 50\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        let out_index = baseline
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(lossy.final_node_voltages[out_index] < baseline.final_node_voltages[out_index]);
    }

    #[test]
    fn internal_transient_supports_space_separated_transmission_line_loss_keywords() {
        let baseline = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0 50 td 3p\nR1 out 0 50\n.tran 1p 6p\n.end\n",
        )
        .unwrap();
        let lossy = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0 50 td 3p loss 0.3\nR1 out 0 50\n.tran 1p 6p\n.end\n",
        )
        .unwrap();

        let out_index = baseline
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(lossy.final_node_voltages[out_index] < baseline.final_node_voltages[out_index]);
    }

    #[test]
    fn internal_transient_supports_comma_separated_transmission_line_keywords() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50, td=1p, loss=0.1\nR1 out 0 50\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.final_node_voltages[out_index] > 0.2);
    }

    #[test]
    fn internal_transient_supports_transmission_line_alias_keywords() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 zo=50 tau=1p atten=0.1\nR1 out 0 50\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.final_node_voltages[out_index] > 0.2);
    }

    #[test]
    fn internal_transient_rejects_transmission_line_when_loss_and_alpha_are_both_set() {
        let err = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=3p loss=0.2 alpha=1e11\nR1 out 0 50\n.tran 1p 6p\n.end\n",
        )
        .unwrap_err();

        assert!(err.contains("internal_transient_invalid_transmission_parameter"));
    }

    #[test]
    fn internal_transient_rejects_transmission_line_loss_out_of_range() {
        let err = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=3p loss=1.2\nR1 out 0 50\n.tran 1p 6p\n.end\n",
        )
        .unwrap_err();

        assert!(err.contains("internal_transient_invalid_transmission_loss"));
    }

    #[test]
    fn internal_transient_supports_minimal_junction_with_iterative_stamp() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 icrit=0.5m rn=20 cj=0.5p\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_rejects_junction_without_required_params() {
        let err = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1m\nJ1 in 0 rn=20\n.tran 1p 4p\n.end\n",
        )
        .unwrap_err();

        assert!(err.contains("internal_transient_invalid_junction_icrit"));
    }

    #[test]
    fn internal_transient_supports_junction_model_card_defaults() {
        let result = super::run_internal_transient(
            ".title demo\n.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 jjmod\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_junction_model_card_with_instance_override() {
        let result = super::run_internal_transient(
            ".title demo\n.model jjmod jj icrit=0.5m rn=20 cj=0.5p\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 jjmod rn=25\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_junction_model_keyword_reference() {
        let result = super::run_internal_transient(
            ".title demo\n.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 model=jjmod\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_junction_model_keyword_reference_with_override() {
        let result = super::run_internal_transient(
            ".title demo\n.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 model = jjmod rn=25\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_comma_separated_junction_instance_parameters() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 icrit=0.5m, rn=20, cj=0.5p\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_comma_separated_junction_model_override() {
        let result = super::run_internal_transient(
            ".title demo\n.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 model=jjmod, rn=25\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_space_separated_junction_instance_parameters() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 icrit 0.5m rn 20 cj 0.5p\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_space_separated_junction_model_override() {
        let result = super::run_internal_transient(
            ".title demo\n.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 model jjmod rn 25\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_supports_space_separated_junction_model_card_defaults() {
        let result = super::run_internal_transient(
            ".title demo\n.model jjmod jj icrit 0.5m rn 20 cj 0.5p\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 jjmod\n.tran 1p 8p\n.end\n",
        )
        .unwrap();

        let n1_index = result
            .node_names
            .iter()
            .position(|name| name == "n1")
            .unwrap();
        let peak = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[n1_index])
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0e-4);
    }

    #[test]
    fn internal_transient_parses_pi_junction_instance_flag() {
        let deck = ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m rn=20 pi=1\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let critical_current = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    critical_current_a, ..
                } = element
                {
                    Some(*critical_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!(critical_current < 0.0);
    }

    #[test]
    fn internal_transient_parses_pi_junction_model_default() {
        let deck = ".title demo\n.model jjmod jj(icrit=0.5m rn=20 pi=1)\nV1 in 0 DC 1m\nJ1 in 0 jjmod\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let critical_current = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    critical_current_a, ..
                } = element
                {
                    Some(*critical_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!(critical_current < 0.0);
    }

    #[test]
    fn internal_transient_parses_nonzero_numeric_pi_junction_flag() {
        let deck =
            ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m rn=20 pi=-2\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let critical_current = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    critical_current_a, ..
                } = element
                {
                    Some(*critical_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!(critical_current < 0.0);
    }

    #[test]
    fn internal_transient_rejects_invalid_junction_pi_flag() {
        let err = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m rn=20 pi=0.5\n.tran 1p 4p\n.end\n",
        )
        .unwrap_err();

        assert!(err.contains("internal_transient_invalid_junction_pi"));
    }

    #[test]
    fn internal_transient_parses_second_harmonic_junction_instance_param() {
        let deck =
            ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m ic2=0.2m rn=20\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let harmonic = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    second_harmonic_current_a,
                    ..
                } = element
                {
                    Some(*second_harmonic_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!((harmonic - 2.0e-4).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_second_harmonic_from_junction_model() {
        let deck = ".title demo\n.model jjmod jj(icrit=0.5m icrit2=0.3m rn=20)\nV1 in 0 DC 1m\nJ1 in 0 jjmod\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let harmonic = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    second_harmonic_current_a,
                    ..
                } = element
                {
                    Some(*second_harmonic_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!((harmonic - 3.0e-4).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_third_harmonic_junction_instance_param() {
        let deck =
            ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m ic3=0.05m rn=20\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let harmonic = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    third_harmonic_current_a,
                    ..
                } = element
                {
                    Some(*third_harmonic_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!((harmonic - 5.0e-5).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_fourth_harmonic_junction_instance_param() {
        let deck =
            ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m ic4=0.01m rn=20\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let harmonic = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    fourth_harmonic_current_a,
                    ..
                } = element
                {
                    Some(*fourth_harmonic_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!((harmonic - 1.0e-5).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_fourth_harmonic_from_junction_model() {
        let deck = ".title demo\n.model jjmod jj(icrit=0.5m icrit4=0.03m rn=20)\nV1 in 0 DC 1m\nJ1 in 0 jjmod\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let harmonic = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    fourth_harmonic_current_a,
                    ..
                } = element
                {
                    Some(*fourth_harmonic_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!((harmonic - 3.0e-5).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_fifth_harmonic_junction_instance_param() {
        let deck =
            ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m ic5=0.005m rn=20\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let harmonic = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    fifth_harmonic_current_a,
                    ..
                } = element
                {
                    Some(*fifth_harmonic_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!((harmonic - 5.0e-6).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_fifth_harmonic_from_junction_model() {
        let deck = ".title demo\n.model jjmod jj(icrit=0.5m icrit5=0.015m rn=20)\nV1 in 0 DC 1m\nJ1 in 0 jjmod\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let harmonic = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    fifth_harmonic_current_a,
                    ..
                } = element
                {
                    Some(*fifth_harmonic_current_a)
                } else {
                    None
                }
            })
            .unwrap();

        assert!((harmonic - 1.5e-5).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_native_cpr_from_junction_model() {
        let deck = ".title demo\n.model jjmod jj(icrit=0.5m cpr={1,0.2,0.05,0.01,0.005} rn=20)\nV1 in 0 DC 1m\nJ1 in 0 jjmod\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let junction = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    critical_current_a,
                    second_harmonic_current_a,
                    third_harmonic_current_a,
                    fourth_harmonic_current_a,
                    fifth_harmonic_current_a,
                    ..
                } = element
                {
                    Some((
                        *critical_current_a,
                        *second_harmonic_current_a,
                        *third_harmonic_current_a,
                        *fourth_harmonic_current_a,
                        *fifth_harmonic_current_a,
                    ))
                } else {
                    None
                }
            })
            .unwrap();

        assert!((junction.0 - 5.0e-4).abs() < 1.0e-12);
        assert!((junction.1 - 1.0e-4).abs() < 1.0e-12);
        assert!((junction.2 - 2.5e-5).abs() < 1.0e-12);
        assert!((junction.3 - 5.0e-6).abs() < 1.0e-12);
        assert!((junction.4 - 2.5e-6).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_parses_native_cpr_from_junction_instance() {
        let deck = ".title demo\nV1 in 0 DC 1m\nJ1 in 0 icrit=0.5m cpr={1,0.2,0.05,0.01,0.005} rn=20\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let junction = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    critical_current_a,
                    second_harmonic_current_a,
                    third_harmonic_current_a,
                    fourth_harmonic_current_a,
                    fifth_harmonic_current_a,
                    ..
                } = element
                {
                    Some((
                        *critical_current_a,
                        *second_harmonic_current_a,
                        *third_harmonic_current_a,
                        *fourth_harmonic_current_a,
                        *fifth_harmonic_current_a,
                    ))
                } else {
                    None
                }
            })
            .unwrap();

        assert!((junction.0 - 5.0e-4).abs() < 1.0e-12);
        assert!((junction.1 - 1.0e-4).abs() < 1.0e-12);
        assert!((junction.2 - 2.5e-5).abs() < 1.0e-12);
        assert!((junction.3 - 5.0e-6).abs() < 1.0e-12);
        assert!((junction.4 - 2.5e-6).abs() < 1.0e-12);
    }

    #[test]
    fn internal_transient_pi_junction_flips_third_harmonic_sign() {
        let deck = ".title demo\n.model jjmod jj(icrit=0.5m icrit3=0.05m rn=20 pi=1)\nV1 in 0 DC 1m\nJ1 in 0 jjmod\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let junction = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    critical_current_a,
                    third_harmonic_current_a,
                    ..
                } = element
                {
                    Some((*critical_current_a, *third_harmonic_current_a))
                } else {
                    None
                }
            })
            .unwrap();

        assert!(junction.0 < 0.0);
        assert!(junction.1 < 0.0);
    }

    #[test]
    fn internal_transient_accepts_pure_second_harmonic_junction_without_primary_icrit() {
        let deck = ".title demo\n.model jjmod jj(rn=20 cj=0.5p icrit2=0.2m)\nV1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)\nR1 in n1 10\nJ1 n1 0 model=jjmod icrit2=0.2m\n.tran 0.25p 8p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();
        let result = super::run_internal_transient(deck).unwrap();

        let junction = netlist
            .elements
            .iter()
            .find_map(|element| {
                if let super::InternalElement::JosephsonJunction {
                    critical_current_a,
                    second_harmonic_current_a,
                    ..
                } = element
                {
                    Some((critical_current_a, second_harmonic_current_a))
                } else {
                    None
                }
            })
            .unwrap();

        assert!(junction.0.abs() < 1.0e-18);
        assert!((junction.1 - 2.0e-4).abs() < 1.0e-12);
        assert!(!result.captured_samples.is_empty());
    }

    #[test]
    fn internal_transient_supports_dc_voltage_sources_with_ignored_ac_tail() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 DC 1 AC 0\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 5);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.final_node_voltages[out_index] > 0.9);
    }

    #[test]
    fn internal_transient_reports_non_convergence_when_max_substeps_are_exhausted() {
        let deck =
            ".title demo\nV1 in 0 PWL(0,0,2p,1)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 2p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();
        let previous_solution = vec![0.0; netlist.node_names.len() + netlist.auxiliary_count];

        let err = super::advance_internal_transient_step_with_limits(
            &netlist,
            &previous_solution,
            1.0e-12,
            0.0,
            2,
            0.0,
            &std::collections::VecDeque::from([(0.0, previous_solution.clone())]),
        )
        .unwrap_err();

        assert!(err.starts_with("internal_transient_timestep_not_converged:"));
        assert!(err.contains("substeps=2"));
        assert!(err.contains("tol=0.000000e0"));
    }

    #[test]
    fn internal_transient_substep_count_increases_when_step_crosses_pwl_breakpoint() {
        let deck = ".title demo\nV1 in 0 PWL(0,0,2p,1,4p,0)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let plain_substeps = super::internal_substep_count(&netlist, 1.0e-12, 0.0);
        let breakpoint_substeps = super::internal_substep_count(&netlist, 1.0e-12, 1.5e-12);

        assert!(breakpoint_substeps > plain_substeps);
    }

    #[test]
    fn internal_transient_substep_count_increases_for_finite_delay_transmission_line() {
        let plain_deck =
            ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=0\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let delayed_deck = ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=0.2p\nR1 out 0 50\n.tran 1p 4p\n.end\n";

        let plain_parsed = parse_deck(plain_deck).unwrap();
        let delayed_parsed = parse_deck(delayed_deck).unwrap();
        let plain_netlist =
            super::parse_internal_transient_netlist(plain_deck, &plain_parsed).unwrap();
        let delayed_netlist =
            super::parse_internal_transient_netlist(delayed_deck, &delayed_parsed).unwrap();

        let plain_substeps = super::internal_substep_count(&plain_netlist, 1.0e-12, 0.0);
        let delayed_substeps = super::internal_substep_count(&delayed_netlist, 1.0e-12, 0.0);

        assert!(delayed_substeps > plain_substeps);
    }

    #[test]
    fn internal_transient_substep_boundaries_align_to_pwl_breakpoint() {
        let deck = ".title demo\nV1 in 0 PWL(0,0,2p,1,4p,0)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let boundaries = super::substep_boundaries_with_breakpoints(&netlist, 1.5e-12, 1.0e-12, 4);

        assert!(boundaries
            .iter()
            .any(|time_s| (*time_s - 2.0e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_substep_boundaries_align_to_transmission_delay_breakpoint() {
        let deck = ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=0.2p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let boundaries = super::substep_boundaries_with_breakpoints(&netlist, 0.0, 1.0e-12, 4);

        assert!(boundaries
            .iter()
            .any(|time_s| (*time_s - 0.2e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_skips_intermediate_transmission_delay_breakpoints() {
        let deck = ".title demo\nV1 in 0 DC 1\nT1 in 0 mid 0 z0=50 td=0.2p\nT2 mid 0 out 0 z0=50 td=0.3p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints =
            super::collect_transmission_breakpoints_within_step(&netlist, 0.0, 1.0e-12);

        assert!(!breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.2e-12).abs() < 1.0e-18));
        assert!(!breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.3e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_skips_dangling_transmission_delay_breakpoint() {
        let deck = ".title demo\nV1 in 0 DC 1\nT1 in 0 out 0 z0=50 td=0.2p\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints =
            super::collect_transmission_breakpoints_within_step(&netlist, 0.0, 1.0e-12);

        assert!(!breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.2e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_collects_sin_delay_breakpoint_within_step() {
        let breakpoints = super::collect_source_breakpoints_within_step(
            &super::InternalSourceSpec::Sin {
                offset: 0.0,
                amplitude: 1.0,
                frequency_hz: 100.0e9,
                delay_s: 0.5e-12,
                damping_hz: 0.0,
                phase_rad: 0.0,
            },
            0.0,
            1.0e-12,
        );

        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.5e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_collects_netlist_breakpoints_within_step() {
        let deck = ".title demo\nV1 in 0 PWL(0,0,2p,1,4p,0)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints =
            super::collect_netlist_breakpoints_within_step(&netlist, 1.5e-12, 1.0e-12);

        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 2.0e-12).abs() < 1.0e-18));
        assert!((breakpoints.first().copied().unwrap_or_default() - 1.5e-12).abs() < 1.0e-18);
        assert!((breakpoints.last().copied().unwrap_or_default() - 2.5e-12).abs() < 1.0e-18);
    }

    #[test]
    fn internal_transient_collects_delayed_transmission_source_breakpoint_within_step() {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.1p,0.1p,0.3p,2p)\nT1 in 0 out 0 z0=50 td=0.2p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints = super::collect_netlist_breakpoints_within_step(&netlist, 0.0, 1.0e-12);

        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.2e-12).abs() < 1.0e-18));
        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.3e-12).abs() < 1.0e-18));
        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.6e-12).abs() < 1.0e-18));
        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.7e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_substep_boundaries_align_to_delayed_transmission_source_breakpoint() {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.1p,0.1p,0.3p,2p)\nT1 in 0 out 0 z0=50 td=0.2p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let boundaries = super::substep_boundaries_with_breakpoints(&netlist, 0.0, 1.0e-12, 4);

        assert!(boundaries
            .iter()
            .any(|time_s| (*time_s - 0.3e-12).abs() < 1.0e-18));
        assert!(boundaries
            .iter()
            .any(|time_s| (*time_s - 0.6e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_collects_multi_hop_transmission_source_breakpoint_within_step() {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.1p,0.1p,0.3p,2p)\nT1 in 0 mid 0 z0=50 td=0.2p\nT2 mid 0 out 0 z0=50 td=0.3p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints = super::collect_netlist_breakpoints_within_step(&netlist, 0.0, 1.0e-12);

        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.5e-12).abs() < 1.0e-18));
        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.6e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_substep_boundaries_align_to_multi_hop_transmission_source_breakpoint() {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.1p,0.1p,0.3p,2p)\nT1 in 0 mid 0 z0=50 td=0.2p\nT2 mid 0 out 0 z0=50 td=0.3p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let boundaries = super::substep_boundaries_with_breakpoints(&netlist, 0.0, 1.0e-12, 4);

        assert!(boundaries
            .iter()
            .any(|time_s| (*time_s - 0.5e-12).abs() < 1.0e-18));
        assert!(boundaries
            .iter()
            .any(|time_s| (*time_s - 0.6e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_does_not_collect_intermediate_chain_arrival_breakpoint() {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.1p,0.1p,0.3p,2p)\nT1 in 0 mid 0 z0=50 td=0.2p\nT2 mid 0 out 0 z0=50 td=0.3p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints = super::collect_netlist_breakpoints_within_step(&netlist, 0.0, 1.0e-12);

        assert!(!breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.7e-12).abs() < 1.0e-18));
        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.5e-12).abs() < 1.0e-18));
        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.6e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_skips_unobserved_intermediate_transmission_breakpoint() {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.11p,0.11p,0.29p,2p)\nT1 in 0 mid 0 z0=50 td=0.23p\nT2 mid 0 out 0 z0=50 td=0.31p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints = super::collect_netlist_breakpoints_within_step(&netlist, 0.0, 1.0e-12);

        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.54e-12).abs() < 1.0e-18));
        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.65e-12).abs() < 1.0e-18));
        assert!(!breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.34e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_collects_parallel_path_transmission_source_breakpoint_within_step() {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.1p,0.1p,0.3p,2p)\nT1 in 0 out 0 z0=50 td=0.2p\nT2 in 0 out 0 z0=50 td=0.3p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let breakpoints = super::collect_netlist_breakpoints_within_step(&netlist, 0.0, 1.0e-12);

        assert!(breakpoints
            .iter()
            .any(|time_s| (*time_s - 0.4e-12).abs() < 1.0e-18));
    }

    #[test]
    fn internal_transient_substep_boundaries_align_to_parallel_path_transmission_source_breakpoint()
    {
        let deck = ".title demo\nV1 in 0 PULSE(0,1,0,0.1p,0.1p,0.3p,2p)\nT1 in 0 out 0 z0=50 td=0.2p\nT2 in 0 out 0 z0=50 td=0.3p\nR1 out 0 50\n.tran 1p 4p\n.end\n";
        let parsed = parse_deck(deck).unwrap();
        let netlist = super::parse_internal_transient_netlist(deck, &parsed).unwrap();

        let boundaries = super::substep_boundaries_with_breakpoints(&netlist, 0.0, 1.0e-12, 4);

        assert!(boundaries
            .iter()
            .any(|time_s| (*time_s - 0.4e-12).abs() < 1.0e-18));
    }

    #[test]
    fn parse_simulator_output_accepts_standard_sim_summary_keys() {
        let stdout = "SIM_EVENTS=12\nSIM_RESULT=OK\nSIM_WAVEFORM_PATH=wave.csv\nSIM_VIOLATIONS=2\nSIM_WORST_DELAY_PS=18.5\nSIM_DELAY_DETAIL=name=n1,delay_ps=12.5,from=a:1,to=b:2\nSIM_MEASUREMENT_DETAIL=name=out_rms,kind=rms,value=0.001,at=out\nSIM_VIOLATION_DETAIL=kind=hold,detail=late,at=n1:3\n";

        let (
            events,
            result,
            waveform,
            summary_contract,
            violations,
            worst_delay,
            delay_details,
            measurement_details,
            violation_details,
        ) = super::parse_simulator_output(stdout);

        assert_eq!(events, Some(12));
        assert_eq!(result.as_deref(), Some("ok"));
        assert_eq!(waveform.as_deref(), Some("wave.csv"));
        assert_eq!(summary_contract.as_deref(), Some("sim_v1"));
        assert_eq!(violations, Some(2));
        assert_eq!(worst_delay, Some(18.5));
        assert_eq!(delay_details.len(), 1);
        assert_eq!(delay_details[0].name, "n1");
        assert_eq!(
            delay_details[0].from_ref.as_ref().map(|r| r.node.as_str()),
            Some("a")
        );
        assert_eq!(
            delay_details[0].to_ref.as_ref().and_then(|r| r.port),
            Some(2)
        );
        assert_eq!(measurement_details.len(), 1);
        assert_eq!(measurement_details[0].name, "out_rms");
        assert_eq!(measurement_details[0].kind, "rms");
        assert_eq!(measurement_details[0].measured_value, 0.001);
        assert_eq!(
            measurement_details[0]
                .at_ref
                .as_ref()
                .map(|r| r.node.as_str()),
            Some("out")
        );
        assert_eq!(violation_details.len(), 1);
        assert_eq!(violation_details[0].kind, "hold");
        assert_eq!(
            violation_details[0].at_ref.as_ref().map(|r| r.raw.as_str()),
            Some("n1:3")
        );
    }

    #[test]
    fn parse_simulator_output_accepts_mixed_standard_and_legacy_keys() {
        let stdout = "RFLOW_EVENTS=4\nSIM_RESULT=PASS\nRAW_FILE=legacy.csv\nVIOLATION_COUNT=1\nSIM_WORST_DELAY_PS=7.25\nDELAY_DETAIL=node_a,3.5\nSIM_VIOLATION_DETAIL=kind=setup,detail=critical,at=n9\n";

        let (
            events,
            result,
            waveform,
            summary_contract,
            violations,
            worst_delay,
            delay_details,
            measurement_details,
            violation_details,
        ) = super::parse_simulator_output(stdout);

        assert_eq!(events, Some(4));
        assert_eq!(result.as_deref(), Some("pass"));
        assert_eq!(waveform.as_deref(), Some("legacy.csv"));
        assert_eq!(summary_contract.as_deref(), Some("mixed"));
        assert_eq!(violations, Some(1));
        assert_eq!(worst_delay, Some(7.25));
        assert_eq!(delay_details.len(), 1);
        assert_eq!(delay_details[0].name, "node_a");
        assert_eq!(delay_details[0].delay_ps, 3.5);
        assert!(measurement_details.is_empty());
        assert_eq!(violation_details.len(), 1);
        assert_eq!(violation_details[0].kind, "setup");
        assert_eq!(violation_details[0].detail, "critical");
    }

    #[test]
    fn internal_transient_supports_sin_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN(0,1,100g)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 5p\n.end\n",
        )
        .unwrap();

        assert_eq!(result.simulated_steps, 5);
        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let peak_out_voltage = result
            .captured_samples
            .iter()
            .map(|sample| sample.node_voltages[out_index].abs())
            .fold(0.0_f64, f64::max);
        assert!(result.captured_samples.len() >= 2);
        assert!(peak_out_voltage > 0.1);
    }

    #[test]
    fn internal_transient_supports_sin_phase_argument() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN(0,1,100g,0,90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.5);
    }

    #[test]
    fn internal_transient_supports_sin_damping_and_phase_arguments() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN(0,1,100g,0,300g,90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let first = result.captured_samples[1].node_voltages[out_index].abs();
        let last = result.captured_samples.last().unwrap().node_voltages[out_index].abs();
        assert!(first > 0.2);
        assert!(last < first);
    }

    #[test]
    fn internal_transient_supports_whitespace_separated_sin_phase_argument() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN(0 1 100g 0 90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.5);
    }

    #[test]
    fn internal_transient_supports_keyword_sin_voltage_sources() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN(vo=0,va=1,freq=100g,td=0,phase=90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.5);
    }

    #[test]
    fn internal_transient_supports_keyword_sin_with_spaced_equals_and_damping() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN(vo = 0 va = 1 freq = 100g td = 0 theta = 300g phase = 90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 4p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        let first = result.captured_samples[1].node_voltages[out_index].abs();
        let last = result.captured_samples.last().unwrap().node_voltages[out_index].abs();
        assert!(first > 0.2);
        assert!(last < first);
    }

    #[test]
    fn internal_transient_supports_keyword_sin_with_frequency_and_phase_aliases() {
        let result = super::run_internal_transient(
            ".title demo\nV1 in 0 SIN(offset=0 amplitude=1 f=100g delay=0 phi=90)\nR1 in out 1\nC1 out 0 1p\n.tran 1p 3p\n.end\n",
        )
        .unwrap();

        let out_index = result
            .node_names
            .iter()
            .position(|name| name == "out")
            .unwrap();
        assert!(result.captured_samples[1].node_voltages[out_index] > 0.5);
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rflux-sim-test-{}-{}",
            label,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ))
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn parse_simulator_output(stdout: &str) -> ParsedSimulatorOutput {
    let mut events = None;
    let mut result = None;
    let mut waveform_path = None;
    let mut saw_sim_contract_key = false;
    let mut saw_legacy_contract_key = false;
    let mut reported_violations = None;
    let mut reported_worst_delay_ps = None;
    let mut delay_details = Vec::new();
    let mut measurement_details = Vec::new();
    let mut violation_details = Vec::new();

    for line in stdout.lines() {
        let separator_index = match (line.find(':'), line.find('=')) {
            (Some(colon_index), Some(equals_index)) => Some(colon_index.min(equals_index)),
            (Some(colon_index), None) => Some(colon_index),
            (None, Some(equals_index)) => Some(equals_index),
            (None, None) => None,
        };
        let Some(separator_index) = separator_index else {
            continue;
        };
        let (raw_key, raw_value_with_separator) = line.split_at(separator_index);
        let raw_value = &raw_value_with_separator[1..];

        let key = raw_key.trim().to_ascii_uppercase();
        let value = raw_value.trim();
        match key.as_str() {
            "RFLOW_EVENTS" | "EVENTS" | "MEASURED_EVENTS" | "SIM_EVENTS" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                events = value.parse::<usize>().ok();
            }
            "RFLOW_RESULT" | "RESULT" | "STATUS" | "SIM_RESULT" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                result = Some(value.to_ascii_lowercase());
            }
            "RFLOW_WAVEFORM" | "WAVEFORM" | "RAW_FILE" | "SIM_WAVEFORM_PATH" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                waveform_path = Some(value.to_string());
            }
            "RFLOW_VIOLATIONS" | "VIOLATIONS" | "VIOLATION_COUNT" | "SIM_VIOLATIONS" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                reported_violations = value.parse::<usize>().ok();
            }
            "RFLOW_WORST_DELAY_PS"
            | "WORST_DELAY_PS"
            | "MEASURED_DELAY_PS"
            | "SIM_WORST_DELAY_PS" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                reported_worst_delay_ps = value.parse::<f64>().ok();
            }
            "RFLOW_DELAY_DETAIL" | "DELAY_DETAIL" | "SIM_DELAY_DETAIL" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                if let Some(detail) = parse_delay_detail(value) {
                    delay_details.push(detail);
                }
            }
            "RFLOW_MEASUREMENT_DETAIL" | "MEASUREMENT_DETAIL" | "SIM_MEASUREMENT_DETAIL" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                if let Some(detail) = parse_measurement_detail(value) {
                    measurement_details.push(detail);
                }
            }
            "RFLOW_VIOLATION_DETAIL" | "VIOLATION_DETAIL" | "SIM_VIOLATION_DETAIL" => {
                if key.starts_with("SIM_") {
                    saw_sim_contract_key = true;
                } else {
                    saw_legacy_contract_key = true;
                }
                if let Some(detail) = parse_violation_detail(value) {
                    violation_details.push(detail);
                }
            }
            _ => {}
        }
    }

    let external_summary_contract = if saw_sim_contract_key && saw_legacy_contract_key {
        Some("mixed".to_string())
    } else if saw_sim_contract_key {
        Some("sim_v1".to_string())
    } else if saw_legacy_contract_key {
        Some("legacy".to_string())
    } else {
        None
    };

    (
        events,
        result,
        waveform_path,
        external_summary_contract,
        reported_violations,
        reported_worst_delay_ps,
        delay_details,
        measurement_details,
        violation_details,
    )
}

fn parse_delay_detail(value: &str) -> Option<SimulationDelayDetail> {
    let mut name = None;
    let mut delay_ps = None;
    let mut from_ref = None;
    let mut to_ref = None;

    let parts = value.split(',').map(str::trim).collect::<Vec<_>>();
    let has_named_fields = parts.iter().any(|part| part.contains('='));
    if has_named_fields {
        for part in parts {
            let (key, raw_value) = part.split_once('=')?;
            match key.trim().to_ascii_lowercase().as_str() {
                "name" => name = Some(raw_value.trim().to_string()),
                "delay_ps" => delay_ps = raw_value.trim().parse::<f64>().ok(),
                "from" => from_ref = parse_endpoint_ref(raw_value.trim()),
                "to" => to_ref = parse_endpoint_ref(raw_value.trim()),
                _ => {}
            }
        }
    } else {
        let (raw_name, raw_delay_ps) = value.split_once(',')?;
        name = Some(raw_name.trim().to_string());
        delay_ps = raw_delay_ps.trim().parse::<f64>().ok();
    }

    Some(SimulationDelayDetail {
        name: name?,
        delay_ps: delay_ps?,
        from_ref,
        to_ref,
    })
}

fn parse_measurement_detail(value: &str) -> Option<SimulationMeasurementDetail> {
    let mut name = None;
    let mut kind = None;
    let mut measured_value = None;
    let mut at_ref = None;

    let parts = value.split(',').map(str::trim).collect::<Vec<_>>();
    let has_named_fields = parts.iter().any(|part| part.contains('='));
    if has_named_fields {
        for part in parts {
            let (key, raw_value) = part.split_once('=')?;
            match key.trim().to_ascii_lowercase().as_str() {
                "name" => name = Some(raw_value.trim().to_string()),
                "kind" => kind = Some(raw_value.trim().to_string()),
                "value" | "measured_value" => measured_value = raw_value.trim().parse::<f64>().ok(),
                "at" | "node" => at_ref = parse_endpoint_ref(raw_value.trim()),
                _ => {}
            }
        }
    } else {
        let parts = value.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 3 {
            return None;
        }
        name = Some(parts[0].to_string());
        kind = Some(parts[1].to_string());
        measured_value = parts[2].parse::<f64>().ok();
    }

    Some(SimulationMeasurementDetail {
        name: name?,
        kind: kind?,
        measured_value: measured_value?,
        at_ref,
    })
}

fn parse_violation_detail(value: &str) -> Option<SimulationViolationDetail> {
    let mut kind = None;
    let mut detail = None;
    let mut at_ref = None;

    let parts = value.split(',').map(str::trim).collect::<Vec<_>>();
    let has_named_fields = parts.iter().any(|part| part.contains('='));
    if has_named_fields {
        for part in parts {
            let (key, raw_value) = part.split_once('=')?;
            match key.trim().to_ascii_lowercase().as_str() {
                "kind" => kind = Some(raw_value.trim().to_string()),
                "detail" => detail = Some(raw_value.trim().to_string()),
                "at" => at_ref = parse_endpoint_ref(raw_value.trim()),
                _ => {}
            }
        }
    } else {
        let (raw_kind, raw_detail) = value.split_once(',')?;
        kind = Some(raw_kind.trim().to_string());
        detail = Some(raw_detail.trim().to_string());
    }

    Some(SimulationViolationDetail {
        kind: kind?,
        detail: detail?,
        at_ref,
    })
}

fn parse_endpoint_ref(value: &str) -> Option<SimulationEndpointRef> {
    let raw = value.trim().to_string();
    if raw.is_empty() {
        return None;
    }

    let (node, port) = if let Some((node, raw_port)) = raw.rsplit_once(':') {
        (node.trim().to_string(), raw_port.trim().parse::<u16>().ok())
    } else {
        (raw.clone(), None)
    };

    Some(SimulationEndpointRef { raw, node, port })
}
