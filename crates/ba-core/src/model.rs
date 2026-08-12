use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::path::Path;

use serde::Serialize;

use crate::fingerprint::{CanonicalNode, SemanticFingerprint, object};
use crate::id::{
    BannerId, ChargeGroupId, RewardScheduleId, RulesetId, ScenarioId, StrategyId, StudentId,
};
use crate::schema::{
    REWARD_SCHEDULE_DOCUMENT_TYPE, RULESET_DOCUMENT_TYPE, RawRewardScheduleV1, RawRulesetV1,
    RawScenarioV1, RawStrategyKind, SCENARIO_DOCUMENT_TYPE, SCHEMA_VERSION_V1,
};
use crate::{CoreError, ProbabilityRatio, ResourceKind, Resources};

#[derive(Debug, Clone)]
pub struct CompiledRuleset {
    id: RulesetId,
    paid_single_cost: NonZeroU64,
    paid_single_action_size: NonZeroU64,
    ticket_action_size: NonZeroU64,
    ordinary_pickup_probability: ProbabilityRatio,
    maximum_pre_recruitment_charge: u64,
    hit_reset_charge: u64,
    miss_increment: NonZeroU64,
    threshold_overrides: Vec<(u64, ProbabilityRatio)>,
}

#[derive(Debug, Clone)]
pub struct RulesetMechanics {
    pub paid_single_cost: u64,
    pub paid_single_action_size: u64,
    pub ticket_action_size: u64,
    pub ordinary_pickup_probability: ProbabilityRatio,
    pub maximum_pre_recruitment_charge: u64,
    pub hit_reset_charge: u64,
    pub miss_increment: u64,
    pub threshold_overrides: Vec<(u64, ProbabilityRatio)>,
}

impl CompiledRuleset {
    pub fn from_raw_provisional(raw: RawRulesetV1, path: Option<&Path>) -> Result<Self, CoreError> {
        validate_header(
            raw.schema_version,
            &raw.document_type,
            RULESET_DOCUMENT_TYPE,
            path,
        )?;
        let id = RulesetId::new(raw.ruleset_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let ordinary = ProbabilityRatio::new(
            raw.ordinary_pickup_probability.numerator,
            raw.ordinary_pickup_probability.denominator,
        )
        .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let mut overrides = Vec::with_capacity(raw.threshold_overrides.len());
        for item in raw.threshold_overrides {
            overrides.push((
                item.pre_charge,
                ProbabilityRatio::new(
                    item.pickup_probability.numerator,
                    item.pickup_probability.denominator,
                )
                .map_err(|error| CoreError::validation(path, error.to_string()))?,
            ));
        }
        let mechanics = RulesetMechanics {
            paid_single_cost: raw.paid_single_cost,
            paid_single_action_size: raw.paid_single_action_size,
            ticket_action_size: raw.ticket_action_size,
            ordinary_pickup_probability: ordinary,
            maximum_pre_recruitment_charge: raw.maximum_pre_recruitment_charge,
            hit_reset_charge: raw.hit_reset_charge,
            miss_increment: raw.miss_increment,
            threshold_overrides: overrides,
        };
        validate_provisional_v1(&mechanics, path)?;
        Self::from_parts(id, mechanics)
            .map_err(|error| CoreError::validation(path, error.to_string()))
    }

    pub fn from_parts(id: RulesetId, mechanics: RulesetMechanics) -> Result<Self, CoreError> {
        let paid_single_cost = NonZeroU64::new(mechanics.paid_single_cost)
            .ok_or_else(|| CoreError::validation(None, "paid_single_cost must be positive"))?;
        let paid_single_action_size = NonZeroU64::new(mechanics.paid_single_action_size)
            .ok_or_else(|| {
                CoreError::validation(None, "paid_single_action_size must be positive")
            })?;
        let ticket_action_size = NonZeroU64::new(mechanics.ticket_action_size)
            .ok_or_else(|| CoreError::validation(None, "ticket_action_size must be positive"))?;
        let miss_increment = NonZeroU64::new(mechanics.miss_increment)
            .ok_or_else(|| CoreError::validation(None, "miss_increment must be positive"))?;
        if mechanics.hit_reset_charge > mechanics.maximum_pre_recruitment_charge {
            return Err(CoreError::validation(
                None,
                "hit_reset_charge exceeds maximum_pre_recruitment_charge",
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
        validate_safe_misses(
            mechanics.maximum_pre_recruitment_charge,
            miss_increment.get(),
            &mechanics.threshold_overrides,
        )?;
        Ok(Self {
            id,
            paid_single_cost,
            paid_single_action_size,
            ticket_action_size,
            ordinary_pickup_probability: mechanics.ordinary_pickup_probability,
            maximum_pre_recruitment_charge: mechanics.maximum_pre_recruitment_charge,
            hit_reset_charge: mechanics.hit_reset_charge,
            miss_increment,
            threshold_overrides: mechanics.threshold_overrides,
        })
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
    pub const fn hit_reset_charge(&self) -> u64 {
        self.hit_reset_charge
    }

    #[must_use]
    pub const fn miss_increment(&self) -> u64 {
        self.miss_increment.get()
    }

    #[must_use]
    pub fn pickup_probability(&self, pre_charge: u64) -> ProbabilityRatio {
        self.threshold_overrides
            .binary_search_by_key(&pre_charge, |(charge, _)| *charge)
            .ok()
            .and_then(|index| self.threshold_overrides.get(index))
            .map_or(self.ordinary_pickup_probability, |(_, ratio)| *ratio)
    }

    #[must_use]
    pub fn threshold_overrides(&self) -> &[(u64, ProbabilityRatio)] {
        &self.threshold_overrides
    }

    #[must_use]
    pub const fn ordinary_pickup_probability(&self) -> ProbabilityRatio {
        self.ordinary_pickup_probability
    }

    pub fn semantic_node(&self) -> CanonicalNode {
        object([
            (
                "document_type",
                CanonicalNode::String(RULESET_DOCUMENT_TYPE.to_owned()),
            ),
            (
                "hit_reset_charge",
                CanonicalNode::Integer(self.hit_reset_charge),
            ),
            (
                "maximum_pre_recruitment_charge",
                CanonicalNode::Integer(self.maximum_pre_recruitment_charge),
            ),
            (
                "miss_increment",
                CanonicalNode::Integer(self.miss_increment.get()),
            ),
            (
                "ordinary_pickup_probability",
                ratio_node(self.ordinary_pickup_probability),
            ),
            (
                "paid_single_action_size",
                CanonicalNode::Integer(self.paid_single_action_size.get()),
            ),
            (
                "paid_single_cost",
                CanonicalNode::Integer(self.paid_single_cost.get()),
            ),
            ("ruleset_id", CanonicalNode::String(self.id.to_string())),
            ("schema_version", CanonicalNode::Integer(SCHEMA_VERSION_V1)),
            (
                "threshold_overrides",
                CanonicalNode::Array(
                    self.threshold_overrides
                        .iter()
                        .map(|(charge, ratio)| {
                            object([
                                ("pickup_probability", ratio_node(*ratio)),
                                ("pre_charge", CanonicalNode::Integer(*charge)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "ticket_action_size",
                CanonicalNode::Integer(self.ticket_action_size.get()),
            ),
        ])
    }

    pub fn fingerprint(&self) -> Result<SemanticFingerprint, CoreError> {
        SemanticFingerprint::from_node(&self.semantic_node())
    }
}

fn validate_safe_misses(
    maximum: u64,
    increment: u64,
    overrides: &[(u64, ProbabilityRatio)],
) -> Result<(), CoreError> {
    let unsafe_start = maximum.saturating_sub(increment.saturating_sub(1));
    let unsafe_count = maximum
        .checked_sub(unsafe_start)
        .and_then(|value| value.checked_add(1))
        .ok_or(CoreError::ArithmeticOverflow {
            context: "validating maximum-charge miss coverage",
        })?;
    let override_count =
        u64::try_from(overrides.len()).map_err(|_| CoreError::ArithmeticOverflow {
            context: "converting threshold override count",
        })?;
    if unsafe_count > override_count {
        return Err(CoreError::validation(
            None,
            "every charge whose miss would exceed the maximum must have a probability-one override",
        ));
    }
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

fn validate_provisional_v1(
    mechanics: &RulesetMechanics,
    path: Option<&Path>,
) -> Result<(), CoreError> {
    let ordinary = ProbabilityRatio::new(7, 1_000)?;
    let half = ProbabilityRatio::new(1, 2)?;
    let certain = ProbabilityRatio::new(1, 1)?;
    let expected_thresholds = vec![(99, half), (199, certain)];
    let valid = mechanics.paid_single_cost == 120
        && mechanics.paid_single_action_size == 1
        && mechanics.ticket_action_size == 10
        && mechanics.ordinary_pickup_probability == ordinary
        && mechanics.maximum_pre_recruitment_charge == 199
        && mechanics.hit_reset_charge == 0
        && mechanics.miss_increment == 1
        && mechanics.threshold_overrides == expected_thresholds;
    if valid {
        Ok(())
    } else {
        Err(CoreError::validation(
            path,
            "schema-v1 rulesets must exactly implement jp_2026_07_29_provisional_v1 mechanics",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reward {
    pub resource: ResourceKind,
    pub quantity: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Milestone {
    pub count: u64,
    pub rewards: Vec<Reward>,
}

#[derive(Debug, Clone)]
pub struct RewardSchedule {
    id: RewardScheduleId,
    compatible_ruleset_ids: Vec<RulesetId>,
    milestones: Vec<Milestone>,
    cumulative_resources: Vec<Resources>,
    total_ticket_rewards: u64,
}

impl RewardSchedule {
    pub fn from_raw(raw: RawRewardScheduleV1, path: Option<&Path>) -> Result<Self, CoreError> {
        validate_header(
            raw.schema_version,
            &raw.document_type,
            REWARD_SCHEDULE_DOCUMENT_TYPE,
            path,
        )?;
        let id = RewardScheduleId::new(raw.reward_schedule_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let mut compatible_ruleset_ids = Vec::with_capacity(raw.compatible_ruleset_ids.len());
        let mut compatible_set = BTreeSet::new();
        for value in raw.compatible_ruleset_ids {
            let value = RulesetId::new(value)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !compatible_set.insert(value.clone()) {
                return Err(CoreError::validation(
                    path,
                    format!("duplicate compatible ruleset ID {value}"),
                ));
            }
            compatible_ruleset_ids.push(value);
        }
        if compatible_ruleset_ids.is_empty() {
            return Err(CoreError::validation(
                path,
                "compatible_ruleset_ids must not be empty",
            ));
        }
        compatible_ruleset_ids.sort();

        let mut milestones = Vec::with_capacity(raw.milestones.len());
        let mut cumulative_resources = Vec::with_capacity(raw.milestones.len());
        let mut resources_through_milestone = Resources::default();
        let mut previous_count = None;
        for raw_milestone in raw.milestones {
            if raw_milestone.count == 0
                || previous_count.is_some_and(|value| raw_milestone.count <= value)
            {
                return Err(CoreError::validation(
                    path,
                    "milestone counts must be positive and strictly increasing",
                ));
            }
            previous_count = Some(raw_milestone.count);
            if raw_milestone.rewards.is_empty() {
                return Err(CoreError::validation(
                    path,
                    format!("milestone {} has no rewards", raw_milestone.count),
                ));
            }
            let mut kinds = BTreeSet::new();
            let mut rewards = Vec::with_capacity(raw_milestone.rewards.len());
            for raw_reward in raw_milestone.rewards {
                if raw_reward.quantity == 0 {
                    return Err(CoreError::validation(
                        path,
                        format!("milestone {} has a zero reward", raw_milestone.count),
                    ));
                }
                if raw_reward.resource == ResourceKind::Pyroxene {
                    return Err(CoreError::validation(
                        path,
                        "pyroxene milestone rewards are unsupported",
                    ));
                }
                if !kinds.insert(raw_reward.resource) {
                    return Err(CoreError::validation(
                        path,
                        format!(
                            "milestone {} repeats resource kind {:?}",
                            raw_milestone.count, raw_reward.resource
                        ),
                    ));
                }
                rewards.push(Reward {
                    resource: raw_reward.resource,
                    quantity: raw_reward.quantity,
                });
            }
            rewards.sort_by(|left, right| {
                left.quantity.cmp(&right.quantity).then_with(|| {
                    resource_kind_name(left.resource).cmp(resource_kind_name(right.resource))
                })
            });
            for reward in &rewards {
                resources_through_milestone
                    .checked_add_kind(reward.resource, reward.quantity)
                    .map_err(|_| {
                        CoreError::validation(
                            path,
                            format!(
                                "cumulative {} milestone rewards exceed u64",
                                resource_kind_name(reward.resource)
                            ),
                        )
                    })?;
            }
            milestones.push(Milestone {
                count: raw_milestone.count,
                rewards,
            });
            cumulative_resources.push(resources_through_milestone);
        }
        let total_ticket_rewards = cumulative_resources
            .last()
            .copied()
            .unwrap_or_default()
            .limited_ten_recruitment_tickets;
        Ok(Self {
            id,
            compatible_ruleset_ids,
            milestones,
            cumulative_resources,
            total_ticket_rewards,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &RewardScheduleId {
        &self.id
    }

    #[must_use]
    pub fn compatible_ruleset_ids(&self) -> &[RulesetId] {
        &self.compatible_ruleset_ids
    }

    #[must_use]
    pub fn milestones(&self) -> &[Milestone] {
        &self.milestones
    }

    #[must_use]
    pub const fn total_ticket_rewards(&self) -> u64 {
        self.total_ticket_rewards
    }

    #[must_use]
    pub fn milestone_at(&self, count: u64) -> Option<&Milestone> {
        self.milestones
            .binary_search_by_key(&count, |milestone| milestone.count)
            .ok()
            .and_then(|index| self.milestones.get(index))
    }

    pub fn resources_earned_through(&self, count: u64) -> Result<Resources, CoreError> {
        let reached = self
            .milestones
            .partition_point(|milestone| milestone.count <= count);
        if reached == 0 {
            return Ok(Resources::default());
        }
        self.cumulative_resources
            .get(reached - 1)
            .copied()
            .ok_or_else(|| CoreError::InternalInvariant {
                message: "reward prefix cache is inconsistent with validated milestones".to_owned(),
            })
    }

    pub fn semantic_node(&self) -> CanonicalNode {
        let mut compatible = self
            .compatible_ruleset_ids
            .iter()
            .map(|id| CanonicalNode::String(id.to_string()))
            .collect::<Vec<_>>();
        compatible.sort();
        object([
            ("compatible_ruleset_ids", CanonicalNode::Array(compatible)),
            (
                "document_type",
                CanonicalNode::String(REWARD_SCHEDULE_DOCUMENT_TYPE.to_owned()),
            ),
            (
                "milestones",
                CanonicalNode::Array(
                    self.milestones
                        .iter()
                        .map(|milestone| {
                            let mut rewards = milestone
                                .rewards
                                .iter()
                                .map(|reward| {
                                    object([
                                        ("quantity", CanonicalNode::Integer(reward.quantity)),
                                        (
                                            "resource",
                                            CanonicalNode::String(
                                                resource_kind_name(reward.resource).to_owned(),
                                            ),
                                        ),
                                    ])
                                })
                                .collect::<Vec<_>>();
                            rewards.sort();
                            object([
                                ("count", CanonicalNode::Integer(milestone.count)),
                                ("rewards", CanonicalNode::Array(rewards)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "reward_schedule_id",
                CanonicalNode::String(self.id.to_string()),
            ),
            ("schema_version", CanonicalNode::Integer(SCHEMA_VERSION_V1)),
        ])
    }

    pub fn fingerprint(&self) -> Result<SemanticFingerprint, CoreError> {
        SemanticFingerprint::from_node(&self.semantic_node())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Banner {
    pub banner_id: BannerId,
    pub featured_student_id: StudentId,
    pub charge_group_id: ChargeGroupId,
    pub charge_group_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    pub student_id: StudentId,
    pub banner_id: BannerId,
    pub banner_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StrategyConstraints {
    pub max_total_recruitments: Option<NonZeroU64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StrategyConfiguration {
    pub strategy_id: StrategyId,
    pub kind: RawStrategyKind,
    pub constraints: StrategyConstraints,
}

#[derive(Debug, Clone)]
pub struct ValidatedScenario {
    id: ScenarioId,
    ruleset_id: RulesetId,
    reward_schedule_id: RewardScheduleId,
    students: Vec<StudentId>,
    banners: Vec<Banner>,
    charge_groups: Vec<ChargeGroupId>,
    initial_charges: Vec<u64>,
    initial_resources: Resources,
    initial_owned_mask: u8,
    initial_owned_targets: Vec<StudentId>,
    strategy: StrategyConfiguration,
    targets: Vec<Target>,
    termination_bound: u64,
}

impl ValidatedScenario {
    pub fn from_raw(
        raw: RawScenarioV1,
        ruleset: &CompiledRuleset,
        rewards: &RewardSchedule,
        path: Option<&Path>,
    ) -> Result<Self, CoreError> {
        validate_header(
            raw.schema_version,
            &raw.document_type,
            SCENARIO_DOCUMENT_TYPE,
            path,
        )?;
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
                    message: "canonical charge group index missing".to_owned(),
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

        if !(1..=2).contains(&raw.targets.len()) {
            return Err(CoreError::validation(
                path,
                "a scenario must contain exactly one or two ordered targets",
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
                    message: "validated banner index out of range".to_owned(),
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
        let mut initial_charges = Vec::with_capacity(charge_groups.len());
        for group in &charge_groups {
            initial_charges.push(*charge_by_group.get(group).ok_or_else(|| {
                CoreError::validation(path, format!("missing initial charge for group {group}"))
            })?);
        }

        let mut initial_owned_mask = 0_u8;
        let mut owned_set = BTreeSet::new();
        for raw_owned in raw.initial_owned_targets {
            let owned = StudentId::new(raw_owned)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !owned_set.insert(owned.clone()) {
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
            initial_owned_mask |= 1_u8
                .checked_shl(
                    u32::try_from(index).map_err(|_| CoreError::ArithmeticOverflow {
                        context: "converting target mask index",
                    })?,
                )
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "constructing initial ownership mask",
                })?;
        }
        let initial_owned_targets = owned_set.into_iter().collect::<Vec<_>>();

        let strategy_id = StrategyId::new(raw.strategy.strategy_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let max_total_recruitments = match raw.strategy.max_total_recruitments {
            crate::schema::NullablePositive::Missing => {
                return Err(CoreError::validation(
                    path,
                    "strategy max_total_recruitments is required and must be null or a positive integer",
                ));
            }
            crate::schema::NullablePositive::Present(value) => value,
        };
        let strategy = StrategyConfiguration {
            strategy_id,
            kind: raw.strategy.kind,
            constraints: StrategyConstraints {
                max_total_recruitments,
            },
        };
        if strategy.kind != RawStrategyKind::SequentialTargetsPreferTickets {
            return Err(CoreError::validation(path, "unsupported strategy kind"));
        }

        let all_rewards = rewards
            .resources_earned_through(u64::MAX)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let mut maximum_resources = raw.initial_resources;
        maximum_resources
            .checked_add(all_rewards)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;

        let paid_actions = raw.initial_resources.pyroxene / ruleset.paid_single_cost();
        let paid_draws = paid_actions
            .checked_mul(ruleset.paid_single_action_size())
            .ok_or_else(|| {
                CoreError::validation(path, "paid recruitment termination bound exceeds u64")
            })?;
        let total_tickets = maximum_resources.limited_ten_recruitment_tickets;
        let ticket_draws = total_tickets
            .checked_mul(ruleset.ticket_action_size())
            .ok_or_else(|| {
                CoreError::validation(path, "ticket recruitment termination bound exceeds u64")
            })?;
        let termination_bound = paid_draws.checked_add(ticket_draws).ok_or_else(|| {
            CoreError::validation(path, "total recruitment termination bound exceeds u64")
        })?;

        Ok(Self {
            id,
            ruleset_id,
            reward_schedule_id,
            students,
            banners,
            charge_groups,
            initial_charges,
            initial_resources: raw.initial_resources,
            initial_owned_mask,
            initial_owned_targets,
            strategy,
            targets,
            termination_bound,
        })
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
    pub const fn initial_resources(&self) -> Resources {
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
    pub const fn strategy(&self) -> &StrategyConfiguration {
        &self.strategy
    }

    #[must_use]
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    #[must_use]
    pub const fn termination_bound(&self) -> u64 {
        self.termination_bound
    }

    #[must_use]
    pub fn all_targets_mask(&self) -> u8 {
        match self.targets.len() {
            1 => 0b01,
            2 => 0b11,
            _ => 0,
        }
    }

    #[must_use]
    pub fn target_index_for_student(&self, student: &StudentId) -> Option<usize> {
        self.targets
            .iter()
            .position(|target| &target.student_id == student)
    }

    pub fn semantic_node(&self) -> CanonicalNode {
        let students = self
            .students
            .iter()
            .map(|id| CanonicalNode::String(id.to_string()))
            .collect();
        let banners = self
            .banners
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
            .collect();
        let initial_charges = self
            .charge_groups
            .iter()
            .zip(&self.initial_charges)
            .map(|(group, charge)| {
                object([
                    ("charge_group_id", CanonicalNode::String(group.to_string())),
                    ("pre_recruitment_charge", CanonicalNode::Integer(*charge)),
                ])
            })
            .collect();
        let initial_owned = self
            .initial_owned_targets
            .iter()
            .map(|id| CanonicalNode::String(id.to_string()))
            .collect();
        let targets = self
            .targets
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
            .collect();
        object([
            ("banners", CanonicalNode::Array(banners)),
            (
                "document_type",
                CanonicalNode::String(SCENARIO_DOCUMENT_TYPE.to_owned()),
            ),
            ("initial_charges", CanonicalNode::Array(initial_charges)),
            ("initial_owned_targets", CanonicalNode::Array(initial_owned)),
            ("initial_resources", resources_node(self.initial_resources)),
            (
                "reward_schedule_id",
                CanonicalNode::String(self.reward_schedule_id.to_string()),
            ),
            (
                "ruleset_id",
                CanonicalNode::String(self.ruleset_id.to_string()),
            ),
            ("scenario_id", CanonicalNode::String(self.id.to_string())),
            ("schema_version", CanonicalNode::Integer(SCHEMA_VERSION_V1)),
            (
                "strategy",
                object([
                    (
                        "kind",
                        CanonicalNode::String("sequential_targets_prefer_tickets".to_owned()),
                    ),
                    (
                        "max_total_recruitments",
                        self.strategy
                            .constraints
                            .max_total_recruitments
                            .map_or(CanonicalNode::Null, |value| {
                                CanonicalNode::Integer(value.get())
                            }),
                    ),
                    (
                        "strategy_id",
                        CanonicalNode::String(self.strategy.strategy_id.to_string()),
                    ),
                ]),
            ),
            ("students", CanonicalNode::Array(students)),
            ("targets", CanonicalNode::Array(targets)),
        ])
    }

    pub fn fingerprint(&self) -> Result<SemanticFingerprint, CoreError> {
        SemanticFingerprint::from_node(&self.semantic_node())
    }
}

fn validate_header(
    version: u64,
    actual_type: &str,
    expected_type: &str,
    path: Option<&Path>,
) -> Result<(), CoreError> {
    if version != SCHEMA_VERSION_V1 || actual_type != expected_type {
        Err(CoreError::validation(
            path,
            format!(
                "typed document header mismatch: expected schema_version=1 and document_type={expected_type}"
            ),
        ))
    } else {
        Ok(())
    }
}

fn ratio_node(ratio: ProbabilityRatio) -> CanonicalNode {
    object([
        ("denominator", CanonicalNode::Integer(ratio.denominator())),
        ("numerator", CanonicalNode::Integer(ratio.numerator())),
    ])
}

fn resources_node(resources: Resources) -> CanonicalNode {
    object([
        (
            "advanced_bd_selectors",
            CanonicalNode::Integer(resources.advanced_bd_selectors),
        ),
        (
            "advanced_tech_note_selectors",
            CanonicalNode::Integer(resources.advanced_tech_note_selectors),
        ),
        ("eligma", CanonicalNode::Integer(resources.eligma)),
        ("gift_boxes", CanonicalNode::Integer(resources.gift_boxes)),
        (
            "limited_ten_recruitment_tickets",
            CanonicalNode::Integer(resources.limited_ten_recruitment_tickets),
        ),
        ("pyroxene", CanonicalNode::Integer(resources.pyroxene)),
        (
            "superior_tech_note_selectors",
            CanonicalNode::Integer(resources.superior_tech_note_selectors),
        ),
    ])
}

#[must_use]
pub const fn resource_kind_name(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Pyroxene => "pyroxene",
        ResourceKind::LimitedTenRecruitmentTickets => "limited_ten_recruitment_tickets",
        ResourceKind::Eligma => "eligma",
        ResourceKind::AdvancedBdSelectors => "advanced_bd_selectors",
        ResourceKind::AdvancedTechNoteSelectors => "advanced_tech_note_selectors",
        ResourceKind::SuperiorTechNoteSelectors => "superior_tech_note_selectors",
        ResourceKind::GiftBoxes => "gift_boxes",
    }
}
