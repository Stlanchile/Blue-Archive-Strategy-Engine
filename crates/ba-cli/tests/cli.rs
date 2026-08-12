use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix::fs::symlink;
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

fn run_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ba-strategy"))
        .current_dir(directory)
        .args(args)
        .output()
        .expect("CLI should execute")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("UTF-8 stdout")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 stderr")
}

#[test]
fn validate_and_analyze_keep_success_on_stdout() {
    let validate = run(&[
        "validate",
        "data/rulesets/jp_2026_07_29_provisional_v1.json",
        "--format",
        "json",
    ]);
    assert_eq!(validate.status.code(), Some(0));
    assert!(stderr(&validate).is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&validate.stdout).expect("validation JSON");
    assert_eq!(report["valid"], true);

    let analyze = run(&["analyze", "dual_shared_200", "--format", "text"]);
    assert_eq!(analyze.status.code(), Some(0));
    assert!(stderr(&analyze).is_empty());
    let text = stdout(&analyze);
    assert!(text.contains("Expected terminal primitive recruitments"));
    assert!(text.contains("0.640898240397476"));
}

#[test]
fn trace_uses_concrete_labels_and_requires_one_run() {
    let trace = run(&[
        "simulate",
        "charge_199_one",
        "--runs",
        "1",
        "--seed",
        "42",
        "--trace",
        "--format",
        "text",
    ]);
    assert_eq!(trace.status.code(), Some(0));
    let text = stdout(&trace);
    assert!(text.contains("Master seed: 42"));
    assert!(text.contains("Paid pyroxene spent: 120"));
    assert!(!text.contains("Expected paid pyroxene spent"));

    let invalid = run(&[
        "simulate",
        "charge_199_one",
        "--runs",
        "2",
        "--seed",
        "42",
        "--trace",
        "--format",
        "json",
    ]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    let body: serde_json::Value = serde_json::from_slice(&invalid.stderr).expect("JSON error");
    assert_eq!(body["error"]["class"], "cli_usage");
}

#[test]
fn omitted_seed_uses_reported_entropy_and_can_be_replayed_explicitly() {
    let random = run(&[
        "simulate",
        "charge_99_one",
        "--runs",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(random.status.code(), Some(0));
    assert!(stderr(&random).is_empty());
    let body: serde_json::Value =
        serde_json::from_slice(&random.stdout).expect("random simulation JSON");
    let seed = body["rng"]["master_seed"]
        .as_u64()
        .expect("reported OS-generated master seed");

    let replayed = run(&[
        "simulate",
        "charge_99_one",
        "--runs",
        "1",
        "--seed",
        &seed.to_string(),
        "--format",
        "json",
    ]);
    assert_eq!(replayed.status.code(), Some(0));
    assert!(stderr(&replayed).is_empty());
    assert_eq!(random.stdout, replayed.stdout);
}

#[test]
fn usage_prepass_produces_json_errors() {
    let output = run(&["analyze", "--format=json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let body: serde_json::Value = serde_json::from_slice(&output.stderr).expect("usage JSON");
    assert_eq!(body["error"]["code"], "cli_usage");
}

#[test]
fn help_and_version_are_successful_stdout_displays() {
    for args in [&["--help"][..], &["--version"][..]] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(0));
        assert!(stderr(&output).is_empty());
        assert!(!stdout(&output).is_empty());
    }
}

#[test]
fn validation_catalog_and_engine_failures_have_stable_exit_classes() {
    let temp = TempDir::new().expect("tempdir");
    let malformed = temp.path().join("malformed.json");
    fs::write(
        &malformed,
        r#"{"schema_version":1,"document_type":"ruleset","schema_version":1}"#,
    )
    .expect("fixture");
    let validation = run(&[
        "validate",
        malformed.to_str().expect("UTF-8 path"),
        "--format",
        "json",
    ]);
    assert_eq!(validation.status.code(), Some(3));
    assert!(validation.stdout.is_empty());
    let body: serde_json::Value =
        serde_json::from_slice(&validation.stderr).expect("validation error JSON");
    assert_eq!(body["error"]["class"], "validation");

    let missing = run(&[
        "validate",
        "/definitely/not/present/ba-strategy.json",
        "--format",
        "json",
    ]);
    assert_eq!(missing.status.code(), Some(4));
    assert!(missing.stdout.is_empty());
    let body: serde_json::Value = serde_json::from_slice(&missing.stderr).expect("I/O error JSON");
    assert_eq!(body["error"]["class"], "catalog_io");
}

#[test]
fn compare_disagreement_is_still_success() {
    let output = run(&[
        "compare",
        "charge_99_one",
        "--runs",
        "1",
        "--seed",
        "123",
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).expect("comparison JSON");
    assert_eq!(body["engine_kind"], "comparison");
}

#[test]
fn positive_run_count_is_enforced_by_clap() {
    let output = run(&[
        "simulate",
        "charge_99_one",
        "--runs",
        "0",
        "--seed",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn excessive_run_count_fails_as_an_engine_guard_without_starting_work() {
    let output = run(&[
        "simulate",
        "initial_success",
        "--runs",
        "18446744073709551615",
        "--seed",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let body: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("engine error JSON");
    assert_eq!(body["error"]["code"], "simulation_run_limit_exceeded");
}

#[test]
fn help_is_version_neutral_and_version_comes_from_package_metadata() {
    let help = run(&["--help"]);
    let text = stdout(&help);
    assert!(text.contains("Blue Archive Strategy Engine"));
    assert!(!text.contains("Blue Archive Strategy Engine v0.1"));

    let version = run(&["--version"]);
    assert_eq!(
        stdout(&version).trim(),
        format!("ba-strategy {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn catalog_listing_and_inspection_are_deterministic_and_exclude_test_fixtures() {
    let args = ["catalog", "list", "all", "--format", "json"];
    let first = run(&args);
    let second = run(&args);
    assert_eq!(first.status.code(), Some(0));
    assert!(first.stderr.is_empty());
    assert_eq!(first.stdout, second.stdout);
    let body: serde_json::Value = serde_json::from_slice(&first.stdout).expect("catalog JSON");
    assert_eq!(body["output_schema_version"], 1);
    let rendered = stdout(&first);
    assert!(rendered.contains("jp_2026_07_29_provisional_v1"));
    assert!(rendered.contains("jp_2026_07_29_provisional_v2"));
    assert!(rendered.contains("jp_2026_07_29_empty_v2"));
    assert!(!rendered.contains("synthetic_non_v1"));

    let inspect = run(&[
        "catalog",
        "inspect",
        "rulesets",
        "jp_2026_07_29_provisional_v2",
        "--format",
        "json",
    ]);
    assert_eq!(inspect.status.code(), Some(0));
    let body: serde_json::Value = serde_json::from_slice(&inspect.stdout).expect("inspection JSON");
    assert_eq!(body["schema_version"], 2);
    assert_eq!(body["provenance"]["verification_status"], "provisional");

    let text = run(&["catalog", "list", "rulesets"]);
    assert_eq!(text.status.code(), Some(0), "{}", stderr(&text));
    let rendered = stdout(&text);
    assert!(!rendered.trim_start().starts_with('{'));
    assert!(rendered.contains("output_schema_version: 1"));
    assert!(rendered.contains("jp_2026_07_29_provisional_v2"));
}

#[test]
fn scenario_directory_resolves_names_by_selected_directory_not_cwd_shadows() {
    let temp = TempDir::new().expect("tempdir");
    let selected = temp.path().join("selected");
    fs::create_dir(&selected).expect("selected");
    fs::copy(
        workspace_path("scenarios/examples/single_target_v2.json"),
        selected.join("single_target_v2.json"),
    )
    .expect("scenario");
    fs::write(temp.path().join("single_target_v2"), b"{not json").expect("shadow");
    fs::write(temp.path().join("single_target_v2.json"), b"{not json").expect("shadow");
    let data = workspace_path("data");
    for supplied in ["single_target_v2", "single_target_v2.json"] {
        let output = run_in(
            temp.path(),
            &[
                "--data-dir",
                data.to_str().expect("data path"),
                "--scenario-dir",
                selected.to_str().expect("scenario path"),
                "analyze",
                supplied,
                "--format",
                "json",
            ],
        );
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("analysis JSON");
        assert_eq!(
            body["provenance"]["scenario_id"],
            "example_single_target_v2"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn scenario_directory_symlinks_and_all_explicit_path_forms_resolve_securely() {
    let temp = TempDir::new().expect("tempdir");
    let selected = temp.path().join("selected");
    let work = temp.path().join("work");
    let nested = work.join("nested");
    fs::create_dir(&selected).expect("selected");
    fs::create_dir(&work).expect("work");
    fs::create_dir(&nested).expect("nested");
    let source = workspace_path("scenarios/examples/single_target_v2.json");
    fs::copy(&source, selected.join("single_target_v2.json")).expect("selected scenario");
    fs::copy(&source, work.join("explicit.json")).expect("explicit scenario");
    fs::copy(&source, nested.join("explicit.json")).expect("nested scenario");
    let selected_link = temp.path().join("selected-link");
    symlink(&selected, &selected_link).expect("selected symlink");
    let data = workspace_path("data");
    let data = data.to_str().expect("data path");
    let scenario_dir = selected_link.to_str().expect("scenario dir");
    let absolute = selected
        .join("single_target_v2.json")
        .to_str()
        .expect("absolute path")
        .to_owned();

    for supplied in [
        "single_target_v2",
        "./explicit.json",
        "nested/explicit.json",
        "../selected/single_target_v2.json",
        absolute.as_str(),
    ] {
        let output = run_in(
            &work,
            &[
                "--data-dir",
                data,
                "--scenario-dir",
                scenario_dir,
                "analyze",
                supplied,
                "--format",
                "json",
            ],
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{supplied}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn scenario_explanation_and_template_round_trip_without_analysis() {
    let explain = run(&[
        "--scenario-dir",
        "scenarios/examples",
        "scenario",
        "explain",
        "example_dual_target_paid_first_v2",
        "--format",
        "json",
    ]);
    assert_eq!(explain.status.code(), Some(0), "{}", stderr(&explain));
    let body: serde_json::Value =
        serde_json::from_slice(&explain.stdout).expect("explanation JSON");
    assert_eq!(body["document_type"], "scenario_explanation");
    assert_eq!(body["compatibility_profile"], "v2");
    assert_eq!(
        body["compiled_strategy"]["funding_priority"][0],
        "paid_single"
    );
    assert!(body.get("success_probability").is_none());
    assert!(body.get("rng").is_none());

    let template = run(&[
        "scenario",
        "template",
        "--scenario-id",
        "generated_v2",
        "--ruleset",
        "jp_2026_07_29_provisional_v2",
        "--reward-schedule",
        "jp_2026_07_29_empty_v2",
        "--target-count",
        "2",
    ]);
    assert_eq!(template.status.code(), Some(0), "{}", stderr(&template));
    let value: serde_json::Value = serde_json::from_slice(&template.stdout).expect("template JSON");
    assert_eq!(
        value["strategy"]["max_total_recruitments"],
        serde_json::json!(200)
    );
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("generated_v2.json");
    fs::write(&path, &template.stdout).expect("template file");
    let validate = run(&["validate", path.to_str().expect("path"), "--format", "json"]);
    assert_eq!(validate.status.code(), Some(0), "{}", stderr(&validate));
}

#[test]
fn diagnostics_are_versioned_and_do_not_change_default_errors() {
    let temp = TempDir::new().expect("tempdir");
    let malformed = temp.path().join("malformed.json");
    fs::write(
        &malformed,
        "{\n  \"schema_version\": 2,\n  \"document_type\": \"scenario\",\n",
    )
    .expect("malformed");
    let output = run(&[
        "validate",
        malformed.to_str().expect("path"),
        "--diagnostics",
    ]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let body: serde_json::Value = serde_json::from_slice(&output.stderr).expect("diagnostic JSON");
    assert_eq!(body["diagnostics_schema_version"], 1);
    assert_eq!(body["error"]["code"], "invalid_json");
    assert!(body["error"]["line"].as_u64().is_some());
    assert!(body["error"]["column"].as_u64().is_some());

    let default = run(&["validate", malformed.to_str().expect("path")]);
    assert_eq!(default.status.code(), Some(3));
    assert!(default.stdout.is_empty());
    assert!(stderr(&default).starts_with("error [validation:invalid_json]:"));
}
