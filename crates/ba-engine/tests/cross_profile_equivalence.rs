use std::fs;
use std::path::{Path, PathBuf};

use ba_core::schema::{RawRewardScheduleV3, RawRulesetV3, RawScenarioV3};
use ba_core::{
    AnyValidatedScenarioBundle, CompiledRulesetV3, RewardScheduleV3, ValidatedScenarioBundleV3,
    load_any_bundle, load_bundle,
};
use ba_engine::{ExactSolverOptions, analyze_exact, analyze_exact_v3};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn reward_prefix_v3_bundle() -> ValidatedScenarioBundleV3 {
    let raw_ruleset: RawRulesetV3 = serde_json::from_value(serde_json::json!({
        "schema_version": 3,
        "document_type": "ruleset",
        "ruleset_id": "equivalent_rules_v3",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "paid_single_cost": 120,
        "paid_single_action_size": 1,
        "ticket_action_size": 10,
        "ordinary_featured_target_probability": {
            "numerator": 7,
            "denominator": 1000
        },
        "maximum_pre_recruitment_charge": 199,
        "featured_hit_reset_charge": 0,
        "non_featured_increment": 1,
        "threshold_overrides": [
            {
                "pre_charge": 99,
                "featured_target_probability": {
                    "numerator": 1,
                    "denominator": 2
                }
            },
            {
                "pre_charge": 199,
                "featured_target_probability": {
                    "numerator": 1,
                    "denominator": 1
                }
            }
        ]
    }))
    .expect("v3 rules");
    let rules = CompiledRulesetV3::from_raw(raw_ruleset, None).expect("compiled rules");

    let source = fs::read_to_string(workspace_path(
        "data/rewards/jp_2026_07_29_campaign_v2.json",
    ))
    .expect("v2 rewards");
    let source: serde_json::Value = serde_json::from_str(&source).expect("reward JSON");
    let raw_rewards: RawRewardScheduleV3 = serde_json::from_value(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "equivalent_rewards_v3",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["equivalent_rules_v3"],
        "initial_milestones": source["milestones"].clone(),
        "repeating_cycle": null
    }))
    .expect("v3 rewards");
    let rewards = RewardScheduleV3::from_raw(raw_rewards, None).expect("compiled rewards");

    let source = fs::read_to_string(workspace_path("scenarios/golden/campaign_dual_310.json"))
        .expect("v2 scenario");
    let source: serde_json::Value = serde_json::from_str(&source).expect("scenario JSON");
    let mut resources = source["initial_resources"].clone();
    for field in [
        "keystone_fragments",
        "secret_tech_notes",
        "superior_bd_selectors",
        "high_grade_gift_boxes",
    ] {
        resources[field] = serde_json::json!(0);
    }
    let other = |target: &str| serde_json::json!([{"target_id": target, "weight": 0}]);
    let raw_scenario: RawScenarioV3 = serde_json::from_value(serde_json::json!({
        "schema_version": 3,
        "document_type": "scenario",
        "scenario_id": "equivalent_campaign_v3",
        "ruleset_id": "equivalent_rules_v3",
        "reward_schedule_id": "equivalent_rewards_v3",
        "authority": {
            "scenario": "user_authored",
            "banner_topology": "user_authored",
            "target_order": "user_authored",
            "initial_state": "user_authored",
            "cross_target_probabilities": "user_authored",
            "strategy": "user_authored"
        },
        "initial_recruitment_count": 0,
        "students": source["students"].clone(),
        "banners": source["banners"].clone(),
        "initial_charges": source["initial_charges"].clone(),
        "initial_resources": resources,
        "initial_owned_targets": source["initial_owned_targets"].clone(),
        "targets": source["targets"].clone(),
        "cross_target_probability_tables": [
            {
                "banner_id": "banner_a",
                "ordinary": {
                    "denominator": 1000,
                    "other_target_weights": other("target_b")
                },
                "threshold_overrides": [
                    {
                        "pre_charge": 99,
                        "denominator": 2,
                        "other_target_weights": other("target_b")
                    },
                    {
                        "pre_charge": 199,
                        "denominator": 1,
                        "other_target_weights": other("target_b")
                    }
                ]
            },
            {
                "banner_id": "banner_b",
                "ordinary": {
                    "denominator": 1000,
                    "other_target_weights": other("target_a")
                },
                "threshold_overrides": [
                    {
                        "pre_charge": 99,
                        "denominator": 2,
                        "other_target_weights": other("target_a")
                    },
                    {
                        "pre_charge": 199,
                        "denominator": 1,
                        "other_target_weights": other("target_a")
                    }
                ]
            }
        ],
        "strategy": {
            "strategy_schema_version": 2,
            "strategy_id": "sequential_v3",
            "kind": "sequential_targets",
            "funding_priority": ["ticket_ten", "paid_single"],
            "max_additional_recruitments": 310
        }
    }))
    .expect("v3 scenario");
    ValidatedScenarioBundleV3::from_programmatic(raw_scenario, rules, rewards).expect("v3 bundle")
}

#[test]
fn empty_reward_single_target_profiles_match_on_common_exact_surface() {
    let v2 = load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/single_target_200.json"),
    )
    .expect("v2");
    let v3 = match load_any_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/v3_single_cross_target_zero.json"),
    )
    .expect("v3")
    {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    };
    let v2 = analyze_exact(&v2, ExactSolverOptions::default()).expect("v2 exact");
    let v3 = analyze_exact_v3(&v3, ExactSolverOptions::default()).expect("v3 exact");
    assert!((v2.success_probability - v3.all_target_success_probability).abs() <= 1e-12);
    assert!(
        (v2.expected_terminal_primitive_recruitments
            - v3.expected_additional_primitive_recruitments)
            .abs()
            <= 1e-12
    );
    assert!((v2.expected_paid_pyroxene_spent - v3.expected_paid_pyroxene_spent).abs() <= 1e-12);
    assert_eq!(
        v2.owned_target_terminal_probabilities.len(),
        v3.terminal_owned_set_probabilities.len()
    );
    assert!(v2.milestone_reach_probabilities.is_empty());
    assert!(
        v3.absolute_campaign_milestone_reach_probabilities
            .is_empty()
    );
}

#[test]
fn finite_reward_prefix_through_310_matches_on_common_exact_surface() {
    let v2 = load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/campaign_dual_310.json"),
    )
    .expect("v2");
    let v2 = analyze_exact(&v2, ExactSolverOptions::default()).expect("v2 exact");
    let v3 = analyze_exact_v3(&reward_prefix_v3_bundle(), ExactSolverOptions::default())
        .expect("v3 exact");
    for (left, right) in [
        (v2.success_probability, v3.all_target_success_probability),
        (
            v2.expected_terminal_primitive_recruitments,
            v3.expected_additional_primitive_recruitments,
        ),
        (
            v2.expected_paid_pyroxene_spent,
            v3.expected_paid_pyroxene_spent,
        ),
        (
            v2.expected_ticket_funded_primitive_recruitments,
            v3.expected_ticket_funded_primitive_recruitments,
        ),
        (
            v2.expected_milestone_rewards_acquired.eligma,
            v3.expected_milestone_rewards_acquired.eligma,
        ),
        (
            v2.expected_milestone_rewards_acquired
                .limited_ten_recruitment_tickets,
            v3.expected_milestone_rewards_acquired
                .limited_ten_recruitment_tickets,
        ),
    ] {
        assert!((left - right).abs() <= 1e-12, "{left} != {right}");
    }
    assert_eq!(
        v2.milestone_reach_probabilities.len(),
        v3.absolute_campaign_milestone_reach_probabilities.len()
    );
}
