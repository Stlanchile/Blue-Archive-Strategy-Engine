use std::fs;
use std::path::{Path, PathBuf};

use ba_core::schema::RawScenarioV2;
use ba_core::strict_json::BufferedDocument;
use ba_core::{
    AnyValidatedScenarioBundle, Catalog, CoreError, DocumentProfile, compile_any_buffered_bundle,
    load_any_bundle, validate_document,
};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn shipped_v3_documents_and_small_multi_target_bundles_validate() {
    let data = workspace_path("data");
    let rules = validate_document(
        &data,
        workspace_path("data/rulesets/jp_2026_07_29_provisional_v3.json"),
    )
    .expect("v3 ruleset");
    assert_eq!(rules.schema_version, 3);
    assert!(rules.verification_status.is_none());
    assert_eq!(
        rules.provenance_status.map(|status| status.as_str()),
        Some("provisional")
    );

    let rewards = validate_document(
        &data,
        workspace_path("data/rewards/jp_2026_07_29_empty_v3.json"),
    )
    .expect("v3 rewards");
    assert_eq!(rewards.schema_version, 3);

    for scenario in [
        "scenarios/golden/v3_three_target_exact_small.json",
        "scenarios/golden/v3_four_target_exact_small.json",
    ] {
        let bundle = load_any_bundle(&data, workspace_path(scenario)).expect("v3 bundle");
        assert_eq!(bundle.profile(), DocumentProfile::V3);
        assert!(matches!(bundle, AnyValidatedScenarioBundle::V3(_)));
    }
}

#[test]
fn mixed_profile_bundle_is_rejected_before_domain_execution() {
    let temporary = TempDir::new().expect("tempdir");
    let source = fs::read_to_string(workspace_path(
        "scenarios/golden/v3_three_target_exact_small.json",
    ))
    .expect("scenario");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("JSON");
    value["ruleset_id"] = serde_json::json!("jp_2026_07_29_provisional_v2");
    let path = temporary.path().join("mixed.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).expect("render")).expect("fixture");

    let error = load_any_bundle(workspace_path("data"), &path).expect_err("mixed bundle");
    assert!(
        error
            .to_string()
            .contains("mixed-profile bundles are unsupported")
    );
}

#[test]
fn v2_typed_resources_still_reject_v3_only_fields() {
    let temporary = TempDir::new().expect("tempdir");
    let source = fs::read_to_string(workspace_path("scenarios/golden/single_target_200.json"))
        .expect("scenario");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("JSON");
    value["initial_resources"]["keystone_fragments"] = serde_json::json!(0);
    let path = temporary.path().join("widened-v2.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).expect("render")).expect("fixture");
    let document = BufferedDocument::read(&path).expect("dispatch remains schema v2");
    let error = document
        .parse_typed::<RawScenarioV2>()
        .expect_err("v2 must reject a v3 resource field");
    assert!(matches!(error, CoreError::InvalidJson { .. }));
}

#[test]
fn mixed_catalog_compiles_profiles_through_one_atomic_catalog() {
    let catalog = Catalog::load(workspace_path("data")).expect("mixed catalog");
    assert!(!catalog.rulesets().is_empty());
    assert!(!catalog.rulesets_v3().is_empty());
    assert!(!catalog.reward_schedules().is_empty());
    assert!(!catalog.reward_schedules_v3().is_empty());
    let document = BufferedDocument::read(workspace_path(
        "scenarios/golden/v3_three_target_exact_small.json",
    ))
    .expect("scenario");
    assert!(matches!(
        compile_any_buffered_bundle(&catalog, &document).expect("compile"),
        AnyValidatedScenarioBundle::V3(_)
    ));
}

#[test]
fn invalid_unreferenced_v3_document_rejects_the_complete_catalog() {
    let temporary = TempDir::new().expect("tempdir");
    fs::create_dir(temporary.path().join("rulesets")).expect("rulesets");
    fs::create_dir(temporary.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("data/rulesets/jp_2026_07_29_provisional_v2.json"),
        temporary.path().join("rulesets/v2.json"),
    )
    .expect("v2");
    fs::copy(
        workspace_path("data/rewards/jp_2026_07_29_empty_v2.json"),
        temporary.path().join("rewards/v2.json"),
    )
    .expect("rewards");
    fs::write(
        temporary.path().join("rulesets/invalid-v3.json"),
        br#"{
          "schema_version": 3,
          "document_type": "ruleset",
          "ruleset_id": "unreferenced_invalid_v3",
          "unexpected": true
        }"#,
    )
    .expect("invalid");
    assert!(Catalog::load(temporary.path()).is_err());
}

#[test]
fn catalog_ids_are_global_across_profiles() {
    let temporary = TempDir::new().expect("tempdir");
    fs::create_dir(temporary.path().join("rulesets")).expect("rulesets");
    fs::create_dir(temporary.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("data/rulesets/jp_2026_07_29_provisional_v2.json"),
        temporary.path().join("rulesets/v2.json"),
    )
    .expect("v2");
    let source = fs::read_to_string(workspace_path(
        "data/rulesets/jp_2026_07_29_provisional_v3.json",
    ))
    .expect("v3");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("JSON");
    value["ruleset_id"] = serde_json::json!("jp_2026_07_29_provisional_v2");
    fs::write(
        temporary.path().join("rulesets/v3.json"),
        serde_json::to_vec_pretty(&value).expect("render"),
    )
    .expect("v3");
    fs::copy(
        workspace_path("data/rewards/jp_2026_07_29_empty_v2.json"),
        temporary.path().join("rewards/v2.json"),
    )
    .expect("rewards");
    let error = Catalog::load(temporary.path()).expect_err("duplicate ID");
    assert!(error.to_string().contains("duplicate catalog ruleset ID"));
}
