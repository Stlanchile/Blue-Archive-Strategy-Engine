use std::fs;
use std::path::{Path, PathBuf};

use ba_core::schema::{RawRulesetV2, RawScenarioV2};
use ba_core::strict_json::BufferedDocument;
use ba_core::{
    Catalog, CoreError, CoreErrorClass, MAX_CATALOG_DIRECTORY_ENTRIES, MAX_CATALOG_DOCUMENT_BYTES,
    MAX_CATALOG_ENTRIES, MAX_DOCUMENT_BYTES, load_bundle, validate_document,
};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("test directory should be creatable");
    }
    fs::write(path, bytes).expect("test fixture should be writable");
}

fn valid_ruleset(id: &str) -> String {
    format!(
        r#"{{
  "schema_version":2,
  "document_type":"ruleset",
  "ruleset_id":"{id}",
  "provenance":{{"verification_status":"provisional","sources":[]}},
  "paid_single_cost":120,
  "paid_single_action_size":1,
  "ticket_action_size":10,
  "ordinary_pickup_probability":{{"numerator":7,"denominator":1000}},
  "maximum_pre_recruitment_charge":199,
  "hit_reset_charge":0,
  "miss_increment":1,
  "threshold_overrides":[
    {{"pre_charge":99,"pickup_probability":{{"numerator":1,"denominator":2}}}},
    {{"pre_charge":199,"pickup_probability":{{"numerator":1,"denominator":1}}}}
  ]
}}"#
    )
}

fn valid_reward_schedule(id: &str) -> String {
    format!(
        r#"{{
  "schema_version":2,
  "document_type":"reward_schedule",
  "reward_schedule_id":"{id}",
  "provenance":{{"verification_status":"provisional","sources":[]}},
  "compatible_ruleset_ids":["ruleset_000"],
  "milestones":[]
}}"#
    )
}

#[test]
fn duplicate_keys_are_rejected_at_every_depth_and_after_escape_decoding() {
    let temp = TempDir::new().expect("tempdir");
    let cases = [
        r#"{"schema_version":2,"schema_version":2,"document_type":"ruleset"}"#,
        r#"{"schema_version":2,"document_type":"ruleset","document_type":"ruleset"}"#,
        r#"{"schema_version":2,"document_type":"scenario","x":{"banner_id":"a","banner_\u0069d":"b"}}"#,
        r#"{"schema_version":2,"document_type":"scenario","x":{"charge_group_id":"a","charge_group_id":"b"}}"#,
        r#"{"schema_version":2,"document_type":"ruleset","x":{"numerator":1,"numerator":2}}"#,
        r#"{"schema_version":2,"document_type":"ruleset","x":{"denominator":1,"denominator":2}}"#,
        r#"{"schema_version":2,"document_type":"reward_schedule","x":{"resource":"eligma","resource":"gift_boxes"}}"#,
        r#"{"schema_version":2,"document_type":"scenario","x":{"quantity":1,"quantity":2}}"#,
    ];
    for (index, document) in cases.into_iter().enumerate() {
        let path = temp.path().join(format!("{index}.json"));
        write(&path, document);
        let error = BufferedDocument::read(&path).expect_err("duplicate must fail");
        assert!(
            error.to_string().contains("duplicate object key"),
            "{error}"
        );
    }
}

#[test]
fn strict_typed_parse_rejects_unknown_fields_and_trailing_data() {
    let temp = TempDir::new().expect("tempdir");
    let unknown = valid_ruleset("unknown_field").replace(
        r#""ruleset_id":"unknown_field","#,
        r#""ruleset_id":"unknown_field","mystery":1,"#,
    );
    let unknown_path = temp.path().join("unknown.json");
    write(&unknown_path, unknown);
    let buffered = BufferedDocument::read(&unknown_path).expect("scan should succeed");
    assert!(buffered.parse_typed::<RawRulesetV2>().is_err());

    let trailing_path = temp.path().join("trailing.json");
    write(
        &trailing_path,
        format!("{} null", valid_ruleset("trailing")),
    );
    assert!(BufferedDocument::read(&trailing_path).is_err());
}

#[test]
fn depth_malformed_utf8_and_non_object_roots_fail_without_panicking() {
    let temp = TempDir::new().expect("tempdir");
    let deep = format!(
        r#"{{"schema_version":2,"document_type":"ruleset","x":{}{}}}"#,
        "[".repeat(65),
        "]".repeat(65)
    );
    let deep_path = temp.path().join("deep.json");
    write(&deep_path, deep);
    let error = BufferedDocument::read(&deep_path).expect_err("depth must fail");
    assert!(error.to_string().contains("nesting depth"), "{error}");

    let utf8_path = temp.path().join("utf8.json");
    write(&utf8_path, [b'{', 0xff, b'}']);
    assert!(BufferedDocument::read(&utf8_path).is_err());

    for (name, bytes) in [("array.json", b"[]".as_slice()), ("scalar.json", b"1")] {
        let path = temp.path().join(name);
        write(&path, bytes);
        assert!(BufferedDocument::read(&path).is_err());
    }
}

#[test]
fn document_size_boundary_is_complete_and_fail_closed() {
    let temp = TempDir::new().expect("tempdir");
    let base = valid_ruleset("exact_size");
    let maximum = usize::try_from(MAX_DOCUMENT_BYTES).expect("limit fits usize");
    assert!(base.len() < maximum);
    let exact = format!("{base}{}", " ".repeat(maximum - base.len()));
    let exact_path = temp.path().join("exact.json");
    write(&exact_path, exact);
    let document = BufferedDocument::read(&exact_path).expect("exact limit should scan");
    document
        .parse_typed::<RawRulesetV2>()
        .expect("exact limit should parse completely");
    let catalog_root = temp.path().join("catalog");
    fs::create_dir_all(catalog_root.join("rulesets")).expect("rules catalog");
    fs::create_dir_all(catalog_root.join("rewards")).expect("rewards catalog");
    fs::copy(&exact_path, catalog_root.join("rulesets/exact.json"))
        .expect("copy exact-size catalog entry");
    assert_eq!(
        Catalog::load(&catalog_root)
            .expect("exact-size catalog entry should be accepted")
            .rulesets()
            .len(),
        1
    );

    let oversized_path = temp.path().join("oversized.json");
    let oversized = format!(
        "{}{}x",
        valid_ruleset("oversized"),
        " ".repeat(maximum - valid_ruleset("oversized").len())
    );
    write(&oversized_path, oversized);
    let error = BufferedDocument::read(&oversized_path).expect_err("oversize must fail");
    match error {
        CoreError::DocumentSizeLimitExceeded {
            maximum, observed, ..
        } => {
            assert_eq!(maximum, MAX_DOCUMENT_BYTES);
            assert!(observed.to_string().contains("1048577"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_document_paths_are_handled_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = TempDir::new().expect("tempdir");
    let mut name = OsString::from_vec(vec![b'r', 0xff, b'u', b'l', b'e']);
    name.push(".json");
    let path = temp.path().join(name);
    write(&path, valid_ruleset("non_utf8_path"));
    let document = BufferedDocument::read(&path).expect("non-UTF-8 path should load");
    document
        .parse_typed::<RawRulesetV2>()
        .expect("non-UTF-8 path should not affect JSON");
}

#[test]
fn catalog_accepts_all_256_or_rejects_all_257_before_parsing() {
    let accepted = TempDir::new().expect("tempdir");
    let rules_dir = accepted.path().join("rulesets");
    let rewards_dir = accepted.path().join("rewards");
    fs::create_dir_all(&rules_dir).expect("rules dir");
    fs::create_dir_all(&rewards_dir).expect("rewards dir");
    for index in (0..MAX_CATALOG_ENTRIES).rev() {
        write(
            &rules_dir.join(format!("{index:03}.json")),
            valid_ruleset(&format!("ruleset_{index:03}")),
        );
    }
    for index in MAX_CATALOG_ENTRIES..MAX_CATALOG_DIRECTORY_ENTRIES {
        write(&rules_dir.join(format!("note_{index:03}.txt")), "ignored");
    }
    let catalog = Catalog::load(accepted.path()).expect("256 candidates should be accepted");
    assert_eq!(catalog.rulesets().len(), MAX_CATALOG_ENTRIES);
    let ids = catalog
        .rulesets()
        .keys()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(ids.windows(2).all(|window| window[0] < window[1]));
    write(&rules_dir.join("one_too_many.txt"), "rejected");
    assert!(matches!(
        Catalog::load(accepted.path()),
        Err(CoreError::CatalogDirectoryEntryLimitExceeded {
            observed: 513,
            maximum: 512,
            ..
        })
    ));

    let rejected = TempDir::new().expect("tempdir");
    let rules_dir = rejected.path().join("rulesets");
    fs::create_dir_all(&rules_dir).expect("rules dir");
    fs::create_dir_all(rejected.path().join("rewards")).expect("rewards dir");
    for index in 0..=MAX_CATALOG_ENTRIES {
        let contents = if index == 0 {
            "{not valid JSON".to_owned()
        } else {
            valid_ruleset(&format!("ruleset_{index:03}"))
        };
        write(&rules_dir.join(format!("{index:03}.json")), contents);
    }
    let error = Catalog::load(rejected.path()).expect_err("257 candidates must fail");
    match error {
        CoreError::CatalogEntryLimitExceeded {
            observed, maximum, ..
        } => {
            assert_eq!(observed, 257);
            assert_eq!(maximum, 256);
        }
        other => panic!("expected count error before parsing, got {other}"),
    }
}

#[test]
fn aggregate_catalog_byte_limit_is_shared_across_rulesets_and_rewards() {
    let temp = TempDir::new().expect("tempdir");
    let rules = temp.path().join("rulesets");
    let rewards = temp.path().join("rewards");
    fs::create_dir_all(&rules).expect("rules");
    fs::create_dir_all(&rewards).expect("rewards");
    let document_bytes = usize::try_from(MAX_DOCUMENT_BYTES).expect("document limit fits usize");

    for index in 0..8 {
        let mut document = valid_ruleset(&format!("ruleset_{index:03}"));
        document.push_str(&" ".repeat(document_bytes - document.len()));
        write(&rules.join(format!("{index:03}.json")), document);
    }
    for index in 0..9 {
        let mut document = valid_reward_schedule(&format!("rewards_{index:03}"));
        document.push_str(&" ".repeat(document_bytes - document.len()));
        write(&rewards.join(format!("{index:03}.json")), document);
    }

    let error = Catalog::load(temp.path()).expect_err("combined catalog exceeds byte budget");
    assert!(matches!(
        error,
        CoreError::CatalogDocumentBytesLimitExceeded {
            observed,
            maximum: MAX_CATALOG_DOCUMENT_BYTES,
            ..
        } if observed == MAX_CATALOG_DOCUMENT_BYTES + MAX_DOCUMENT_BYTES
    ));
}

#[test]
fn one_bad_unreferenced_catalog_entry_fails_the_whole_catalog() {
    let temp = TempDir::new().expect("tempdir");
    let rules = temp.path().join("rulesets");
    let rewards = temp.path().join("rewards");
    fs::create_dir_all(&rules).expect("rules");
    fs::create_dir_all(&rewards).expect("rewards");
    write(&rules.join("valid.json"), valid_ruleset("valid"));
    write(&rules.join("bad.json"), b"{");
    assert!(Catalog::load(temp.path()).is_err());

    fs::remove_file(rules.join("bad.json")).expect("remove test fixture");
    write(
        &rules.join("unsupported.json"),
        r#"{"schema_version":2,"document_type":"ruleset"}"#,
    );
    assert!(Catalog::load(temp.path()).is_err());
}

#[cfg(unix)]
#[test]
fn catalog_and_documents_reject_json_symlinks_and_non_regular_entries() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().expect("tempdir");
    let rules = temp.path().join("rulesets");
    let rewards = temp.path().join("rewards");
    fs::create_dir_all(&rules).expect("rules");
    fs::create_dir_all(&rewards).expect("rewards");
    let target = temp.path().join("target");
    write(&target, valid_ruleset("symlinked"));
    symlink(&target, rules.join("link.json")).expect("symlink");
    assert!(matches!(
        Catalog::load(temp.path()),
        Err(CoreError::PathPolicy { .. })
    ));

    let direct_link = temp.path().join("direct.json");
    symlink(&target, &direct_link).expect("symlink");
    assert!(matches!(
        BufferedDocument::read(&direct_link),
        Err(CoreError::PathPolicy { .. })
    ));

    fs::remove_file(rules.join("link.json")).expect("remove link");
    fs::create_dir(rules.join("directory.json")).expect("json directory");
    assert!(matches!(
        Catalog::load(temp.path()),
        Err(CoreError::PathPolicy { .. })
    ));
}

#[test]
fn required_positive_horizon_and_every_resource_field_are_enforced() {
    let source = fs::read_to_string(workspace_path("scenarios/golden/single_target_200.json"))
        .expect("shipped scenario");
    let temp = TempDir::new().expect("tempdir");

    let missing_horizon = source.replace("    \"max_total_recruitments\": 200\n", "");
    let horizon_path = temp.path().join("missing_horizon.json");
    write(&horizon_path, missing_horizon);
    assert!(
        load_bundle(workspace_path("data"), &horizon_path).is_err(),
        "missing required positive horizon must fail typed validation"
    );

    let missing_zero = source.replace("    \"eligma\": 0,\n", "");
    let resource_path = temp.path().join("missing_resource.json");
    write(&resource_path, missing_zero);
    let document = BufferedDocument::read(&resource_path).expect("scan");
    assert!(document.parse_typed::<RawScenarioV2>().is_err());
}

#[test]
fn user_derived_overflow_is_rejected_as_domain_validation() {
    let temp = TempDir::new().expect("tempdir");
    let rewards = temp.path().join("overflow_rewards.json");
    write(
        &rewards,
        format!(
            r#"{{
  "schema_version":2,
  "document_type":"reward_schedule",
  "reward_schedule_id":"overflow",
  "provenance":{{"verification_status":"provisional","sources":[]}},
  "compatible_ruleset_ids":["jp_2026_07_29_provisional_v2"],
  "milestones":[
    {{"count":1,"rewards":[{{"resource":"limited_ten_recruitment_tickets","quantity":{}}}]}},
    {{"count":2,"rewards":[{{"resource":"limited_ten_recruitment_tickets","quantity":1}}]}}
  ]
}}"#,
            u64::MAX
        ),
    );
    let error = validate_document(workspace_path("data"), &rewards)
        .expect_err("ticket reward sum overflow must fail");
    assert_eq!(error.class(), CoreErrorClass::Validation);

    let non_ticket_rewards = temp.path().join("overflow_non_ticket_rewards.json");
    write(
        &non_ticket_rewards,
        format!(
            r#"{{
  "schema_version":2,
  "document_type":"reward_schedule",
  "reward_schedule_id":"overflow_non_ticket",
  "provenance":{{"verification_status":"provisional","sources":[]}},
  "compatible_ruleset_ids":["jp_2026_07_29_provisional_v2"],
  "milestones":[
    {{"count":1,"rewards":[{{"resource":"eligma","quantity":{}}}]}},
    {{"count":2,"rewards":[{{"resource":"eligma","quantity":1}}]}}
  ]
}}"#,
            u64::MAX
        ),
    );
    let error = validate_document(workspace_path("data"), &non_ticket_rewards)
        .expect_err("every cumulative resource overflow must fail");
    assert_eq!(error.class(), CoreErrorClass::Validation);
    assert!(
        error
            .to_string()
            .contains("cumulative eligma milestone rewards exceed u64")
    );
    let overflow_catalog = temp.path().join("overflow_catalog");
    fs::create_dir_all(overflow_catalog.join("rulesets")).expect("overflow rules catalog");
    fs::create_dir_all(overflow_catalog.join("rewards")).expect("overflow rewards catalog");
    fs::copy(
        &non_ticket_rewards,
        overflow_catalog.join("rewards/overflow.json"),
    )
    .expect("copy overflowing reward schedule");
    let error = Catalog::load(&overflow_catalog).expect_err("catalog must reject reward overflow");
    assert_eq!(error.class(), CoreErrorClass::Validation);

    let source = fs::read_to_string(workspace_path("scenarios/golden/single_target_200.json"))
        .expect("shipped scenario");
    let scenario = temp.path().join("overflow_scenario.json");
    write(
        &scenario,
        source.replace(
            "\"limited_ten_recruitment_tickets\": 0",
            &format!("\"limited_ten_recruitment_tickets\": {}", u64::MAX),
        ),
    );
    let error =
        load_bundle(workspace_path("data"), &scenario).expect_err("termination bound must fail");
    assert_eq!(error.class(), CoreErrorClass::Validation);
}

#[test]
fn semantic_fingerprint_vectors_and_loaded_bundle_are_stable() {
    let bundle = load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/single_target_200.json"),
    )
    .expect("bundle");
    assert_eq!(
        bundle.fingerprints().ruleset.to_hex(),
        "db0af908e4436e396e9a55e7e0bd39aa8ae30d8a148c34abb07c24cb347fb6ad"
    );
    assert_eq!(
        bundle.fingerprints().reward_schedule.to_hex(),
        "41387171a8507e76f595d0571b3d21028c166cb487bc49e64f3c072c09e6b10e"
    );
    assert_eq!(
        bundle.fingerprints().scenario.to_hex(),
        "95eb5b476e17e148909fefd24da3ab00e0e15c4cbc8fdd5ed26758bc4fdb66af"
    );
}

#[test]
fn every_shipped_document_validates_through_the_public_entrypoint() {
    let data = workspace_path("data");
    for directory in ["data/rulesets", "data/rewards", "scenarios/golden"] {
        let mut paths = fs::read_dir(workspace_path(directory))
            .expect("shipped directory")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect::<Vec<_>>();
        paths.sort();
        assert!(!paths.is_empty());
        for path in paths {
            let report = validate_document(&data, &path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(report.valid, "{}", path.display());
        }
    }
}
