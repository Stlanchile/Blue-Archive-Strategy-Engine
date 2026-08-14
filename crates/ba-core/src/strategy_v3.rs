use crate::catalog::ValidatedScenarioBundleV3;
use crate::kernel::{ActionFundingKind, RequestedAction, TerminalReason, WorldStateKey};
use crate::model::FundingKind;
use crate::{CoreError, OwnershipMask};

pub fn decide_v3(
    bundle: &ValidatedScenarioBundleV3,
    state: &WorldStateKey,
) -> Result<crate::StrategyDecision, CoreError> {
    if state.owned_target_mask == bundle.scenario().all_targets_mask() {
        return Ok(crate::StrategyDecision::Stop(
            TerminalReason::TargetsAcquired,
        ));
    }
    let horizon = bundle.compiled_strategy().max_additional_recruitments.get();
    let remaining = horizon
        .checked_sub(state.cumulative_primitive_recruitments)
        .ok_or_else(|| CoreError::InvalidAction {
            message: "world count exceeds the configured v3 additional horizon".to_owned(),
        })?;
    if remaining == 0 {
        return Ok(crate::StrategyDecision::Stop(
            TerminalReason::StrategyHorizonReached,
        ));
    }

    let ownership =
        OwnershipMask::from_raw(state.owned_target_mask, bundle.scenario().targets().len())?;
    let target_index = (0..bundle.scenario().targets().len())
        .find(|index| ownership.contains(*index).is_ok_and(|owned| !owned))
        .ok_or_else(|| CoreError::InternalInvariant {
            message: "incomplete v3 ownership mask has no unowned ordered target".to_owned(),
        })?;
    let target = bundle
        .scenario()
        .targets()
        .get(target_index)
        .ok_or_else(|| CoreError::InternalInvariant {
            message: "selected v3 target index is out of range".to_owned(),
        })?;

    let ticket_affordable = state.available_ticket_count > 0;
    let paid_affordable = state.remaining_pyroxene >= bundle.ruleset().paid_single_cost();
    let ticket_fits = bundle.ruleset().ticket_action_size() <= remaining;
    let paid_fits = bundle.ruleset().paid_single_action_size() <= remaining;

    for funding in bundle.compiled_strategy().funding_priority {
        let (affordable, fits, action_funding) = match funding {
            FundingKind::TicketTen => {
                (ticket_affordable, ticket_fits, ActionFundingKind::TicketTen)
            }
            FundingKind::PaidSingle => (paid_affordable, paid_fits, ActionFundingKind::PaidSingle),
        };
        if affordable && fits {
            return Ok(crate::StrategyDecision::Act(RequestedAction {
                banner_index: target.banner_index,
                funding: action_funding,
            }));
        }
    }
    if ticket_affordable || paid_affordable {
        Ok(crate::StrategyDecision::Stop(
            TerminalReason::StrategyHorizonReached,
        ))
    } else {
        Ok(crate::StrategyDecision::Stop(
            TerminalReason::ResourcesExhausted,
        ))
    }
}
