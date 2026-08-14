use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU64;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::{RawBanner, RawInitialCharge, RawProbabilityRatio, RawStudent, RawTarget};
use crate::{RawResourceKindV3, ResourcesV3};

pub const MAX_THRESHOLD_OVERRIDES_V3: usize = 4_096;
pub const MAX_COMPATIBLE_RULESET_IDS_V3: usize = 256;
pub const MAX_MILESTONES_V3: usize = 4_096;
pub const MAX_REWARDS_PER_MILESTONE_V3: usize = 11;
pub const MAX_PROVENANCE_SOURCES_V3: usize = 64;
pub const MAX_CLAIM_BINDINGS_V3: usize = 16;
pub const MAX_SOURCE_IDS_PER_BINDING_V3: usize = 64;
pub const MAX_SCENARIO_ITEMS_V3: usize = 4;
pub const MAX_EFFECTIVE_MILESTONES_V3: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawProvenanceStatusV3 {
    Provisional,
    SourceBacked,
}

impl RawProvenanceStatusV3 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::SourceBacked => "source_backed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawSourceCategoryV3 {
    FirstPartyOfficial,
    PlatformOfficial,
    SecondaryReference,
}

impl RawSourceCategoryV3 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstPartyOfficial => "first_party_official",
            Self::PlatformOfficial => "platform_official",
            Self::SecondaryReference => "secondary_reference",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawClaimGroupV3 {
    RecruitmentCost,
    OrdinaryFeaturedTargetProbability,
    ChargeThresholds,
    ChargeResetBehavior,
    ChargeCarryAndGroupScope,
    AtomicTenRecruitmentContinuation,
    LimitedTicketActionSizeAndEligibility,
    PeriodScopeAndReset,
    FirstTimeMilestones,
    RepeatingCycle,
    MilestoneTicketAwards,
}

impl RawClaimGroupV3 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecruitmentCost => "recruitment_cost",
            Self::OrdinaryFeaturedTargetProbability => "ordinary_featured_target_probability",
            Self::ChargeThresholds => "charge_thresholds",
            Self::ChargeResetBehavior => "charge_reset_behavior",
            Self::ChargeCarryAndGroupScope => "charge_carry_and_group_scope",
            Self::AtomicTenRecruitmentContinuation => "atomic_ten_recruitment_continuation",
            Self::LimitedTicketActionSizeAndEligibility => {
                "limited_ticket_action_size_and_eligibility"
            }
            Self::PeriodScopeAndReset => "period_scope_and_reset",
            Self::FirstTimeMilestones => "first_time_milestones",
            Self::RepeatingCycle => "repeating_cycle",
            Self::MilestoneTicketAwards => "milestone_ticket_awards",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawProvenanceSourceV3 {
    pub source_id: String,
    pub source_category: RawSourceCategoryV3,
    pub label: String,
    pub reference: String,
    #[serde(default)]
    pub published_on: Option<String>,
    pub retrieved_on: String,
    #[serde(default)]
    pub content_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawClaimBindingV3 {
    pub claim_group: RawClaimGroupV3,
    #[serde(deserialize_with = "source_ids")]
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawProvenanceV3 {
    pub provenance_status: RawProvenanceStatusV3,
    #[serde(deserialize_with = "sources")]
    pub sources: Vec<RawProvenanceSourceV3>,
    #[serde(deserialize_with = "claim_bindings")]
    pub claim_bindings: Vec<RawClaimBindingV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawThresholdOverrideV3 {
    pub pre_charge: u64,
    pub featured_target_probability: RawProbabilityRatio,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRulesetV3 {
    pub schema_version: u64,
    pub document_type: String,
    pub ruleset_id: String,
    pub provenance: RawProvenanceV3,
    pub paid_single_cost: u64,
    pub paid_single_action_size: u64,
    pub ticket_action_size: u64,
    pub ordinary_featured_target_probability: RawProbabilityRatio,
    pub maximum_pre_recruitment_charge: u64,
    pub featured_hit_reset_charge: u64,
    pub non_featured_increment: u64,
    #[serde(deserialize_with = "threshold_overrides")]
    pub threshold_overrides: Vec<RawThresholdOverrideV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRewardV3 {
    pub resource: RawResourceKindV3,
    pub quantity: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMilestoneV3 {
    pub count: u64,
    #[serde(deserialize_with = "milestone_rewards")]
    pub rewards: Vec<RawRewardV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRepeatMilestoneV3 {
    pub offset: u64,
    #[serde(deserialize_with = "milestone_rewards")]
    pub rewards: Vec<RawRewardV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRepeatingCycleV3 {
    pub starts_after_count: u64,
    pub period: NonZeroU64,
    #[serde(deserialize_with = "repeat_milestones")]
    pub milestones: Vec<RawRepeatMilestoneV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRewardScheduleV3 {
    pub schema_version: u64,
    pub document_type: String,
    pub reward_schedule_id: String,
    pub provenance: RawProvenanceV3,
    #[serde(deserialize_with = "compatible_ruleset_ids")]
    pub compatible_ruleset_ids: Vec<String>,
    #[serde(deserialize_with = "milestones")]
    pub initial_milestones: Vec<RawMilestoneV3>,
    pub repeating_cycle: Option<RawRepeatingCycleV3>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawFundingKindV3 {
    TicketTen,
    PaidSingle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawStrategyKindV3 {
    SequentialTargets,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStrategyV3 {
    pub strategy_schema_version: u64,
    pub strategy_id: String,
    pub kind: RawStrategyKindV3,
    #[serde(deserialize_with = "funding_priority")]
    pub funding_priority: Vec<RawFundingKindV3>,
    pub max_additional_recruitments: NonZeroU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawScenarioAuthorityValueV3 {
    UserAuthored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScenarioAuthorityV3 {
    pub scenario: RawScenarioAuthorityValueV3,
    pub banner_topology: RawScenarioAuthorityValueV3,
    pub target_order: RawScenarioAuthorityValueV3,
    pub initial_state: RawScenarioAuthorityValueV3,
    pub cross_target_probabilities: RawScenarioAuthorityValueV3,
    pub strategy: RawScenarioAuthorityValueV3,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawOtherTargetWeightV3 {
    pub target_id: String,
    pub weight: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCrossTargetProbabilityRowV3 {
    pub denominator: u64,
    #[serde(deserialize_with = "other_target_weights")]
    pub other_target_weights: Vec<RawOtherTargetWeightV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCrossTargetThresholdRowV3 {
    pub pre_charge: u64,
    pub denominator: u64,
    #[serde(deserialize_with = "other_target_weights")]
    pub other_target_weights: Vec<RawOtherTargetWeightV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCrossTargetProbabilityTableV3 {
    pub banner_id: String,
    pub ordinary: RawCrossTargetProbabilityRowV3,
    #[serde(deserialize_with = "cross_target_thresholds")]
    pub threshold_overrides: Vec<RawCrossTargetThresholdRowV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScenarioV3 {
    pub schema_version: u64,
    pub document_type: String,
    pub scenario_id: String,
    pub ruleset_id: String,
    pub reward_schedule_id: String,
    pub authority: RawScenarioAuthorityV3,
    pub initial_recruitment_count: u64,
    #[serde(deserialize_with = "scenario_items")]
    pub students: Vec<RawStudent>,
    #[serde(deserialize_with = "scenario_items")]
    pub banners: Vec<RawBanner>,
    #[serde(deserialize_with = "scenario_items")]
    pub initial_charges: Vec<RawInitialCharge>,
    pub initial_resources: ResourcesV3,
    #[serde(deserialize_with = "scenario_items")]
    pub initial_owned_targets: Vec<String>,
    #[serde(deserialize_with = "scenario_items")]
    pub targets: Vec<RawTarget>,
    #[serde(deserialize_with = "scenario_items")]
    pub cross_target_probability_tables: Vec<RawCrossTargetProbabilityTableV3>,
    pub strategy: RawStrategyV3,
}

fn threshold_overrides<'de, D>(deserializer: D) -> Result<Vec<RawThresholdOverrideV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawThresholdOverrideV3, MAX_THRESHOLD_OVERRIDES_V3>(
        deserializer,
        "threshold overrides",
    )
}

fn compatible_ruleset_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, String, MAX_COMPATIBLE_RULESET_IDS_V3>(deserializer, "compatible ruleset IDs")
}

fn milestones<'de, D>(deserializer: D) -> Result<Vec<RawMilestoneV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawMilestoneV3, MAX_MILESTONES_V3>(deserializer, "initial milestones")
}

fn repeat_milestones<'de, D>(deserializer: D) -> Result<Vec<RawRepeatMilestoneV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawRepeatMilestoneV3, MAX_MILESTONES_V3>(deserializer, "repeat milestones")
}

fn milestone_rewards<'de, D>(deserializer: D) -> Result<Vec<RawRewardV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawRewardV3, MAX_REWARDS_PER_MILESTONE_V3>(deserializer, "milestone rewards")
}

fn sources<'de, D>(deserializer: D) -> Result<Vec<RawProvenanceSourceV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawProvenanceSourceV3, MAX_PROVENANCE_SOURCES_V3>(
        deserializer,
        "provenance sources",
    )
}

fn claim_bindings<'de, D>(deserializer: D) -> Result<Vec<RawClaimBindingV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawClaimBindingV3, MAX_CLAIM_BINDINGS_V3>(deserializer, "claim bindings")
}

fn source_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, String, MAX_SOURCE_IDS_PER_BINDING_V3>(deserializer, "claim source IDs")
}

fn scenario_items<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    bounded_vec::<D, T, MAX_SCENARIO_ITEMS_V3>(deserializer, "scenario entries")
}

fn other_target_weights<'de, D>(deserializer: D) -> Result<Vec<RawOtherTargetWeightV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawOtherTargetWeightV3, MAX_SCENARIO_ITEMS_V3>(
        deserializer,
        "other-target weights",
    )
}

fn cross_target_thresholds<'de, D>(
    deserializer: D,
) -> Result<Vec<RawCrossTargetThresholdRowV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawCrossTargetThresholdRowV3, MAX_THRESHOLD_OVERRIDES_V3>(
        deserializer,
        "cross-target threshold rows",
    )
}

fn funding_priority<'de, D>(deserializer: D) -> Result<Vec<RawFundingKindV3>, D::Error>
where
    D: Deserializer<'de>,
{
    bounded_vec::<D, RawFundingKindV3, 2>(deserializer, "funding priority")
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
