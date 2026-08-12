use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU64;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::{
    RawBanner, RawInitialCharge, RawMilestone, RawProbabilityRatio, RawStudent, RawTarget,
    RawThresholdOverride,
};
use crate::Resources;

pub const MAX_THRESHOLD_OVERRIDES_V2: usize = 4_096;
pub const MAX_COMPATIBLE_RULESET_IDS_V2: usize = 256;
pub const MAX_MILESTONES_V2: usize = 4_096;
pub const MAX_REWARDS_PER_MILESTONE_V2: usize = 7;
pub const MAX_PROVENANCE_SOURCES_V2: usize = 32;
pub const MAX_SCENARIO_ITEMS_V2: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Provisional,
    SourceBacked,
    Verified,
}

impl VerificationStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::SourceBacked => "source_backed",
            Self::Verified => "verified",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawProvenanceSource {
    pub label: String,
    pub reference: String,
    #[serde(default)]
    pub retrieved_on: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawProvenance {
    pub verification_status: VerificationStatus,
    #[serde(deserialize_with = "sources")]
    pub sources: Vec<RawProvenanceSource>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRulesetV2 {
    pub schema_version: u64,
    pub document_type: String,
    pub ruleset_id: String,
    pub provenance: RawProvenance,
    pub paid_single_cost: u64,
    pub paid_single_action_size: u64,
    pub ticket_action_size: u64,
    pub ordinary_pickup_probability: RawProbabilityRatio,
    pub maximum_pre_recruitment_charge: u64,
    pub hit_reset_charge: u64,
    pub miss_increment: u64,
    #[serde(deserialize_with = "threshold_overrides")]
    pub threshold_overrides: Vec<RawThresholdOverride>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMilestoneV2 {
    pub count: u64,
    #[serde(deserialize_with = "milestone_rewards")]
    pub rewards: Vec<super::RawReward>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRewardScheduleV2 {
    pub schema_version: u64,
    pub document_type: String,
    pub reward_schedule_id: String,
    pub provenance: RawProvenance,
    #[serde(deserialize_with = "compatible_ruleset_ids")]
    pub compatible_ruleset_ids: Vec<String>,
    #[serde(deserialize_with = "milestones")]
    pub milestones: Vec<RawMilestoneV2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawFundingKind {
    TicketTen,
    PaidSingle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawStrategyKindV2 {
    SequentialTargets,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStrategyV2 {
    pub strategy_schema_version: u64,
    pub strategy_id: String,
    pub kind: RawStrategyKindV2,
    #[serde(deserialize_with = "funding_priority")]
    pub funding_priority: Vec<RawFundingKind>,
    pub max_total_recruitments: NonZeroU64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScenarioV2 {
    pub schema_version: u64,
    pub document_type: String,
    pub scenario_id: String,
    pub ruleset_id: String,
    pub reward_schedule_id: String,
    #[serde(deserialize_with = "scenario_items")]
    pub students: Vec<RawStudent>,
    #[serde(deserialize_with = "scenario_items")]
    pub banners: Vec<RawBanner>,
    #[serde(deserialize_with = "scenario_items")]
    pub initial_charges: Vec<RawInitialCharge>,
    pub initial_resources: Resources,
    #[serde(deserialize_with = "scenario_items")]
    pub initial_owned_targets: Vec<String>,
    pub strategy: RawStrategyV2,
    #[serde(deserialize_with = "scenario_items")]
    pub targets: Vec<RawTarget>,
}

fn threshold_overrides<'de, D>(deserializer: D) -> Result<Vec<RawThresholdOverride>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawThresholdOverride, MAX_THRESHOLD_OVERRIDES_V2>(
        deserializer,
        "threshold overrides",
    )
}

fn compatible_ruleset_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, String, MAX_COMPATIBLE_RULESET_IDS_V2>(deserializer, "compatible ruleset IDs")
}

fn milestones<'de, D>(deserializer: D) -> Result<Vec<RawMilestoneV2>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawMilestoneV2, MAX_MILESTONES_V2>(deserializer, "milestones")
}

fn milestone_rewards<'de, D>(deserializer: D) -> Result<Vec<super::RawReward>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, super::RawReward, MAX_REWARDS_PER_MILESTONE_V2>(
        deserializer,
        "milestone rewards",
    )
}

fn sources<'de, D>(deserializer: D) -> Result<Vec<RawProvenanceSource>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawProvenanceSource, MAX_PROVENANCE_SOURCES_V2>(
        deserializer,
        "provenance sources",
    )
}

fn scenario_items<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    bounded_vec::<D, T, MAX_SCENARIO_ITEMS_V2>(deserializer, "scenario entries")
}

fn funding_priority<'de, D>(deserializer: D) -> Result<Vec<RawFundingKind>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawFundingKind, 2>(deserializer, "funding priority")
}

fn bounded_vec<'de, D, T, const MAXIMUM: usize>(
    deserializer: D,
    name: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct BoundedVisitor<T, const MAXIMUM: usize> {
        name: &'static str,
        marker: PhantomData<T>,
    }

    impl<'de, T, const MAXIMUM: usize> Visitor<'de> for BoundedVisitor<T, MAXIMUM>
    where
        T: Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "{} array containing at most {MAXIMUM} entries",
                self.name
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAXIMUM));
            while let Some(value) = sequence.next_element()? {
                if values.len() == MAXIMUM {
                    return Err(serde::de::Error::custom(format!(
                        "{} exceeds maximum {MAXIMUM}",
                        self.name
                    )));
                }
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(BoundedVisitor::<T, MAXIMUM> {
        name,
        marker: PhantomData,
    })
}

impl From<RawMilestoneV2> for RawMilestone {
    fn from(value: RawMilestoneV2) -> Self {
        Self {
            count: value.count,
            rewards: value.rewards,
        }
    }
}
