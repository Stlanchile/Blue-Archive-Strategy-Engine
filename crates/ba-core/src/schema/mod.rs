mod common;
mod dispatch;
mod v2;

pub use common::{
    REWARD_SCHEDULE_DOCUMENT_TYPE, RULESET_DOCUMENT_TYPE, RawBanner, RawInitialCharge,
    RawMilestone, RawProbabilityRatio, RawReward, RawStudent, RawTarget, RawThresholdOverride,
    SCENARIO_DOCUMENT_TYPE,
};
pub use dispatch::{DocumentDispatch, DocumentKind, SCHEMA_VERSION};
pub use v2::{
    RawFundingKind, RawMilestoneV2, RawProvenance, RawProvenanceSource, RawRewardScheduleV2,
    RawRulesetV2, RawScenarioV2, RawStrategyKindV2, RawStrategyV2, VerificationStatus,
};
