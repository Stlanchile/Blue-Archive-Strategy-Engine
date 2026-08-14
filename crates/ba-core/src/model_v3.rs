use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::path::Path;

use serde::Serialize;

use crate::fingerprint::{CanonicalNode, SemanticFingerprint, object};
use crate::model::{Banner, FundingKind, Target};
use crate::probability_v3::CompiledOutcomeDistribution;
use crate::provenance_v3::{
    ProvenanceStatusV3, ProvenanceSubjectV3, ProvenanceV3, provenance_node_v3,
};
use crate::schema::{
    RULESET_DOCUMENT_TYPE, RawCrossTargetProbabilityRowV3, RawCrossTargetThresholdRowV3,
    RawFundingKindV3, RawRulesetV3, RawScenarioV3, RawStrategyKindV3, SCENARIO_DOCUMENT_TYPE,
};
use crate::{
    BannerId, ChargeGroupId, CoreError, DOCUMENT_SCHEMA_VERSION_V3, LedgerResourceKind,
    OwnershipMask, ProbabilityRatio, ResourceLedger, ResourcesV3, RewardScheduleId,
    RewardScheduleV3, RulesetId, STRATEGY_SCHEMA_VERSION_V3, ScenarioId, StrategyId, StudentId,
    TargetIndex, resource_kind_name_v3,
};

#[derive(Debug, Clone)]
pub struct CompiledRulesetV3 {
    id: RulesetId,
    paid_single_cost: NonZeroU64,
    paid_single_action_size: NonZeroU64,
    ticket_action_size: NonZeroU64,
    ordinary_featured_target_probability: ProbabilityRatio,
    maximum_pre_recruitment_charge: u64,
    featured_hit_reset_charge: u64,
    non_featured_increment: NonZeroU64,
    threshold_overrides: Vec<(u64, ProbabilityRatio)>,
    provenance: ProvenanceV3,
}

#[derive(Debug, Clone)]
pub struct RulesetMechanicsV3 {
    pub paid_single_cost: u64,
    pub paid_single_action_size: u64,
    pub ticket_action_size: u64,
    pub ordinary_featured_target_probability: ProbabilityRatio,
    pub maximum_pre_recruitment_charge: u64,
    pub featured_hit_reset_charge: u64,
    pub non_featured_increment: u64,
    pub threshold_overrides: Vec<(u64, ProbabilityRatio)>,
}

impl CompiledRulesetV3 {
    pub fn from_raw(raw: RawRulesetV3, path: Option<&Path>) -> Result<Self, CoreError> {
        if raw.schema_version != DOCUMENT_SCHEMA_VERSION_V3
            || raw.document_type != RULESET_DOCUMENT_TYPE
        {
            return Err(CoreError::validation(
                path,
                "typed document header mismatch: expected schema_version=3 and document_type=ruleset",
            ));
        }
        let id = RulesetId::new(raw.ruleset_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let ordinary = ProbabilityRatio::new(
            raw.ordinary_featured_target_probability.numerator,
            raw.ordinary_featured_target_probability.denominator,
        )
        .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let mut threshold_overrides = Vec::with_capacity(raw.threshold_overrides.len());
        for threshold in raw.threshold_overrides {
            threshold_overrides.push((
                threshold.pre_charge,
                ProbabilityRatio::new(
                    threshold.featured_target_probability.numerator,
                    threshold.featured_target_probability.denominator,
                )
                .map_err(|error| CoreError::validation(path, error.to_string()))?,
            ));
        }
        let provenance =
            ProvenanceV3::from_raw(raw.provenance, ProvenanceSubjectV3::Ruleset, path)?;
        Self::compile_parts(
            id,
            RulesetMechanicsV3 {
                paid_single_cost: raw.paid_single_cost,
                paid_single_action_size: raw.paid_single_action_size,
                ticket_action_size: raw.ticket_action_size,
                ordinary_featured_target_probability: ordinary,
                maximum_pre_recruitment_charge: raw.maximum_pre_recruitment_charge,
                featured_hit_reset_charge: raw.featured_hit_reset_charge,
                non_featured_increment: raw.non_featured_increment,
                threshold_overrides,
            },
            provenance,
        )
        .map_err(|error| CoreError::validation(path, error.to_string()))
    }

    pub fn from_parts(id: RulesetId, mechanics: RulesetMechanicsV3) -> Result<Self, CoreError> {
        Self::compile_parts(
            id,
            mechanics,
            ProvenanceV3 {
                provenance_status: ProvenanceStatusV3::Provisional,
                sources: Vec::new(),
                claim_bindings: Vec::new(),
            },
        )
    }

    fn compile_parts(
        id: RulesetId,
        mechanics: RulesetMechanicsV3,
        provenance: ProvenanceV3,
    ) -> Result<Self, CoreError> {
        let paid_single_cost = NonZeroU64::new(mechanics.paid_single_cost)
            .ok_or_else(|| CoreError::validation(None, "paid_single_cost must be positive"))?;
        let paid_single_action_size = NonZeroU64::new(mechanics.paid_single_action_size)
            .ok_or_else(|| {
                CoreError::validation(None, "paid_single_action_size must be positive")
            })?;
        let ticket_action_size = NonZeroU64::new(mechanics.ticket_action_size)
            .ok_or_else(|| CoreError::validation(None, "ticket_action_size must be positive"))?;
        let non_featured_increment =
            NonZeroU64::new(mechanics.non_featured_increment).ok_or_else(|| {
                CoreError::validation(None, "non_featured_increment must be positive")
            })?;
        if mechanics.featured_hit_reset_charge > mechanics.maximum_pre_recruitment_charge {
            return Err(CoreError::validation(
                None,
                "featured_hit_reset_charge exceeds maximum_pre_recruitment_charge",
            ));
        }
        let mut previous = None;
        for (charge, _) in &mechanics.threshold_overrides {
            if *charge > mechanics.maximum_pre_recruitment_charge {
                return Err(CoreError::validation(
                    None,
                    format!(
                        "threshold charge {charge} exceeds maximum {}",
                        mechanics.maximum_pre_recruitment_charge
                    ),
                ));
            }
            if previous.is_some_and(|value| *charge <= value) {
                return Err(CoreError::validation(
                    None,
                    "threshold overrides must be unique and strictly increasing",
                ));
            }
            previous = Some(*charge);
        }
        validate_safe_non_featured(
            mechanics.maximum_pre_recruitment_charge,
            non_featured_increment.get(),
            &mechanics.threshold_overrides,
        )?;
        Ok(Self {
            id,
            paid_single_cost,
            paid_single_action_size,
            ticket_action_size,
            ordinary_featured_target_probability: mechanics.ordinary_featured_target_probability,
            maximum_pre_recruitment_charge: mechanics.maximum_pre_recruitment_charge,
            featured_hit_reset_charge: mechanics.featured_hit_reset_charge,
            non_featured_increment,
            threshold_overrides: mechanics.threshold_overrides,
            provenance,
        })
    }

    #[must_use]
    pub const fn schema_version(&self) -> u64 {
        DOCUMENT_SCHEMA_VERSION_V3
    }

    #[must_use]
    pub const fn id(&self) -> &RulesetId {
        &self.id
    }

    #[must_use]
    pub const fn paid_single_cost(&self) -> u64 {
        self.paid_single_cost.get()
    }

    #[must_use]
    pub const fn paid_single_action_size(&self) -> u64 {
        self.paid_single_action_size.get()
    }

    #[must_use]
    pub const fn ticket_action_size(&self) -> u64 {
        self.ticket_action_size.get()
    }

    #[must_use]
    pub const fn maximum_pre_recruitment_charge(&self) -> u64 {
        self.maximum_pre_recruitment_charge
    }

    #[must_use]
    pub const fn featured_hit_reset_charge(&self) -> u64 {
        self.featured_hit_reset_charge
    }

    #[must_use]
    pub const fn non_featured_increment(&self) -> u64 {
        self.non_featured_increment.get()
    }

    #[must_use]
    pub const fn ordinary_featured_target_probability(&self) -> ProbabilityRatio {
        self.ordinary_featured_target_probability
    }

    #[must_use]
    pub fn featured_target_probability(&self, pre_charge: u64) -> ProbabilityRatio {
        self.threshold_overrides
            .binary_search_by_key(&pre_charge, |(charge, _)| *charge)
            .ok()
            .and_then(|index| self.threshold_overrides.get(index))
            .map_or(self.ordinary_featured_target_probability, |(_, ratio)| {
                *ratio
            })
    }

    #[must_use]
    pub fn threshold_overrides(&self) -> &[(u64, ProbabilityRatio)] {
        &self.threshold_overrides
    }

    #[must_use]
    pub const fn provenance(&self) -> &ProvenanceV3 {
        &self.provenance
    }

    #[must_use]
    pub fn semantic_node(&self) -> CanonicalNode {
        object([
            (
                "document_type",
                CanonicalNode::String(RULESET_DOCUMENT_TYPE.to_owned()),
            ),
            (
                "featured_hit_reset_charge",
                CanonicalNode::Integer(self.featured_hit_reset_charge),
            ),
            (
                "maximum_pre_recruitment_charge",
                CanonicalNode::Integer(self.maximum_pre_recruitment_charge),
            ),
            (
                "non_featured_increment",
                CanonicalNode::Integer(self.non_featured_increment.get()),
            ),
            (
                "ordinary_featured_target_probability",
                ratio_node(self.ordinary_featured_target_probability),
            ),
            (
                "paid_single_action_size",
                CanonicalNode::Integer(self.paid_single_action_size.get()),
            ),
            (
                "paid_single_cost",
                CanonicalNode::Integer(self.paid_single_cost.get()),
            ),
            ("provenance", provenance_node_v3(&self.provenance)),
            ("ruleset_id", CanonicalNode::String(self.id.to_string())),
            (
                "schema_version",
                CanonicalNode::Integer(DOCUMENT_SCHEMA_VERSION_V3),
            ),
            (
                "threshold_overrides",
                threshold_overrides_node(&self.threshold_overrides),
            ),
            (
                "ticket_action_size",
                CanonicalNode::Integer(self.ticket_action_size.get()),
            ),
        ])
    }

    #[must_use]
    pub fn behavior_node(&self) -> CanonicalNode {
        object([
            ("behavior_schema_version", CanonicalNode::Integer(3)),
            (
                "featured_hit_reset_charge",
                CanonicalNode::Integer(self.featured_hit_reset_charge),
            ),
            (
                "maximum_pre_recruitment_charge",
                CanonicalNode::Integer(self.maximum_pre_recruitment_charge),
            ),
            (
                "non_featured_increment",
                CanonicalNode::Integer(self.non_featured_increment.get()),
            ),
            (
                "ordinary_featured_target_probability",
                ratio_node(self.ordinary_featured_target_probability),
            ),
            (
                "paid_single_action_size",
                CanonicalNode::Integer(self.paid_single_action_size.get()),
            ),
            (
                "paid_single_cost",
                CanonicalNode::Integer(self.paid_single_cost.get()),
            ),
            (
                "threshold_overrides",
                threshold_overrides_node(&self.threshold_overrides),
            ),
            (
                "ticket_action_size",
                CanonicalNode::Integer(self.ticket_action_size.get()),
            ),
        ])
    }

    pub fn behavior_fingerprint(&self) -> Result<SemanticFingerprint, CoreError> {
        SemanticFingerprint::from_node(&self.behavior_node())
    }

    pub fn document_fingerprint(&self) -> Result<SemanticFingerprint, CoreError> {
        SemanticFingerprint::from_node(&self.semantic_node())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledStrategyV3 {
    pub strategy_schema_version: u64,
    pub strategy_id: StrategyId,
    pub funding_priority: [FundingKind; 2],
    pub max_additional_recruitments: NonZeroU64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScenarioAuthorityV3 {
    pub scenario: &'static str,
    pub banner_topology: &'static str,
    pub target_order: &'static str,
    pub initial_state: &'static str,
    pub cross_target_probabilities: &'static str,
    pub strategy: &'static str,
}

impl Default for ScenarioAuthorityV3 {
    fn default() -> Self {
        Self {
            scenario: "user_authored",
            banner_topology: "user_authored",
            target_order: "user_authored",
            initial_state: "user_authored",
            cross_target_probabilities: "user_authored",
            strategy: "user_authored",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalProbabilityRowV3 {
    pub denominator: u64,
    pub other_target_weights: Vec<(TargetIndex, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BannerProbabilityProfileV3 {
    pub banner_id: BannerId,
    pub ordinary_original: OriginalProbabilityRowV3,
    pub ordinary: CompiledOutcomeDistribution,
    pub threshold_original: Vec<(u64, OriginalProbabilityRowV3)>,
    pub threshold_overrides: Vec<(u64, CompiledOutcomeDistribution)>,
}

impl BannerProbabilityProfileV3 {
    #[must_use]
    pub fn distribution(&self, pre_charge: u64) -> &CompiledOutcomeDistribution {
        self.threshold_overrides
            .binary_search_by_key(&pre_charge, |(charge, _)| *charge)
            .ok()
            .and_then(|index| self.threshold_overrides.get(index))
            .map_or(&self.ordinary, |(_, distribution)| distribution)
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedScenarioV3 {
    id: ScenarioId,
    ruleset_id: RulesetId,
    reward_schedule_id: RewardScheduleId,
    authority: ScenarioAuthorityV3,
    initial_recruitment_count: u64,
    maximum_absolute_campaign_count: u64,
    students: Vec<StudentId>,
    banners: Vec<Banner>,
    charge_groups: Vec<ChargeGroupId>,
    initial_charges: Vec<u64>,
    initial_resources: ResourceLedger,
    initial_owned_mask: u8,
    initial_owned_targets: Vec<StudentId>,
    targets: Vec<Target>,
    strategy: CompiledStrategyV3,
    probability_profiles: Vec<BannerProbabilityProfileV3>,
    effective_milestones: Vec<crate::MilestoneV3>,
}

impl ValidatedScenarioV3 {
    pub fn from_raw(
        raw: RawScenarioV3,
        ruleset: &CompiledRulesetV3,
        rewards: &RewardScheduleV3,
        path: Option<&Path>,
    ) -> Result<Self, CoreError> {
        if raw.schema_version != DOCUMENT_SCHEMA_VERSION_V3
            || raw.document_type != SCENARIO_DOCUMENT_TYPE
        {
            return Err(CoreError::validation(
                path,
                "typed document header mismatch: expected schema_version=3 and document_type=scenario",
            ));
        }
        if raw.strategy.strategy_schema_version != STRATEGY_SCHEMA_VERSION_V3 {
            return Err(CoreError::validation(
                path,
                format!(
                    "unsupported strategy_schema_version {}",
                    raw.strategy.strategy_schema_version
                ),
            ));
        }
        if raw.strategy.kind != RawStrategyKindV3::SequentialTargets {
            return Err(CoreError::validation(path, "unsupported strategy kind"));
        }
        let funding_priority = match raw.strategy.funding_priority.as_slice() {
            [RawFundingKindV3::TicketTen, RawFundingKindV3::PaidSingle] => {
                [FundingKind::TicketTen, FundingKind::PaidSingle]
            }
            [RawFundingKindV3::PaidSingle, RawFundingKindV3::TicketTen] => {
                [FundingKind::PaidSingle, FundingKind::TicketTen]
            }
            _ => {
                return Err(CoreError::validation(
                    path,
                    "funding_priority must be an exact permutation of ticket_ten and paid_single",
                ));
            }
        };

        let id = ScenarioId::new(raw.scenario_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let ruleset_id = RulesetId::new(raw.ruleset_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let reward_schedule_id = RewardScheduleId::new(raw.reward_schedule_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        if &ruleset_id != ruleset.id() {
            return Err(CoreError::validation(
                path,
                format!(
                    "scenario references ruleset {ruleset_id}, but bundle supplied {}",
                    ruleset.id()
                ),
            ));
        }
        if &reward_schedule_id != rewards.id() {
            return Err(CoreError::validation(
                path,
                format!(
                    "scenario references reward schedule {reward_schedule_id}, but bundle supplied {}",
                    rewards.id()
                ),
            ));
        }
        if !rewards.compatible_ruleset_ids().contains(&ruleset_id) {
            return Err(CoreError::validation(
                path,
                format!("reward schedule {reward_schedule_id} is incompatible with {ruleset_id}"),
            ));
        }

        let mut student_set = BTreeSet::new();
        for raw_student in raw.students {
            let student = StudentId::new(raw_student.student_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !student_set.insert(student) {
                return Err(CoreError::validation(path, "duplicate student ID"));
            }
        }

        let mut raw_banners = Vec::with_capacity(raw.banners.len());
        let mut banner_ids = BTreeSet::new();
        let mut featured_students = BTreeSet::new();
        let mut group_set = BTreeSet::new();
        for raw_banner in raw.banners {
            let banner_id = BannerId::new(raw_banner.banner_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            let featured_student_id = StudentId::new(raw_banner.featured_student_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            let charge_group_id = ChargeGroupId::new(raw_banner.charge_group_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !banner_ids.insert(banner_id.clone()) {
                return Err(CoreError::validation(path, "duplicate banner ID"));
            }
            if !featured_students.insert(featured_student_id.clone()) {
                return Err(CoreError::validation(
                    path,
                    "each banner must feature a distinct configured target",
                ));
            }
            group_set.insert(charge_group_id.clone());
            raw_banners.push((banner_id, featured_student_id, charge_group_id));
        }
        raw_banners.sort_by(|left, right| left.0.cmp(&right.0));
        let charge_groups = group_set.into_iter().collect::<Vec<_>>();
        let group_indices = charge_groups
            .iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), index))
            .collect::<BTreeMap<_, _>>();
        let mut banners = Vec::with_capacity(raw_banners.len());
        for (banner_id, featured_student_id, charge_group_id) in raw_banners {
            let charge_group_index = group_indices.get(&charge_group_id).copied().ok_or(
                CoreError::InternalInvariant {
                    message: "canonical v3 charge group index missing".to_owned(),
                },
            )?;
            banners.push(Banner {
                banner_id,
                featured_student_id,
                charge_group_id,
                charge_group_index,
            });
        }
        let banner_indices = banners
            .iter()
            .enumerate()
            .map(|(index, banner)| (banner.banner_id.clone(), index))
            .collect::<BTreeMap<_, _>>();

        if !(1..=4).contains(&raw.targets.len()) {
            return Err(CoreError::validation(
                path,
                "a v3 scenario must contain one through four ordered targets",
            ));
        }
        let mut target_students = BTreeSet::new();
        let mut target_banners = BTreeSet::new();
        let mut targets = Vec::with_capacity(raw.targets.len());
        for raw_target in raw.targets {
            let student_id = StudentId::new(raw_target.student_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            let banner_id = BannerId::new(raw_target.banner_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !target_students.insert(student_id.clone()) {
                return Err(CoreError::validation(path, "duplicate target student"));
            }
            if !target_banners.insert(banner_id.clone()) {
                return Err(CoreError::validation(path, "duplicate target banner"));
            }
            let banner_index = banner_indices.get(&banner_id).copied().ok_or_else(|| {
                CoreError::validation(
                    path,
                    format!("target references unknown banner {banner_id}"),
                )
            })?;
            let banner = banners
                .get(banner_index)
                .ok_or(CoreError::InternalInvariant {
                    message: "validated v3 banner index is out of range".to_owned(),
                })?;
            if banner.featured_student_id != student_id {
                return Err(CoreError::validation(
                    path,
                    format!(
                        "banner {} features {}, not target {}",
                        banner.banner_id, banner.featured_student_id, student_id
                    ),
                ));
            }
            targets.push(Target {
                student_id,
                banner_id,
                banner_index,
            });
        }
        if student_set != target_students
            || featured_students != target_students
            || banner_ids != target_banners
        {
            return Err(CoreError::validation(
                path,
                "students, featured students, banners, and ordered targets must describe exactly the same reachable target set",
            ));
        }
        let students = student_set.into_iter().collect::<Vec<_>>();

        let mut charge_by_group = BTreeMap::new();
        for raw_charge in raw.initial_charges {
            let group = ChargeGroupId::new(raw_charge.charge_group_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !group_indices.contains_key(&group) {
                return Err(CoreError::validation(
                    path,
                    format!("initial charge references unused group {group}"),
                ));
            }
            if raw_charge.pre_recruitment_charge > ruleset.maximum_pre_recruitment_charge() {
                return Err(CoreError::validation(
                    path,
                    format!(
                        "initial charge {} exceeds ruleset maximum",
                        raw_charge.pre_recruitment_charge
                    ),
                ));
            }
            if charge_by_group
                .insert(group, raw_charge.pre_recruitment_charge)
                .is_some()
            {
                return Err(CoreError::validation(
                    path,
                    "duplicate initial charge group",
                ));
            }
        }
        if charge_by_group.len() != charge_groups.len() {
            return Err(CoreError::validation(
                path,
                "every used charge group must have exactly one initial charge",
            ));
        }
        let initial_charges = charge_groups
            .iter()
            .map(|group| {
                charge_by_group.get(group).copied().ok_or_else(|| {
                    CoreError::validation(path, format!("missing initial charge for group {group}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut initial_owned_mask = OwnershipMask::empty();
        let mut initial_owned_set = BTreeSet::new();
        for raw_owned in raw.initial_owned_targets {
            let owned = StudentId::new(raw_owned)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !initial_owned_set.insert(owned.clone()) {
                return Err(CoreError::validation(
                    path,
                    "duplicate initially owned target",
                ));
            }
            let index = targets
                .iter()
                .position(|target| target.student_id == owned)
                .ok_or_else(|| {
                    CoreError::validation(
                        path,
                        format!("initially owned student {owned} is not a target"),
                    )
                })?;
            initial_owned_mask.insert(index)?;
        }
        let initial_owned_targets = initial_owned_set.into_iter().collect::<Vec<_>>();

        let strategy_id = StrategyId::new(raw.strategy.strategy_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let strategy = CompiledStrategyV3 {
            strategy_schema_version: raw.strategy.strategy_schema_version,
            strategy_id,
            funding_priority,
            max_additional_recruitments: raw.strategy.max_additional_recruitments,
        };
        let maximum_absolute_campaign_count = raw
            .initial_recruitment_count
            .checked_add(strategy.max_additional_recruitments.get())
            .ok_or_else(|| {
                CoreError::validation(path, "initial plus additional campaign count exceeds u64")
            })?;
        let effective_milestones = rewards
            .materialized_future_milestones(
                raw.initial_recruitment_count,
                strategy.max_additional_recruitments.get(),
            )
            .map_err(|error| CoreError::validation(path, error.to_string()))?;

        let mut tables_by_banner = BTreeMap::new();
        for table in raw.cross_target_probability_tables {
            let banner_id = BannerId::new(table.banner_id.clone())
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !banner_indices.contains_key(&banner_id) {
                return Err(CoreError::validation(
                    path,
                    format!("cross-target table references unknown banner {banner_id}"),
                ));
            }
            if tables_by_banner.insert(banner_id.clone(), table).is_some() {
                return Err(CoreError::validation(
                    path,
                    format!("duplicate cross-target table for banner {banner_id}"),
                ));
            }
        }
        if tables_by_banner.len() != banners.len() {
            return Err(CoreError::validation(
                path,
                "every configured banner must have exactly one cross-target probability table",
            ));
        }
        let mut probability_profiles = Vec::with_capacity(banners.len());
        for banner in &banners {
            let table = tables_by_banner.remove(&banner.banner_id).ok_or_else(|| {
                CoreError::validation(
                    path,
                    format!(
                        "missing cross-target probability table for {}",
                        banner.banner_id
                    ),
                )
            })?;
            let current_index = targets
                .iter()
                .position(|target| target.student_id == banner.featured_student_id)
                .ok_or(CoreError::InternalInvariant {
                    message: "v3 banner featured student is not an ordered target".to_owned(),
                })?;
            if table.threshold_overrides.len() != ruleset.threshold_overrides().len() {
                return Err(CoreError::validation(
                    path,
                    format!(
                        "cross-target threshold rows for {} must correspond exactly to the ruleset",
                        banner.banner_id
                    ),
                ));
            }
            let (ordinary_original, ordinary) = compile_probability_row(
                &table.ordinary,
                ruleset.ordinary_featured_target_probability(),
                current_index,
                &targets,
                path,
            )?;
            let mut threshold_original = Vec::with_capacity(table.threshold_overrides.len());
            let mut threshold_overrides = Vec::with_capacity(table.threshold_overrides.len());
            for (raw_threshold, (expected_charge, featured_probability)) in table
                .threshold_overrides
                .iter()
                .zip(ruleset.threshold_overrides())
            {
                if raw_threshold.pre_charge != *expected_charge {
                    return Err(CoreError::validation(
                        path,
                        format!(
                            "cross-target threshold rows for {} must match ruleset charge order",
                            banner.banner_id
                        ),
                    ));
                }
                let (original, compiled) = compile_threshold_row(
                    raw_threshold,
                    *featured_probability,
                    current_index,
                    &targets,
                    path,
                )?;
                threshold_original.push((*expected_charge, original));
                threshold_overrides.push((*expected_charge, compiled));
            }
            probability_profiles.push(BannerProbabilityProfileV3 {
                banner_id: banner.banner_id.clone(),
                ordinary_original,
                ordinary,
                threshold_original,
                threshold_overrides,
            });
        }

        let _authority_input = raw.authority;
        Ok(Self {
            id,
            ruleset_id,
            reward_schedule_id,
            authority: ScenarioAuthorityV3::default(),
            initial_recruitment_count: raw.initial_recruitment_count,
            maximum_absolute_campaign_count,
            students,
            banners,
            charge_groups,
            initial_charges,
            initial_resources: ResourceLedger::from(raw.initial_resources),
            initial_owned_mask: initial_owned_mask.raw(),
            initial_owned_targets,
            targets,
            strategy,
            probability_profiles,
            effective_milestones,
        })
    }

    #[must_use]
    pub const fn schema_version(&self) -> u64 {
        DOCUMENT_SCHEMA_VERSION_V3
    }

    #[must_use]
    pub const fn id(&self) -> &ScenarioId {
        &self.id
    }

    #[must_use]
    pub const fn ruleset_id(&self) -> &RulesetId {
        &self.ruleset_id
    }

    #[must_use]
    pub const fn reward_schedule_id(&self) -> &RewardScheduleId {
        &self.reward_schedule_id
    }

    #[must_use]
    pub const fn authority(&self) -> &ScenarioAuthorityV3 {
        &self.authority
    }

    #[must_use]
    pub const fn initial_recruitment_count(&self) -> u64 {
        self.initial_recruitment_count
    }

    #[must_use]
    pub const fn maximum_absolute_campaign_count(&self) -> u64 {
        self.maximum_absolute_campaign_count
    }

    pub fn absolute_campaign_count(&self, additional: u64) -> Result<u64, CoreError> {
        self.initial_recruitment_count
            .checked_add(additional)
            .ok_or(CoreError::ArithmeticOverflow {
                context: "computing absolute v3 campaign count",
            })
    }

    #[must_use]
    pub fn students(&self) -> &[StudentId] {
        &self.students
    }

    #[must_use]
    pub fn banners(&self) -> &[Banner] {
        &self.banners
    }

    #[must_use]
    pub fn charge_groups(&self) -> &[ChargeGroupId] {
        &self.charge_groups
    }

    #[must_use]
    pub fn initial_charges(&self) -> &[u64] {
        &self.initial_charges
    }

    #[must_use]
    pub const fn initial_resources(&self) -> ResourceLedger {
        self.initial_resources
    }

    #[must_use]
    pub const fn initial_owned_mask(&self) -> u8 {
        self.initial_owned_mask
    }

    #[must_use]
    pub fn initial_owned_targets(&self) -> &[StudentId] {
        &self.initial_owned_targets
    }

    #[must_use]
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    #[must_use]
    pub const fn compiled_strategy(&self) -> &CompiledStrategyV3 {
        &self.strategy
    }

    #[must_use]
    pub fn probability_profiles(&self) -> &[BannerProbabilityProfileV3] {
        &self.probability_profiles
    }

    pub fn probability_distribution(
        &self,
        banner_index: usize,
        pre_charge: u64,
    ) -> Result<&CompiledOutcomeDistribution, CoreError> {
        self.probability_profiles
            .get(banner_index)
            .map(|profile| profile.distribution(pre_charge))
            .ok_or_else(|| CoreError::InvalidTransition {
                message: "v3 probability profile index is out of range".to_owned(),
            })
    }

    #[must_use]
    pub fn effective_milestones(&self) -> &[crate::MilestoneV3] {
        &self.effective_milestones
    }

    #[must_use]
    pub fn all_targets_mask(&self) -> u8 {
        OwnershipMask::all(self.targets.len()).map_or(0, OwnershipMask::raw)
    }

    #[must_use]
    pub fn target_index_for_student(&self, student: &StudentId) -> Option<TargetIndex> {
        self.targets
            .iter()
            .position(|target| &target.student_id == student)
            .and_then(|index| TargetIndex::new(index, self.targets.len()).ok())
    }

    #[must_use]
    pub fn semantic_node(&self) -> CanonicalNode {
        object([
            (
                "authority",
                object([
                    (
                        "banner_topology",
                        CanonicalNode::String(self.authority.banner_topology.to_owned()),
                    ),
                    (
                        "cross_target_probabilities",
                        CanonicalNode::String(self.authority.cross_target_probabilities.to_owned()),
                    ),
                    (
                        "initial_state",
                        CanonicalNode::String(self.authority.initial_state.to_owned()),
                    ),
                    (
                        "scenario",
                        CanonicalNode::String(self.authority.scenario.to_owned()),
                    ),
                    (
                        "strategy",
                        CanonicalNode::String(self.authority.strategy.to_owned()),
                    ),
                    (
                        "target_order",
                        CanonicalNode::String(self.authority.target_order.to_owned()),
                    ),
                ]),
            ),
            ("banners", banners_node(&self.banners)),
            (
                "cross_target_probability_tables",
                original_probability_profiles_node(&self.probability_profiles),
            ),
            (
                "document_type",
                CanonicalNode::String(SCENARIO_DOCUMENT_TYPE.to_owned()),
            ),
            (
                "initial_charges",
                charges_node(&self.charge_groups, &self.initial_charges),
            ),
            (
                "initial_owned_targets",
                CanonicalNode::Array(
                    self.initial_owned_targets
                        .iter()
                        .map(|id| CanonicalNode::String(id.to_string()))
                        .collect(),
                ),
            ),
            (
                "initial_recruitment_count",
                CanonicalNode::Integer(self.initial_recruitment_count),
            ),
            (
                "initial_resources",
                resource_ledger_node(self.initial_resources),
            ),
            (
                "reward_schedule_id",
                CanonicalNode::String(self.reward_schedule_id.to_string()),
            ),
            (
                "ruleset_id",
                CanonicalNode::String(self.ruleset_id.to_string()),
            ),
            ("scenario_id", CanonicalNode::String(self.id.to_string())),
            (
                "schema_version",
                CanonicalNode::Integer(DOCUMENT_SCHEMA_VERSION_V3),
            ),
            ("strategy", strategy_node(&self.strategy)),
            (
                "students",
                CanonicalNode::Array(
                    self.students
                        .iter()
                        .map(|id| object([("student_id", CanonicalNode::String(id.to_string()))]))
                        .collect(),
                ),
            ),
            ("targets", targets_node(&self.targets)),
        ])
    }

    #[must_use]
    pub fn behavior_node(&self) -> CanonicalNode {
        let mut normalized_groups = BTreeMap::<usize, usize>::new();
        let mut initial_charges = Vec::new();
        let mut targets = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            let Some(banner) = self.banners.get(target.banner_index) else {
                targets.push(CanonicalNode::Null);
                continue;
            };
            let group_index = banner.charge_group_index;
            let normalized_index = if let Some(index) = normalized_groups.get(&group_index) {
                *index
            } else {
                let index = normalized_groups.len();
                normalized_groups.insert(group_index, index);
                initial_charges.push(
                    self.initial_charges
                        .get(group_index)
                        .copied()
                        .map_or(CanonicalNode::Null, CanonicalNode::Integer),
                );
                index
            };
            targets.push(object([
                (
                    "banner_probability_profile",
                    self.probability_profiles
                        .get(target.banner_index)
                        .map_or(CanonicalNode::Null, canonical_probability_profile_node),
                ),
                (
                    "charge_group",
                    CanonicalNode::Integer(normalized_index as u64),
                ),
            ]));
        }
        let initial_owned = self
            .targets
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let owned = (self.initial_owned_mask & (1_u8 << index)) != 0;
                CanonicalNode::Bool(owned)
            })
            .collect();
        object([
            ("behavior_schema_version", CanonicalNode::Integer(3)),
            ("initial_charges", CanonicalNode::Array(initial_charges)),
            (
                "initial_owned_target_positions",
                CanonicalNode::Array(initial_owned),
            ),
            (
                "initial_recruitment_count",
                CanonicalNode::Integer(self.initial_recruitment_count),
            ),
            (
                "initial_resources",
                resource_ledger_node(self.initial_resources),
            ),
            ("strategy", strategy_behavior_node(&self.strategy)),
            ("targets", CanonicalNode::Array(targets)),
        ])
    }

    pub fn behavior_fingerprint(&self) -> Result<SemanticFingerprint, CoreError> {
        SemanticFingerprint::from_node(&self.behavior_node())
    }

    pub fn document_fingerprint(&self) -> Result<SemanticFingerprint, CoreError> {
        SemanticFingerprint::from_node(&self.semantic_node())
    }
}

fn compile_probability_row(
    row: &RawCrossTargetProbabilityRowV3,
    featured: ProbabilityRatio,
    current_index: usize,
    targets: &[Target],
    path: Option<&Path>,
) -> Result<(OriginalProbabilityRowV3, CompiledOutcomeDistribution), CoreError> {
    compile_probability_parts(
        row.denominator,
        &row.other_target_weights,
        featured,
        current_index,
        targets,
        path,
    )
}

fn compile_threshold_row(
    row: &RawCrossTargetThresholdRowV3,
    featured: ProbabilityRatio,
    current_index: usize,
    targets: &[Target],
    path: Option<&Path>,
) -> Result<(OriginalProbabilityRowV3, CompiledOutcomeDistribution), CoreError> {
    compile_probability_parts(
        row.denominator,
        &row.other_target_weights,
        featured,
        current_index,
        targets,
        path,
    )
}

fn compile_probability_parts(
    denominator: u64,
    raw_weights: &[crate::schema::RawOtherTargetWeightV3],
    featured: ProbabilityRatio,
    current_index: usize,
    targets: &[Target],
    path: Option<&Path>,
) -> Result<(OriginalProbabilityRowV3, CompiledOutcomeDistribution), CoreError> {
    let expected = targets
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != current_index)
        .collect::<Vec<_>>();
    if raw_weights.len() != expected.len() {
        return Err(CoreError::validation(
            path,
            "other_target_weights must contain every non-featured configured target",
        ));
    }
    let mut ordered = Vec::with_capacity(expected.len());
    for (raw, (expected_index, expected_target)) in raw_weights.iter().zip(expected) {
        let supplied = StudentId::new(raw.target_id.clone())
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        if supplied != expected_target.student_id {
            return Err(CoreError::validation(
                path,
                "other_target_weights must appear in configured target order and exclude the featured target",
            ));
        }
        ordered.push((
            TargetIndex::new(expected_index, targets.len())
                .map_err(|error| CoreError::validation(path, error.to_string()))?,
            raw.weight,
        ));
    }
    let original = OriginalProbabilityRowV3 {
        denominator,
        other_target_weights: ordered.clone(),
    };
    let compiled = CompiledOutcomeDistribution::compile(featured, denominator, &ordered)
        .map_err(|error| CoreError::validation(path, error.to_string()))?;
    Ok((original, compiled))
}

fn validate_safe_non_featured(
    maximum: u64,
    increment: u64,
    overrides: &[(u64, ProbabilityRatio)],
) -> Result<(), CoreError> {
    let unsafe_start = maximum.saturating_sub(increment.saturating_sub(1));
    for charge in unsafe_start..=maximum {
        let probability = overrides
            .binary_search_by_key(&charge, |(candidate, _)| *candidate)
            .ok()
            .and_then(|index| overrides.get(index))
            .map(|(_, ratio)| *ratio);
        if !probability.is_some_and(ProbabilityRatio::is_one) {
            return Err(CoreError::validation(
                None,
                format!(
                    "pre-charge {charge} requires a probability-one override to prevent overflow"
                ),
            ));
        }
        if charge == u64::MAX {
            break;
        }
    }
    Ok(())
}

fn ratio_node(ratio: ProbabilityRatio) -> CanonicalNode {
    object([
        ("denominator", CanonicalNode::Integer(ratio.denominator())),
        ("numerator", CanonicalNode::Integer(ratio.numerator())),
    ])
}

fn threshold_overrides_node(overrides: &[(u64, ProbabilityRatio)]) -> CanonicalNode {
    CanonicalNode::Array(
        overrides
            .iter()
            .map(|(charge, ratio)| {
                object([
                    ("featured_target_probability", ratio_node(*ratio)),
                    ("pre_charge", CanonicalNode::Integer(*charge)),
                ])
            })
            .collect(),
    )
}

fn strategy_node(strategy: &CompiledStrategyV3) -> CanonicalNode {
    object([
        (
            "funding_priority",
            funding_priority_node(strategy.funding_priority),
        ),
        (
            "kind",
            CanonicalNode::String("sequential_targets".to_owned()),
        ),
        (
            "max_additional_recruitments",
            CanonicalNode::Integer(strategy.max_additional_recruitments.get()),
        ),
        (
            "strategy_id",
            CanonicalNode::String(strategy.strategy_id.to_string()),
        ),
        (
            "strategy_schema_version",
            CanonicalNode::Integer(strategy.strategy_schema_version),
        ),
    ])
}

fn strategy_behavior_node(strategy: &CompiledStrategyV3) -> CanonicalNode {
    object([
        (
            "funding_priority",
            funding_priority_node(strategy.funding_priority),
        ),
        (
            "kind",
            CanonicalNode::String("sequential_targets".to_owned()),
        ),
        (
            "max_additional_recruitments",
            CanonicalNode::Integer(strategy.max_additional_recruitments.get()),
        ),
        (
            "strategy_schema_version",
            CanonicalNode::Integer(strategy.strategy_schema_version),
        ),
    ])
}

fn funding_priority_node(priority: [FundingKind; 2]) -> CanonicalNode {
    CanonicalNode::Array(
        priority
            .into_iter()
            .map(|kind| {
                CanonicalNode::String(
                    match kind {
                        FundingKind::TicketTen => "ticket_ten",
                        FundingKind::PaidSingle => "paid_single",
                    }
                    .to_owned(),
                )
            })
            .collect(),
    )
}

fn resource_ledger_node(resources: ResourceLedger) -> CanonicalNode {
    CanonicalNode::Object(
        resources
            .iter_canonical()
            .map(|(kind, value)| {
                (
                    resource_kind_name_v3(kind).to_owned(),
                    CanonicalNode::Integer(value),
                )
            })
            .collect(),
    )
}

fn banners_node(banners: &[Banner]) -> CanonicalNode {
    CanonicalNode::Array(
        banners
            .iter()
            .map(|banner| {
                object([
                    (
                        "banner_id",
                        CanonicalNode::String(banner.banner_id.to_string()),
                    ),
                    (
                        "charge_group_id",
                        CanonicalNode::String(banner.charge_group_id.to_string()),
                    ),
                    (
                        "featured_student_id",
                        CanonicalNode::String(banner.featured_student_id.to_string()),
                    ),
                ])
            })
            .collect(),
    )
}

fn targets_node(targets: &[Target]) -> CanonicalNode {
    CanonicalNode::Array(
        targets
            .iter()
            .map(|target| {
                object([
                    (
                        "banner_id",
                        CanonicalNode::String(target.banner_id.to_string()),
                    ),
                    (
                        "student_id",
                        CanonicalNode::String(target.student_id.to_string()),
                    ),
                ])
            })
            .collect(),
    )
}

fn charges_node(groups: &[ChargeGroupId], charges: &[u64]) -> CanonicalNode {
    CanonicalNode::Array(
        groups
            .iter()
            .zip(charges)
            .map(|(group, charge)| {
                object([
                    ("charge_group_id", CanonicalNode::String(group.to_string())),
                    ("pre_recruitment_charge", CanonicalNode::Integer(*charge)),
                ])
            })
            .collect(),
    )
}

fn original_probability_profiles_node(profiles: &[BannerProbabilityProfileV3]) -> CanonicalNode {
    CanonicalNode::Array(
        profiles
            .iter()
            .map(|profile| {
                object([
                    (
                        "banner_id",
                        CanonicalNode::String(profile.banner_id.to_string()),
                    ),
                    (
                        "ordinary",
                        original_probability_row_node(&profile.ordinary_original),
                    ),
                    (
                        "threshold_overrides",
                        CanonicalNode::Array(
                            profile
                                .threshold_original
                                .iter()
                                .map(|(charge, row)| {
                                    let CanonicalNode::Object(mut fields) =
                                        original_probability_row_node(row)
                                    else {
                                        unreachable!("probability row is an object");
                                    };
                                    fields.insert(
                                        "pre_charge".to_owned(),
                                        CanonicalNode::Integer(*charge),
                                    );
                                    CanonicalNode::Object(fields)
                                })
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect(),
    )
}

fn original_probability_row_node(row: &OriginalProbabilityRowV3) -> CanonicalNode {
    object([
        ("denominator", CanonicalNode::Integer(row.denominator)),
        (
            "other_target_weights",
            CanonicalNode::Array(
                row.other_target_weights
                    .iter()
                    .map(|(target, weight)| {
                        object([
                            (
                                "target_index",
                                CanonicalNode::Integer(u64::from(target.get())),
                            ),
                            ("weight", CanonicalNode::Integer(*weight)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn canonical_probability_profile_node(profile: &BannerProbabilityProfileV3) -> CanonicalNode {
    object([
        ("ordinary", profile.ordinary.canonical_node()),
        (
            "threshold_overrides",
            CanonicalNode::Array(
                profile
                    .threshold_overrides
                    .iter()
                    .map(|(charge, distribution)| {
                        object([
                            ("distribution", distribution.canonical_node()),
                            ("pre_charge", CanonicalNode::Integer(*charge)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

#[allow(dead_code)]
fn _resources_v3_anchor(_: ResourcesV3, _: LedgerResourceKind) {}
