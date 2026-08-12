use crate::catalog::ValidatedScenarioBundle;
use crate::kernel::{ActionFundingKind, RequestedAction, TerminalReason, WorldStateKey};
use crate::model::{Target, ValidatedScenario};
use crate::{CompiledRuleset, CoreError, Resources};

#[derive(Debug)]
pub struct DecisionView<'a> {
    pub ordered_targets: &'a [Target],
    pub owned_target_mask: u8,
    pub resources: Resources,
    pub charges: &'a [u64],
    pub cumulative_primitive_recruitments: u64,
    pub configured_horizon: Option<u64>,
    pub remaining_horizon: Option<u64>,
    pub ruleset: &'a CompiledRuleset,
    scenario: &'a ValidatedScenario,
}

impl<'a> DecisionView<'a> {
    pub fn new(
        bundle: &'a ValidatedScenarioBundle,
        state: &'a WorldStateKey,
    ) -> Result<Self, CoreError> {
        let configured_horizon = bundle
            .scenario()
            .strategy()
            .constraints
            .max_total_recruitments
            .map(std::num::NonZeroU64::get);
        let remaining_horizon = configured_horizon
            .map(|horizon| {
                horizon
                    .checked_sub(state.cumulative_primitive_recruitments)
                    .ok_or_else(|| CoreError::InvalidAction {
                        message: "world count exceeds the configured strategy horizon".to_owned(),
                    })
            })
            .transpose()?;
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
        if view.all_targets_owned() {
            return Ok(StrategyDecision::Stop(TerminalReason::TargetsAcquired));
        }
        if view.remaining_horizon == Some(0) {
            return Ok(StrategyDecision::Stop(
                TerminalReason::StrategyHorizonReached,
            ));
        }

        let target_index = (0..view.ordered_targets.len())
            .find(|index| {
                let shift = u32::try_from(*index).unwrap_or(u32::MAX);
                let bit = 1_u8.checked_shl(shift).unwrap_or(0);
                view.owned_target_mask & bit == 0
            })
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

        if ticket_affordable && ticket_fits {
            return Ok(StrategyDecision::Act(RequestedAction {
                banner_index: target.banner_index,
                funding: ActionFundingKind::TicketTen,
            }));
        }
        if paid_affordable && paid_fits {
            return Ok(StrategyDecision::Act(RequestedAction {
                banner_index: target.banner_index,
                funding: ActionFundingKind::PaidSingle,
            }));
        }
        if ticket_affordable || paid_affordable {
            Ok(StrategyDecision::Stop(
                TerminalReason::StrategyHorizonReached,
            ))
        } else {
            Ok(StrategyDecision::Stop(TerminalReason::ResourcesExhausted))
        }
    }
}

pub fn decide(
    bundle: &ValidatedScenarioBundle,
    state: &WorldStateKey,
) -> Result<StrategyDecision, CoreError> {
    let view = DecisionView::new(bundle, state)?;
    SequentialTargetsPreferTickets.decide(&view)
}

fn fits(remaining: Option<u64>, action_size: u64) -> bool {
    remaining.is_none_or(|value| action_size <= value)
}
