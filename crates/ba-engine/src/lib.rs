#![forbid(unsafe_code)]

mod error;
mod exact;
mod options;
mod result;
mod simulation;

pub use error::{EngineError, EngineErrorClass, ExactAnalysisFailure};
pub use exact::{analyze_exact, analyze_exact_detailed};
pub use options::{
    DEFAULT_CONSERVATION_TOLERANCE, DEFAULT_MAX_ACTIVE_STATES,
    DEFAULT_MAX_MONTE_CARLO_PRIMITIVES_PER_RUN, DEFAULT_MAX_MONTE_CARLO_RUNS,
    DEFAULT_MAX_MONTE_CARLO_TOTAL_PRIMITIVES, DEFAULT_MAX_PROCESSED_STATES,
    DEFAULT_MAX_TRACE_PRIMITIVES, DEFAULT_MAX_TRANSITION_EXPANSIONS, ExactSolverOptions,
    SimulationLimits,
};
pub use result::{
    AnalysisContext, AnalysisProvenance, ComparisonResult, ConfidenceInterval,
    ENGINE_SEMANTICS_VERSION, EstimateDiagnostics, ExactAnalysisResult, ExpectedResources,
    FirstSuccessProbability, MilestoneReachProbability, MonteCarloAnalysisResult,
    MonteCarloEstimationMetadata, MonteCarloProbabilityIntervals, MonteCarloSampleCounts,
    OwnedTargetProbabilityInterval, OwnedTargetTerminalProbability,
    ProbabilityConservationDiagnostics, RESULT_SCHEMA_VERSION, RNG_ALGORITHM,
    RecruitmentCountProbabilityInterval, ResourceEstimateDiagnostics, RngProvenance, RunTraceEvent,
    RunTraceResult, STREAM_DERIVATION_VERSION, SolverDiagnostics, TerminalReasonProbability,
    TerminalReasonProbabilityInterval,
};
pub use simulation::{
    compare, derive_run_seed, replay, replay_with_limits, simulate_monte_carlo,
    simulate_monte_carlo_with_limits, simulate_trace, simulate_trace_with_limits,
};
