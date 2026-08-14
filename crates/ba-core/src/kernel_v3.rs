use serde::Serialize;

use crate::catalog::ValidatedScenarioBundleV3;
use crate::kernel::{
    ActionCompletedEvent, ActionFundingKind, ActionStartedEvent, InFlightStateKey,
    ReconstructedFunding, RequestedAction, WorldStateKey,
};
use crate::model_v3::CompiledRulesetV3;
use crate::probability_v3::{CompiledOutcomeDistribution, PrimitiveAcquisition};
use crate::reward_schedule_v3::{MilestoneV3, RewardV3};
use crate::{BannerId, CoreError, LedgerResourceKind, OwnershipMask, ResourceLedger, StudentId};

#[derive(Debug, Clone, Serialize)]
pub struct PrimitiveTransitionEventV3 {
    pub additional_recruitment_count: u64,
    pub absolute_campaign_recruitment_count: u64,
    pub banner_id: BannerId,
    pub featured_student_id: StudentId,
    pub acquired_target_id: Option<StudentId>,
    pub outcome: PrimitiveAcquisition,
    pub pre_charge: u64,
    pub post_charge: u64,
    pub target_newly_owned: bool,
    pub first_all_targets_completed: bool,
    pub milestone_count: Option<u64>,
    pub rewards: Vec<RewardV3>,
    pub tickets_deferred: u64,
}

#[derive(Debug, Clone)]
pub struct TransitionResultV3 {
    pub state: InFlightStateKey,
    pub event: PrimitiveTransitionEventV3,
}

#[must_use]
pub fn initial_world_v3(bundle: &ValidatedScenarioBundleV3) -> WorldStateKey {
    let resources = bundle.scenario().initial_resources();
    WorldStateKey {
        owned_target_mask: bundle.scenario().initial_owned_mask(),
        charges: bundle.scenario().initial_charges().to_vec(),
        cumulative_primitive_recruitments: 0,
        remaining_pyroxene: resources.get(LedgerResourceKind::Pyroxene),
        available_ticket_count: resources.get(LedgerResourceKind::LimitedTenRecruitmentTickets),
    }
}

pub fn outcome_distribution_v3<'a>(
    bundle: &'a ValidatedScenarioBundleV3,
    state: &InFlightStateKey,
) -> Result<&'a CompiledOutcomeDistribution, CoreError> {
    let banner = bundle
        .scenario()
        .banners()
        .get(state.locked_banner_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "locked v3 banner index is out of range".to_owned(),
        })?;
    let pre_charge = *state
        .world
        .charges
        .get(banner.charge_group_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "v3 charge group index is out of range".to_owned(),
        })?;
    if pre_charge > bundle.ruleset().maximum_pre_recruitment_charge() {
        return Err(CoreError::InvalidTransition {
            message: "v3 pre-charge exceeds the compiled ruleset maximum".to_owned(),
        });
    }
    bundle
        .scenario()
        .probability_distribution(state.locked_banner_index, pre_charge)
}

pub fn begin_action_v3(
    bundle: &ValidatedScenarioBundleV3,
    state: &WorldStateKey,
    action: &RequestedAction,
) -> Result<(InFlightStateKey, ActionStartedEvent), CoreError> {
    let banner = bundle
        .scenario()
        .banners()
        .get(action.banner_index)
        .ok_or_else(|| CoreError::InvalidAction {
            message: "requested v3 banner index is out of range".to_owned(),
        })?;
    if state.owned_target_mask == bundle.scenario().all_targets_mask() {
        return Err(CoreError::InvalidAction {
            message: "cannot begin a v3 action after all targets are acquired".to_owned(),
        });
    }
    if state.charges.len() != bundle.scenario().charge_groups().len() {
        return Err(CoreError::InvalidAction {
            message: "v3 world charge vector has the wrong canonical length".to_owned(),
        });
    }
    let (primitive_draws, pyroxene_deducted, tickets_deducted) = match action.funding {
        ActionFundingKind::PaidSingle => {
            let cost = bundle.ruleset().paid_single_cost();
            if state.remaining_pyroxene < cost {
                return Err(CoreError::InvalidAction {
                    message: "v3 paid action is unaffordable".to_owned(),
                });
            }
            (bundle.ruleset().paid_single_action_size(), cost, 0)
        }
        ActionFundingKind::TicketTen => {
            if state.available_ticket_count == 0 {
                return Err(CoreError::InvalidAction {
                    message: "v3 ticket action is unaffordable".to_owned(),
                });
            }
            (bundle.ruleset().ticket_action_size(), 0, 1)
        }
    };
    let completion = state
        .cumulative_primitive_recruitments
        .checked_add(primitive_draws)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "checking v3 action fit against additional horizon",
        })?;
    if completion > bundle.compiled_strategy().max_additional_recruitments.get() {
        return Err(CoreError::InvalidAction {
            message: "requested v3 action crosses the additional horizon".to_owned(),
        });
    }
    let mut world = state.clone();
    world.remaining_pyroxene = world
        .remaining_pyroxene
        .checked_sub(pyroxene_deducted)
        .ok_or(CoreError::InvalidAction {
            message: "v3 paid action deduction underflowed".to_owned(),
        })?;
    world.available_ticket_count = world
        .available_ticket_count
        .checked_sub(tickets_deducted)
        .ok_or(CoreError::InvalidAction {
            message: "v3 ticket action deduction underflowed".to_owned(),
        })?;
    Ok((
        InFlightStateKey {
            world,
            locked_banner_index: action.banner_index,
            remaining_primitive_draws: primitive_draws,
            action_funding_kind: action.funding,
            deferred_ticket_count: 0,
        },
        ActionStartedEvent {
            banner_id: banner.banner_id.clone(),
            funding: action.funding,
            primitive_draws,
            pyroxene_deducted,
            tickets_deducted,
        },
    ))
}

pub fn apply_primitive_transition_v3(
    bundle: &ValidatedScenarioBundleV3,
    state: &InFlightStateKey,
    outcome: PrimitiveAcquisition,
) -> Result<TransitionResultV3, CoreError> {
    if state.remaining_primitive_draws == 0 {
        return Err(CoreError::InvalidTransition {
            message: "cannot recruit after a v3 action has completed".to_owned(),
        });
    }
    if !outcome_distribution_v3(bundle, state)?.contains(outcome) {
        return Err(CoreError::InvalidTransition {
            message: format!("v3 outcome {outcome:?} has zero probability in this state"),
        });
    }
    let banner = bundle
        .scenario()
        .banners()
        .get(state.locked_banner_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "locked v3 banner index is out of range".to_owned(),
        })?;
    let banner_id = banner.banner_id.clone();
    let featured_student_id = banner.featured_student_id.clone();
    let charge_group_index = banner.charge_group_index;
    let current_target = bundle
        .scenario()
        .target_index_for_student(&featured_student_id)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "v3 banner featured student is not a target".to_owned(),
        })?;
    let mut next = state.clone();
    let pre_charge = *next.world.charges.get(charge_group_index).ok_or_else(|| {
        CoreError::InvalidTransition {
            message: "v3 charge group index is out of range".to_owned(),
        }
    })?;
    let was_complete = next.world.owned_target_mask == bundle.scenario().all_targets_mask();
    let mut ownership = OwnershipMask::from_raw(
        next.world.owned_target_mask,
        bundle.scenario().targets().len(),
    )?;
    let (acquired_target, target_newly_owned, featured_hit) = match outcome {
        PrimitiveAcquisition::CurrentFeaturedTarget => (
            Some(featured_student_id.clone()),
            ownership.insert_target(current_target),
            true,
        ),
        PrimitiveAcquisition::OtherConfiguredTarget { target_index } => {
            if target_index == current_target {
                return Err(CoreError::InvalidTransition {
                    message: "current featured target cannot be encoded as an other target"
                        .to_owned(),
                });
            }
            let target = bundle
                .scenario()
                .targets()
                .get(target_index.as_usize())
                .ok_or_else(|| CoreError::InvalidTransition {
                    message: "other-target index is outside configured v3 targets".to_owned(),
                })?;
            (
                Some(target.student_id.clone()),
                ownership.insert_target(target_index),
                false,
            )
        }
        PrimitiveAcquisition::NoConfiguredTarget => (None, false, false),
    };
    next.world.owned_target_mask = ownership.raw();
    let post_charge = if featured_hit {
        bundle.ruleset().featured_hit_reset_charge()
    } else {
        increment_non_featured(bundle.ruleset(), pre_charge)?
    };
    *next
        .world
        .charges
        .get_mut(charge_group_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "v3 charge group index is out of range".to_owned(),
        })? = post_charge;

    next.world.cumulative_primitive_recruitments = next
        .world
        .cumulative_primitive_recruitments
        .checked_add(1)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "incrementing v3 additional recruitment count",
        })?;
    next.remaining_primitive_draws =
        next.remaining_primitive_draws
            .checked_sub(1)
            .ok_or(CoreError::InternalInvariant {
                message: "v3 action remaining draw count underflowed".to_owned(),
            })?;
    let absolute_count = bundle
        .scenario()
        .absolute_campaign_count(next.world.cumulative_primitive_recruitments)?;
    let milestone = bundle.reward_schedule().milestone_at(absolute_count);
    let rewards = milestone
        .as_ref()
        .map_or_else(Vec::new, |value| value.rewards.clone());
    let tickets_deferred = deferred_tickets_v3(milestone.as_ref())?;
    next.deferred_ticket_count = next
        .deferred_ticket_count
        .checked_add(tickets_deferred)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "deferring v3 ticket rewards during an action",
        })?;
    let is_complete = next.world.owned_target_mask == bundle.scenario().all_targets_mask();

    Ok(TransitionResultV3 {
        event: PrimitiveTransitionEventV3 {
            additional_recruitment_count: next.world.cumulative_primitive_recruitments,
            absolute_campaign_recruitment_count: absolute_count,
            banner_id,
            featured_student_id,
            acquired_target_id: acquired_target,
            outcome,
            pre_charge,
            post_charge,
            target_newly_owned,
            first_all_targets_completed: !was_complete && is_complete,
            milestone_count: milestone.as_ref().map(|value| value.count),
            rewards,
            tickets_deferred,
        },
        state: next,
    })
}

pub fn reconstruct_funding_v3(
    bundle: &ValidatedScenarioBundleV3,
    terminal: &WorldStateKey,
) -> Result<ReconstructedFunding, CoreError> {
    let initial = bundle.scenario().initial_resources();
    let spent = initial
        .get(LedgerResourceKind::Pyroxene)
        .checked_sub(terminal.remaining_pyroxene)
        .ok_or(CoreError::InternalInvariant {
            message: "terminal v3 pyroxene exceeds initial pyroxene".to_owned(),
        })?;
    let paid_cost = bundle.ruleset().paid_single_cost();
    if spent % paid_cost != 0 {
        return Err(CoreError::InternalInvariant {
            message: "v3 paid spend is not divisible by compiled action cost".to_owned(),
        });
    }
    let paid_action_count = spent / paid_cost;
    let paid_funded_primitive_recruitments = paid_action_count
        .checked_mul(bundle.ruleset().paid_single_action_size())
        .ok_or(CoreError::ArithmeticOverflow {
            context: "reconstructing v3 paid-funded primitive recruitments",
        })?;
    let ticket_funded_primitive_recruitments = terminal
        .cumulative_primitive_recruitments
        .checked_sub(paid_funded_primitive_recruitments)
        .ok_or(CoreError::InternalInvariant {
            message: "reconstructed v3 paid draws exceed terminal draws".to_owned(),
        })?;
    let ticket_size = bundle.ruleset().ticket_action_size();
    if ticket_funded_primitive_recruitments % ticket_size != 0 {
        return Err(CoreError::InternalInvariant {
            message: "v3 ticket-funded draws are not divisible by ticket action size".to_owned(),
        });
    }
    Ok(ReconstructedFunding {
        paid_pyroxene_spent: spent,
        paid_action_count,
        paid_funded_primitive_recruitments,
        ticket_funded_primitive_recruitments,
        ticket_action_count: ticket_funded_primitive_recruitments / ticket_size,
    })
}

pub fn terminal_resources_v3(
    bundle: &ValidatedScenarioBundleV3,
    terminal: &WorldStateKey,
) -> Result<ResourceLedger, CoreError> {
    let mut resources = bundle.scenario().initial_resources();
    let absolute = bundle
        .scenario()
        .absolute_campaign_count(terminal.cumulative_primitive_recruitments)?;
    resources.checked_add_ledger(
        bundle
            .reward_schedule()
            .resources_earned_between(bundle.scenario().initial_recruitment_count(), absolute)?,
    )?;
    set_active_resources(
        &mut resources,
        terminal.remaining_pyroxene,
        terminal.available_ticket_count,
    )?;
    Ok(resources)
}

pub fn milestone_rewards_acquired_v3(
    bundle: &ValidatedScenarioBundleV3,
    additional_count: u64,
) -> Result<ResourceLedger, CoreError> {
    let absolute = bundle
        .scenario()
        .absolute_campaign_count(additional_count)?;
    bundle
        .reward_schedule()
        .resources_earned_between(bundle.scenario().initial_recruitment_count(), absolute)
}

fn set_active_resources(
    resources: &mut ResourceLedger,
    pyroxene: u64,
    tickets: u64,
) -> Result<(), CoreError> {
    let current_pyroxene = resources.get(LedgerResourceKind::Pyroxene);
    let current_tickets = resources.get(LedgerResourceKind::LimitedTenRecruitmentTickets);
    let mut active = ResourceLedger::default();
    active.checked_add(LedgerResourceKind::Pyroxene, current_pyroxene)?;
    active.checked_add(
        LedgerResourceKind::LimitedTenRecruitmentTickets,
        current_tickets,
    )?;
    resources.checked_sub_ledger(active)?;
    resources.checked_add(LedgerResourceKind::Pyroxene, pyroxene)?;
    resources.checked_add(LedgerResourceKind::LimitedTenRecruitmentTickets, tickets)
}

fn increment_non_featured(ruleset: &CompiledRulesetV3, pre_charge: u64) -> Result<u64, CoreError> {
    let incremented = pre_charge
        .checked_add(ruleset.non_featured_increment())
        .ok_or(CoreError::ArithmeticOverflow {
            context: "incrementing v3 pre-recruitment charge",
        })?;
    if incremented > ruleset.maximum_pre_recruitment_charge() {
        return Err(CoreError::InvalidTransition {
            message: format!(
                "non-featured v3 outcome from pre-charge {pre_charge} would exceed the compiled maximum"
            ),
        });
    }
    Ok(incremented)
}

fn deferred_tickets_v3(milestone: Option<&MilestoneV3>) -> Result<u64, CoreError> {
    let mut tickets = 0_u64;
    if let Some(milestone) = milestone {
        for reward in &milestone.rewards {
            if reward.resource == LedgerResourceKind::LimitedTenRecruitmentTickets {
                tickets =
                    tickets
                        .checked_add(reward.quantity)
                        .ok_or(CoreError::ArithmeticOverflow {
                            context: "summing deferred v3 tickets in one milestone",
                        })?;
            }
        }
    }
    Ok(tickets)
}

pub fn complete_action_v3(
    state: InFlightStateKey,
) -> Result<(WorldStateKey, ActionCompletedEvent), CoreError> {
    crate::complete_action(state)
}
