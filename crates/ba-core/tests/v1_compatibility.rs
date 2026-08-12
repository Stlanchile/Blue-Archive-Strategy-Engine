use std::fs;
use std::path::{Path, PathBuf};

use ba_core::schema::RawRulesetV1;
use ba_core::{CompiledRuleset, ProbabilityRatio, RulesetId, RulesetMechanics};
use sha2::{Digest, Sha256};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn raw_ruleset() -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(workspace_path(
            "data/rulesets/jp_2026_07_29_provisional_v1.json",
        ))
        .expect("shipped ruleset"),
    )
    .expect("ruleset JSON")
}

fn compile(value: serde_json::Value) -> String {
    let raw: RawRulesetV1 = serde_json::from_value(value).expect("typed raw ruleset");
    CompiledRuleset::from_raw_provisional(raw, None)
        .expect_err("multiply invalid ruleset must fail")
        .to_string()
}

#[test]
fn all_frozen_v1_sources_keep_their_raw_sha256() {
    let vectors = [
        (
            "data/rulesets/jp_2026_07_29_provisional_v1.json",
            "71e28f0b082cd8aab8ac42cc4ecd7cd1ec8fc72a006901c53d43c329c7c22c0e",
        ),
        (
            "data/rewards/empty_v1.json",
            "2f00ac378cc2d34e0c8bd0358afbd2827a29ceb2e5fa928bf1dc770783d84cf7",
        ),
        (
            "data/rewards/jp_2026_07_29_campaign_v1.json",
            "859d23812f8567bdcefee5497226f9f5c2e10581b91956f8e2baa7c173b3ad48",
        ),
        (
            "scenarios/golden/campaign_dual_310.json",
            "8a9b5daf320f73344ec00b22a0ff299111f4d50d2d12682dcbf17cae9c9e8777",
        ),
        (
            "scenarios/golden/charge_199_one.json",
            "fd405d31e517e1bf465ce3626a76a9721a7528092b061fab3658d94f8d3a229f",
        ),
        (
            "scenarios/golden/charge_99_one.json",
            "3616e040407c8bb36bce9a4b2f58741d64f5bc387848ae9cf6bda74b81defb68",
        ),
        (
            "scenarios/golden/dual_independent_200.json",
            "f8a6d954347bf98ba5254be40f02aa6cc98b508880c705984800f249507daca8",
        ),
        (
            "scenarios/golden/dual_shared_200.json",
            "325193383a78b6f085b7432a0fceb599076c48ad3ab52067ac09a0714f6fa45c",
        ),
        (
            "scenarios/golden/initial_success.json",
            "f34a85e1b63c5453caffbe3bc4299b328206f73ec622cb2b013e2685b83dd079",
        ),
        (
            "scenarios/golden/single_target_200.json",
            "61a98ae4860103719f57cec19dd4982059fe70a480199254f9f681ce03e35461",
        ),
        (
            "scenarios/golden/ticket_atomic.json",
            "8876cc9c21cb641198ee25161d0d08dfab03b530293a7b9c3495d0d45ffaf997",
        ),
    ];
    for (path, expected) in vectors {
        let digest = Sha256::digest(fs::read(workspace_path(path)).expect("frozen fixture"));
        assert_eq!(format!("{digest:x}"), expected, "{path}");
    }
}

#[test]
fn v1_ruleset_conversion_and_authority_precedence_is_frozen() {
    let mut invalid_id = raw_ruleset();
    invalid_id["ruleset_id"] = serde_json::json!("-invalid");
    invalid_id["ordinary_pickup_probability"]["denominator"] = serde_json::json!(0);
    assert!(compile(invalid_id).contains("invalid ruleset identifier"));

    let mut ordinary_first = raw_ruleset();
    ordinary_first["ordinary_pickup_probability"]["numerator"] = serde_json::json!(2);
    ordinary_first["ordinary_pickup_probability"]["denominator"] = serde_json::json!(1);
    ordinary_first["threshold_overrides"][0]["pickup_probability"]["denominator"] =
        serde_json::json!(0);
    assert!(compile(ordinary_first).contains("numerator 2 exceeds denominator 1"));

    let mut threshold_order = raw_ruleset();
    threshold_order["threshold_overrides"][0]["pickup_probability"]["numerator"] =
        serde_json::json!(2);
    threshold_order["threshold_overrides"][0]["pickup_probability"]["denominator"] =
        serde_json::json!(1);
    threshold_order["threshold_overrides"][1]["pickup_probability"]["denominator"] =
        serde_json::json!(0);
    assert!(compile(threshold_order).contains("numerator 2 exceeds denominator 1"));

    let mut authority_before_cost = raw_ruleset();
    authority_before_cost["paid_single_cost"] = serde_json::json!(0);
    assert!(compile(authority_before_cost).contains("schema-v1 rulesets must exactly implement"));

    let mut authority_before_relation = raw_ruleset();
    authority_before_relation["maximum_pre_recruitment_charge"] = serde_json::json!(0);
    authority_before_relation["hit_reset_charge"] = serde_json::json!(1);
    assert!(
        compile(authority_before_relation).contains("schema-v1 rulesets must exactly implement")
    );
}

#[test]
fn programmatic_from_parts_retains_generic_validation_order() {
    let mechanics = RulesetMechanics {
        paid_single_cost: 0,
        paid_single_action_size: 0,
        ticket_action_size: 0,
        ordinary_pickup_probability: ProbabilityRatio::new(1, 2).expect("ratio"),
        maximum_pre_recruitment_charge: 0,
        hit_reset_charge: 1,
        miss_increment: 0,
        threshold_overrides: Vec::new(),
    };
    let error =
        CompiledRuleset::from_parts(RulesetId::new("generic_order").expect("ID"), mechanics)
            .expect_err("invalid mechanics");
    assert!(
        error
            .to_string()
            .contains("paid_single_cost must be positive")
    );
}
