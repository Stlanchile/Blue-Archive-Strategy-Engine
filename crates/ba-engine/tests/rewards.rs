mod common;

use std::path::{Path, PathBuf};

use ba_core::{ProbabilityRatio, Resources, RulesetMechanics, TerminalReason, load_bundle};
use ba_engine::{ExactSolverOptions, analyze_exact};

use common::{synthetic_bundle, ticket_reward};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn shipped_campaign_schedule_is_applied_exactly_once_at_reached_counts() {
    let bundle = load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/campaign_dual_310.json"),
    )
    .expect("campaign bundle");
    assert_eq!(bundle.reward_schedule().milestones().len(), 16);
    assert_eq!(bundle.reward_schedule().milestones()[0].count, 10);
    assert_eq!(bundle.reward_schedule().milestones()[15].count, 310);
    assert_eq!(bundle.reward_schedule().total_ticket_rewards(), 5);

    let result = analyze_exact(&bundle, ExactSolverOptions::default()).expect("campaign exact");
    assert_eq!(result.milestone_reach_probabilities.len(), 16);
    assert!(
        result
            .milestone_reach_probabilities
            .windows(2)
            .all(|window| window[0].probability >= window[1].probability)
    );
    assert!(result.expected_milestone_rewards_acquired.eligma > 0.0);
    assert!(
        result
            .expected_milestone_rewards_acquired
            .advanced_bd_selectors
            > 0.0
    );
    assert!(result.expected_ticket_funded_primitive_recruitments > 0.0);
}

#[test]
fn a_milestone_ticket_recursively_enables_the_next_atomic_action() {
    let mechanics = RulesetMechanics {
        paid_single_cost: 1,
        paid_single_action_size: 1,
        ticket_action_size: 10,
        ordinary_pickup_probability: ProbabilityRatio::new(0, 1).expect("zero"),
        maximum_pre_recruitment_charge: 100,
        hit_reset_charge: 0,
        miss_increment: 1,
        threshold_overrides: vec![(100, ProbabilityRatio::new(1, 1).expect("certain"))],
    };
    let bundle = synthetic_bundle(
        "recursive_ticket",
        mechanics,
        Resources {
            limited_ten_recruitment_tickets: 7,
            ..Resources::default()
        },
        0,
        80,
        vec![ticket_reward(70, 1)],
    );
    let result = analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact");
    assert_eq!(result.success_probability, 0.0);
    assert_eq!(result.expected_terminal_primitive_recruitments, 80.0);
    assert_eq!(result.expected_ticket_funded_primitive_recruitments, 80.0);
    assert_eq!(
        result
            .expected_milestone_rewards_acquired
            .limited_ten_recruitment_tickets,
        1.0
    );
    assert_eq!(
        result
            .expected_residual_resources
            .limited_ten_recruitment_tickets,
        0.0
    );
    assert!(result.terminal_reason_probabilities.iter().any(|entry| {
        entry.terminal_reason == TerminalReason::StrategyHorizonReached && entry.probability == 1.0
    }));
}
