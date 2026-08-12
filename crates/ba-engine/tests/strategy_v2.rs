use std::fs;
use std::path::{Path, PathBuf};

use ba_core::{CoreErrorClass, RecruitOutcome, load_bundle};
use ba_engine::{
    ExactSolverOptions, RunTraceEvent, analyze_exact, replay, simulate_monte_carlo, simulate_trace,
};
use std::num::NonZeroU64;
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn load(relative: &str) -> ba_core::ValidatedScenarioBundle {
    load_bundle(workspace_path("data"), workspace_path(relative)).expect("bundle")
}

#[test]
fn funding_order_controls_the_atomic_action_across_all_execution_modes() {
    let paid = load("scenarios/golden/v2_funding_paid_first.json");
    let ticket = load("scenarios/golden/v2_funding_ticket_first.json");

    let paid_exact = analyze_exact(&paid, ExactSolverOptions::default()).expect("paid exact");
    let ticket_exact = analyze_exact(&ticket, ExactSolverOptions::default()).expect("ticket exact");
    assert_eq!(paid_exact.expected_terminal_primitive_recruitments, 1.0);
    assert_eq!(paid_exact.expected_paid_pyroxene_spent, 120.0);
    assert!((ticket_exact.expected_terminal_primitive_recruitments - 10.0).abs() < 1.0e-12);
    assert!((ticket_exact.expected_ticket_funded_primitive_recruitments - 10.0).abs() < 1.0e-12);

    let paid_trace = simulate_trace(&paid, 42).expect("paid trace");
    let ticket_trace = simulate_trace(&ticket, 42).expect("ticket trace");
    assert_eq!(paid_trace.terminal_primitive_recruitments, 1);
    assert_eq!(
        paid_trace
            .terminal_resources
            .limited_ten_recruitment_tickets,
        1
    );
    assert_eq!(ticket_trace.terminal_primitive_recruitments, 10);
    assert_eq!(
        ticket_trace
            .terminal_resources
            .limited_ten_recruitment_tickets,
        0
    );
    assert_eq!(ticket_trace.first_success_recruitment_count, Some(1));

    let replayed = replay(&ticket, &ticket_trace.replay_outcomes).expect("replay");
    assert_eq!(replayed.terminal_primitive_recruitments, 10);
    let paid_mc =
        simulate_monte_carlo(&paid, NonZeroU64::new(8).expect("runs"), 42).expect("paid MC");
    let ticket_mc =
        simulate_monte_carlo(&ticket, NonZeroU64::new(8).expect("runs"), 42).expect("ticket MC");
    assert_eq!(paid_mc.expected_terminal_primitive_recruitments, 1.0);
    assert_eq!(ticket_mc.expected_terminal_primitive_recruitments, 10.0);
}

#[test]
fn v2_horizon_is_mandatory_positive_and_actions_must_fit_completely() {
    let source = fs::read_to_string(workspace_path(
        "scenarios/golden/v2_funding_ticket_first.json",
    ))
    .expect("scenario");
    let temp = TempDir::new().expect("tempdir");
    for (name, replacement) in [
        (
            "missing",
            source.replace("    \"max_total_recruitments\": 10\n", ""),
        ),
        (
            "null",
            source.replace(
                "\"max_total_recruitments\": 10",
                "\"max_total_recruitments\": null",
            ),
        ),
        (
            "zero",
            source.replace(
                "\"max_total_recruitments\": 10",
                "\"max_total_recruitments\": 0",
            ),
        ),
        (
            "negative",
            source.replace(
                "\"max_total_recruitments\": 10",
                "\"max_total_recruitments\": -1",
            ),
        ),
        (
            "fractional",
            source.replace(
                "\"max_total_recruitments\": 10",
                "\"max_total_recruitments\": 1.5",
            ),
        ),
        (
            "overflow",
            source.replace(
                "\"max_total_recruitments\": 10",
                "\"max_total_recruitments\": 18446744073709551616",
            ),
        ),
    ] {
        let path = temp.path().join(format!("{name}.json"));
        fs::write(&path, replacement).expect("write invalid scenario");
        let error = load_bundle(workspace_path("data"), path).expect_err("invalid horizon");
        assert_eq!(error.class(), CoreErrorClass::Validation);
    }

    let one_short = source
        .replace("\"pyroxene\": 120", "\"pyroxene\": 0")
        .replace(
            "\"max_total_recruitments\": 10",
            "\"max_total_recruitments\": 9",
        );
    let path = temp.path().join("one_short.json");
    fs::write(&path, one_short).expect("one short");
    let bundle = load_bundle(workspace_path("data"), path).expect("valid scenario");
    let result = analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact");
    assert_eq!(result.expected_terminal_primitive_recruitments, 0.0);
    assert_eq!(result.success_probability, 0.0);
}

#[test]
fn v2_funding_priority_requires_the_exact_two_kind_permutation() {
    let source = fs::read_to_string(workspace_path(
        "scenarios/golden/v2_funding_ticket_first.json",
    ))
    .expect("scenario");
    let temp = TempDir::new().expect("tempdir");
    let duplicate = source.replace(
        "\"ticket_ten\",\n      \"paid_single\"",
        "\"ticket_ten\",\n      \"ticket_ten\"",
    );
    let path = temp.path().join("duplicate.json");
    fs::write(&path, duplicate).expect("duplicate");
    assert!(load_bundle(workspace_path("data"), path).is_err());

    let mut one_kind: serde_json::Value = serde_json::from_str(&source).expect("scenario JSON");
    one_kind["strategy"]["funding_priority"] = serde_json::json!(["ticket_ten"]);
    let path = temp.path().join("one_kind.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&one_kind).expect("serialize"),
    )
    .expect("one kind");
    assert!(load_bundle(workspace_path("data"), path).is_err());

    let mut extra_kind: serde_json::Value = serde_json::from_str(&source).expect("scenario JSON");
    extra_kind["strategy"]["funding_priority"] =
        serde_json::json!(["ticket_ten", "paid_single", "ticket_ten"]);
    let path = temp.path().join("extra_kind.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&extra_kind).expect("serialize"),
    )
    .expect("extra kind");
    assert!(load_bundle(workspace_path("data"), path).is_err());

    let mut unknown_version: serde_json::Value =
        serde_json::from_str(&source).expect("scenario JSON");
    unknown_version["strategy"]["strategy_schema_version"] = serde_json::json!(2);
    let path = temp.path().join("unknown_version.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&unknown_version).expect("serialize"),
    )
    .expect("unknown version");
    assert!(load_bundle(workspace_path("data"), path).is_err());

    let mut unknown_kind: serde_json::Value = serde_json::from_str(&source).expect("scenario JSON");
    unknown_kind["strategy"]["kind"] = serde_json::json!("unknown");
    let path = temp.path().join("unknown_kind.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&unknown_kind).expect("serialize"),
    )
    .expect("unknown kind");
    assert!(load_bundle(workspace_path("data"), path).is_err());
}

#[test]
fn synthetic_v2_mechanics_drive_costs_probabilities_charges_and_action_sizes() {
    let temp = TempDir::new().expect("tempdir");
    fs::create_dir_all(temp.path().join("rulesets")).expect("rulesets");
    fs::create_dir_all(temp.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/non_v1_ruleset.json"),
        temp.path().join("rulesets/rules.json"),
    )
    .expect("ruleset");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/non_v1_reward.json"),
        temp.path().join("rewards/rewards.json"),
    )
    .expect("rewards");
    let scenario_path = workspace_path("tests/fixtures/schema_v2/non_v1_scenario.json");
    let bundle = load_bundle(temp.path(), &scenario_path).expect("synthetic bundle");

    let exact = analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact");
    assert_eq!(exact.success_probability, 1.0);
    assert_eq!(exact.expected_terminal_primitive_recruitments, 2.0);
    assert_eq!(exact.expected_paid_pyroxene_spent, 10.0);
    assert_eq!(exact.first_success_pmf.len(), 2);
    assert!((exact.first_success_pmf[0].probability - 0.01).abs() < 1.0e-12);
    assert!((exact.first_success_pmf[1].probability - 0.99).abs() < 1.0e-12);

    let replayed =
        replay(&bundle, &[RecruitOutcome::Miss, RecruitOutcome::Pickup]).expect("replay");
    let primitive = replayed
        .events
        .iter()
        .filter_map(|event| match event {
            RunTraceEvent::PrimitiveTransition(event) => Some(event),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(primitive.len(), 2);
    assert_eq!((primitive[0].pre_charge, primitive[0].post_charge), (8, 9));
    assert_eq!((primitive[1].pre_charge, primitive[1].post_charge), (9, 1));
    assert_eq!(replayed.paid_pyroxene_spent, 10);
    assert_eq!(replayed.terminal_resources.pyroxene, 10);
    assert_eq!(
        replayed.terminal_resources.limited_ten_recruitment_tickets,
        1
    );
    assert_eq!(replayed.milestone_rewards_acquired.gift_boxes, 1);

    let mut ticket_scenario: serde_json::Value =
        serde_json::from_slice(&fs::read(&scenario_path).expect("scenario")).expect("JSON");
    ticket_scenario["scenario_id"] = serde_json::json!("synthetic_ticket_action");
    ticket_scenario["initial_resources"]["pyroxene"] = serde_json::json!(0);
    ticket_scenario["strategy"]["funding_priority"] =
        serde_json::json!(["ticket_ten", "paid_single"]);
    ticket_scenario["strategy"]["max_total_recruitments"] = serde_json::json!(3);
    let ticket_path = temp.path().join("ticket_scenario.json");
    fs::write(
        &ticket_path,
        serde_json::to_vec_pretty(&ticket_scenario).expect("serialize"),
    )
    .expect("ticket scenario");
    let ticket_bundle = load_bundle(temp.path(), ticket_path).expect("ticket bundle");
    let ticket_trace = replay(
        &ticket_bundle,
        &[
            RecruitOutcome::Miss,
            RecruitOutcome::Pickup,
            RecruitOutcome::Miss,
        ],
    )
    .expect("atomic ticket replay");
    assert_eq!(ticket_trace.terminal_primitive_recruitments, 3);
    assert_eq!(ticket_trace.ticket_funded_primitive_recruitments, 3);
}
