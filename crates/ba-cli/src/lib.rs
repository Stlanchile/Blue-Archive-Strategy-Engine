#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::Write;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use ba_core::{
    CoreError, CoreErrorClass, ScenarioId, ValidationReport, load_bundle, validate_document,
};
use ba_engine::{
    AnalysisProvenance, ComparisonResult, EngineError, EngineErrorClass, ExactAnalysisFailure,
    ExactAnalysisResult, ExactSolverOptions, MonteCarloAnalysisResult, RunTraceResult,
    analyze_exact_detailed, compare, simulate_monte_carlo, simulate_trace,
};
use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};
use rand_core::{OsRng, TryRngCore};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "ba-strategy",
    version,
    about = "Blue Archive Strategy Engine v0.1"
)]
struct Cli {
    #[arg(long, global = true, default_value = "./data")]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Strictly validate one versioned JSON document.
    Validate {
        document: PathBuf,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Exhaustively enumerate every modeled probability branch.
    Analyze {
        scenario: PathBuf,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Run Monte Carlo with an OS-random seed unless --seed is supplied.
    Simulate {
        scenario: PathBuf,
        #[arg(long)]
        runs: NonZeroU64,
        /// Reproduce a run with this master seed instead of using OS entropy.
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long)]
        trace: bool,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Compare exact analysis with OS-seeded or explicitly seeded Monte Carlo.
    Compare {
        scenario: PathBuf,
        #[arg(long)]
        runs: NonZeroU64,
        /// Reproduce a run with this master seed instead of using OS entropy.
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

#[derive(Debug)]
enum AppError {
    Core(CoreError),
    Engine(EngineError),
    Exact(ExactAnalysisFailure),
    Entropy(String),
    Usage(String),
    Internal(String),
}

impl From<CoreError> for AppError {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}

impl From<EngineError> for AppError {
    fn from(value: EngineError) -> Self {
        Self::Engine(value)
    }
}

impl From<ExactAnalysisFailure> for AppError {
    fn from(value: ExactAnalysisFailure) -> Self {
        Self::Exact(value)
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    class: &'static str,
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<AnalysisProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_exact_options: Option<ExactSolverOptions>,
}

pub fn run<I, T>(args: I, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let usage_json = requests_json(&raw);
    let cli = match Cli::try_parse_from(raw) {
        Ok(value) => value,
        Err(error) => {
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let rendered = error.render().to_string();
                return if stdout.write_all(rendered.as_bytes()).is_ok() {
                    0
                } else {
                    let _ = stderr.write_all(b"unexpected failure: could not write stdout\n");
                    70
                };
            }
            let message = error.to_string();
            let body = ErrorBody {
                class: "cli_usage",
                code: "cli_usage",
                message,
                provenance: None,
                effective_exact_options: None,
            };
            let rendered = if usage_json {
                render_json(&ErrorEnvelope { error: body }).unwrap_or_else(|_| {
                    "{\"error\":{\"class\":\"cli_usage\",\"code\":\"cli_usage\",\"message\":\"invalid command line\"}}\n".to_owned()
                })
            } else {
                error.render().to_string()
            };
            let _ = stderr.write_all(rendered.as_bytes());
            return 2;
        }
    };

    let (format, result) = execute(cli);
    match result {
        Ok(rendered) => {
            if stdout.write_all(rendered.as_bytes()).is_ok() {
                0
            } else {
                let _ = stderr.write_all(b"unexpected failure: could not write stdout\n");
                70
            }
        }
        Err(error) => {
            let (exit, body) = classify_error(error);
            let rendered = match format {
                OutputFormat::Json => render_json(&ErrorEnvelope { error: body })
                    .unwrap_or_else(|_| {
                        "{\"error\":{\"class\":\"internal\",\"code\":\"render_failure\",\"message\":\"could not render error\"}}\n".to_owned()
                    }),
                OutputFormat::Text => {
                    format!(
                        "error [{}:{}]: {}\n",
                        body.class, body.code, body.message
                    )
                }
            };
            let _ = stderr.write_all(rendered.as_bytes());
            exit
        }
    }
}

fn execute(cli: Cli) -> (OutputFormat, Result<String, AppError>) {
    match cli.command {
        Command::Validate { document, format } => {
            let result = validate_document(&cli.data_dir, document)
                .map_err(AppError::from)
                .and_then(|value| render_validation(&value, format));
            (format, result)
        }
        Command::Analyze { scenario, format } => {
            let path = resolve_scenario(&cli.data_dir, &scenario);
            let result = load_bundle(&cli.data_dir, path)
                .map_err(AppError::from)
                .and_then(|bundle| {
                    analyze_exact_detailed(&bundle, ExactSolverOptions::default())
                        .map_err(AppError::from)
                })
                .and_then(|value| render_exact(&value, format));
            (format, result)
        }
        Command::Simulate {
            scenario,
            runs,
            seed,
            trace,
            format,
        } => {
            let path = resolve_scenario(&cli.data_dir, &scenario);
            let result = load_bundle(&cli.data_dir, path)
                .map_err(AppError::from)
                .and_then(|bundle| {
                    if trace && runs.get() != 1 {
                        return Err(AppError::Usage(
                            "simulate --trace requires --runs 1".to_owned(),
                        ));
                    }
                    let seed = resolve_master_seed(seed)?;
                    if trace {
                        simulate_trace(&bundle, seed)
                            .map(|value| SimulationOutput::Trace(Box::new(value)))
                            .map_err(AppError::from)
                    } else {
                        simulate_monte_carlo(&bundle, runs, seed)
                            .map(|value| SimulationOutput::Aggregate(Box::new(value)))
                            .map_err(AppError::from)
                    }
                })
                .and_then(|value| match value {
                    SimulationOutput::Trace(value) => render_trace(&value, format),
                    SimulationOutput::Aggregate(value) => render_monte_carlo(&value, format),
                });
            (format, result)
        }
        Command::Compare {
            scenario,
            runs,
            seed,
            format,
        } => {
            let path = resolve_scenario(&cli.data_dir, &scenario);
            let result = load_bundle(&cli.data_dir, path)
                .map_err(AppError::from)
                .and_then(|bundle| {
                    let seed = resolve_master_seed(seed)?;
                    compare(&bundle, runs, seed).map_err(AppError::from)
                })
                .and_then(|value| render_comparison(&value, format));
            (format, result)
        }
    }
}

enum SimulationOutput {
    Trace(Box<RunTraceResult>),
    Aggregate(Box<MonteCarloAnalysisResult>),
}

fn resolve_scenario(data_dir: &Path, supplied: &Path) -> PathBuf {
    if supplied.exists() || supplied.components().count() > 1 || supplied.extension().is_some() {
        return supplied.to_path_buf();
    }
    let Some(name) = supplied.to_str() else {
        return supplied.to_path_buf();
    };
    if ScenarioId::new(name).is_err() {
        return supplied.to_path_buf();
    }
    data_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("scenarios")
        .join("golden")
        .join(format!("{name}.json"))
}

fn resolve_master_seed(seed: Option<u64>) -> Result<u64, AppError> {
    resolve_master_seed_with(seed, || {
        OsRng
            .try_next_u64()
            .map_err(|error| format!("operating-system entropy is unavailable: {error}"))
    })
}

fn resolve_master_seed_with(
    seed: Option<u64>,
    entropy: impl FnOnce() -> Result<u64, String>,
) -> Result<u64, AppError> {
    match seed {
        Some(seed) => Ok(seed),
        None => entropy().map_err(AppError::Entropy),
    }
}

fn render_validation(value: &ValidationReport, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Valid: yes\nDocument type: {}\nSchema version: {}\nID: {}\nFingerprint: {}\n",
            value.document_type, value.schema_version, value.id, value.fingerprint
        )),
    }
}

fn render_exact(value: &ExactAnalysisResult, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: exact\nScenario: {}\nSuccess probability: {:.15}\nExpected terminal primitive recruitments: {:.10}\nExpected first-success recruitment count given success: {}\nExpected paid pyroxene spent: {:.10}\nExpected ticket-funded primitive recruitments: {:.10}\n",
            value.provenance.scenario_id,
            value.success_probability,
            value.expected_terminal_primitive_recruitments,
            display_optional(value.expected_first_success_recruitment_count_given_success),
            value.expected_paid_pyroxene_spent,
            value.expected_ticket_funded_primitive_recruitments,
        )),
    }
}

fn render_monte_carlo(
    value: &MonteCarloAnalysisResult,
    format: OutputFormat,
) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: Monte Carlo\nScenario: {}\nRuns: {}\nMaster seed: {}\nSuccess probability estimate: {:.15}\nExpected terminal primitive recruitments: {:.10}\nExpected paid pyroxene spent: {:.10}\nExpected ticket-funded primitive recruitments: {:.10}\n",
            value.provenance.scenario_id,
            value.rng.run_count,
            value.rng.master_seed,
            value.success_probability,
            value.expected_terminal_primitive_recruitments,
            value.expected_paid_pyroxene_spent,
            value.expected_ticket_funded_primitive_recruitments,
        )),
    }
}

fn render_trace(value: &RunTraceResult, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: trace\nScenario: {}\nMaster seed: {}\nTerminal reason: {:?}\nTerminal primitive recruitments: {}\nFirst-success recruitment count: {}\nPaid pyroxene spent: {}\nTicket-funded primitive recruitments: {}\n",
            value.provenance.scenario_id,
            value
                .rng
                .as_ref()
                .map_or_else(|| "none".to_owned(), |rng| rng.master_seed.to_string()),
            value.terminal_reason,
            value.terminal_primitive_recruitments,
            value
                .first_success_recruitment_count
                .map_or_else(|| "none".to_owned(), |count| count.to_string()),
            value.paid_pyroxene_spent,
            value.ticket_funded_primitive_recruitments,
        )),
    }
}

fn render_comparison(value: &ComparisonResult, format: OutputFormat) -> Result<String, AppError> {
    match format {
        OutputFormat::Json => render_json(value),
        OutputFormat::Text => Ok(format!(
            "Engine: comparison\nScenario: {}\nMaster seed: {}\nExact success probability: {:.15}\nMonte Carlo success probability: {:.15}\nDifference: {:.15}\nExact value inside Monte Carlo 95% interval: {}\n",
            value.exact.provenance.scenario_id,
            value.monte_carlo.rng.master_seed,
            value.exact.success_probability,
            value.monte_carlo.success_probability,
            value.success_probability_difference,
            if value.success_probability_within_monte_carlo_interval {
                "yes"
            } else {
                "no"
            },
        )),
    }
}

fn render_json(value: &impl Serialize) -> Result<String, AppError> {
    let mut rendered = serde_json::to_string_pretty(value)
        .map_err(|error| AppError::Internal(format!("JSON rendering failed: {error}")))?;
    rendered.push('\n');
    Ok(rendered)
}

fn display_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_owned(), |number| format!("{number:.10}"))
}

fn requests_json(args: &[OsString]) -> bool {
    args.iter().enumerate().any(|(index, value)| {
        value == "--format=json"
            || (value == "--format" && args.get(index + 1).is_some_and(|next| next == "json"))
    })
}

fn classify_error(error: AppError) -> (i32, ErrorBody) {
    match error {
        AppError::Core(error) => {
            let (exit, class) = match error.class() {
                CoreErrorClass::Validation => (3, "validation"),
                CoreErrorClass::CatalogIo => (4, "catalog_io"),
                CoreErrorClass::Engine => (5, "engine"),
                CoreErrorClass::Internal => (70, "internal"),
            };
            let code = core_error_code(&error);
            (
                exit,
                ErrorBody {
                    class,
                    code,
                    message: error.to_string(),
                    provenance: None,
                    effective_exact_options: None,
                },
            )
        }
        AppError::Engine(error) => {
            let (exit, class) = match error.class() {
                EngineErrorClass::GuardOrInvariant => (5, "engine"),
                EngineErrorClass::Internal => (70, "internal"),
            };
            let code = engine_error_code(&error);
            (
                exit,
                ErrorBody {
                    class,
                    code,
                    message: error.to_string(),
                    provenance: None,
                    effective_exact_options: None,
                },
            )
        }
        AppError::Exact(failure) => {
            let (exit, class) = match failure.error.class() {
                EngineErrorClass::GuardOrInvariant => (5, "engine"),
                EngineErrorClass::Internal => (70, "internal"),
            };
            let code = engine_error_code(&failure.error);
            (
                exit,
                ErrorBody {
                    class,
                    code,
                    message: failure.error.to_string(),
                    provenance: Some(*failure.provenance),
                    effective_exact_options: Some(failure.effective_options),
                },
            )
        }
        AppError::Entropy(message) => (
            4,
            ErrorBody {
                class: "entropy_io",
                code: "entropy_unavailable",
                message,
                provenance: None,
                effective_exact_options: None,
            },
        ),
        AppError::Usage(message) => (
            2,
            ErrorBody {
                class: "cli_usage",
                code: "cli_usage",
                message,
                provenance: None,
                effective_exact_options: None,
            },
        ),
        AppError::Internal(message) => (
            70,
            ErrorBody {
                class: "internal",
                code: "internal_failure",
                message,
                provenance: None,
                effective_exact_options: None,
            },
        ),
    }
}

fn core_error_code(error: &CoreError) -> &'static str {
    match error {
        CoreError::Io { .. } => "io_failure",
        CoreError::PathPolicy { .. } => "path_policy",
        CoreError::CatalogEntryLimitExceeded { .. } => "catalog_entry_limit_exceeded",
        CoreError::CatalogDirectoryEntryLimitExceeded { .. } => {
            "catalog_directory_entry_limit_exceeded"
        }
        CoreError::DocumentSizeLimitExceeded { .. } => "document_size_limit_exceeded",
        CoreError::InvalidJson { .. } => "invalid_json",
        CoreError::UnsupportedDocument { .. } => "unsupported_document",
        CoreError::Validation { .. } => "validation_failed",
        CoreError::ArithmeticOverflow { .. } => "arithmetic_overflow",
        CoreError::InvalidAction { .. } => "invalid_action",
        CoreError::InvalidTransition { .. } => "invalid_transition",
        CoreError::InternalInvariant { .. } => "internal_invariant",
    }
}

fn engine_error_code(error: &EngineError) -> &'static str {
    match error {
        EngineError::SolverStateLimitExceeded { .. } => "solver_state_limit_exceeded",
        EngineError::SolverProcessedStateLimitExceeded { .. } => {
            "solver_processed_state_limit_exceeded"
        }
        EngineError::SolverTransitionLimitExceeded { .. } => "solver_transition_limit_exceeded",
        EngineError::SimulationRunLimitExceeded { .. } => "simulation_run_limit_exceeded",
        EngineError::SimulationPrimitiveLimitExceeded { .. } => {
            "simulation_primitive_limit_exceeded"
        }
        EngineError::ProbabilityInvariantViolation { .. } => "probability_invariant_violation",
        EngineError::ArithmeticOverflow { .. } => "arithmetic_overflow",
        EngineError::InvalidAction { .. } => "invalid_action",
        EngineError::InvalidTransition { .. } => "invalid_transition",
        EngineError::InternalInvariantViolation { .. } => "internal_invariant",
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, resolve_master_seed_with};

    #[test]
    fn explicit_seed_does_not_consult_entropy() {
        let seed =
            resolve_master_seed_with(Some(42), || Err("entropy callback must not run".to_owned()))
                .expect("explicit seed");
        assert_eq!(seed, 42);
    }

    #[test]
    fn entropy_failure_is_fail_closed() {
        assert!(matches!(
            resolve_master_seed_with(None, || Err("unavailable".to_owned())),
            Err(AppError::Entropy(message)) if message == "unavailable"
        ));
    }
}
