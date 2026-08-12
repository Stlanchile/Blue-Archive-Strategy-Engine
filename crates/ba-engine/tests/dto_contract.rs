use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use ba_core::load_bundle;
use ba_engine::{ExactSolverOptions, analyze_exact, simulate_monte_carlo, simulate_trace};
use serde_json::Value;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn bundle() -> ba_core::ValidatedScenarioBundle {
    load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/charge_99_one.json"),
    )
    .expect("bundle")
}

fn top_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("result is an object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn aggregate_dtos_use_expectation_names_and_trace_uses_concrete_names() {
    let bundle = bundle();
    let exact =
        serde_json::to_value(analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact"))
            .expect("serialize exact");
    let monte_carlo = serde_json::to_value(
        simulate_monte_carlo(&bundle, NonZeroU64::new(4).expect("runs"), 1).expect("MC"),
    )
    .expect("serialize MC");
    let trace =
        serde_json::to_value(simulate_trace(&bundle, 1).expect("trace")).expect("serialize trace");

    for aggregate in [&exact, &monte_carlo] {
        let object = aggregate.as_object().expect("object");
        assert!(object.contains_key("expected_paid_pyroxene_spent"));
        assert!(object.contains_key("expected_ticket_funded_primitive_recruitments"));
        assert!(object.contains_key("expected_residual_resources"));
        assert!(!object.contains_key("paid_pyroxene_spent"));
        assert!(!object.contains_key("ticket_funded_primitive_recruitments"));
        assert!(!object.contains_key("terminal_resources"));
        assert!(!object.contains_key("expected_pulls"));
    }
    let trace = trace.as_object().expect("trace object");
    assert!(trace.contains_key("paid_pyroxene_spent"));
    assert!(trace.contains_key("ticket_funded_primitive_recruitments"));
    assert!(trace.contains_key("terminal_resources"));
    assert!(!trace.contains_key("expected_paid_pyroxene_spent"));
    assert!(!trace.contains_key("expected_residual_resources"));
}

#[test]
fn exact_wire_field_names_are_frozen_and_provenance_excludes_machine_state() {
    let result = serde_json::to_value(
        analyze_exact(&bundle(), ExactSolverOptions::default()).expect("exact"),
    )
    .expect("serialize");
    let keys = top_keys(&result);
    let expected = [
        "context",
        "engine_kind",
        "exact_options",
        "expected_first_success_recruitment_count_given_success",
        "expected_milestone_rewards_acquired",
        "expected_paid_pyroxene_spent",
        "expected_residual_resources",
        "expected_terminal_primitive_recruitments",
        "expected_terminal_primitive_recruitments_given_success",
        "expected_ticket_funded_primitive_recruitments",
        "first_success_cdf",
        "first_success_pmf",
        "milestone_reach_probabilities",
        "owned_target_terminal_probabilities",
        "probability_conservation",
        "provenance",
        "solver_diagnostics",
        "success_probability",
        "terminal_reason_probabilities",
    ];
    assert_eq!(
        keys,
        expected.into_iter().map(str::to_owned).collect::<Vec<_>>()
    );
    let provenance = result["provenance"].as_object().expect("provenance");
    for forbidden in [
        "git",
        "timestamp",
        "hostname",
        "source_path",
        "repository_path",
    ] {
        assert!(!provenance.contains_key(forbidden));
    }
}
