#![forbid(unsafe_code)]

pub mod catalog;
pub mod document;
pub mod error;
pub mod fingerprint;
mod fs_secure;
pub mod id;
pub mod kernel;
pub mod kernel_v3;
pub mod model;
pub mod model_v3;
pub mod ownership;
pub mod probability_v3;
pub mod profile;
pub mod provenance_v3;
pub mod ratio;
pub mod resources;
pub mod reward_schedule_v3;
pub mod schema;
pub mod strategy;
pub mod strategy_v3;
pub mod strict_json;

pub use catalog::{
    AnyValidatedScenarioBundle, BundleFingerprints, Catalog, SourcePaths, ValidatedScenarioBundle,
    ValidatedScenarioBundleV3, ValidationReport, compile_any_buffered_bundle,
    compile_buffered_bundle, load_any_buffered_bundle, load_any_bundle, load_buffered_bundle,
    load_bundle, validate_document,
};
pub use error::{
    CoreError, CoreErrorClass, MAX_CATALOG_DIRECTORY_ENTRIES, MAX_CATALOG_ENTRIES,
    MAX_DOCUMENT_BYTES, MAX_JSON_DEPTH, ObservedSize,
};
pub use fingerprint::{CanonicalNode, SEMANTIC_ENCODING_VERSION, SemanticFingerprint};
pub use id::{
    BannerId, ChargeGroupId, RewardScheduleId, RulesetId, ScenarioId, StrategyId, StudentId,
};
pub use kernel::{
    ActionCompletedEvent, ActionFundingKind, ActionStartedEvent, InFlightStateKey, OutcomeBranch,
    PrimitiveTransitionEvent, ReconstructedFunding, RecruitOutcome, RequestedAction,
    TerminalReason, TransitionResult, WorldStateKey, apply_primitive_transition, begin_action,
    complete_action, initial_world, milestone_rewards_acquired, outcome_distribution,
    reconstruct_funding, terminal_resources,
};
pub use kernel_v3::{
    PrimitiveTransitionEventV3, TransitionResultV3, apply_primitive_transition_v3, begin_action_v3,
    complete_action_v3, initial_world_v3, milestone_rewards_acquired_v3, outcome_distribution_v3,
    reconstruct_funding_v3, terminal_resources_v3,
};
pub use model::{
    Banner, CompiledRuleset, CompiledStrategy, FundingKind, Milestone, Provenance,
    ProvenanceSource, Reward, RewardSchedule, RulesetMechanics, StrategyConfiguration,
    StrategyConstraints, Target, ValidatedScenario, resource_kind_name,
};
pub use model_v3::{
    BannerProbabilityProfileV3, CompiledRulesetV3, CompiledStrategyV3, OriginalProbabilityRowV3,
    RulesetMechanicsV3, ScenarioAuthorityV3, ValidatedScenarioV3,
};
pub use ownership::{OwnershipMask, TargetIndex};
pub use probability_v3::{
    CompiledOutcomeBranch, CompiledOutcomeDistribution, PrimitiveAcquisition,
};
pub use profile::{
    DOCUMENT_SCHEMA_VERSION_V2, DOCUMENT_SCHEMA_VERSION_V3, DocumentProfile,
    STRATEGY_SCHEMA_VERSION_V2, STRATEGY_SCHEMA_VERSION_V3,
};
pub use provenance_v3::{
    ClaimBindingV3, ClaimGroupV3, ProvenanceSourceV3, ProvenanceStatusV3, ProvenanceSubjectV3,
    ProvenanceV3, REWARD_SCHEDULE_CLAIM_GROUPS_V3, RULESET_CLAIM_GROUPS_V3, SourceCategoryV3,
};
pub use ratio::ProbabilityRatio;
pub use resources::{
    LedgerResourceKind, RESOURCE_KINDS_V3, RawResourceKindV2, RawResourceKindV3, ResourceKind,
    ResourceLedger, Resources, ResourcesV3, resource_kind_name_v3,
};
pub use reward_schedule_v3::{
    MilestoneV3, RepeatMilestoneV3, RepeatingCycleV3, RewardScheduleV3, RewardV3,
};
pub use schema::{DocumentKind, VerificationStatus};
pub use strategy::{
    DecisionView, SequentialTargetsPreferTickets, Strategy, StrategyDecision, decide,
};
pub use strategy_v3::decide_v3;
