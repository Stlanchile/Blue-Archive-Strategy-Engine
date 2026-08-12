#![forbid(unsafe_code)]

pub mod catalog;
pub mod error;
pub mod fingerprint;
pub mod id;
pub mod kernel;
pub mod model;
pub mod ratio;
pub mod resources;
pub mod schema;
pub mod strategy;
pub mod strict_json;

pub use catalog::{
    BundleFingerprints, Catalog, SourcePaths, ValidatedScenarioBundle, ValidationReport,
    load_bundle, validate_document,
};
pub use error::{
    CoreError, CoreErrorClass, MAX_CATALOG_ENTRIES, MAX_DOCUMENT_BYTES, MAX_JSON_DEPTH,
    ObservedSize,
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
pub use model::{
    Banner, CompiledRuleset, Milestone, Reward, RewardSchedule, RulesetMechanics,
    StrategyConfiguration, StrategyConstraints, Target, ValidatedScenario, resource_kind_name,
};
pub use ratio::ProbabilityRatio;
pub use resources::{ResourceKind, Resources};
pub use strategy::{
    DecisionView, SequentialTargetsPreferTickets, Strategy, StrategyDecision, decide,
};
