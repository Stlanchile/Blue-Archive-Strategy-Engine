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
