use std::num::NonZeroU64;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::{ResourceKind, Resources};

pub const SCHEMA_VERSION_V1: u64 = 1;
pub const RULESET_DOCUMENT_TYPE: &str = "ruleset";
pub const REWARD_SCHEDULE_DOCUMENT_TYPE: &str = "reward_schedule";
pub const SCENARIO_DOCUMENT_TYPE: &str = "scenario";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Ruleset,
    RewardSchedule,
    Scenario,
}

impl DocumentKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ruleset => RULESET_DOCUMENT_TYPE,
            Self::RewardSchedule => REWARD_SCHEDULE_DOCUMENT_TYPE,
            Self::Scenario => SCENARIO_DOCUMENT_TYPE,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub enum NullablePositive {
    #[default]
    Missing,
    Present(Option<NonZeroU64>),
}

impl<'de> Deserialize<'de> for NullablePositive {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct NullableVisitor;

        impl<'de> Visitor<'de> for NullableVisitor {
            type Value = NullablePositive;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("null or a positive integer")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(NullablePositive::Present(None))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(NullablePositive::Present(None))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                NonZeroU64::deserialize(deserializer)
                    .map(|value| NullablePositive::Present(Some(value)))
            }
        }

        deserializer.deserialize_option(NullableVisitor)
    }
}

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
pub struct RawRulesetV1 {
    pub schema_version: u64,
    pub document_type: String,
    pub ruleset_id: String,
    pub paid_single_cost: u64,
    pub paid_single_action_size: u64,
    pub ticket_action_size: u64,
    pub ordinary_pickup_probability: RawProbabilityRatio,
    pub maximum_pre_recruitment_charge: u64,
    pub hit_reset_charge: u64,
    pub miss_increment: u64,
    pub threshold_overrides: Vec<RawThresholdOverride>,
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
pub struct RawRewardScheduleV1 {
    pub schema_version: u64,
    pub document_type: String,
    pub reward_schedule_id: String,
    pub compatible_ruleset_ids: Vec<String>,
    pub milestones: Vec<RawMilestone>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawStrategyKind {
    SequentialTargetsPreferTickets,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStrategy {
    pub strategy_id: String,
    pub kind: RawStrategyKind,
    #[serde(default)]
    pub max_total_recruitments: NullablePositive,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTarget {
    pub student_id: String,
    pub banner_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScenarioV1 {
    pub schema_version: u64,
    pub document_type: String,
    pub scenario_id: String,
    pub ruleset_id: String,
    pub reward_schedule_id: String,
    pub students: Vec<RawStudent>,
    pub banners: Vec<RawBanner>,
    pub initial_charges: Vec<RawInitialCharge>,
    pub initial_resources: Resources,
    pub initial_owned_targets: Vec<String>,
    pub strategy: RawStrategy,
    pub targets: Vec<RawTarget>,
}
