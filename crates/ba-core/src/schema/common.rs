use serde::Deserialize;

use crate::ResourceKind;

pub const RULESET_DOCUMENT_TYPE: &str = "ruleset";
pub const REWARD_SCHEDULE_DOCUMENT_TYPE: &str = "reward_schedule";
pub const SCENARIO_DOCUMENT_TYPE: &str = "scenario";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawProbabilityRatio {
    pub numerator: u64,
    pub denominator: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawThresholdOverride {
    pub pre_charge: u64,
    pub pickup_probability: RawProbabilityRatio,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawReward {
    pub resource: ResourceKind,
    pub quantity: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMilestone {
    pub count: u64,
    pub rewards: Vec<RawReward>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStudent {
    pub student_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawBanner {
    pub banner_id: String,
    pub featured_student_id: String,
    pub charge_group_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawInitialCharge {
    pub charge_group_id: String,
    pub pre_recruitment_charge: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTarget {
    pub student_id: String,
    pub banner_id: String,
}
