use crate::catalog::ValidatedScenarioBundle;
use crate::kernel::{ActionFundingKind, RequestedAction, TerminalReason, WorldStateKey};
use crate::model::{CompiledStrategy, FundingKind, Target, ValidatedScenario};
use crate::{CompiledRuleset, CoreError, OwnershipMask, Resources};

#[derive(Debug)]
pub struct DecisionView<'a> {
    pub ordered_targets: &'a [Target],
    pub owned_target_mask: u8,
    pub resources: Resources,
    pub charges: &'a [u64],
    pub cumulative_primitive_recruitments: u64,
    pub configured_horizon: u64,
    pub remaining_horizon: u64,
    pub ruleset: &'a CompiledRuleset,
    scenario: &'a ValidatedScenario,
}

impl<'a> DecisionView<'a> {
    pub fn new(
        bundle: &'a ValidatedScenarioBundle,
        state: &'a WorldStateKey,
    ) -> Result<Self, CoreError> {
        let configured_horizon = bundle.compiled_strategy().max_total_recruitments().get();
        let remaining_horizon = configured_horizon
            .checked_sub(state.cumulative_primitive_recruitments)
            .ok_or_else(|| CoreError::InvalidAction {
                message: "world count exceeds the configured strategy horizon".to_owned(),
            })?;
        let mut resources = bundle.scenario().initial_resources();
        resources.checked_add(
            bundle
                .reward_schedule()
                .resources_earned_through(state.cumulative_primitive_recruitments)?,
        )?;
        resources.pyroxene = state.remaining_pyroxene;
        resources.limited_ten_recruitment_tickets = state.available_ticket_count;
        Ok(Self {
            ordered_targets: bundle.scenario().targets(),
            owned_target_mask: state.owned_target_mask,
            resources,
            charges: &state.charges,
            cumulative_primitive_recruitments: state.cumulative_primitive_recruitments,
            configured_horizon,
            remaining_horizon,
            ruleset: bundle.ruleset(),
            scenario: bundle.scenario(),
        })
    }

    #[must_use]
    pub fn all_targets_owned(&self) -> bool {
        self.owned_target_mask == self.scenario.all_targets_mask()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyDecision {
    Act(RequestedAction),
    Stop(TerminalReason),
}

pub trait Strategy {
    fn decide(&self, view: &DecisionView<'_>) -> Result<StrategyDecision, CoreError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SequentialTargetsPreferTickets;

impl Strategy for SequentialTargetsPreferTickets {
    fn decide(&self, view: &DecisionView<'_>) -> Result<StrategyDecision, CoreError> {
        decide_sequential(view, [FundingKind::TicketTen, FundingKind::PaidSingle])
    }
}

fn decide_sequential(
    view: &DecisionView<'_>,
    funding_priority: [FundingKind; 2],
) -> Result<StrategyDecision, CoreError> {
    if view.all_targets_owned() {
        return Ok(StrategyDecision::Stop(TerminalReason::TargetsAcquired));
    }
    if view.remaining_horizon == 0 {
        return Ok(StrategyDecision::Stop(
            TerminalReason::StrategyHorizonReached,
        ));
    }

    let ownership = OwnershipMask::from_raw(view.owned_target_mask, view.ordered_targets.len())?;
    let target_index = (0..view.ordered_targets.len())
        .find(|index| ownership.contains(*index).is_ok_and(|owned| !owned))
        .ok_or_else(|| CoreError::InternalInvariant {
            message: "incomplete ownership mask has no unowned ordered target".to_owned(),
        })?;
    let target =
        view.ordered_targets
            .get(target_index)
            .ok_or_else(|| CoreError::InternalInvariant {
                message: "selected target index is out of range".to_owned(),
            })?;

    let ticket_affordable = view.resources.limited_ten_recruitment_tickets > 0;
    let paid_affordable = view.resources.pyroxene >= view.ruleset.paid_single_cost();
    let ticket_fits = fits(view.remaining_horizon, view.ruleset.ticket_action_size());
    let paid_fits = fits(
        view.remaining_horizon,
        view.ruleset.paid_single_action_size(),
    );

    for funding in funding_priority {
        let (affordable, fits, action_funding) = match funding {
            FundingKind::TicketTen => {
                (ticket_affordable, ticket_fits, ActionFundingKind::TicketTen)
            }
            FundingKind::PaidSingle => (paid_affordable, paid_fits, ActionFundingKind::PaidSingle),
        };
        if affordable && fits {
            return Ok(StrategyDecision::Act(RequestedAction {
                banner_index: target.banner_index,
                funding: action_funding,
            }));
        }
    }
    if ticket_affordable || paid_affordable {
        Ok(StrategyDecision::Stop(
            TerminalReason::StrategyHorizonReached,
        ))
    } else {
        Ok(StrategyDecision::Stop(TerminalReason::ResourcesExhausted))
    }
}

pub fn decide(
    bundle: &ValidatedScenarioBundle,
    state: &WorldStateKey,
) -> Result<StrategyDecision, CoreError> {
    let view = DecisionView::new(bundle, state)?;
    match bundle.compiled_strategy() {
        CompiledStrategy::SequentialTargetsV2 {
            funding_priority, ..
        } => decide_sequential(&view, *funding_priority),
    }
}

fn fits(remaining: u64, action_size: u64) -> bool {
    action_size <= remaining
}
