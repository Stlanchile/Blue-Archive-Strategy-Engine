use serde::{Deserialize, Serialize};

use crate::catalog::ValidatedScenarioBundle;
use crate::id::{BannerId, StudentId};
use crate::model::{Milestone, Reward};
use crate::{CoreError, OwnershipMask, ProbabilityRatio, ResourceKind, Resources};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct WorldStateKey {
    pub owned_target_mask: u8,
    pub charges: Vec<u64>,
    pub cumulative_primitive_recruitments: u64,
    pub remaining_pyroxene: u64,
    pub available_ticket_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionFundingKind {
    PaidSingle,
    TicketTen,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct InFlightStateKey {
    pub world: WorldStateKey,
    pub locked_banner_index: usize,
    pub remaining_primitive_draws: u64,
    pub action_funding_kind: ActionFundingKind,
    pub deferred_ticket_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecruitOutcome {
    Pickup,
    Miss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutcomeBranch {
    pub outcome: RecruitOutcome,
    pub probability: ProbabilityRatio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedAction {
    pub banner_index: usize,
    pub funding: ActionFundingKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionStartedEvent {
    pub banner_id: BannerId,
    pub funding: ActionFundingKind,
    pub primitive_draws: u64,
    pub pyroxene_deducted: u64,
    pub tickets_deducted: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrimitiveTransitionEvent {
    pub recruitment_count: u64,
    pub banner_id: BannerId,
    pub featured_student_id: StudentId,
    pub outcome: RecruitOutcome,
    pub pre_charge: u64,
    pub post_charge: u64,
    pub target_newly_owned: bool,
    pub first_success: bool,
    pub milestone_count: Option<u64>,
    pub rewards: Vec<Reward>,
    pub tickets_deferred: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionCompletedEvent {
    pub tickets_activated: u64,
    pub available_ticket_count: u64,
}

#[derive(Debug, Clone)]
pub struct TransitionResult {
    pub state: InFlightStateKey,
    pub event: PrimitiveTransitionEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    TargetsAcquired,
    ResourcesExhausted,
    StrategyHorizonReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconstructedFunding {
    pub paid_pyroxene_spent: u64,
    pub paid_action_count: u64,
    pub paid_funded_primitive_recruitments: u64,
    pub ticket_funded_primitive_recruitments: u64,
    pub ticket_action_count: u64,
}

#[must_use]
pub fn initial_world(bundle: &ValidatedScenarioBundle) -> WorldStateKey {
    let resources = bundle.scenario().initial_resources();
    WorldStateKey {
        owned_target_mask: bundle.scenario().initial_owned_mask(),
        charges: bundle.scenario().initial_charges().to_vec(),
        cumulative_primitive_recruitments: 0,
        remaining_pyroxene: resources.pyroxene,
        available_ticket_count: resources.limited_ten_recruitment_tickets,
    }
}

pub fn outcome_distribution(
    bundle: &ValidatedScenarioBundle,
    state: &InFlightStateKey,
) -> Result<Vec<OutcomeBranch>, CoreError> {
    let banner = bundle
        .scenario()
        .banners()
        .get(state.locked_banner_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "locked banner index is out of range".to_owned(),
        })?;
    let pre_charge = *state
        .world
        .charges
        .get(banner.charge_group_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "charge group index is out of range".to_owned(),
        })?;
    if pre_charge > bundle.ruleset().maximum_pre_recruitment_charge() {
        return Err(CoreError::InvalidTransition {
            message: "pre-charge exceeds the compiled ruleset maximum".to_owned(),
        });
    }
    let pickup = bundle.ruleset().pickup_probability(pre_charge);
    let mut branches = Vec::with_capacity(2);
    if !pickup.is_zero() {
        branches.push(OutcomeBranch {
            outcome: RecruitOutcome::Pickup,
            probability: pickup,
        });
    }
    let miss = pickup.complement();
    if !miss.is_zero() {
        branches.push(OutcomeBranch {
            outcome: RecruitOutcome::Miss,
            probability: miss,
        });
    }
    if branches.is_empty() {
        return Err(CoreError::InternalInvariant {
            message: "validated probability produced no outcome branches".to_owned(),
        });
    }
    Ok(branches)
}

pub fn begin_action(
    bundle: &ValidatedScenarioBundle,
    state: &WorldStateKey,
    action: &RequestedAction,
) -> Result<(InFlightStateKey, ActionStartedEvent), CoreError> {
    let banner = bundle
        .scenario()
        .banners()
        .get(action.banner_index)
        .ok_or_else(|| CoreError::InvalidAction {
            message: "requested banner index is out of range".to_owned(),
        })?;
    if state.owned_target_mask == bundle.scenario().all_targets_mask() {
        return Err(CoreError::InvalidAction {
            message: "cannot begin an action after all targets are acquired".to_owned(),
        });
    }
    if state.charges.len() != bundle.scenario().charge_groups().len() {
        return Err(CoreError::InvalidAction {
            message: "world charge vector has the wrong canonical length".to_owned(),
        });
    }
    let (primitive_draws, pyroxene_deducted, tickets_deducted) = match action.funding {
        ActionFundingKind::PaidSingle => {
            let cost = bundle.ruleset().paid_single_cost();
            if state.remaining_pyroxene < cost {
                return Err(CoreError::InvalidAction {
                    message: "paid action is unaffordable".to_owned(),
                });
            }
            (bundle.ruleset().paid_single_action_size(), cost, 0_u64)
        }
        ActionFundingKind::TicketTen => {
            if state.available_ticket_count == 0 {
                return Err(CoreError::InvalidAction {
                    message: "ticket action is unaffordable".to_owned(),
                });
            }
            (bundle.ruleset().ticket_action_size(), 0_u64, 1_u64)
        }
    };
    if let Some(horizon) = bundle.compiled_strategy().max_total_recruitments() {
        let completion = state
            .cumulative_primitive_recruitments
            .checked_add(primitive_draws)
            .ok_or(CoreError::ArithmeticOverflow {
                context: "checking action fit against strategy horizon",
            })?;
        if completion > horizon.get() {
            return Err(CoreError::InvalidAction {
                message: "requested action crosses the strategy horizon".to_owned(),
            });
        }
    }
    let mut world = state.clone();
    world.remaining_pyroxene = world
        .remaining_pyroxene
        .checked_sub(pyroxene_deducted)
        .ok_or(CoreError::InvalidAction {
            message: "paid action deduction underflowed".to_owned(),
        })?;
    world.available_ticket_count = world
        .available_ticket_count
        .checked_sub(tickets_deducted)
        .ok_or(CoreError::InvalidAction {
            message: "ticket action deduction underflowed".to_owned(),
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

pub fn apply_primitive_transition(
    bundle: &ValidatedScenarioBundle,
    state: &InFlightStateKey,
    outcome: RecruitOutcome,
) -> Result<TransitionResult, CoreError> {
    if state.remaining_primitive_draws == 0 {
        return Err(CoreError::InvalidTransition {
            message: "cannot recruit after an action has completed".to_owned(),
        });
    }
    let distribution = outcome_distribution(bundle, state)?;
    if !distribution.iter().any(|branch| branch.outcome == outcome) {
        return Err(CoreError::InvalidTransition {
            message: format!("outcome {outcome:?} has zero probability in this state"),
        });
    }
    let banner = bundle
        .scenario()
        .banners()
        .get(state.locked_banner_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "locked banner index is out of range".to_owned(),
        })?;
    let mut next = state.clone();
    let charge = next
        .world
        .charges
        .get_mut(banner.charge_group_index)
        .ok_or_else(|| CoreError::InvalidTransition {
            message: "charge group index is out of range".to_owned(),
        })?;
    let pre_charge = *charge;
    let was_complete = next.world.owned_target_mask == bundle.scenario().all_targets_mask();
    let mut target_newly_owned = false;
    match outcome {
        RecruitOutcome::Pickup => {
            let target_index = bundle
                .scenario()
                .target_index_for_student(&banner.featured_student_id)
                .ok_or_else(|| CoreError::InvalidTransition {
                    message: "banner featured student is not a target".to_owned(),
                })?;
            let mut ownership = OwnershipMask::from_raw(
                next.world.owned_target_mask,
                bundle.scenario().targets().len(),
            )?;
            target_newly_owned = ownership.insert(target_index)?;
            next.world.owned_target_mask = ownership.raw();
            *charge = bundle.ruleset().hit_reset_charge();
        }
        RecruitOutcome::Miss => {
            let incremented = pre_charge
                .checked_add(bundle.ruleset().miss_increment())
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "incrementing pre-recruitment charge",
                })?;
            if incremented > bundle.ruleset().maximum_pre_recruitment_charge() {
                return Err(CoreError::InvalidTransition {
                    message: format!(
                        "miss from pre-charge {pre_charge} would exceed the compiled maximum"
                    ),
                });
            }
            *charge = incremented;
        }
    }
    next.world.cumulative_primitive_recruitments = next
        .world
        .cumulative_primitive_recruitments
        .checked_add(1)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "incrementing cumulative recruitment count",
        })?;
    next.remaining_primitive_draws =
        next.remaining_primitive_draws
            .checked_sub(1)
            .ok_or(CoreError::InternalInvariant {
                message: "action remaining draw count underflowed".to_owned(),
            })?;

    let milestone = bundle
        .reward_schedule()
        .milestone_at(next.world.cumulative_primitive_recruitments);
    let rewards = milestone.map_or_else(Vec::new, |value| value.rewards.clone());
    let tickets_deferred = deferred_tickets(milestone)?;
    next.deferred_ticket_count = next
        .deferred_ticket_count
        .checked_add(tickets_deferred)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "deferring ticket rewards during an action",
        })?;

    let is_complete = next.world.owned_target_mask == bundle.scenario().all_targets_mask();
    Ok(TransitionResult {
        event: PrimitiveTransitionEvent {
            recruitment_count: next.world.cumulative_primitive_recruitments,
            banner_id: banner.banner_id.clone(),
            featured_student_id: banner.featured_student_id.clone(),
            outcome,
            pre_charge,
            post_charge: *charge,
            target_newly_owned,
            first_success: !was_complete && is_complete,
            milestone_count: milestone.map(|value| value.count),
            rewards,
            tickets_deferred,
        },
        state: next,
    })
}

pub fn complete_action(
    state: InFlightStateKey,
) -> Result<(WorldStateKey, ActionCompletedEvent), CoreError> {
    if state.remaining_primitive_draws != 0 {
        return Err(CoreError::InvalidAction {
            message: "cannot complete an action with primitive draws remaining".to_owned(),
        });
    }
    let mut world = state.world;
    world.available_ticket_count = world
        .available_ticket_count
        .checked_add(state.deferred_ticket_count)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "activating deferred ticket rewards",
        })?;
    Ok((
        world.clone(),
        ActionCompletedEvent {
            tickets_activated: state.deferred_ticket_count,
            available_ticket_count: world.available_ticket_count,
        },
    ))
}

pub fn reconstruct_funding(
    bundle: &ValidatedScenarioBundle,
    terminal: &WorldStateKey,
) -> Result<ReconstructedFunding, CoreError> {
    let initial = bundle.scenario().initial_resources();
    let spent = initial
        .pyroxene
        .checked_sub(terminal.remaining_pyroxene)
        .ok_or(CoreError::InternalInvariant {
            message: "terminal pyroxene exceeds initial pyroxene".to_owned(),
        })?;
    let paid_cost = bundle.ruleset().paid_single_cost();
    if spent % paid_cost != 0 {
        return Err(CoreError::InternalInvariant {
            message: "paid spend is not divisible by compiled paid action cost".to_owned(),
        });
    }
    let paid_action_count = spent / paid_cost;
    let paid_funded_primitive_recruitments = paid_action_count
        .checked_mul(bundle.ruleset().paid_single_action_size())
        .ok_or(CoreError::ArithmeticOverflow {
            context: "reconstructing paid-funded primitive recruitments",
        })?;
    let ticket_funded_primitive_recruitments = terminal
        .cumulative_primitive_recruitments
        .checked_sub(paid_funded_primitive_recruitments)
        .ok_or(CoreError::InternalInvariant {
            message: "reconstructed paid draws exceed terminal draws".to_owned(),
        })?;
    let ticket_size = bundle.ruleset().ticket_action_size();
    if ticket_funded_primitive_recruitments % ticket_size != 0 {
        return Err(CoreError::InternalInvariant {
            message: "ticket-funded draws are not divisible by compiled ticket action size"
                .to_owned(),
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

pub fn terminal_resources(
    bundle: &ValidatedScenarioBundle,
    terminal: &WorldStateKey,
) -> Result<Resources, CoreError> {
    let mut resources = bundle.scenario().initial_resources();
    resources.checked_add(
        bundle
            .reward_schedule()
            .resources_earned_through(terminal.cumulative_primitive_recruitments)?,
    )?;
    resources.pyroxene = terminal.remaining_pyroxene;
    resources.limited_ten_recruitment_tickets = terminal.available_ticket_count;
    Ok(resources)
}

pub fn milestone_rewards_acquired(
    bundle: &ValidatedScenarioBundle,
    terminal_count: u64,
) -> Result<Resources, CoreError> {
    bundle
        .reward_schedule()
        .resources_earned_through(terminal_count)
}

fn deferred_tickets(milestone: Option<&Milestone>) -> Result<u64, CoreError> {
    let mut tickets = 0_u64;
    if let Some(milestone) = milestone {
        for reward in &milestone.rewards {
            if reward.resource == ResourceKind::LimitedTenRecruitmentTickets {
                tickets =
                    tickets
                        .checked_add(reward.quantity)
                        .ok_or(CoreError::ArithmeticOverflow {
                            context: "summing deferred tickets in one milestone",
                        })?;
            }
        }
    }
    Ok(tickets)
}
