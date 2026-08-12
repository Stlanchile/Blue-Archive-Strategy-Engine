use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use ba_core::{BundleCompatibilityProfile, load_bundle};
use ba_engine::{
    ExactSolverOptions, analyze_exact, derive_run_seed, replay, simulate_monte_carlo,
    simulate_trace,
};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn write_json(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(value).expect("serialize")).expect("write");
}

fn stage_catalog(root: &Path, altered: bool) {
    fs::create_dir_all(root.join("rulesets")).expect("rulesets");
    fs::create_dir_all(root.join("rewards")).expect("rewards");
    let mut ruleset: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace_path(
            "data/rulesets/jp_2026_07_29_provisional_v2.json",
        ))
        .expect("ruleset"),
    )
    .expect("JSON");
    let mut rewards: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace_path("data/rewards/jp_2026_07_29_empty_v2.json")).expect("rewards"),
    )
    .expect("JSON");
    if altered {
        let source = serde_json::json!([{
            "label": "metamorphic source",
            "reference": "https://example.invalid/metamorphic",
            "retrieved_on": "2026-08-12",
            "content_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }]);
        ruleset["provenance"]["verification_status"] = serde_json::json!("source_backed");
        ruleset["provenance"]["sources"] = source.clone();
        rewards["provenance"]["verification_status"] = serde_json::json!("verified");
        rewards["provenance"]["sources"] = source;
    }
    write_json(&root.join("rulesets/rules.json"), &ruleset);
    write_json(&root.join("rewards/rewards.json"), &rewards);
}

fn strip_document_identity(value: &mut serde_json::Value) {
    let provenance = value
        .get_mut("provenance")
        .and_then(serde_json::Value::as_object_mut)
        .expect("result provenance");
    for key in [
        "ruleset_document_fingerprint",
        "ruleset_verification_status",
        "ruleset_provenance",
        "reward_schedule_document_fingerprint",
        "reward_schedule_verification_status",
        "reward_schedule_provenance",
    ] {
        provenance.remove(key);
    }
}

#[test]
fn v1_projection_remains_exactly_v1_while_v2_is_selected_only_by_scenario() {
    let v1 = load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/charge_99_one.json"),
    )
    .expect("v1");
    assert_eq!(v1.profile(), BundleCompatibilityProfile::V1);
    let value =
        serde_json::to_value(analyze_exact(&v1, ExactSolverOptions::default()).expect("v1 exact"))
            .expect("serialize");
    assert_eq!(value["provenance"]["engine_semantics_version"], 1);
    assert_eq!(value["provenance"]["result_schema_version"], 1);
    assert!(
        value["provenance"]
            .get("ruleset_document_fingerprint")
            .is_none()
    );

    let v2_with_v1 = {
        let temp = TempDir::new().expect("tempdir");
        let mut scenario: serde_json::Value = serde_json::from_slice(
            &fs::read(workspace_path("scenarios/examples/single_target_v2.json"))
                .expect("scenario"),
        )
        .expect("JSON");
        scenario["ruleset_id"] = serde_json::json!("jp_2026_07_29_provisional_v1");
        scenario["reward_schedule_id"] = serde_json::json!("empty_v1");
        let path = temp.path().join("scenario.json");
        write_json(&path, &scenario);
        load_bundle(workspace_path("data"), path).expect("v2 with v1 references")
    };
    let value = serde_json::to_value(
        analyze_exact(&v2_with_v1, ExactSolverOptions::default()).expect("v2 exact"),
    )
    .expect("serialize");
    assert_eq!(value["provenance"]["engine_semantics_version"], 2);
    assert_eq!(value["provenance"]["result_schema_version"], 2);
    assert_eq!(
        value["provenance"]["ruleset_behavior_fingerprint"],
        value["provenance"]["ruleset_document_fingerprint"]
    );
    assert!(value["provenance"]["ruleset_verification_status"].is_null());
}

#[test]
fn provenance_only_mutation_changes_identity_but_not_any_execution_behavior() {
    let first = TempDir::new().expect("first");
    let second = TempDir::new().expect("second");
    stage_catalog(first.path(), false);
    stage_catalog(second.path(), true);
    let scenario = workspace_path("scenarios/examples/single_target_v2.json");
    let first_bundle = load_bundle(first.path(), &scenario).expect("first bundle");
    let second_bundle = load_bundle(second.path(), &scenario).expect("second bundle");

    assert_eq!(
        first_bundle.fingerprints().ruleset,
        second_bundle.fingerprints().ruleset
    );
    assert_eq!(
        first_bundle.fingerprints().reward_schedule,
        second_bundle.fingerprints().reward_schedule
    );
    assert_ne!(
        first_bundle.fingerprints().ruleset_document,
        second_bundle.fingerprints().ruleset_document
    );
    assert_ne!(
        first_bundle.fingerprints().reward_schedule_document,
        second_bundle.fingerprints().reward_schedule_document
    );
    for run_index in 0..16 {
        assert_eq!(
            derive_run_seed(&first_bundle, 42, run_index),
            derive_run_seed(&second_bundle, 42, run_index)
        );
    }

    let mut first_exact = serde_json::to_value(
        analyze_exact(&first_bundle, ExactSolverOptions::default()).expect("first exact"),
    )
    .expect("serialize");
    let mut second_exact = serde_json::to_value(
        analyze_exact(&second_bundle, ExactSolverOptions::default()).expect("second exact"),
    )
    .expect("serialize");
    assert_ne!(first_exact, second_exact);
    strip_document_identity(&mut first_exact);
    strip_document_identity(&mut second_exact);
    assert_eq!(first_exact, second_exact);

    let runs = NonZeroU64::new(64).expect("runs");
    let mut first_mc =
        serde_json::to_value(simulate_monte_carlo(&first_bundle, runs, 42).expect("first MC"))
            .expect("serialize");
    let mut second_mc =
        serde_json::to_value(simulate_monte_carlo(&second_bundle, runs, 42).expect("second MC"))
            .expect("serialize");
    strip_document_identity(&mut first_mc);
    strip_document_identity(&mut second_mc);
    assert_eq!(first_mc, second_mc);

    let first_trace = simulate_trace(&first_bundle, 42).expect("first trace");
    let second_trace = simulate_trace(&second_bundle, 42).expect("second trace");
    assert_eq!(
        serde_json::to_value(&first_trace.events).expect("events"),
        serde_json::to_value(&second_trace.events).expect("events")
    );
    assert_eq!(first_trace.replay_outcomes, second_trace.replay_outcomes);
    assert_eq!(
        first_trace.terminal_primitive_recruitments,
        second_trace.terminal_primitive_recruitments
    );
    assert_eq!(
        first_trace.terminal_resources,
        second_trace.terminal_resources
    );
    assert_eq!(
        first_trace.milestone_rewards_acquired,
        second_trace.milestone_rewards_acquired
    );
    assert_eq!(
        first_trace.terminal_owned_targets,
        second_trace.terminal_owned_targets
    );
    assert_eq!(first_trace.terminal_reason, second_trace.terminal_reason);
    let first_replay = replay(&first_bundle, &first_trace.replay_outcomes).expect("first replay");
    let second_replay =
        replay(&second_bundle, &second_trace.replay_outcomes).expect("second replay");
    assert_eq!(
        serde_json::to_value(&first_replay.events).expect("events"),
        serde_json::to_value(&second_replay.events).expect("events")
    );
    assert_eq!(
        first_replay.terminal_resources,
        second_replay.terminal_resources
    );
    assert_eq!(first_replay.terminal_reason, second_replay.terminal_reason);
}

#[test]
fn scenario_identity_only_mutation_preserves_behavior_fingerprint_and_streams() {
    let temp = TempDir::new().expect("tempdir");
    let original: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace_path("scenarios/examples/single_target_v2.json")).expect("scenario"),
    )
    .expect("JSON");
    let mut renamed = original.clone();
    renamed["scenario_id"] = serde_json::json!("renamed_scenario");
    renamed["strategy"]["strategy_id"] = serde_json::json!("renamed_strategy");
    renamed["ruleset_id"] = serde_json::json!("renamed_ruleset");
    renamed["reward_schedule_id"] = serde_json::json!("renamed_rewards");

    let renamed_data = temp.path().join("data");
    fs::create_dir_all(renamed_data.join("rulesets")).expect("rulesets");
    fs::create_dir_all(renamed_data.join("rewards")).expect("rewards");
    let mut ruleset: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace_path(
            "data/rulesets/jp_2026_07_29_provisional_v2.json",
        ))
        .expect("ruleset"),
    )
    .expect("ruleset JSON");
    ruleset["ruleset_id"] = serde_json::json!("renamed_ruleset");
    let mut rewards: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace_path("data/rewards/jp_2026_07_29_empty_v2.json")).expect("rewards"),
    )
    .expect("rewards JSON");
    rewards["reward_schedule_id"] = serde_json::json!("renamed_rewards");
    rewards["compatible_ruleset_ids"] = serde_json::json!(["renamed_ruleset"]);
    write_json(&renamed_data.join("rulesets/rules.json"), &ruleset);
    write_json(&renamed_data.join("rewards/rewards.json"), &rewards);

    let original_path = temp.path().join("original.json");
    let renamed_path = temp.path().join("renamed.json");
    write_json(&original_path, &original);
    write_json(&renamed_path, &renamed);

    let original_bundle = load_bundle(workspace_path("data"), original_path).expect("original");
    let renamed_bundle = load_bundle(&renamed_data, renamed_path).expect("renamed");
    assert_eq!(
        original_bundle.fingerprints().scenario,
        renamed_bundle.fingerprints().scenario
    );
    assert_ne!(
        original_bundle.fingerprints().scenario_document,
        renamed_bundle.fingerprints().scenario_document
    );
    assert_eq!(
        original_bundle.fingerprints().ruleset,
        renamed_bundle.fingerprints().ruleset
    );
    assert_ne!(
        original_bundle.fingerprints().ruleset_document,
        renamed_bundle.fingerprints().ruleset_document
    );
    assert_eq!(
        original_bundle.fingerprints().reward_schedule,
        renamed_bundle.fingerprints().reward_schedule
    );
    assert_ne!(
        original_bundle.fingerprints().reward_schedule_document,
        renamed_bundle.fingerprints().reward_schedule_document
    );
    for run_index in 0..16 {
        assert_eq!(
            derive_run_seed(&original_bundle, 42, run_index),
            derive_run_seed(&renamed_bundle, 42, run_index)
        );
    }
}

#[test]
fn scenario_label_alpha_renames_preserve_normalized_behavior_topology() {
    let temp = TempDir::new().expect("tempdir");
    let mut original: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace_path(
            "scenarios/examples/dual_target_ticket_first_v2.json",
        ))
        .expect("scenario"),
    )
    .expect("JSON");
    original["initial_charges"][0]["pre_recruitment_charge"] = serde_json::json!(3);
    original["initial_charges"][1]["pre_recruitment_charge"] = serde_json::json!(7);
    let mut renamed = original.clone();
    renamed["students"][0]["student_id"] = serde_json::json!("z_student");
    renamed["students"][1]["student_id"] = serde_json::json!("a_student");
    renamed["banners"][0]["banner_id"] = serde_json::json!("z_banner");
    renamed["banners"][0]["featured_student_id"] = serde_json::json!("z_student");
    renamed["banners"][0]["charge_group_id"] = serde_json::json!("z_group");
    renamed["banners"][1]["banner_id"] = serde_json::json!("a_banner");
    renamed["banners"][1]["featured_student_id"] = serde_json::json!("a_student");
    renamed["banners"][1]["charge_group_id"] = serde_json::json!("a_group");
    renamed["initial_charges"][0]["charge_group_id"] = serde_json::json!("z_group");
    renamed["initial_charges"][1]["charge_group_id"] = serde_json::json!("a_group");
    renamed["targets"][0]["student_id"] = serde_json::json!("z_student");
    renamed["targets"][0]["banner_id"] = serde_json::json!("z_banner");
    renamed["targets"][1]["student_id"] = serde_json::json!("a_student");
    renamed["targets"][1]["banner_id"] = serde_json::json!("a_banner");
    let original_path = temp.path().join("topology-original.json");
    let renamed_path = temp.path().join("topology-renamed.json");
    write_json(&original_path, &original);
    write_json(&renamed_path, &renamed);

    let original_bundle = load_bundle(workspace_path("data"), original_path).expect("original");
    let renamed_bundle = load_bundle(workspace_path("data"), renamed_path).expect("renamed");
    assert_eq!(
        original_bundle.fingerprints().scenario,
        renamed_bundle.fingerprints().scenario
    );
    assert_ne!(
        original_bundle.fingerprints().scenario_document,
        renamed_bundle.fingerprints().scenario_document
    );
    for run_index in 0..16 {
        assert_eq!(
            derive_run_seed(&original_bundle, 42, run_index),
            derive_run_seed(&renamed_bundle, 42, run_index)
        );
    }
}

#[test]
fn behavior_mutation_changes_behavior_fingerprint_seed_and_reached_execution() {
    let first = TempDir::new().expect("first");
    let second = TempDir::new().expect("second");
    stage_catalog(first.path(), false);
    stage_catalog(second.path(), false);
    let rules_path = second.path().join("rulesets/rules.json");
    let mut rules: serde_json::Value =
        serde_json::from_slice(&fs::read(&rules_path).expect("rules")).expect("JSON");
    rules["ordinary_pickup_probability"] = serde_json::json!({"numerator": 1, "denominator": 2});
    write_json(&rules_path, &rules);

    let scenario = workspace_path("scenarios/examples/single_target_v2.json");
    let first_bundle = load_bundle(first.path(), &scenario).expect("first bundle");
    let second_bundle = load_bundle(second.path(), &scenario).expect("second bundle");
    assert_ne!(
        first_bundle.fingerprints().ruleset,
        second_bundle.fingerprints().ruleset
    );
    assert_ne!(
        derive_run_seed(&first_bundle, 42, 0),
        derive_run_seed(&second_bundle, 42, 0)
    );
    let first_exact =
        analyze_exact(&first_bundle, ExactSolverOptions::default()).expect("first exact");
    let second_exact =
        analyze_exact(&second_bundle, ExactSolverOptions::default()).expect("second exact");
    assert_ne!(
        first_exact.success_probability,
        second_exact.success_probability
    );
}
