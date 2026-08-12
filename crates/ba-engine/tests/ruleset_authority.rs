mod common;

use std::num::NonZeroU64;

use ba_core::{
    ActionFundingKind, ProbabilityRatio, RecruitOutcome, Resources, RulesetMechanics,
    StrategyDecision, TerminalReason, apply_primitive_transition, begin_action, decide,
    initial_world, reconstruct_funding,
};
use ba_engine::{ExactSolverOptions, analyze_exact, simulate_monte_carlo};

use common::synthetic_bundle;

fn mechanics(cost: u64, paid_size: u64, ordinary: ProbabilityRatio) -> RulesetMechanics {
    RulesetMechanics {
        paid_single_cost: cost,
        paid_single_action_size: paid_size,
        ticket_action_size: 3,
        ordinary_pickup_probability: ordinary,
        maximum_pre_recruitment_charge: 1,
        hit_reset_charge: 0,
        miss_increment: 1,
        threshold_overrides: vec![(1, ProbabilityRatio::new(1, 1).expect("certain"))],
    }
}

#[test]
fn changed_cost_controls_affordability_deduction_and_metric_reconstruction() {
    let affordable = synthetic_bundle(
        "cost_affordable",
        mechanics(100, 1, ProbabilityRatio::new(1, 1).expect("certain")),
        Resources {
            pyroxene: 100,
            ..Resources::default()
        },
        0,
        1,
        Vec::new(),
    );
    let state = initial_world(&affordable);
    let action = match decide(&affordable, &state).expect("decision") {
        StrategyDecision::Act(action) => action,
        other => panic!("expected action, got {other:?}"),
    };
    let (in_flight, started) = begin_action(&affordable, &state, &action).expect("start");
    assert_eq!(started.pyroxene_deducted, 100);
    let terminal = apply_primitive_transition(&affordable, &in_flight, RecruitOutcome::Pickup)
        .expect("pickup")
        .state
        .world;
    assert_eq!(
        reconstruct_funding(&affordable, &terminal)
            .expect("metrics")
            .paid_pyroxene_spent,
        100
    );

    let unaffordable = synthetic_bundle(
        "cost_unaffordable",
        mechanics(101, 1, ProbabilityRatio::new(1, 1).expect("certain")),
        Resources {
            pyroxene: 100,
            ..Resources::default()
        },
        0,
        1,
        Vec::new(),
    );
    assert!(matches!(
        decide(&unaffordable, &initial_world(&unaffordable)),
        Ok(StrategyDecision::Stop(TerminalReason::ResourcesExhausted))
    ));
}

#[test]
fn changed_action_size_controls_horizon_fit_and_atomic_expansion() {
    let too_large = synthetic_bundle(
        "size_horizon",
        mechanics(50, 2, ProbabilityRatio::new(1, 1).expect("certain")),
        Resources {
            pyroxene: 50,
            ..Resources::default()
        },
        0,
        1,
        Vec::new(),
    );
    assert!(matches!(
        decide(&too_large, &initial_world(&too_large)),
        Ok(StrategyDecision::Stop(
            TerminalReason::StrategyHorizonReached
        ))
    ));

    let fits = synthetic_bundle(
        "size_expansion",
        mechanics(50, 2, ProbabilityRatio::new(1, 1).expect("certain")),
        Resources {
            pyroxene: 50,
            ..Resources::default()
        },
        0,
        2,
        Vec::new(),
    );
    let exact = analyze_exact(&fits, ExactSolverOptions::default()).expect("exact");
    assert_eq!(exact.expected_terminal_primitive_recruitments, 2.0);
    assert_eq!(exact.solver_diagnostics.transition_expansions, 2);
    assert_eq!(exact.expected_paid_pyroxene_spent, 50.0);
}

#[test]
fn changed_thresholds_flow_through_the_common_exact_and_monte_carlo_kernel() {
    let threshold_mechanics = |probability: ProbabilityRatio| RulesetMechanics {
        paid_single_cost: 1,
        paid_single_action_size: 1,
        ticket_action_size: 3,
        ordinary_pickup_probability: ProbabilityRatio::new(1, 2).expect("ordinary half"),
        maximum_pre_recruitment_charge: 2,
        hit_reset_charge: 0,
        miss_increment: 1,
        threshold_overrides: vec![
            (1, probability),
            (2, ProbabilityRatio::new(1, 1).expect("certain maximum")),
        ],
    };
    let never = synthetic_bundle(
        "threshold_never",
        threshold_mechanics(ProbabilityRatio::new(0, 1).expect("zero")),
        Resources {
            pyroxene: 1,
            ..Resources::default()
        },
        1,
        1,
        Vec::new(),
    );
    let always = synthetic_bundle(
        "threshold_always",
        threshold_mechanics(ProbabilityRatio::new(1, 1).expect("one")),
        Resources {
            pyroxene: 1,
            ..Resources::default()
        },
        1,
        1,
        Vec::new(),
    );
    assert_eq!(
        analyze_exact(&never, ExactSolverOptions::default())
            .expect("exact never")
            .success_probability,
        0.0
    );
    assert_eq!(
        analyze_exact(&always, ExactSolverOptions::default())
            .expect("exact always")
            .success_probability,
        1.0
    );
    assert_eq!(
        simulate_monte_carlo(&never, NonZeroU64::new(8).expect("runs"), 4)
            .expect("MC never")
            .success_probability,
        0.0
    );
    assert_eq!(
        simulate_monte_carlo(&always, NonZeroU64::new(8).expect("runs"), 4)
            .expect("MC always")
            .success_probability,
        1.0
    );
}

#[test]
fn maximum_reset_and_increment_are_compiled_ruleset_authority() {
    let mechanics = RulesetMechanics {
        paid_single_cost: 1,
        paid_single_action_size: 2,
        ticket_action_size: 4,
        ordinary_pickup_probability: ProbabilityRatio::new(1, 2).expect("half"),
        maximum_pre_recruitment_charge: 5,
        hit_reset_charge: 3,
        miss_increment: 2,
        threshold_overrides: vec![
            (4, ProbabilityRatio::new(1, 1).expect("certain")),
            (5, ProbabilityRatio::new(1, 1).expect("certain")),
        ],
    };
    let bundle = synthetic_bundle(
        "charge_authority",
        mechanics,
        Resources {
            pyroxene: 1,
            ..Resources::default()
        },
        2,
        2,
        Vec::new(),
    );
    let action = match decide(&bundle, &initial_world(&bundle)).expect("decision") {
        StrategyDecision::Act(action) => action,
        other => panic!("expected action, got {other:?}"),
    };
    assert_eq!(action.funding, ActionFundingKind::PaidSingle);
    let (state, _) = begin_action(&bundle, &initial_world(&bundle), &action).expect("start");
    let missed = apply_primitive_transition(&bundle, &state, RecruitOutcome::Miss).expect("miss");
    assert_eq!(missed.event.post_charge, 4);
    let hit = apply_primitive_transition(&bundle, &missed.state, RecruitOutcome::Pickup)
        .expect("forced hit");
    assert_eq!(hit.event.post_charge, 3);
    assert_eq!(bundle.ruleset().maximum_pre_recruitment_charge(), 5);
    assert_eq!(bundle.ruleset().miss_increment(), 2);
    assert_eq!(bundle.ruleset().hit_reset_charge(), 3);
}
