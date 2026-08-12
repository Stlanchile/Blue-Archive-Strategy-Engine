use ba_core::{
    ActionCompletedEvent, ActionStartedEvent, CompiledStrategy, FundingKind,
    PrimitiveTransitionEvent, Provenance, RecruitOutcome, Resources, Reward,
    SEMANTIC_ENCODING_VERSION, StrategyConstraints, StudentId, TerminalReason,
    ValidatedScenarioBundle, VerificationStatus,
};
use serde::Serialize;

use crate::ExactSolverOptions;

pub const ENGINE_SEMANTICS_VERSION: u64 = 2;
pub const RESULT_SCHEMA_VERSION: u64 = 2;
pub const STREAM_DERIVATION_VERSION: &str = "mc-run-stream-v1";
pub const RNG_ALGORITHM: &str = "chacha8";

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisProvenance {
    pub engine_version: &'static str,
    pub engine_semantics_version: u64,
    pub result_schema_version: u64,
    pub semantic_encoding_version: &'static str,
    pub scenario_id: String,
    pub scenario_schema_version: u64,
    pub scenario_behavior_fingerprint: String,
    pub scenario_document_fingerprint: String,
    pub ruleset_id: String,
    pub ruleset_schema_version: u64,
    pub ruleset_behavior_fingerprint: String,
    pub ruleset_document_fingerprint: String,
    pub ruleset_verification_status: VerificationStatus,
    pub ruleset_provenance: Provenance,
    pub reward_schedule_id: String,
    pub reward_schedule_schema_version: u64,
    pub reward_schedule_behavior_fingerprint: String,
    pub reward_schedule_document_fingerprint: String,
    pub reward_schedule_verification_status: VerificationStatus,
    pub reward_schedule_provenance: Provenance,
}

impl AnalysisProvenance {
    #[must_use]
    pub fn from_bundle(bundle: &ValidatedScenarioBundle) -> Self {
        Self {
            engine_version: env!("CARGO_PKG_VERSION"),
            engine_semantics_version: ENGINE_SEMANTICS_VERSION,
            result_schema_version: RESULT_SCHEMA_VERSION,
            semantic_encoding_version: SEMANTIC_ENCODING_VERSION,
            scenario_id: bundle.scenario().id().to_string(),
            ruleset_id: bundle.ruleset().id().to_string(),
            reward_schedule_id: bundle.reward_schedule().id().to_string(),
            scenario_schema_version: bundle.scenario().schema_version(),
            ruleset_schema_version: bundle.ruleset().schema_version(),
            reward_schedule_schema_version: bundle.reward_schedule().schema_version(),
            scenario_behavior_fingerprint: bundle.fingerprints().scenario.to_hex(),
            scenario_document_fingerprint: bundle.fingerprints().scenario_document.to_hex(),
            ruleset_behavior_fingerprint: bundle.fingerprints().ruleset.to_hex(),
            ruleset_document_fingerprint: bundle.fingerprints().ruleset_document.to_hex(),
            reward_schedule_behavior_fingerprint: bundle.fingerprints().reward_schedule.to_hex(),
            reward_schedule_document_fingerprint: bundle
                .fingerprints()
                .reward_schedule_document
                .to_hex(),
            ruleset_verification_status: bundle.ruleset().provenance().verification_status,
            reward_schedule_verification_status: bundle
                .reward_schedule()
                .provenance()
                .verification_status,
            ruleset_provenance: bundle.ruleset().provenance().clone(),
            reward_schedule_provenance: bundle.reward_schedule().provenance().clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ExpectedResources {
    pub pyroxene: f64,
    pub limited_ten_recruitment_tickets: f64,
    pub eligma: f64,
    pub advanced_bd_selectors: f64,
    pub advanced_tech_note_selectors: f64,
    pub superior_tech_note_selectors: f64,
    pub gift_boxes: f64,
}

impl ExpectedResources {
    pub(crate) fn add_weighted(&mut self, resources: Resources, weight: f64) {
        self.pyroxene += resources.pyroxene as f64 * weight;
        self.limited_ten_recruitment_tickets +=
            resources.limited_ten_recruitment_tickets as f64 * weight;
        self.eligma += resources.eligma as f64 * weight;
        self.advanced_bd_selectors += resources.advanced_bd_selectors as f64 * weight;
        self.advanced_tech_note_selectors += resources.advanced_tech_note_selectors as f64 * weight;
        self.superior_tech_note_selectors += resources.superior_tech_note_selectors as f64 * weight;
        self.gift_boxes += resources.gift_boxes as f64 * weight;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OwnedTargetTerminalProbability {
    pub owned_targets: Vec<StudentId>,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalReasonProbability {
    pub terminal_reason: TerminalReason,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneReachProbability {
    pub recruitment_count: u64,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirstSuccessProbability {
    pub recruitment_count: u64,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbabilityConservationDiagnostics {
    pub maximum_observed_deviation: f64,
    pub final_terminal_probability: f64,
    pub first_success_probability: f64,
    pub first_success_success_deviation: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SolverDiagnostics {
    pub peak_boundary_frontier: usize,
    pub peak_in_flight_frontier: usize,
    pub processed_states: u64,
    pub transition_expansions: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisContext {
    pub strategy_constraints: StrategyConstraints,
    pub compiled_strategy: CompiledStrategyContext,
    pub initial_resources: Resources,
    pub ordered_targets: Vec<StudentId>,
    pub ordered_banners: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledStrategyContext {
    pub strategy_schema_version: u64,
    pub strategy_id: String,
    pub kind: &'static str,
    pub funding_priority: [FundingKind; 2],
    pub max_total_recruitments: u64,
}

impl AnalysisContext {
    #[must_use]
    pub fn from_bundle(bundle: &ValidatedScenarioBundle) -> Self {
        let compiled_strategy = match bundle.compiled_strategy() {
            CompiledStrategy::SequentialTargetsV2 {
                strategy_schema_version,
                strategy_id,
                funding_priority,
                max_total_recruitments,
            } => CompiledStrategyContext {
                strategy_schema_version: *strategy_schema_version,
                strategy_id: strategy_id.to_string(),
                kind: "sequential_targets",
                funding_priority: *funding_priority,
                max_total_recruitments: max_total_recruitments.get(),
            },
        };
        Self {
            strategy_constraints: bundle.scenario().strategy().constraints.clone(),
            initial_resources: bundle.scenario().initial_resources(),
            ordered_targets: bundle
                .scenario()
                .targets()
                .iter()
                .map(|target| target.student_id.clone())
                .collect(),
            ordered_banners: bundle
                .scenario()
                .targets()
                .iter()
                .map(|target| target.banner_id.to_string())
                .collect(),
            compiled_strategy,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExactAnalysisResult {
    pub engine_kind: &'static str,
    pub provenance: AnalysisProvenance,
    pub context: AnalysisContext,
    pub exact_options: ExactSolverOptions,
    pub success_probability: f64,
    pub owned_target_terminal_probabilities: Vec<OwnedTargetTerminalProbability>,
    pub terminal_reason_probabilities: Vec<TerminalReasonProbability>,
    pub expected_terminal_primitive_recruitments: f64,
    pub expected_terminal_primitive_recruitments_given_success: Option<f64>,
    pub expected_first_success_recruitment_count_given_success: Option<f64>,
    pub expected_paid_pyroxene_spent: f64,
    pub expected_ticket_funded_primitive_recruitments: f64,
    pub expected_residual_resources: ExpectedResources,
    pub expected_milestone_rewards_acquired: ExpectedResources,
    pub milestone_reach_probabilities: Vec<MilestoneReachProbability>,
    pub first_success_pmf: Vec<FirstSuccessProbability>,
    pub first_success_cdf: Vec<FirstSuccessProbability>,
    pub probability_conservation: ProbabilityConservationDiagnostics,
    pub solver_diagnostics: SolverDiagnostics,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ConfidenceInterval {
    pub lower: f64,
    pub upper: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EstimateDiagnostics {
    pub standard_error: f64,
    pub confidence_interval_95: Option<ConfidenceInterval>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloEstimationMetadata {
    pub success_probability_interval_95: ConfidenceInterval,
    pub expected_terminal_primitive_recruitments: EstimateDiagnostics,
    pub expected_terminal_primitive_recruitments_given_success: Option<EstimateDiagnostics>,
    pub expected_first_success_recruitment_count_given_success: Option<EstimateDiagnostics>,
    pub expected_paid_pyroxene_spent: EstimateDiagnostics,
    pub expected_ticket_funded_primitive_recruitments: EstimateDiagnostics,
    pub expected_residual_resources: ResourceEstimateDiagnostics,
    pub expected_milestone_rewards_acquired: ResourceEstimateDiagnostics,
    pub probability_intervals_95: MonteCarloProbabilityIntervals,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceEstimateDiagnostics {
    pub pyroxene: EstimateDiagnostics,
    pub limited_ten_recruitment_tickets: EstimateDiagnostics,
    pub eligma: EstimateDiagnostics,
    pub advanced_bd_selectors: EstimateDiagnostics,
    pub advanced_tech_note_selectors: EstimateDiagnostics,
    pub superior_tech_note_selectors: EstimateDiagnostics,
    pub gift_boxes: EstimateDiagnostics,
}

#[derive(Debug, Clone, Serialize)]
pub struct OwnedTargetProbabilityInterval {
    pub owned_targets: Vec<StudentId>,
    pub sample_count: u64,
    pub confidence_interval_95: ConfidenceInterval,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalReasonProbabilityInterval {
    pub terminal_reason: TerminalReason,
    pub sample_count: u64,
    pub confidence_interval_95: ConfidenceInterval,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecruitmentCountProbabilityInterval {
    pub recruitment_count: u64,
    pub sample_count: u64,
    pub confidence_interval_95: ConfidenceInterval,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloProbabilityIntervals {
    pub owned_target_terminal_probabilities: Vec<OwnedTargetProbabilityInterval>,
    pub terminal_reason_probabilities: Vec<TerminalReasonProbabilityInterval>,
    pub milestone_reach_probabilities: Vec<RecruitmentCountProbabilityInterval>,
    pub first_success_pmf: Vec<RecruitmentCountProbabilityInterval>,
    pub first_success_cdf: Vec<RecruitmentCountProbabilityInterval>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RngProvenance {
    pub rng_algorithm: &'static str,
    pub master_seed: u64,
    pub run_count: u64,
    pub run_index_contract: &'static str,
    pub stream_derivation_version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloAnalysisResult {
    pub engine_kind: &'static str,
    pub provenance: AnalysisProvenance,
    pub context: AnalysisContext,
    pub rng: RngProvenance,
    pub success_probability: f64,
    pub owned_target_terminal_probabilities: Vec<OwnedTargetTerminalProbability>,
    pub terminal_reason_probabilities: Vec<TerminalReasonProbability>,
    pub expected_terminal_primitive_recruitments: f64,
    pub expected_terminal_primitive_recruitments_given_success: Option<f64>,
    pub expected_first_success_recruitment_count_given_success: Option<f64>,
    pub expected_paid_pyroxene_spent: f64,
    pub expected_ticket_funded_primitive_recruitments: f64,
    pub expected_residual_resources: ExpectedResources,
    pub expected_milestone_rewards_acquired: ExpectedResources,
    pub milestone_reach_probabilities: Vec<MilestoneReachProbability>,
    pub first_success_pmf: Vec<FirstSuccessProbability>,
    pub first_success_cdf: Vec<FirstSuccessProbability>,
    pub sample_counts: MonteCarloSampleCounts,
    pub estimation: MonteCarloEstimationMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloSampleCounts {
    pub total_runs: u64,
    pub successful_runs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComparisonResult {
    pub engine_kind: &'static str,
    pub exact: ExactAnalysisResult,
    pub monte_carlo: MonteCarloAnalysisResult,
    pub success_probability_difference: f64,
    pub success_probability_within_monte_carlo_interval: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum RunTraceEvent {
    ActionStarted(ActionStartedEvent),
    PrimitiveTransition(PrimitiveTransitionEvent),
    RewardGranted {
        recruitment_count: u64,
        rewards: Vec<Reward>,
    },
    FirstSuccess {
        recruitment_count: u64,
    },
    ActionCompleted(ActionCompletedEvent),
    Terminal {
        terminal_reason: TerminalReason,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTraceResult {
    pub engine_kind: &'static str,
    pub provenance: AnalysisProvenance,
    pub context: AnalysisContext,
    pub rng: Option<RngProvenance>,
    pub terminal_primitive_recruitments: u64,
    pub first_success_recruitment_count: Option<u64>,
    pub paid_pyroxene_spent: u64,
    pub ticket_funded_primitive_recruitments: u64,
    pub terminal_resources: Resources,
    pub milestone_rewards_acquired: Resources,
    pub terminal_owned_targets: Vec<StudentId>,
    pub terminal_reason: TerminalReason,
    pub replay_outcomes: Vec<RecruitOutcome>,
    pub events: Vec<RunTraceEvent>,
}
