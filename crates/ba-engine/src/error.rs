use ba_core::CoreError;
use thiserror::Error;

use crate::{AnalysisProvenance, ExactSolverOptions};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("active-state limit exceeded: observed {observed}, maximum {maximum}")]
    SolverStateLimitExceeded { observed: usize, maximum: usize },

    #[error("processed-state limit exceeded: observed {observed}, maximum {maximum}")]
    SolverProcessedStateLimitExceeded { observed: u64, maximum: u64 },

    #[error("transition-expansion limit exceeded: observed {observed}, maximum {maximum}")]
    SolverTransitionLimitExceeded { observed: u64, maximum: u64 },

    #[error("Monte Carlo run limit exceeded: requested {requested}, maximum {maximum}")]
    SimulationRunLimitExceeded { requested: u64, maximum: u64 },

    #[error(
        "simulation primitive-transition limit exceeded for {scope}: observed {observed}, maximum {maximum}"
    )]
    SimulationPrimitiveLimitExceeded {
        scope: &'static str,
        observed: u64,
        maximum: u64,
    },

    #[error("probability invariant violation: {message}")]
    ProbabilityInvariantViolation { message: String },

    #[error("arithmetic overflow while {context}")]
    ArithmeticOverflow { context: &'static str },

    #[error("invalid strategy action: {message}")]
    InvalidAction { message: String },

    #[error("invalid transition: {message}")]
    InvalidTransition { message: String },

    #[error("internal invariant violation: {message}")]
    InternalInvariantViolation { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineErrorClass {
    GuardOrInvariant,
    Internal,
}

impl EngineError {
    #[must_use]
    pub const fn class(&self) -> EngineErrorClass {
        match self {
            Self::SolverStateLimitExceeded { .. }
            | Self::SolverProcessedStateLimitExceeded { .. }
            | Self::SolverTransitionLimitExceeded { .. }
            | Self::SimulationRunLimitExceeded { .. }
            | Self::SimulationPrimitiveLimitExceeded { .. }
            | Self::ProbabilityInvariantViolation { .. }
            | Self::ArithmeticOverflow { .. }
            | Self::InvalidAction { .. }
            | Self::InvalidTransition { .. } => EngineErrorClass::GuardOrInvariant,
            Self::InternalInvariantViolation { .. } => EngineErrorClass::Internal,
        }
    }
}

impl From<CoreError> for EngineError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::ArithmeticOverflow { context } => Self::ArithmeticOverflow { context },
            CoreError::InvalidAction { message } => Self::InvalidAction { message },
            CoreError::InvalidTransition { message } => Self::InvalidTransition { message },
            CoreError::InternalInvariant { message } => {
                Self::InternalInvariantViolation { message }
            }
            other => Self::InternalInvariantViolation {
                message: format!("validated bundle produced an unexpected core error: {other}"),
            },
        }
    }
}

#[derive(Debug)]
pub struct ExactAnalysisFailure {
    pub error: EngineError,
    pub effective_options: ExactSolverOptions,
    pub provenance: Box<AnalysisProvenance>,
}

impl std::fmt::Display for ExactAnalysisFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for ExactAnalysisFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
