mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ba_core::{
    ActionFundingKind, RecruitOutcome, Resources, StrategyDecision, apply_primitive_transition,
    begin_action, complete_action, decide, initial_world, load_bundle, outcome_distribution,
};
use ba_engine::{EngineError, ExactSolverOptions, analyze_exact, analyze_exact_detailed, replay};
use tempfile::TempDir;

use common::{half_probability_mechanics, synthetic_bundle};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn first_success_history_merges_outside_the_markov_key_and_matches_oracle() {
    let bundle = synthetic_bundle(
        "merge",
        half_probability_mechanics(),
        Resources {
            limited_ten_recruitment_tickets: 1,
            ..Resources::default()
        },
        0,
        Some(10),
        Vec::new(),
    );
    let initial = initial_world(&bundle);
    let action = match decide(&bundle, &initial).expect("decision") {
        StrategyDecision::Act(action) => action,
        other => panic!("expected action, got {other:?}"),
    };
    assert_eq!(action.funding, ActionFundingKind::TicketTen);
    let (started, _) = begin_action(&bundle, &initial, &action).expect("start");

    let mut prefix_a = started.clone();
    for outcome in [
        RecruitOutcome::Pickup,
        RecruitOutcome::Miss,
        RecruitOutcome::Miss,
        RecruitOutcome::Miss,
        RecruitOutcome::Pickup,
    ] {
        prefix_a = apply_primitive_transition(&bundle, &prefix_a, outcome)
            .expect("prefix A")
            .state;
    }
    let mut prefix_b = started.clone();
    for outcome in [
        RecruitOutcome::Miss,
        RecruitOutcome::Miss,
        RecruitOutcome::Miss,
        RecruitOutcome::Miss,
        RecruitOutcome::Pickup,
    ] {
        prefix_b = apply_primitive_transition(&bundle, &prefix_b, outcome)
            .expect("prefix B")
            .state;
    }
    assert_eq!(prefix_a, prefix_b);

    let mut frontier = BTreeMap::from([(started, 1.0_f64)]);
    let mut fifth_layer_children = 0_usize;
    for draw in 1..=5 {
        let mut next = BTreeMap::new();
        for (state, mass) in frontier {
            let branches = outcome_distribution(&bundle, &state).expect("branches");
            if draw == 5 {
                fifth_layer_children += branches.len();
            }
            for branch in branches {
                let transitioned = apply_primitive_transition(&bundle, &state, branch.outcome)
                    .expect("transition");
                *next.entry(transitioned.state).or_insert(0.0) +=
                    mass * branch.probability.as_f64();
            }
        }
        frontier = next;
    }
    assert_eq!(fifth_layer_children, 10);
    assert_eq!(frontier.len(), 6);

    let exact = analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact");
    assert!((exact.success_probability - 1023.0 / 1024.0).abs() <= 1.0e-12);
    assert!(
        (exact
            .expected_first_success_recruitment_count_given_success
            .expect("positive success")
            - 2036.0 / 1023.0)
            .abs()
            <= 1.0e-12
    );
    for point in &exact.first_success_pmf {
        let expected = 2_f64.powi(-(point.recruitment_count as i32));
        assert!((point.probability - expected).abs() <= 1.0e-12);
    }

    let mut oracle_successes = 0_u64;
    let mut oracle_first_counts = BTreeMap::<u64, u64>::new();
    for bits in 0_u16..1024 {
        let outcomes = (0..10)
            .map(|index| {
                if bits & (1 << index) == 0 {
                    RecruitOutcome::Miss
                } else {
                    RecruitOutcome::Pickup
                }
            })
            .collect::<Vec<_>>();
        let trace = replay(&bundle, &outcomes).expect("all half-probability paths are valid");
        if let Some(first) = trace.first_success_recruitment_count {
            oracle_successes += 1;
            *oracle_first_counts.entry(first).or_default() += 1;
        }
    }
    assert_eq!(oracle_successes, 1023);
    for point in &exact.first_success_pmf {
        assert_eq!(
            oracle_first_counts.get(&point.recruitment_count).copied(),
            Some(1_u64 << (10 - point.recruitment_count))
        );
    }
    let pmf_total = exact
        .first_success_pmf
        .iter()
        .map(|point| point.probability)
        .sum::<f64>();
    assert!((pmf_total - exact.success_probability).abs() <= 1.0e-12);
}

#[test]
fn exact_guards_fail_without_returning_partial_results() {
    let bundle = synthetic_bundle(
        "guards",
        half_probability_mechanics(),
        Resources {
            limited_ten_recruitment_tickets: 1,
            ..Resources::default()
        },
        0,
        Some(10),
        Vec::new(),
    );
    let active = ExactSolverOptions {
        max_active_states: 1,
        ..ExactSolverOptions::default()
    };
    assert!(matches!(
        analyze_exact(&bundle, active),
        Err(EngineError::SolverStateLimitExceeded { .. })
    ));
    let detailed =
        analyze_exact_detailed(&bundle, active).expect_err("detailed analysis must fail");
    assert!(matches!(
        detailed.error,
        EngineError::SolverStateLimitExceeded { .. }
    ));
    assert_eq!(detailed.effective_options, active);
    assert_eq!(
        detailed.provenance.scenario_id,
        bundle.scenario().id().to_string()
    );
    let processed = ExactSolverOptions {
        max_processed_states: 1,
        ..ExactSolverOptions::default()
    };
    assert!(matches!(
        analyze_exact(&bundle, processed),
        Err(EngineError::SolverProcessedStateLimitExceeded { .. })
    ));
    let transitions = ExactSolverOptions {
        max_transition_expansions: 1,
        ..ExactSolverOptions::default()
    };
    assert!(matches!(
        analyze_exact(&bundle, transitions),
        Err(EngineError::SolverTransitionLimitExceeded { .. })
    ));
    for tolerance in [f64::NAN, f64::INFINITY, 1.0e-16, 1.0e-11] {
        let options = ExactSolverOptions {
            conservation_tolerance: tolerance,
            ..ExactSolverOptions::default()
        };
        assert!(matches!(
            analyze_exact(&bundle, options),
            Err(EngineError::ProbabilityInvariantViolation { .. })
        ));
    }
}

#[test]
fn immutable_bundle_analysis_never_rereads_source_files() {
    let temp = TempDir::new().expect("tempdir");
    let data = temp.path().join("data");
    let scenarios = temp.path().join("scenarios");
    fs::create_dir_all(data.join("rulesets")).expect("rules");
    fs::create_dir_all(data.join("rewards")).expect("rewards");
    fs::create_dir_all(&scenarios).expect("scenarios");
    fs::copy(
        workspace_path("data/rulesets/jp_2026_07_29_provisional_v1.json"),
        data.join("rulesets/rules.json"),
    )
    .expect("copy rules");
    fs::copy(
        workspace_path("data/rewards/empty_v1.json"),
        data.join("rewards/empty.json"),
    )
    .expect("copy rewards");
    let scenario = scenarios.join("scenario.json");
    fs::copy(
        workspace_path("scenarios/golden/charge_99_one.json"),
        &scenario,
    )
    .expect("copy scenario");
    let bundle = load_bundle(&data, &scenario).expect("load once");
    fs::remove_dir_all(&data).expect("remove catalog after load");
    fs::remove_file(&scenario).expect("remove scenario after load");
    let result = analyze_exact(&bundle, ExactSolverOptions::default())
        .expect("analysis should use only immutable bundle");
    assert_eq!(result.success_probability, 0.5);
}

#[test]
fn action_completion_activates_deferred_tickets_only_at_boundary() {
    let bundle = synthetic_bundle(
        "deferred",
        half_probability_mechanics(),
        Resources {
            limited_ten_recruitment_tickets: 1,
            ..Resources::default()
        },
        0,
        Some(20),
        vec![common::ticket_reward(5, 1)],
    );
    let world = initial_world(&bundle);
    let action = match decide(&bundle, &world).expect("decision") {
        StrategyDecision::Act(action) => action,
        other => panic!("expected action, got {other:?}"),
    };
    let (mut in_flight, _) = begin_action(&bundle, &world, &action).expect("start");
    assert_eq!(in_flight.world.available_ticket_count, 0);
    for _ in 0..5 {
        in_flight = apply_primitive_transition(&bundle, &in_flight, RecruitOutcome::Miss)
            .expect("miss")
            .state;
    }
    assert_eq!(in_flight.deferred_ticket_count, 1);
    assert_eq!(in_flight.world.available_ticket_count, 0);
    while in_flight.remaining_primitive_draws > 0 {
        let outcome = outcome_distribution(&bundle, &in_flight)
            .expect("branches")
            .first()
            .expect("branch")
            .outcome;
        in_flight = apply_primitive_transition(&bundle, &in_flight, outcome)
            .expect("transition")
            .state;
    }
    let (boundary, completed) = complete_action(in_flight).expect("complete");
    assert_eq!(completed.tickets_activated, 1);
    assert_eq!(boundary.available_ticket_count, 1);
}
