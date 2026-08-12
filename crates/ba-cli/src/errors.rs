use ba_core::{CoreError, CoreErrorClass};
use ba_engine::{
    AnalysisProvenance, EngineError, EngineErrorClass, ExactAnalysisFailure, ExactSolverOptions,
};
use serde::Serialize;

#[derive(Debug)]
pub(crate) enum AppError {
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
pub(crate) struct ErrorEnvelope {
    pub(crate) error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub(crate) struct ErrorBody {
    pub(crate) class: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<AnalysisProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_exact_options: Option<ExactSolverOptions>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticEnvelope {
    pub(crate) diagnostics_schema_version: u64,
    pub(crate) error: DiagnosticBody,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticBody {
    class: &'static str,
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pointer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<&'static str>,
}

pub(crate) struct ClassifiedError {
    pub(crate) exit: i32,
    pub(crate) body: ErrorBody,
    pub(crate) diagnostic: DiagnosticBody,
}

pub(crate) fn usage_error(message: String) -> ClassifiedError {
    classified(
        2,
        "cli_usage",
        "cli_usage",
        message,
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

pub(crate) fn classify_error(error: AppError) -> ClassifiedError {
    match error {
        AppError::Core(error) => {
            let (exit, class) = match error.class() {
                CoreErrorClass::Validation => (3, "validation"),
                CoreErrorClass::CatalogIo => (4, "catalog_io"),
                CoreErrorClass::Engine => (5, "engine"),
                CoreErrorClass::Internal => (70, "internal"),
            };
            let code = core_error_code(&error);
            let message = error.to_string();
            let pointer = core_pointer(&error);
            let (line, column) =
                core_location(&error).unwrap_or_else(|| parse_line_column(&message));
            classified(
                exit,
                class,
                code,
                message,
                None,
                None,
                pointer,
                line,
                column,
                core_hint(&error),
            )
        }
        AppError::Engine(error) => {
            let (exit, class) = match error.class() {
                EngineErrorClass::GuardOrInvariant => (5, "engine"),
                EngineErrorClass::Internal => (70, "internal"),
            };
            classified(
                exit,
                class,
                engine_error_code(&error),
                error.to_string(),
                None,
                None,
                None,
                None,
                None,
                Some("review the configured engine work limits and validated scenario"),
            )
        }
        AppError::Exact(failure) => {
            let (exit, class) = match failure.error.class() {
                EngineErrorClass::GuardOrInvariant => (5, "engine"),
                EngineErrorClass::Internal => (70, "internal"),
            };
            classified(
                exit,
                class,
                engine_error_code(&failure.error),
                failure.error.to_string(),
                Some(*failure.provenance),
                Some(failure.effective_options),
                None,
                None,
                None,
                Some("review the exact solver diagnostics and configured work limits"),
            )
        }
        AppError::Entropy(message) => classified(
            4,
            "entropy_io",
            "entropy_unavailable",
            message,
            None,
            None,
            None,
            None,
            None,
            Some("supply --seed explicitly or restore operating-system entropy"),
        ),
        AppError::Usage(message) => usage_error(message),
        AppError::Internal(message) => classified(
            70,
            "internal",
            "internal_failure",
            message,
            None,
            None,
            None,
            None,
            None,
            None,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn classified(
    exit: i32,
    class: &'static str,
    code: &'static str,
    message: String,
    provenance: Option<AnalysisProvenance>,
    effective_exact_options: Option<ExactSolverOptions>,
    pointer: Option<String>,
    line: Option<u64>,
    column: Option<u64>,
    hint: Option<&'static str>,
) -> ClassifiedError {
    ClassifiedError {
        exit,
        body: ErrorBody {
            class,
            code,
            message: message.clone(),
            provenance,
            effective_exact_options,
        },
        diagnostic: DiagnosticBody {
            class,
            code,
            message,
            pointer,
            line,
            column,
            hint,
        },
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
        CoreError::CatalogGenerationChanged { .. } => "catalog_generation_changed",
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

fn core_pointer(error: &CoreError) -> Option<String> {
    match error {
        CoreError::InvalidJson { pointer, .. } => pointer.clone(),
        _ => None,
    }
}

fn core_location(error: &CoreError) -> Option<(Option<u64>, Option<u64>)> {
    match error {
        CoreError::InvalidJson { line, column, .. } => Some((*line, *column)),
        _ => None,
    }
}

fn core_hint(error: &CoreError) -> Option<&'static str> {
    match error {
        CoreError::InvalidJson { .. } => Some("correct the JSON value at the reported location"),
        CoreError::UnsupportedDocument { .. } => {
            Some("use schema_version 2 with a supported document_type")
        }
        CoreError::PathPolicy { .. } => {
            Some("use a regular JSON file below the selected pinned directory")
        }
        CoreError::CatalogGenerationChanged { .. } => {
            Some("retry after concurrent filesystem mutation has stopped")
        }
        _ => None,
    }
}

fn parse_line_column(message: &str) -> (Option<u64>, Option<u64>) {
    let Some((_, suffix)) = message.rsplit_once(" at line ") else {
        return (None, None);
    };
    let Some((line, column)) = suffix.split_once(" column ") else {
        return (None, None);
    };
    let column = column
        .split(|character: char| !character.is_ascii_digit())
        .next()
        .unwrap_or(column);
    (line.parse().ok(), column.parse().ok())
}
