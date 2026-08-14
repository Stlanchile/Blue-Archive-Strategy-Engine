use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::fingerprint::{CanonicalNode, SemanticFingerprint, object};
use crate::provenance_v3::{ProvenanceSubjectV3, ProvenanceV3, provenance_node_v3};
use crate::schema::{
    MAX_EFFECTIVE_MILESTONES_V3, REWARD_SCHEDULE_DOCUMENT_TYPE, RawMilestoneV3,
    RawRepeatMilestoneV3, RawRewardScheduleV3, RawRewardV3,
};
use crate::{
    CoreError, DOCUMENT_SCHEMA_VERSION_V3, LedgerResourceKind, ResourceLedger, RewardScheduleId,
    RulesetId, resource_kind_name_v3,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RewardV3 {
    pub resource: LedgerResourceKind,
    pub quantity: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MilestoneV3 {
    pub count: u64,
    pub rewards: Vec<RewardV3>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepeatMilestoneV3 {
    pub offset: u64,
    pub rewards: Vec<RewardV3>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepeatingCycleV3 {
    pub starts_after_count: u64,
    pub period: u64,
    pub milestones: Vec<RepeatMilestoneV3>,
}

#[derive(Debug, Clone)]
pub struct RewardScheduleV3 {
    id: RewardScheduleId,
    compatible_ruleset_ids: Vec<RulesetId>,
    initial_milestones: Vec<MilestoneV3>,
    repeating_cycle: Option<RepeatingCycleV3>,
    provenance: ProvenanceV3,
}

impl RewardScheduleV3 {
    pub fn from_raw(raw: RawRewardScheduleV3, path: Option<&Path>) -> Result<Self, CoreError> {
        if raw.schema_version != DOCUMENT_SCHEMA_VERSION_V3
            || raw.document_type != REWARD_SCHEDULE_DOCUMENT_TYPE
        {
            return Err(CoreError::validation(
                path,
                "typed document header mismatch: expected schema_version=3 and document_type=reward_schedule",
            ));
        }
        let id = RewardScheduleId::new(raw.reward_schedule_id)
            .map_err(|error| CoreError::validation(path, error.to_string()))?;
        let provenance =
            ProvenanceV3::from_raw(raw.provenance, ProvenanceSubjectV3::RewardSchedule, path)?;

        let mut compatible_set = BTreeSet::new();
        for raw_id in raw.compatible_ruleset_ids {
            let ruleset_id = RulesetId::new(raw_id)
                .map_err(|error| CoreError::validation(path, error.to_string()))?;
            if !compatible_set.insert(ruleset_id.clone()) {
                return Err(CoreError::validation(
                    path,
                    format!("duplicate compatible ruleset ID {ruleset_id}"),
                ));
            }
        }
        if compatible_set.is_empty() {
            return Err(CoreError::validation(
                path,
                "compatible_ruleset_ids must not be empty",
            ));
        }
        let compatible_ruleset_ids = compatible_set.into_iter().collect();

        let mut previous_count = None;
        let mut finite_ledger = ResourceLedger::default();
        let mut initial_milestones = Vec::with_capacity(raw.initial_milestones.len());
        for milestone in raw.initial_milestones {
            if milestone.count == 0
                || previous_count.is_some_and(|previous| milestone.count <= previous)
            {
                return Err(CoreError::validation(
                    path,
                    "initial milestone counts must be positive and strictly increasing",
                ));
            }
            previous_count = Some(milestone.count);
            let rewards = compile_rewards(milestone.rewards, milestone.count, path)?;
            for reward in &rewards {
                finite_ledger
                    .checked_add(reward.resource, reward.quantity)
                    .map_err(|_| {
                        CoreError::validation(
                            path,
                            format!(
                                "cumulative {} initial rewards exceed u64",
                                resource_kind_name_v3(reward.resource)
                            ),
                        )
                    })?;
            }
            initial_milestones.push(MilestoneV3 {
                count: milestone.count,
                rewards,
            });
        }

        let repeating_cycle = raw
            .repeating_cycle
            .map(|cycle| {
                let last_initial = initial_milestones
                    .last()
                    .map_or(0, |milestone| milestone.count);
                if cycle.starts_after_count < last_initial {
                    return Err(CoreError::validation(
                        path,
                        "repeating cycle starts_after_count precedes the last initial milestone",
                    ));
                }
                if cycle.milestones.is_empty() {
                    return Err(CoreError::validation(
                        path,
                        "a non-null repeating cycle must contain at least one milestone",
                    ));
                }
                let period = cycle.period.get();
                let mut previous_offset = None;
                let mut milestones = Vec::with_capacity(cycle.milestones.len());
                for milestone in cycle.milestones {
                    if milestone.offset == 0
                        || milestone.offset > period
                        || previous_offset.is_some_and(|previous| milestone.offset <= previous)
                    {
                        return Err(CoreError::validation(
                            path,
                            "repeat offsets must be positive, at most the period, and strictly increasing",
                        ));
                    }
                    previous_offset = Some(milestone.offset);
                    cycle
                        .starts_after_count
                        .checked_add(milestone.offset)
                        .ok_or_else(|| {
                            CoreError::validation(
                                path,
                                "first repeating milestone exceeds u64",
                            )
                        })?;
                    milestones.push(RepeatMilestoneV3 {
                        offset: milestone.offset,
                        rewards: compile_rewards(
                            milestone.rewards,
                            milestone.offset,
                            path,
                        )?,
                    });
                }
                Ok(RepeatingCycleV3 {
                    starts_after_count: cycle.starts_after_count,
                    period,
                    milestones,
                })
            })
            .transpose()?;

        Ok(Self {
            id,
            compatible_ruleset_ids,
            initial_milestones,
            repeating_cycle,
            provenance,
        })
    }

    #[must_use]
    pub const fn schema_version(&self) -> u64 {
        DOCUMENT_SCHEMA_VERSION_V3
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
    pub fn initial_milestones(&self) -> &[MilestoneV3] {
        &self.initial_milestones
    }

    #[must_use]
    pub const fn repeating_cycle(&self) -> Option<&RepeatingCycleV3> {
        self.repeating_cycle.as_ref()
    }

    #[must_use]
    pub const fn provenance(&self) -> &ProvenanceV3 {
        &self.provenance
    }

    #[must_use]
    pub fn milestone_at(&self, absolute_count: u64) -> Option<MilestoneV3> {
        if let Ok(index) = self
            .initial_milestones
            .binary_search_by_key(&absolute_count, |milestone| milestone.count)
        {
            return self.initial_milestones.get(index).cloned();
        }
        let cycle = self.repeating_cycle.as_ref()?;
        if absolute_count <= cycle.starts_after_count {
            return None;
        }
        let delta = absolute_count - cycle.starts_after_count;
        let cycle_offset = ((delta - 1) % cycle.period) + 1;
        let index = cycle
            .milestones
            .binary_search_by_key(&cycle_offset, |milestone| milestone.offset)
            .ok()?;
        let milestone = cycle.milestones.get(index)?;
        Some(MilestoneV3 {
            count: absolute_count,
            rewards: milestone.rewards.clone(),
        })
    }

    pub fn resources_earned_between(
        &self,
        start_exclusive: u64,
        end_inclusive: u64,
    ) -> Result<ResourceLedger, CoreError> {
        if start_exclusive > end_inclusive {
            return Err(CoreError::InvalidTransition {
                message: "reward interval start exceeds its end".to_owned(),
            });
        }
        let mut ledger = ResourceLedger::default();
        let first_finite = self
            .initial_milestones
            .partition_point(|milestone| milestone.count <= start_exclusive);
        let after_last_finite = self
            .initial_milestones
            .partition_point(|milestone| milestone.count <= end_inclusive);
        for milestone in &self.initial_milestones[first_finite..after_last_finite] {
            add_rewards(&mut ledger, &milestone.rewards, 1)?;
        }

        if let Some(cycle) = &self.repeating_cycle {
            for milestone in &cycle.milestones {
                let through_end = occurrences_through(
                    end_inclusive,
                    cycle.starts_after_count,
                    cycle.period,
                    milestone.offset,
                )?;
                let through_start = occurrences_through(
                    start_exclusive,
                    cycle.starts_after_count,
                    cycle.period,
                    milestone.offset,
                )?;
                let occurrences =
                    through_end
                        .checked_sub(through_start)
                        .ok_or(CoreError::InternalInvariant {
                            message: "repeat occurrence interval underflowed".to_owned(),
                        })?;
                add_rewards(&mut ledger, &milestone.rewards, occurrences)?;
            }
        }
        Ok(ledger)
    }

    pub fn resources_earned_through(
        &self,
        absolute_count: u64,
    ) -> Result<ResourceLedger, CoreError> {
        self.resources_earned_between(0, absolute_count)
    }

    pub fn first_future_repeat_milestone(
        &self,
        initial_count: u64,
    ) -> Result<Option<u64>, CoreError> {
        let Some(cycle) = &self.repeating_cycle else {
            return Ok(None);
        };
        let mut next = None;
        for milestone in &cycle.milestones {
            let base = cycle
                .starts_after_count
                .checked_add(milestone.offset)
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "computing first repeat milestone",
                })?;
            let cycle_index = if initial_count < base {
                0
            } else {
                initial_count
                    .checked_sub(base)
                    .and_then(|delta| delta.checked_div(cycle.period))
                    .and_then(|value| value.checked_add(1))
                    .ok_or(CoreError::ArithmeticOverflow {
                        context: "computing first future repeat cycle index",
                    })?
            };
            let candidate = base
                .checked_add(cycle_index.checked_mul(cycle.period).ok_or(
                    CoreError::ArithmeticOverflow {
                        context: "computing first future repeat milestone",
                    },
                )?)
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "computing first future repeat milestone",
                })?;
            next = Some(next.map_or(candidate, |current: u64| current.min(candidate)));
        }
        Ok(next)
    }

    pub fn effective_milestone_count(
        &self,
        initial_count: u64,
        additional_horizon: u64,
    ) -> Result<usize, CoreError> {
        let endpoint =
            initial_count
                .checked_add(additional_horizon)
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "computing v3 campaign endpoint",
                })?;
        let finite_start = self
            .initial_milestones
            .partition_point(|milestone| milestone.count <= initial_count);
        let finite_end = self
            .initial_milestones
            .partition_point(|milestone| milestone.count <= endpoint);
        let mut total = u64::try_from(finite_end - finite_start).map_err(|_| {
            CoreError::ArithmeticOverflow {
                context: "converting finite effective milestone count",
            }
        })?;
        if let Some(cycle) = &self.repeating_cycle {
            for milestone in &cycle.milestones {
                let through_end = occurrences_through(
                    endpoint,
                    cycle.starts_after_count,
                    cycle.period,
                    milestone.offset,
                )?;
                let through_start = occurrences_through(
                    initial_count,
                    cycle.starts_after_count,
                    cycle.period,
                    milestone.offset,
                )?;
                total = total
                    .checked_add(through_end.checked_sub(through_start).ok_or(
                        CoreError::InternalInvariant {
                            message: "repeat effective count underflowed".to_owned(),
                        },
                    )?)
                    .ok_or(CoreError::ArithmeticOverflow {
                        context: "counting effective v3 milestones",
                    })?;
            }
        }
        if total > MAX_EFFECTIVE_MILESTONES_V3 as u64 {
            return Err(CoreError::validation(
                None,
                format!(
                    "effective milestone count {total} exceeds maximum {MAX_EFFECTIVE_MILESTONES_V3}"
                ),
            ));
        }
        usize::try_from(total).map_err(|_| CoreError::ArithmeticOverflow {
            context: "converting effective v3 milestone count",
        })
    }

    pub fn materialized_future_milestones(
        &self,
        initial_count: u64,
        additional_horizon: u64,
    ) -> Result<Vec<MilestoneV3>, CoreError> {
        let count = self.effective_milestone_count(initial_count, additional_horizon)?;
        let endpoint =
            initial_count
                .checked_add(additional_horizon)
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "computing v3 campaign endpoint",
                })?;
        let finite_start = self
            .initial_milestones
            .partition_point(|milestone| milestone.count <= initial_count);
        let finite_end = self
            .initial_milestones
            .partition_point(|milestone| milestone.count <= endpoint);
        let mut milestones = Vec::with_capacity(count);
        milestones.extend_from_slice(&self.initial_milestones[finite_start..finite_end]);

        if let Some(cycle) = &self.repeating_cycle {
            for repeat in &cycle.milestones {
                let base = cycle.starts_after_count.checked_add(repeat.offset).ok_or(
                    CoreError::ArithmeticOverflow {
                        context: "computing first repeat milestone",
                    },
                )?;
                let first_index = if initial_count < base {
                    0
                } else {
                    initial_count
                        .checked_sub(base)
                        .and_then(|delta| delta.checked_div(cycle.period))
                        .and_then(|index| index.checked_add(1))
                        .ok_or(CoreError::ArithmeticOverflow {
                            context: "computing first future repeat index",
                        })?
                };
                let mut absolute = base
                    .checked_add(first_index.checked_mul(cycle.period).ok_or(
                        CoreError::ArithmeticOverflow {
                            context: "computing first future repeat milestone",
                        },
                    )?)
                    .ok_or(CoreError::ArithmeticOverflow {
                        context: "computing first future repeat milestone",
                    })?;
                while absolute <= endpoint {
                    milestones.push(MilestoneV3 {
                        count: absolute,
                        rewards: repeat.rewards.clone(),
                    });
                    if absolute == endpoint {
                        break;
                    }
                    absolute = absolute.checked_add(cycle.period).ok_or(
                        CoreError::ArithmeticOverflow {
                            context: "advancing repeat milestone",
                        },
                    )?;
                }
            }
        }
        milestones.sort_by_key(|milestone| milestone.count);
        if milestones.len() != count
            || milestones
                .windows(2)
                .any(|window| window[0].count >= window[1].count)
        {
            return Err(CoreError::InternalInvariant {
                message: "effective v3 milestone materialization is inconsistent".to_owned(),
            });
        }
        Ok(milestones)
    }

    #[must_use]
    pub fn semantic_node(&self) -> CanonicalNode {
        object([
            (
                "compatible_ruleset_ids",
                CanonicalNode::Array(
                    self.compatible_ruleset_ids
                        .iter()
                        .map(|id| CanonicalNode::String(id.to_string()))
                        .collect(),
                ),
            ),
            (
                "document_type",
                CanonicalNode::String(REWARD_SCHEDULE_DOCUMENT_TYPE.to_owned()),
            ),
            (
                "initial_milestones",
                milestones_node(&self.initial_milestones),
            ),
            ("provenance", provenance_node_v3(&self.provenance)),
            (
                "repeating_cycle",
                self.repeating_cycle
                    .as_ref()
                    .map_or(CanonicalNode::Null, repeat_node),
            ),
            (
                "reward_schedule_id",
                CanonicalNode::String(self.id.to_string()),
            ),
            (
                "schema_version",
                CanonicalNode::Integer(DOCUMENT_SCHEMA_VERSION_V3),
            ),
        ])
    }

    #[must_use]
    pub fn behavior_node(&self) -> CanonicalNode {
        object([
            ("behavior_schema_version", CanonicalNode::Integer(3)),
            (
                "initial_milestones",
                milestones_node(&self.initial_milestones),
            ),
            (
                "repeating_cycle",
                self.repeating_cycle
                    .as_ref()
                    .map_or(CanonicalNode::Null, repeat_node),
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

fn compile_rewards(
    raw_rewards: Vec<RawRewardV3>,
    coordinate: u64,
    path: Option<&Path>,
) -> Result<Vec<RewardV3>, CoreError> {
    if raw_rewards.is_empty() {
        return Err(CoreError::validation(
            path,
            format!("milestone {coordinate} has no rewards"),
        ));
    }
    let mut kinds = BTreeSet::new();
    let mut rewards = Vec::with_capacity(raw_rewards.len());
    for raw in raw_rewards {
        let resource = LedgerResourceKind::from(raw.resource);
        if raw.quantity == 0 {
            return Err(CoreError::validation(
                path,
                format!("milestone {coordinate} has a zero reward"),
            ));
        }
        if resource == LedgerResourceKind::Pyroxene {
            return Err(CoreError::validation(
                path,
                "pyroxene milestone rewards are unsupported",
            ));
        }
        if !kinds.insert(resource) {
            return Err(CoreError::validation(
                path,
                format!(
                    "milestone {coordinate} repeats resource kind {}",
                    resource_kind_name_v3(resource)
                ),
            ));
        }
        rewards.push(RewardV3 {
            resource,
            quantity: raw.quantity,
        });
    }
    rewards.sort_by_key(|reward| reward.resource);
    Ok(rewards)
}

fn add_rewards(
    ledger: &mut ResourceLedger,
    rewards: &[RewardV3],
    occurrences: u64,
) -> Result<(), CoreError> {
    for reward in rewards {
        let quantity =
            reward
                .quantity
                .checked_mul(occurrences)
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "multiplying repeating reward quantity",
                })?;
        ledger.checked_add(reward.resource, quantity)?;
    }
    Ok(())
}

fn occurrences_through(
    count: u64,
    starts_after_count: u64,
    period: u64,
    offset: u64,
) -> Result<u64, CoreError> {
    let first = starts_after_count
        .checked_add(offset)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "computing first repeating milestone",
        })?;
    if count < first {
        return Ok(0);
    }
    count
        .checked_sub(first)
        .and_then(|delta| delta.checked_div(period))
        .and_then(|occurrences| occurrences.checked_add(1))
        .ok_or(CoreError::ArithmeticOverflow {
            context: "counting repeating milestone occurrences",
        })
}

fn milestones_node(milestones: &[MilestoneV3]) -> CanonicalNode {
    CanonicalNode::Array(
        milestones
            .iter()
            .map(|milestone| {
                object([
                    ("count", CanonicalNode::Integer(milestone.count)),
                    ("rewards", rewards_node(&milestone.rewards)),
                ])
            })
            .collect(),
    )
}

fn repeat_node(cycle: &RepeatingCycleV3) -> CanonicalNode {
    object([
        (
            "milestones",
            CanonicalNode::Array(
                cycle
                    .milestones
                    .iter()
                    .map(|milestone| {
                        object([
                            ("offset", CanonicalNode::Integer(milestone.offset)),
                            ("rewards", rewards_node(&milestone.rewards)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("period", CanonicalNode::Integer(cycle.period)),
        (
            "starts_after_count",
            CanonicalNode::Integer(cycle.starts_after_count),
        ),
    ])
}

fn rewards_node(rewards: &[RewardV3]) -> CanonicalNode {
    CanonicalNode::Array(
        rewards
            .iter()
            .map(|reward| {
                object([
                    ("quantity", CanonicalNode::Integer(reward.quantity)),
                    (
                        "resource",
                        CanonicalNode::String(resource_kind_name_v3(reward.resource).to_owned()),
                    ),
                ])
            })
            .collect(),
    )
}

#[allow(dead_code)]
fn _type_anchor(_: RawMilestoneV3, _: RawRepeatMilestoneV3) {}
