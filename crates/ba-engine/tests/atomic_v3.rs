use std::fs;
use std::path::{Path, PathBuf};

use ba_core::{
    AnyValidatedScenarioBundle, PrimitiveAcquisition, TargetIndex, ValidatedScenarioBundleV3,
    load_any_bundle,
};
use ba_engine::{RunTraceEventV3, replay_v3};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn bundle() -> ValidatedScenarioBundleV3 {
    match load_any_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/v3_atomic_cross_target.json"),
    )
    .expect("bundle")
    {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3 bundle"),
    }
}

fn campaign_bundle(
    initial_count: u64,
    pyroxene: u64,
    tickets: u64,
    horizon: u64,
) -> ValidatedScenarioBundleV3 {
    let temporary = TempDir::new().expect("tempdir");
    fs::create_dir(temporary.path().join("rulesets")).expect("rulesets");
    fs::create_dir(temporary.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("data/rulesets/jp_2026_07_29_provisional_v3.json"),
        temporary.path().join("rulesets/rules.json"),
    )
    .expect("rules");
    fs::write(
        temporary.path().join("rewards/rewards.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 3,
            "document_type": "reward_schedule",
            "reward_schedule_id": "campaign_test_v3",
            "provenance": {
                "provenance_status": "provisional",
                "sources": [],
                "claim_bindings": []
            },
            "compatible_ruleset_ids": ["jp_2026_07_29_provisional_v3"],
            "initial_milestones": [{
                "count": 390,
                "rewards": [
                    {"resource": "eligma", "quantity": 3},
                    {"resource": "limited_ten_recruitment_tickets", "quantity": 1}
                ]
            }],
            "repeating_cycle": {
                "starts_after_count": 390,
                "period": 200,
                "milestones": [{
                    "offset": 20,
                    "rewards": [{"resource": "gift_boxes", "quantity": 2}]
                }]
            }
        }))
        .expect("render rewards"),
    )
    .expect("rewards");
    let scenario = temporary.path().join("scenario.json");
    fs::write(
        &scenario,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 3,
            "document_type": "scenario",
            "scenario_id": "campaign_test_v3",
            "ruleset_id": "jp_2026_07_29_provisional_v3",
            "reward_schedule_id": "campaign_test_v3",
            "authority": {
                "scenario": "user_authored",
                "banner_topology": "user_authored",
                "target_order": "user_authored",
                "initial_state": "user_authored",
                "cross_target_probabilities": "user_authored",
                "strategy": "user_authored"
            },
            "initial_recruitment_count": initial_count,
            "students": [{"student_id": "target_a"}],
            "banners": [{
                "banner_id": "banner_a",
                "featured_student_id": "target_a",
                "charge_group_id": "shared"
            }],
            "initial_charges": [{
                "charge_group_id": "shared",
                "pre_recruitment_charge": 199
            }],
            "initial_resources": {
                "pyroxene": pyroxene,
                "limited_ten_recruitment_tickets": tickets,
                "eligma": 0,
                "advanced_bd_selectors": 0,
                "advanced_tech_note_selectors": 0,
                "superior_tech_note_selectors": 0,
                "gift_boxes": 0,
                "keystone_fragments": 0,
                "secret_tech_notes": 0,
                "superior_bd_selectors": 0,
                "high_grade_gift_boxes": 0
            },
            "initial_owned_targets": [],
            "targets": [{"student_id": "target_a", "banner_id": "banner_a"}],
            "cross_target_probability_tables": [{
                "banner_id": "banner_a",
                "ordinary": {"denominator": 1000, "other_target_weights": []},
                "threshold_overrides": [
                    {"pre_charge": 99, "denominator": 2, "other_target_weights": []},
                    {"pre_charge": 199, "denominator": 1, "other_target_weights": []}
                ]
            }],
            "strategy": {
                "strategy_schema_version": 2,
                "strategy_id": "sequential_targets_v3",
                "kind": "sequential_targets",
                "funding_priority": ["ticket_ten", "paid_single"],
                "max_additional_recruitments": horizon
            }
        }))
        .expect("render scenario"),
    )
    .expect("scenario");
    match load_any_bundle(temporary.path(), scenario).expect("campaign bundle") {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    }
}

#[test]
fn cross_target_acquisition_does_not_reset_current_charge_and_action_remains_atomic() {
    let other = TargetIndex::new(1, 2).expect("target B");
    let mut outcomes = vec![
        PrimitiveAcquisition::OtherConfiguredTarget {
            target_index: other,
        },
        PrimitiveAcquisition::CurrentFeaturedTarget,
    ];
    outcomes.extend(std::iter::repeat_n(
        PrimitiveAcquisition::NoConfiguredTarget,
        8,
    ));
    let result = replay_v3(&bundle(), &outcomes).expect("replay");
    assert_eq!(result.terminal_additional_primitive_recruitments, 10);
    assert_eq!(result.first_all_target_completion_additional_count, Some(2));
    assert_eq!(result.terminal_owned_targets.len(), 2);
    assert_eq!(result.replay_outcomes, outcomes);

    let transitions = result
        .events
        .iter()
        .filter_map(|event| match event {
            RunTraceEventV3::PrimitiveTransition(event) => Some(event),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(transitions.len(), 10);
    assert_eq!(transitions[0].pre_charge, 99);
    assert_eq!(transitions[0].post_charge, 100);
    assert_eq!(
        transitions[0]
            .acquired_target_id
            .as_ref()
            .map(ToString::to_string),
        Some("target_b".to_owned())
    );
    assert!(!transitions[0].first_all_targets_completed);
    assert_eq!(transitions[1].pre_charge, 100);
    assert_eq!(transitions[1].post_charge, 0);
    assert!(transitions[1].first_all_targets_completed);
    assert!(
        transitions[2..]
            .iter()
            .all(|event| { matches!(event.outcome, PrimitiveAcquisition::NoConfiguredTarget) })
    );
    assert!(matches!(
        result.events[result.events.len() - 2],
        RunTraceEventV3::ActionCompleted(_)
    ));
    assert!(matches!(
        result.events.last(),
        Some(RunTraceEventV3::Terminal { .. })
    ));
}

#[test]
fn replay_rejects_current_target_encoded_as_other_and_extra_outcomes() {
    let current_as_other = PrimitiveAcquisition::OtherConfiguredTarget {
        target_index: TargetIndex::new(0, 2).expect("target A"),
    };
    assert!(replay_v3(&bundle(), &[current_as_other]).is_err());

    let deterministic = match load_any_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/v3_three_target_exact_small.json"),
    )
    .expect("bundle")
    {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    };
    assert!(
        replay_v3(
            &deterministic,
            &[
                PrimitiveAcquisition::CurrentFeaturedTarget,
                PrimitiveAcquisition::CurrentFeaturedTarget,
                PrimitiveAcquisition::CurrentFeaturedTarget,
                PrimitiveAcquisition::CurrentFeaturedTarget,
            ],
        )
        .is_err()
    );
}

#[test]
fn mid_period_rewards_are_interval_scoped_and_tickets_activate_at_action_completion() {
    let bundle = campaign_bundle(385, 0, 1, 10);
    let mut outcomes = vec![PrimitiveAcquisition::CurrentFeaturedTarget];
    outcomes.extend(std::iter::repeat_n(
        PrimitiveAcquisition::NoConfiguredTarget,
        9,
    ));
    let result = replay_v3(&bundle, &outcomes).expect("replay");
    assert_eq!(result.first_all_target_completion_additional_count, Some(1));
    assert_eq!(result.terminal_additional_primitive_recruitments, 10);
    assert_eq!(result.terminal_absolute_campaign_recruitment_count, 395);
    assert_eq!(result.milestone_rewards_acquired.eligma, 3);
    assert_eq!(
        result
            .milestone_rewards_acquired
            .limited_ten_recruitment_tickets,
        1
    );
    assert_eq!(result.terminal_resources.limited_ten_recruitment_tickets, 1);
    assert!(result.events.iter().any(|event| {
        matches!(
            event,
            RunTraceEventV3::RewardGranted {
                absolute_campaign_recruitment_count: 390,
                ..
            }
        )
    }));
}

#[test]
fn repeat_boundary_410_uses_absolute_campaign_count() {
    let bundle = campaign_bundle(409, 120, 0, 1);
    let result =
        replay_v3(&bundle, &[PrimitiveAcquisition::CurrentFeaturedTarget]).expect("replay");
    assert_eq!(result.terminal_additional_primitive_recruitments, 1);
    assert_eq!(result.terminal_absolute_campaign_recruitment_count, 410);
    assert_eq!(result.milestone_rewards_acquired.gift_boxes, 2);
    assert_eq!(result.milestone_rewards_acquired.eligma, 0);
}
