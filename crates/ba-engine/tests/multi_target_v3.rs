use std::fs;
use std::path::{Path, PathBuf};

use ba_core::{AnyValidatedScenarioBundle, ValidatedScenarioBundleV3, load_any_bundle};
use ba_engine::{ExactSolverOptions, analyze_exact_v3};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn bundle(scenario: &str) -> ValidatedScenarioBundleV3 {
    match load_any_bundle(
        workspace_path("data"),
        workspace_path(&format!("scenarios/golden/{scenario}.json")),
    )
    .expect("bundle")
    {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3 bundle"),
    }
}

#[test]
fn exact_supports_small_three_and_four_target_scenarios() {
    for (scenario, targets, expected_count) in [
        ("v3_three_target_exact_small", 3, 3.0),
        ("v3_four_target_exact_small", 4, 4.0),
    ] {
        let result =
            analyze_exact_v3(&bundle(scenario), ExactSolverOptions::default()).expect("exact");
        assert_eq!(result.all_target_success_probability, 1.0);
        assert_eq!(result.per_target_acquisition_probabilities.len(), targets);
        assert_eq!(
            result.ordered_prefix_completion_probabilities.len(),
            targets
        );
        assert_eq!(
            result
                .ordered_prefix_completion_probabilities
                .last()
                .map(|metric| metric.probability),
            Some(result.all_target_success_probability)
        );
        assert_eq!(
            result.expected_additional_primitive_recruitments,
            expected_count
        );
    }
}

#[test]
fn initial_campaign_progress_does_not_change_additional_completion_coordinates() {
    let result = analyze_exact_v3(
        &bundle("v3_four_target_exact_small"),
        ExactSolverOptions::default(),
    )
    .expect("exact");
    assert_eq!(result.context.initial_campaign_recruitment_count, 385);
    assert_eq!(
        result.context.maximum_absolute_campaign_recruitment_count,
        389
    );
    assert_eq!(
        result.first_all_target_completion_pmf[0].additional_recruitment_count,
        4
    );
}

#[test]
fn non_prefix_initial_ownership_is_skipped_in_sequential_order() {
    let source = fs::read_to_string(workspace_path(
        "scenarios/golden/v3_four_target_exact_small.json",
    ))
    .expect("scenario");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("JSON");
    value["scenario_id"] = serde_json::json!("v3_non_prefix_initial");
    value["initial_owned_targets"] = serde_json::json!(["target_b", "target_d"]);
    value["initial_resources"]["pyroxene"] = serde_json::json!(240);
    value["strategy"]["max_additional_recruitments"] = serde_json::json!(2);
    let temporary = TempDir::new().expect("tempdir");
    let path = temporary.path().join("scenario.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).expect("render")).expect("scenario");
    let bundle = match load_any_bundle(workspace_path("data"), path).expect("bundle") {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    };
    let result = analyze_exact_v3(&bundle, ExactSolverOptions::default()).expect("exact");
    assert_eq!(result.all_target_success_probability, 1.0);
    assert_eq!(result.expected_additional_primitive_recruitments, 2.0);
    assert_eq!(
        result
            .per_target_acquisition_probabilities
            .iter()
            .map(|metric| metric.probability)
            .collect::<Vec<_>>(),
        vec![1.0, 1.0, 1.0, 1.0]
    );
}

#[test]
fn initially_complete_scenario_has_completion_mass_at_additional_zero() {
    let source = fs::read_to_string(workspace_path(
        "scenarios/golden/v3_four_target_exact_small.json",
    ))
    .expect("scenario");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("JSON");
    value["scenario_id"] = serde_json::json!("v3_initially_complete");
    value["initial_owned_targets"] =
        serde_json::json!(["target_a", "target_b", "target_c", "target_d"]);
    let temporary = TempDir::new().expect("tempdir");
    let path = temporary.path().join("scenario.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).expect("render")).expect("scenario");
    let bundle = match load_any_bundle(workspace_path("data"), path).expect("bundle") {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    };
    let result = analyze_exact_v3(&bundle, ExactSolverOptions::default()).expect("exact");
    assert_eq!(result.expected_additional_primitive_recruitments, 0.0);
    assert_eq!(
        result.first_all_target_completion_pmf[0].additional_recruitment_count,
        0
    );
    assert_eq!(result.first_all_target_completion_pmf[0].probability, 1.0);
}

#[test]
fn v3_exact_guard_failure_returns_no_result() {
    let options = ExactSolverOptions {
        max_transition_expansions: 2,
        ..ExactSolverOptions::default()
    };
    assert!(analyze_exact_v3(&bundle("v3_three_target_exact_small"), options).is_err());
}
