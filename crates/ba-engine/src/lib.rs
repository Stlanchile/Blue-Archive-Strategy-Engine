#![forbid(unsafe_code)]

mod error;
mod exact;
mod exact_v3;
mod options;
mod result;
mod result_v3;
mod simulation;
mod simulation_v3;

pub use error::{EngineError, EngineErrorClass, ExactAnalysisFailure};
pub use exact::{analyze_exact, analyze_exact_detailed};
pub use exact_v3::analyze_exact_v3;
pub use options::{
    DEFAULT_CONSERVATION_TOLERANCE, DEFAULT_MAX_ACTIVE_STATES,
    DEFAULT_MAX_MONTE_CARLO_PRIMITIVES_PER_RUN, DEFAULT_MAX_MONTE_CARLO_RUNS,
    DEFAULT_MAX_MONTE_CARLO_TOTAL_PRIMITIVES, DEFAULT_MAX_PROCESSED_STATES,
    DEFAULT_MAX_TRACE_PRIMITIVES, DEFAULT_MAX_TRANSITION_EXPANSIONS, ExactSolverOptions,
    SimulationLimits,
};
pub use result::{
    AnalysisContext, AnalysisProvenance, ComparisonResult, CompiledStrategyContext,
    ConfidenceInterval, ENGINE_SEMANTICS_VERSION, EstimateDiagnostics, ExactAnalysisResult,
    ExpectedResources, FirstSuccessProbability, MilestoneReachProbability,
    MonteCarloAnalysisResult, MonteCarloEstimationMetadata, MonteCarloProbabilityIntervals,
    MonteCarloSampleCounts, OwnedTargetProbabilityInterval, OwnedTargetTerminalProbability,
    ProbabilityConservationDiagnostics, RESULT_SCHEMA_VERSION, RNG_ALGORITHM,
    RecruitmentCountProbabilityInterval, ResourceEstimateDiagnostics, RngProvenance, RunTraceEvent,
    RunTraceResult, STREAM_DERIVATION_VERSION, SolverDiagnostics, TerminalReasonProbability,
    TerminalReasonProbabilityInterval,
};
pub use result_v3::{
    AbsoluteMilestoneReachProbabilityV3, AnalysisAuthorityV3, AnalysisContextV3,
    AnalysisProvenanceV3, ComparisonResultV3, CompiledStrategyContextV3,
    ENGINE_SEMANTICS_VERSION_V3, ExactAnalysisResultV3, ExpectedResourcesV3,
    FirstCompletionProbabilityV3, IndicatorProbabilityIntervalV3, MonteCarloAnalysisResultV3,
    MonteCarloEstimationMetadataV3, MonteCarloSampleCountsV3, PrefixCompletionProbabilityV3,
    ProbabilityComparisonV3, ProbabilityConservationDiagnosticsV3, RESULT_SCHEMA_VERSION_V3,
    ResourceEstimateDiagnosticsV3, RunTraceEventV3, RunTraceResultV3,
    TargetAcquisitionProbabilityV3, TerminalOwnedSetProbabilityIntervalV3,
    TerminalOwnedSetProbabilityV3, TerminalReasonProbabilityV3, TerminalSetComparisonV3,
};
pub use simulation::{
    compare, derive_run_seed, replay, replay_with_limits, simulate_monte_carlo,
    simulate_monte_carlo_with_limits, simulate_trace, simulate_trace_with_limits,
};
pub use simulation_v3::{
    compare_v3, derive_run_seed_v3, replay_v3, replay_v3_with_limits, simulate_monte_carlo_v3,
    simulate_monte_carlo_v3_with_limits, simulate_trace_v3, simulate_trace_v3_with_limits,
};
