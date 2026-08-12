use std::fs;
use std::path::{Path, PathBuf};

use ba_core::{
    BundleCompatibilityProfile, Catalog, CoreError, FundingKind, load_bundle, validate_document,
};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn copy_runtime_data(destination: &Path) {
    fs::create_dir_all(destination.join("rulesets")).expect("rulesets dir");
    fs::create_dir_all(destination.join("rewards")).expect("rewards dir");
    for source in [
        "data/rulesets/jp_2026_07_29_provisional_v1.json",
        "data/rulesets/jp_2026_07_29_provisional_v2.json",
        "data/rewards/empty_v1.json",
        "data/rewards/jp_2026_07_29_campaign_v1.json",
        "data/rewards/jp_2026_07_29_empty_v2.json",
    ] {
        let source = workspace_path(source);
        let child = if source
            .parent()
            .is_some_and(|value| value.ends_with("rulesets"))
        {
            "rulesets"
        } else {
            "rewards"
        };
        fs::copy(
            &source,
            destination
                .join(child)
                .join(source.file_name().expect("file name")),
        )
        .expect("copy runtime data");
    }
}

fn write_json(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(value).expect("serialize")).expect("write JSON");
}

fn v2_scenario() -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(workspace_path("scenarios/examples/single_target_v2.json")).expect("v2 example"),
    )
    .expect("scenario JSON")
}

#[test]
fn shipped_v2_documents_are_provisional_and_mirror_v1_mechanics() {
    let catalog = Catalog::load(workspace_path("data")).expect("catalog");
    let v1 = catalog
        .rulesets()
        .iter()
        .find(|(id, _)| id.as_str() == "jp_2026_07_29_provisional_v1")
        .map(|(_, value)| value)
        .expect("v1");
    let v2 = catalog
        .rulesets()
        .iter()
        .find(|(id, _)| id.as_str() == "jp_2026_07_29_provisional_v2")
        .map(|(_, value)| value)
        .expect("v2");
    assert_eq!(v1.paid_single_cost(), v2.paid_single_cost());
    assert_eq!(v1.paid_single_action_size(), v2.paid_single_action_size());
    assert_eq!(v1.ticket_action_size(), v2.ticket_action_size());
    assert_eq!(
        v1.ordinary_pickup_probability(),
        v2.ordinary_pickup_probability()
    );
    assert_eq!(
        v1.maximum_pre_recruitment_charge(),
        v2.maximum_pre_recruitment_charge()
    );
    assert_eq!(v1.hit_reset_charge(), v2.hit_reset_charge());
    assert_eq!(v1.miss_increment(), v2.miss_increment());
    assert_eq!(v1.threshold_overrides(), v2.threshold_overrides());
    let provenance = v2.provenance().expect("v2 provenance");
    assert_eq!(provenance.verification_status.as_str(), "provisional");
    assert!(provenance.sources.is_empty());
}

#[test]
fn all_mixed_schema_profiles_follow_the_scenario_version_gate() {
    let temp = TempDir::new().expect("tempdir");
    copy_runtime_data(temp.path());
    fs::write(
        temp.path().join("rewards/mixed_reward_v1.json"),
        r#"{
  "schema_version": 1,
  "document_type": "reward_schedule",
  "reward_schedule_id": "mixed_reward_v1",
  "compatible_ruleset_ids": ["jp_2026_07_29_provisional_v2"],
  "milestones": []
}"#,
    )
    .expect("mixed reward");

    let v1_v1_v1 = load_bundle(
        temp.path(),
        workspace_path("scenarios/golden/single_target_200.json"),
    )
    .expect("v1 profile");
    assert_eq!(v1_v1_v1.profile(), BundleCompatibilityProfile::V1);

    let mut v1_v2 = serde_json::from_slice::<serde_json::Value>(
        &fs::read(workspace_path("scenarios/golden/single_target_200.json")).expect("v1"),
    )
    .expect("JSON");
    v1_v2["ruleset_id"] = serde_json::json!("jp_2026_07_29_provisional_v2");
    let v1_v2_path = temp.path().join("v1_v2.json");
    write_json(&v1_v2_path, &v1_v2);
    assert!(matches!(
        load_bundle(temp.path(), &v1_v2_path),
        Err(CoreError::IncompatibleSchemaReference {
            referenced_kind: "ruleset",
            ..
        })
    ));

    let mut v1_reward_v2 = serde_json::from_slice::<serde_json::Value>(
        &fs::read(workspace_path("scenarios/golden/single_target_200.json")).expect("v1"),
    )
    .expect("JSON");
    v1_reward_v2["reward_schedule_id"] = serde_json::json!("jp_2026_07_29_empty_v2");
    let v1_reward_v2_path = temp.path().join("v1_reward_v2.json");
    write_json(&v1_reward_v2_path, &v1_reward_v2);
    assert!(matches!(
        load_bundle(temp.path(), &v1_reward_v2_path),
        Err(CoreError::IncompatibleSchemaReference {
            referenced_kind: "reward_schedule",
            ..
        })
    ));

    let combinations = [
        ("jp_2026_07_29_provisional_v1", "empty_v1", "v2_v1_v1"),
        (
            "jp_2026_07_29_provisional_v2",
            "mixed_reward_v1",
            "v2_v2_v1",
        ),
        (
            "jp_2026_07_29_provisional_v1",
            "jp_2026_07_29_empty_v2",
            "v2_v1_v2",
        ),
        (
            "jp_2026_07_29_provisional_v2",
            "jp_2026_07_29_empty_v2",
            "v2_v2_v2",
        ),
    ];
    for (ruleset, rewards, id) in combinations {
        let mut scenario = v2_scenario();
        scenario["scenario_id"] = serde_json::json!(id);
        scenario["ruleset_id"] = serde_json::json!(ruleset);
        scenario["reward_schedule_id"] = serde_json::json!(rewards);
        let path = temp.path().join(format!("{id}.json"));
        write_json(&path, &scenario);
        assert_eq!(
            load_bundle(temp.path(), path)
                .expect("v2 combination")
                .profile(),
            BundleCompatibilityProfile::V2
        );
    }
}

#[test]
fn synthetic_non_v1_documents_execute_only_when_staged_as_test_data() {
    let temp = TempDir::new().expect("tempdir");
    fs::create_dir_all(temp.path().join("rulesets")).expect("rules");
    fs::create_dir_all(temp.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/non_v1_ruleset.json"),
        temp.path().join("rulesets/non_v1.json"),
    )
    .expect("ruleset");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/non_v1_reward.json"),
        temp.path().join("rewards/non_v1.json"),
    )
    .expect("reward");
    let bundle = load_bundle(
        temp.path(),
        workspace_path("tests/fixtures/schema_v2/non_v1_scenario.json"),
    )
    .expect("synthetic bundle");
    assert_eq!(bundle.ruleset().paid_single_cost(), 10);
    assert_eq!(bundle.ruleset().paid_single_action_size(), 2);
    assert_eq!(bundle.ruleset().ticket_action_size(), 3);
    assert_eq!(bundle.ruleset().maximum_pre_recruitment_charge(), 9);
    assert_eq!(
        bundle.compiled_strategy().funding_priority(),
        [FundingKind::PaidSingle, FundingKind::TicketTen]
    );

    let runtime_ids = Catalog::load(workspace_path("data"))
        .expect("runtime catalog")
        .rulesets()
        .keys()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(
        runtime_ids
            .iter()
            .all(|value| !value.starts_with("synthetic_"))
    );
}

#[test]
fn v2_provenance_validation_and_document_fingerprint_separation_are_enforced() {
    let temp = TempDir::new().expect("tempdir");
    let source = workspace_path("data/rulesets/jp_2026_07_29_provisional_v2.json");
    let mut first: serde_json::Value =
        serde_json::from_slice(&fs::read(&source).expect("ruleset")).expect("JSON");
    let first_path = temp.path().join("first.json");
    write_json(&first_path, &first);
    let first_report = validate_document(workspace_path("data"), &first_path).expect("first");

    first["provenance"] = serde_json::json!({
        "verification_status": "source_backed",
        "sources": [{
            "label": "test source",
            "reference": "https://example.invalid/reference",
            "retrieved_on": "2026-08-12",
            "content_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }]
    });
    let second_path = temp.path().join("second.json");
    write_json(&second_path, &first);
    let second_report = validate_document(workspace_path("data"), &second_path).expect("second");
    assert_eq!(
        first_report.behavior_fingerprint,
        second_report.behavior_fingerprint
    );
    assert_ne!(
        first_report.document_fingerprint,
        second_report.document_fingerprint
    );

    first["provenance"]["sources"][0]["retrieved_on"] = serde_json::json!("2025-02-29");
    let invalid_path = temp.path().join("invalid.json");
    write_json(&invalid_path, &first);
    assert!(validate_document(workspace_path("data"), invalid_path).is_err());
}

#[test]
fn duplicate_ids_are_rejected_across_schema_versions() {
    let temp = TempDir::new().expect("tempdir");
    copy_runtime_data(temp.path());
    let mut duplicate: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace_path(
            "data/rulesets/jp_2026_07_29_provisional_v2.json",
        ))
        .expect("v2"),
    )
    .expect("JSON");
    duplicate["ruleset_id"] = serde_json::json!("jp_2026_07_29_provisional_v1");
    write_json(
        &temp.path().join("rulesets/cross_version_duplicate.json"),
        &duplicate,
    );
    assert!(Catalog::load(temp.path()).is_err());
}

#[test]
fn unknown_schema_and_document_type_pairs_are_typed_unsupported_documents() {
    let temp = TempDir::new().expect("tempdir");
    for (name, schema_version, document_type) in [
        ("unknown_version", 3, "ruleset"),
        ("unknown_kind", 2, "other"),
    ] {
        let path = temp.path().join(format!("{name}.json"));
        write_json(
            &path,
            &serde_json::json!({
                "schema_version": schema_version,
                "document_type": document_type,
            }),
        );
        assert!(matches!(
            validate_document(workspace_path("data"), path),
            Err(CoreError::UnsupportedDocument { .. })
        ));
    }
}
