use serde::Serialize;

use crate::EngineError;

pub const DEFAULT_MAX_ACTIVE_STATES: usize = 65_536;
pub const DEFAULT_MAX_PROCESSED_STATES: u64 = 1_048_576;
pub const DEFAULT_MAX_TRANSITION_EXPANSIONS: u64 = 2_097_152;
pub const DEFAULT_CONSERVATION_TOLERANCE: f64 = 1.0e-12;
pub const DEFAULT_MAX_MONTE_CARLO_RUNS: u64 = 1_000_000;
pub const DEFAULT_MAX_MONTE_CARLO_PRIMITIVES_PER_RUN: u64 = 1_048_576;
pub const DEFAULT_MAX_MONTE_CARLO_TOTAL_PRIMITIVES: u64 = 100_000_000;
pub const DEFAULT_MAX_TRACE_PRIMITIVES: u64 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ExactSolverOptions {
    pub conservation_tolerance: f64,
    pub max_active_states: usize,
    pub max_processed_states: u64,
    pub max_transition_expansions: u64,
}

impl Default for ExactSolverOptions {
    fn default() -> Self {
        Self {
            conservation_tolerance: DEFAULT_CONSERVATION_TOLERANCE,
            max_active_states: DEFAULT_MAX_ACTIVE_STATES,
            max_processed_states: DEFAULT_MAX_PROCESSED_STATES,
            max_transition_expansions: DEFAULT_MAX_TRANSITION_EXPANSIONS,
        }
    }
}

impl ExactSolverOptions {
    pub fn validate(self) -> Result<Self, EngineError> {
        if !self.conservation_tolerance.is_finite()
            || !(1.0e-15..=1.0e-12).contains(&self.conservation_tolerance)
        {
            return Err(EngineError::ProbabilityInvariantViolation {
                message: "conservation_tolerance must be finite and within 1e-15..=1e-12"
                    .to_owned(),
            });
        }
        if self.max_active_states == 0
            || self.max_processed_states == 0
            || self.max_transition_expansions == 0
        {
            return Err(EngineError::InternalInvariantViolation {
                message: "all exact solver guards must be positive".to_owned(),
            });
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SimulationLimits {
    pub max_runs: u64,
    pub max_primitive_transitions_per_run: u64,
    pub max_total_primitive_transitions: u64,
    pub max_trace_primitive_transitions: u64,
}

impl Default for SimulationLimits {
    fn default() -> Self {
        Self {
            max_runs: DEFAULT_MAX_MONTE_CARLO_RUNS,
            max_primitive_transitions_per_run: DEFAULT_MAX_MONTE_CARLO_PRIMITIVES_PER_RUN,
            max_total_primitive_transitions: DEFAULT_MAX_MONTE_CARLO_TOTAL_PRIMITIVES,
            max_trace_primitive_transitions: DEFAULT_MAX_TRACE_PRIMITIVES,
        }
    }
}

impl SimulationLimits {
    pub fn validate(self) -> Result<Self, EngineError> {
        if self.max_runs == 0
            || self.max_primitive_transitions_per_run == 0
            || self.max_total_primitive_transitions == 0
            || self.max_trace_primitive_transitions == 0
        {
            return Err(EngineError::InternalInvariantViolation {
                message: "all simulation work limits must be positive".to_owned(),
            });
        }
        Ok(self)
    }
}
