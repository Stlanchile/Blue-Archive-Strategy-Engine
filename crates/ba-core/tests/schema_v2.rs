use std::fs;
use std::path::{Path, PathBuf};

use ba_core::{Catalog, CoreError, FundingKind, load_bundle, validate_document};
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
        "data/rulesets/jp_2026_07_29_provisional_v2.json",
        "data/rewards/jp_2026_07_29_empty_v2.json",
        "data/rewards/jp_2026_07_29_campaign_v2.json",
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

#[test]
fn shipped_documents_are_v2_and_provisional() {
    let catalog = Catalog::load(workspace_path("data")).expect("catalog");
    assert_eq!(catalog.rulesets().len(), 1);
    assert_eq!(catalog.reward_schedules().len(), 2);
    assert!(
        catalog
            .rulesets()
            .values()
            .all(|value| value.schema_version() == 2
                && value.provenance().verification_status.as_str() == "provisional"
                && value.provenance().sources.is_empty())
    );
    assert!(
        catalog
            .reward_schedules()
            .values()
            .all(|value| value.schema_version() == 2
                && value.provenance().verification_status.as_str() == "provisional"
                && value.provenance().sources.is_empty())
    );
    let ruleset_id = catalog.rulesets().keys().next().expect("ruleset");
    assert!(
        catalog
            .reward_schedules()
            .values()
            .all(|value| value.compatible_ruleset_ids() == [ruleset_id.clone()])
    );
}

#[test]
fn schema_v1_documents_are_unsupported() {
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("schema-v1.json");
    write_json(
        &path,
        &serde_json::json!({
            "schema_version": 1,
            "document_type": "ruleset",
        }),
    );
    assert!(matches!(
        validate_document(workspace_path("data"), path),
        Err(CoreError::UnsupportedDocument {
            schema_version: Some(1),
            ..
        })
    ));
}

#[test]
fn synthetic_custom_documents_execute_only_when_staged_as_test_data() {
    let temp = TempDir::new().expect("tempdir");
    fs::create_dir_all(temp.path().join("rulesets")).expect("rules");
    fs::create_dir_all(temp.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/custom_ruleset.json"),
        temp.path().join("rulesets/custom.json"),
    )
    .expect("ruleset");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/custom_reward.json"),
        temp.path().join("rewards/custom.json"),
    )
    .expect("reward");
    let bundle = load_bundle(
        temp.path(),
        workspace_path("tests/fixtures/schema_v2/custom_scenario.json"),
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
fn provenance_validation_and_document_fingerprint_separation_are_enforced() {
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
fn duplicate_ids_are_rejected() {
    let temp = TempDir::new().expect("tempdir");
    copy_runtime_data(temp.path());
    let duplicate = fs::read(workspace_path(
        "data/rulesets/jp_2026_07_29_provisional_v2.json",
    ))
    .expect("ruleset");
    fs::write(
        temp.path().join("rulesets/duplicate-ruleset.json"),
        duplicate,
    )
    .expect("duplicate");
    assert!(Catalog::load(temp.path()).is_err());
}

#[test]
fn unknown_schema_and_document_type_pairs_are_typed_unsupported_documents() {
    let temp = TempDir::new().expect("tempdir");
    for (name, schema_version, document_type) in [
        ("unknown_version", 4, "ruleset"),
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
