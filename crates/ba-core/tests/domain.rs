use std::path::{Path, PathBuf};

use ba_core::{
    ActionFundingKind, RecruitOutcome, StrategyDecision, TerminalReason,
    apply_primitive_transition, begin_action, complete_action, decide, initial_world, load_bundle,
    outcome_distribution, reconstruct_funding,
};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn bundle(name: &str) -> ba_core::ValidatedScenarioBundle {
    load_bundle(
        workspace_path("data"),
        workspace_path(&format!("scenarios/golden/{name}.json")),
    )
    .expect("shipped bundle should validate")
}

#[test]
fn charge_boundaries_duplicate_pickup_and_action_metrics_use_compiled_rules() {
    let bundle = bundle("charge_199_one");
    let world = initial_world(&bundle);
    let action = match decide(&bundle, &world).expect("decision") {
        StrategyDecision::Act(action) => action,
        other => panic!("expected action, got {other:?}"),
    };
    let (in_flight, started) = begin_action(&bundle, &world, &action).expect("begin");
    assert_eq!(
        started.pyroxene_deducted,
        bundle.ruleset().paid_single_cost()
    );
    let distribution = outcome_distribution(&bundle, &in_flight).expect("distribution");
    assert_eq!(distribution.len(), 1);
    assert_eq!(distribution[0].outcome, RecruitOutcome::Pickup);
    let transitioned = apply_primitive_transition(&bundle, &in_flight, RecruitOutcome::Pickup)
        .expect("transition");
    assert_eq!(
        transitioned.event.post_charge,
        bundle.ruleset().hit_reset_charge()
    );
    let (terminal, _) = complete_action(transitioned.state).expect("complete");
    let metrics = reconstruct_funding(&bundle, &terminal).expect("metrics");
    assert_eq!(
        metrics.paid_pyroxene_spent,
        bundle.ruleset().paid_single_cost()
    );
    assert_eq!(
        metrics.paid_funded_primitive_recruitments,
        bundle.ruleset().paid_single_action_size()
    );
}

#[test]
fn ticket_actions_are_atomic_and_do_not_reevaluate_after_success() {
    let bundle = bundle("ticket_atomic");
    let world = initial_world(&bundle);
    let action = match decide(&bundle, &world).expect("decision") {
        StrategyDecision::Act(action) => action,
        other => panic!("expected action, got {other:?}"),
    };
    assert_eq!(action.funding, ActionFundingKind::TicketTen);
    let (mut in_flight, _) = begin_action(&bundle, &world, &action).expect("begin");
    assert_eq!(
        in_flight.remaining_primitive_draws,
        bundle.ruleset().ticket_action_size()
    );
    let first = apply_primitive_transition(&bundle, &in_flight, RecruitOutcome::Pickup)
        .expect("deterministic pickup");
    assert!(first.event.first_success);
    assert!(first.state.remaining_primitive_draws > 0);
    in_flight = first.state;
    while in_flight.remaining_primitive_draws > 0 {
        let outcome = outcome_distribution(&bundle, &in_flight)
            .expect("distribution")
            .first()
            .expect("one branch")
            .outcome;
        in_flight = apply_primitive_transition(&bundle, &in_flight, outcome)
            .expect("remaining atomic draw")
            .state;
    }
    let (terminal, _) = complete_action(in_flight).expect("complete");
    assert_eq!(
        terminal.cumulative_primitive_recruitments,
        bundle.ruleset().ticket_action_size()
    );
    assert!(matches!(
        decide(&bundle, &terminal),
        Ok(StrategyDecision::Stop(TerminalReason::TargetsAcquired))
    ));
}

#[test]
fn strategy_distinguishes_horizon_fit_from_resource_exhaustion() {
    let bundle = bundle("single_target_200");
    let mut state = initial_world(&bundle);
    state.cumulative_primitive_recruitments = 193;
    state.available_ticket_count = 1;
    state.remaining_pyroxene = bundle.ruleset().paid_single_cost();
    let decision = decide(&bundle, &state).expect("decision");
    assert!(matches!(
        decision,
        StrategyDecision::Act(ref action) if action.funding == ActionFundingKind::PaidSingle
    ));

    state.remaining_pyroxene = 0;
    assert!(matches!(
        decide(&bundle, &state),
        Ok(StrategyDecision::Stop(
            TerminalReason::StrategyHorizonReached
        ))
    ));

    state.cumulative_primitive_recruitments = 200;
    assert!(matches!(
        decide(&bundle, &state),
        Ok(StrategyDecision::Stop(
            TerminalReason::StrategyHorizonReached
        ))
    ));

    state.cumulative_primitive_recruitments = 10;
    state.available_ticket_count = 0;
    assert!(matches!(
        decide(&bundle, &state),
        Ok(StrategyDecision::Stop(TerminalReason::ResourcesExhausted))
    ));
}

#[test]
fn shared_and_independent_charge_groups_have_canonical_state() {
    let shared = bundle("dual_shared_200");
    assert_eq!(initial_world(&shared).charges.len(), 1);
    let independent = bundle("dual_independent_200");
    let state = initial_world(&independent);
    assert_eq!(state.charges.len(), 2);
    assert_eq!(state.charges, vec![0, 99]);
}
