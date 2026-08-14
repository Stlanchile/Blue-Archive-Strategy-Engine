mod common;
mod dispatch;
mod v2;
mod v3;

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
pub use v3::{
    MAX_EFFECTIVE_MILESTONES_V3, RawClaimBindingV3, RawClaimGroupV3,
    RawCrossTargetProbabilityRowV3, RawCrossTargetProbabilityTableV3, RawCrossTargetThresholdRowV3,
    RawFundingKindV3, RawMilestoneV3, RawOtherTargetWeightV3, RawProvenanceSourceV3,
    RawProvenanceStatusV3, RawProvenanceV3, RawRepeatMilestoneV3, RawRepeatingCycleV3,
    RawRewardScheduleV3, RawRewardV3, RawRulesetV3, RawScenarioAuthorityV3,
    RawScenarioAuthorityValueV3, RawScenarioV3, RawSourceCategoryV3, RawStrategyKindV3,
    RawStrategyV3, RawThresholdOverrideV3,
};
