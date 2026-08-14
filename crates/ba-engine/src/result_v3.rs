use ba_core::{
    ActionCompletedEvent, ActionStartedEvent, FundingKind, PrimitiveAcquisition,
    PrimitiveTransitionEventV3, ProvenanceStatusV3, ProvenanceV3, ResourceLedger, ResourcesV3,
    RewardV3, SEMANTIC_ENCODING_VERSION, StudentId, TerminalReason, ValidatedScenarioBundleV3,
};
use serde::Serialize;

use crate::ExactSolverOptions;
use crate::result::{ConfidenceInterval, EstimateDiagnostics, RngProvenance, SolverDiagnostics};

pub const ENGINE_SEMANTICS_VERSION_V3: u64 = 3;
pub const RESULT_SCHEMA_VERSION_V3: u64 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisAuthorityV3 {
    pub scenario: &'static str,
    pub featured_target_probabilities: &'static str,
    pub cross_target_probabilities: &'static str,
    pub banner_topology: &'static str,
    pub target_order: &'static str,
    pub initial_state: &'static str,
    pub strategy: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisProvenanceV3 {
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
    pub ruleset_provenance_status: ProvenanceStatusV3,
    pub ruleset_provenance: ProvenanceV3,
    pub reward_schedule_id: String,
    pub reward_schedule_schema_version: u64,
    pub reward_schedule_behavior_fingerprint: String,
    pub reward_schedule_document_fingerprint: String,
    pub reward_schedule_provenance_status: ProvenanceStatusV3,
    pub reward_schedule_provenance: ProvenanceV3,
    pub authority: AnalysisAuthorityV3,
}

impl AnalysisProvenanceV3 {
    #[must_use]
    pub fn from_bundle(bundle: &ValidatedScenarioBundleV3) -> Self {
        Self {
            engine_version: env!("CARGO_PKG_VERSION"),
            engine_semantics_version: ENGINE_SEMANTICS_VERSION_V3,
            result_schema_version: RESULT_SCHEMA_VERSION_V3,
            semantic_encoding_version: SEMANTIC_ENCODING_VERSION,
            scenario_id: bundle.scenario().id().to_string(),
            scenario_schema_version: bundle.scenario().schema_version(),
            scenario_behavior_fingerprint: bundle.fingerprints().scenario.to_hex(),
            scenario_document_fingerprint: bundle.fingerprints().scenario_document.to_hex(),
            ruleset_id: bundle.ruleset().id().to_string(),
            ruleset_schema_version: bundle.ruleset().schema_version(),
            ruleset_behavior_fingerprint: bundle.fingerprints().ruleset.to_hex(),
            ruleset_document_fingerprint: bundle.fingerprints().ruleset_document.to_hex(),
            ruleset_provenance_status: bundle.ruleset().provenance().provenance_status,
            ruleset_provenance: bundle.ruleset().provenance().clone(),
            reward_schedule_id: bundle.reward_schedule().id().to_string(),
            reward_schedule_schema_version: bundle.reward_schedule().schema_version(),
            reward_schedule_behavior_fingerprint: bundle.fingerprints().reward_schedule.to_hex(),
            reward_schedule_document_fingerprint: bundle
                .fingerprints()
                .reward_schedule_document
                .to_hex(),
            reward_schedule_provenance_status: bundle
                .reward_schedule()
                .provenance()
                .provenance_status,
            reward_schedule_provenance: bundle.reward_schedule().provenance().clone(),
            authority: AnalysisAuthorityV3 {
                scenario: "scenario_document_user_authored",
                featured_target_probabilities: "ruleset_document",
                cross_target_probabilities: "scenario_document_user_authored",
                banner_topology: "scenario_document_user_authored",
                target_order: "scenario_document_user_authored",
                initial_state: "scenario_document_user_authored",
                strategy: "scenario_document_user_authored",
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledStrategyContextV3 {
    pub strategy_schema_version: u64,
    pub strategy_id: String,
    pub kind: &'static str,
    pub funding_priority: [FundingKind; 2],
    pub max_additional_recruitments: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChargeGroupTopologyV3 {
    pub banner_id: String,
    pub charge_group_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitialChargeV3 {
    pub charge_group_id: String,
    pub pre_recruitment_charge: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisContextV3 {
    pub compiled_strategy: CompiledStrategyContextV3,
    pub initial_resources: ResourcesV3,
    pub initial_campaign_recruitment_count: u64,
    pub maximum_additional_recruitments: u64,
    pub maximum_absolute_campaign_recruitment_count: u64,
    pub ordered_target_ids: Vec<StudentId>,
    pub ordered_banner_ids: Vec<String>,
    pub charge_group_topology: Vec<ChargeGroupTopologyV3>,
    pub initial_owned_targets: Vec<StudentId>,
    pub initial_charges: Vec<InitialChargeV3>,
    pub schedule_kind: &'static str,
    pub effective_future_milestone_count: usize,
}

impl AnalysisContextV3 {
    #[must_use]
    pub fn from_bundle(bundle: &ValidatedScenarioBundleV3) -> Self {
        let strategy = bundle.compiled_strategy();
        Self {
            compiled_strategy: CompiledStrategyContextV3 {
                strategy_schema_version: strategy.strategy_schema_version,
                strategy_id: strategy.strategy_id.to_string(),
                kind: "sequential_targets",
                funding_priority: strategy.funding_priority,
                max_additional_recruitments: strategy.max_additional_recruitments.get(),
            },
            initial_resources: ResourcesV3::from(bundle.scenario().initial_resources()),
            initial_campaign_recruitment_count: bundle.scenario().initial_recruitment_count(),
            maximum_additional_recruitments: strategy.max_additional_recruitments.get(),
            maximum_absolute_campaign_recruitment_count: bundle
                .scenario()
                .maximum_absolute_campaign_count(),
            ordered_target_ids: bundle
                .scenario()
                .targets()
                .iter()
                .map(|target| target.student_id.clone())
                .collect(),
            ordered_banner_ids: bundle
                .scenario()
                .targets()
                .iter()
                .map(|target| target.banner_id.to_string())
                .collect(),
            charge_group_topology: bundle
                .scenario()
                .banners()
                .iter()
                .map(|banner| ChargeGroupTopologyV3 {
                    banner_id: banner.banner_id.to_string(),
                    charge_group_id: banner.charge_group_id.to_string(),
                })
                .collect(),
            initial_owned_targets: bundle.scenario().initial_owned_targets().to_vec(),
            initial_charges: bundle
                .scenario()
                .charge_groups()
                .iter()
                .zip(bundle.scenario().initial_charges())
                .map(|(group, charge)| InitialChargeV3 {
                    charge_group_id: group.to_string(),
                    pre_recruitment_charge: *charge,
                })
                .collect(),
            schedule_kind: if bundle.reward_schedule().repeating_cycle().is_some() {
                "repeating"
            } else {
                "finite"
            },
            effective_future_milestone_count: bundle.scenario().effective_milestones().len(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ExpectedResourcesV3 {
    pub pyroxene: f64,
    pub limited_ten_recruitment_tickets: f64,
    pub eligma: f64,
    pub advanced_bd_selectors: f64,
    pub advanced_tech_note_selectors: f64,
    pub superior_tech_note_selectors: f64,
    pub gift_boxes: f64,
    pub keystone_fragments: f64,
    pub secret_tech_notes: f64,
    pub superior_bd_selectors: f64,
    pub high_grade_gift_boxes: f64,
}

impl ExpectedResourcesV3 {
    pub(crate) fn from_sums(values: [f64; 11]) -> Self {
        Self {
            pyroxene: values[0],
            limited_ten_recruitment_tickets: values[1],
            eligma: values[2],
            advanced_bd_selectors: values[3],
            advanced_tech_note_selectors: values[4],
            superior_tech_note_selectors: values[5],
            gift_boxes: values[6],
            keystone_fragments: values[7],
            secret_tech_notes: values[8],
            superior_bd_selectors: values[9],
            high_grade_gift_boxes: values[10],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalOwnedSetProbabilityV3 {
    pub owned_targets: Vec<StudentId>,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetAcquisitionProbabilityV3 {
    pub target_id: StudentId,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrefixCompletionProbabilityV3 {
    pub prefix_length: usize,
    pub target_ids: Vec<StudentId>,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalReasonProbabilityV3 {
    pub terminal_reason: TerminalReason,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbsoluteMilestoneReachProbabilityV3 {
    pub absolute_campaign_recruitment_count: u64,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirstCompletionProbabilityV3 {
    pub additional_recruitment_count: u64,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbabilityConservationDiagnosticsV3 {
    pub maximum_observed_deviation: f64,
    pub final_terminal_probability: f64,
    pub first_all_target_completion_probability: f64,
    pub first_completion_success_deviation: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExactAnalysisResultV3 {
    pub engine_kind: &'static str,
    pub provenance: AnalysisProvenanceV3,
    pub context: AnalysisContextV3,
    pub exact_options: ExactSolverOptions,
    pub all_target_success_probability: f64,
    pub terminal_owned_set_probabilities: Vec<TerminalOwnedSetProbabilityV3>,
    pub terminal_reason_probabilities: Vec<TerminalReasonProbabilityV3>,
    pub per_target_acquisition_probabilities: Vec<TargetAcquisitionProbabilityV3>,
    pub ordered_prefix_completion_probabilities: Vec<PrefixCompletionProbabilityV3>,
    pub expected_additional_primitive_recruitments: f64,
    pub expected_additional_primitive_recruitments_given_success: Option<f64>,
    pub expected_first_all_target_completion_count_given_success: Option<f64>,
    pub expected_paid_pyroxene_spent: f64,
    pub expected_ticket_funded_primitive_recruitments: f64,
    pub expected_residual_resources: ExpectedResourcesV3,
    pub expected_milestone_rewards_acquired: ExpectedResourcesV3,
    pub absolute_campaign_milestone_reach_probabilities: Vec<AbsoluteMilestoneReachProbabilityV3>,
    pub first_all_target_completion_pmf: Vec<FirstCompletionProbabilityV3>,
    pub first_all_target_completion_cdf: Vec<FirstCompletionProbabilityV3>,
    pub probability_conservation: ProbabilityConservationDiagnosticsV3,
    pub solver_diagnostics: SolverDiagnostics,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndicatorProbabilityIntervalV3 {
    pub id: String,
    pub sample_count: u64,
    pub confidence_interval_95: ConfidenceInterval,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalOwnedSetProbabilityIntervalV3 {
    pub owned_targets: Vec<StudentId>,
    pub sample_count: u64,
    pub confidence_interval_95: ConfidenceInterval,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceEstimateDiagnosticsV3 {
    pub pyroxene: EstimateDiagnostics,
    pub limited_ten_recruitment_tickets: EstimateDiagnostics,
    pub eligma: EstimateDiagnostics,
    pub advanced_bd_selectors: EstimateDiagnostics,
    pub advanced_tech_note_selectors: EstimateDiagnostics,
    pub superior_tech_note_selectors: EstimateDiagnostics,
    pub gift_boxes: EstimateDiagnostics,
    pub keystone_fragments: EstimateDiagnostics,
    pub secret_tech_notes: EstimateDiagnostics,
    pub superior_bd_selectors: EstimateDiagnostics,
    pub high_grade_gift_boxes: EstimateDiagnostics,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloEstimationMetadataV3 {
    pub all_target_success_probability_interval_95: ConfidenceInterval,
    pub per_target_probability_intervals_95: Vec<IndicatorProbabilityIntervalV3>,
    pub ordered_prefix_probability_intervals_95: Vec<IndicatorProbabilityIntervalV3>,
    pub terminal_owned_set_probability_intervals_95: Vec<TerminalOwnedSetProbabilityIntervalV3>,
    pub expected_additional_primitive_recruitments: EstimateDiagnostics,
    pub expected_residual_resources: ResourceEstimateDiagnosticsV3,
    pub expected_milestone_rewards_acquired: ResourceEstimateDiagnosticsV3,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloSampleCountsV3 {
    pub total_runs: u64,
    pub successful_runs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonteCarloAnalysisResultV3 {
    pub engine_kind: &'static str,
    pub provenance: AnalysisProvenanceV3,
    pub context: AnalysisContextV3,
    pub rng: RngProvenance,
    pub all_target_success_probability: f64,
    pub terminal_owned_set_probabilities: Vec<TerminalOwnedSetProbabilityV3>,
    pub terminal_reason_probabilities: Vec<TerminalReasonProbabilityV3>,
    pub per_target_acquisition_probabilities: Vec<TargetAcquisitionProbabilityV3>,
    pub ordered_prefix_completion_probabilities: Vec<PrefixCompletionProbabilityV3>,
    pub expected_additional_primitive_recruitments: f64,
    pub expected_additional_primitive_recruitments_given_success: Option<f64>,
    pub expected_first_all_target_completion_count_given_success: Option<f64>,
    pub expected_paid_pyroxene_spent: f64,
    pub expected_ticket_funded_primitive_recruitments: f64,
    pub expected_residual_resources: ExpectedResourcesV3,
    pub expected_milestone_rewards_acquired: ExpectedResourcesV3,
    pub absolute_campaign_milestone_reach_probabilities: Vec<AbsoluteMilestoneReachProbabilityV3>,
    pub first_all_target_completion_pmf: Vec<FirstCompletionProbabilityV3>,
    pub first_all_target_completion_cdf: Vec<FirstCompletionProbabilityV3>,
    pub sample_counts: MonteCarloSampleCountsV3,
    pub estimation: MonteCarloEstimationMetadataV3,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbabilityComparisonV3 {
    pub id: String,
    pub simulation_minus_exact: f64,
    pub exact_within_monte_carlo_interval: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalSetComparisonV3 {
    pub owned_targets: Vec<StudentId>,
    pub simulation_minus_exact: f64,
    pub exact_within_monte_carlo_interval: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComparisonResultV3 {
    pub engine_kind: &'static str,
    pub exact: ExactAnalysisResultV3,
    pub monte_carlo: MonteCarloAnalysisResultV3,
    pub all_target_success: ProbabilityComparisonV3,
    pub per_target: Vec<ProbabilityComparisonV3>,
    pub ordered_prefixes: Vec<ProbabilityComparisonV3>,
    pub terminal_owned_sets: Vec<TerminalSetComparisonV3>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum RunTraceEventV3 {
    ActionStarted(ActionStartedEvent),
    PrimitiveTransition(PrimitiveTransitionEventV3),
    RewardGranted {
        absolute_campaign_recruitment_count: u64,
        rewards: Vec<RewardV3>,
    },
    FirstAllTargetsCompleted {
        additional_recruitment_count: u64,
        absolute_campaign_recruitment_count: u64,
    },
    ActionCompleted(ActionCompletedEvent),
    Terminal {
        terminal_reason: TerminalReason,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTraceResultV3 {
    pub engine_kind: &'static str,
    pub provenance: AnalysisProvenanceV3,
    pub context: AnalysisContextV3,
    pub rng: Option<RngProvenance>,
    pub terminal_additional_primitive_recruitments: u64,
    pub terminal_absolute_campaign_recruitment_count: u64,
    pub first_all_target_completion_additional_count: Option<u64>,
    pub paid_pyroxene_spent: u64,
    pub ticket_funded_primitive_recruitments: u64,
    pub terminal_resources: ResourcesV3,
    pub milestone_rewards_acquired: ResourcesV3,
    pub terminal_owned_targets: Vec<StudentId>,
    pub terminal_reason: TerminalReason,
    pub replay_outcomes: Vec<PrimitiveAcquisition>,
    pub events: Vec<RunTraceEventV3>,
}

pub(crate) fn expected_from_ledger_sums(sums: [f64; 11]) -> ExpectedResourcesV3 {
    ExpectedResourcesV3::from_sums(sums)
}

pub(crate) fn ledger_values(resources: ResourceLedger) -> [u64; 11] {
    *resources.as_values()
}
