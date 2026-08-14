use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ba-strategy"))
        .current_dir(workspace_path(""))
        .args(args)
        .output()
        .expect("CLI should execute")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 stderr")
}

#[test]
fn validate_analyze_simulate_and_compare_support_v3() {
    let validate = run(&[
        "validate",
        "data/rulesets/jp_2026_07_29_provisional_v3.json",
        "--format",
        "json",
    ]);
    assert_eq!(validate.status.code(), Some(0), "{}", stderr(&validate));
    let value: serde_json::Value =
        serde_json::from_slice(&validate.stdout).expect("validation JSON");
    assert_eq!(value["schema_version"], 3);
    assert_eq!(value["provenance_status"], "provisional");
    assert!(value.get("verification_status").is_none());

    for scenario in ["v3_three_target_exact_small", "v3_four_target_exact_small"] {
        let analyze = run(&["analyze", scenario, "--format", "json"]);
        assert_eq!(analyze.status.code(), Some(0), "{}", stderr(&analyze));
        let value: serde_json::Value =
            serde_json::from_slice(&analyze.stdout).expect("analysis JSON");
        assert_eq!(value["provenance"]["result_schema_version"], 3);
        assert_eq!(value["all_target_success_probability"], 1.0);
    }

    let simulate = run(&[
        "simulate",
        "v3_atomic_cross_target",
        "--runs",
        "64",
        "--seed",
        "42",
        "--format",
        "json",
    ]);
    assert_eq!(simulate.status.code(), Some(0), "{}", stderr(&simulate));
    let value: serde_json::Value =
        serde_json::from_slice(&simulate.stdout).expect("simulation JSON");
    assert_eq!(value["rng"]["run_count"], 64);

    let compare = run(&[
        "compare",
        "v3_three_target_exact_small",
        "--runs",
        "32",
        "--seed",
        "7",
        "--format",
        "json",
    ]);
    assert_eq!(compare.status.code(), Some(0), "{}", stderr(&compare));
    let value: serde_json::Value =
        serde_json::from_slice(&compare.stdout).expect("comparison JSON");
    assert!(
        value["per_target"]
            .as_array()
            .is_some_and(|values| values.len() == 3)
    );
    assert!(
        value["ordered_prefixes"]
            .as_array()
            .is_some_and(|values| values.len() == 3)
    );
}

#[test]
fn v3_template_is_complete_and_round_trips_through_validation() {
    let template = run(&[
        "scenario",
        "template",
        "--schema-version",
        "3",
        "--scenario-id",
        "template_v3",
        "--ruleset",
        "jp_2026_07_29_provisional_v3",
        "--reward-schedule",
        "jp_2026_07_29_empty_v3",
        "--target-count",
        "4",
    ]);
    assert_eq!(template.status.code(), Some(0), "{}", stderr(&template));
    assert!(template.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&template.stdout).expect("template JSON");
    assert_eq!(value["schema_version"], 3);
    assert_eq!(
        value["cross_target_probability_tables"]
            .as_array()
            .map(Vec::len),
        Some(4)
    );
    assert_eq!(
        value["initial_resources"]
            .as_object()
            .expect("resources")
            .len(),
        11
    );

    let temporary = TempDir::new().expect("tempdir");
    let path = temporary.path().join("template.json");
    fs::write(&path, &template.stdout).expect("template");
    let path = path.to_string_lossy();
    let validate = run(&["validate", &path, "--format", "json"]);
    assert_eq!(validate.status.code(), Some(0), "{}", stderr(&validate));

    let default_v2 = run(&[
        "scenario",
        "template",
        "--scenario-id",
        "template_v2",
        "--ruleset",
        "jp_2026_07_29_provisional_v2",
        "--reward-schedule",
        "jp_2026_07_29_empty_v2",
        "--target-count",
        "2",
    ]);
    let value: serde_json::Value = serde_json::from_slice(&default_v2.stdout).expect("v2 template");
    assert_eq!(value["schema_version"], 2);
    assert!(value.get("authority").is_none());

    let invalid_v2 = run(&[
        "scenario",
        "template",
        "--scenario-id",
        "bad_v2",
        "--ruleset",
        "jp_2026_07_29_provisional_v2",
        "--reward-schedule",
        "jp_2026_07_29_empty_v2",
        "--target-count",
        "4",
    ]);
    assert_eq!(invalid_v2.status.code(), Some(2));
    assert!(invalid_v2.stdout.is_empty());
    assert!(
        stderr(&invalid_v2).contains("target count must be 1 or 2"),
        "{}",
        stderr(&invalid_v2)
    );
}

#[test]
fn catalog_output_versions_follow_emitted_or_subject_profile() {
    let mixed = run(&["catalog", "list", "all", "--format", "json"]);
    assert_eq!(mixed.status.code(), Some(0), "{}", stderr(&mixed));
    let value: serde_json::Value = serde_json::from_slice(&mixed.stdout).expect("mixed catalog");
    assert_eq!(value["output_schema_version"], 2);

    let v2 = run(&[
        "catalog",
        "inspect",
        "rulesets",
        "jp_2026_07_29_provisional_v2",
        "--format",
        "json",
    ]);
    let value: serde_json::Value = serde_json::from_slice(&v2.stdout).expect("v2 inspect");
    assert_eq!(value["output_schema_version"], 1);

    let v3 = run(&[
        "catalog",
        "inspect",
        "rulesets",
        "jp_2026_07_29_provisional_v3",
        "--format",
        "json",
    ]);
    let value: serde_json::Value = serde_json::from_slice(&v3.stdout).expect("v3 inspect");
    assert_eq!(value["output_schema_version"], 2);
    assert_eq!(value["provenance_status"], "provisional");

    let explain = run(&[
        "scenario",
        "explain",
        "v3_four_target_exact_small",
        "--format",
        "json",
    ]);
    let value: serde_json::Value = serde_json::from_slice(&explain.stdout).expect("v3 explain");
    assert_eq!(value["output_schema_version"], 2);
    assert_eq!(value["initial_campaign_recruitment_count"], 385);

    let temporary = TempDir::new().expect("tempdir");
    fs::create_dir(temporary.path().join("rulesets")).expect("rulesets");
    fs::create_dir(temporary.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("data/rulesets/jp_2026_07_29_provisional_v2.json"),
        temporary.path().join("rulesets/rules.json"),
    )
    .expect("rules");
    fs::copy(
        workspace_path("data/rewards/jp_2026_07_29_empty_v2.json"),
        temporary.path().join("rewards/rewards.json"),
    )
    .expect("rewards");
    let root = temporary.path().to_string_lossy();
    let v2_only = run(&[
        "--data-dir",
        &root,
        "catalog",
        "list",
        "rulesets",
        "--format",
        "json",
    ]);
    assert_eq!(v2_only.status.code(), Some(0), "{}", stderr(&v2_only));
    let value: serde_json::Value =
        serde_json::from_slice(&v2_only.stdout).expect("v2-only catalog");
    assert_eq!(value["output_schema_version"], 1);
}

#[test]
fn unsupported_document_hints_are_profile_aware_without_v2_drift() {
    let temporary = TempDir::new().expect("tempdir");
    for (schema_version, expected_hint) in [
        (2, "use schema_version 2 with a supported document_type"),
        (3, "use schema_version 3 with a supported document_type"),
        (
            4,
            "use schema_version 2 or 3 with a supported document_type",
        ),
    ] {
        let path = temporary
            .path()
            .join(format!("unsupported_{schema_version}.json"));
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "schema_version": schema_version,
                "document_type": "unsupported"
            }))
            .expect("JSON"),
        )
        .expect("fixture");
        let output = run(&["validate", path.to_str().expect("path"), "--diagnostics"]);
        assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
        assert!(output.stdout.is_empty());
        let value: serde_json::Value =
            serde_json::from_slice(&output.stderr).expect("diagnostics JSON");
        assert_eq!(value["error"]["hint"], expected_hint);
    }
}
