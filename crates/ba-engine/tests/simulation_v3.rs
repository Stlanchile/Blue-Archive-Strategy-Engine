use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use ba_core::{AnyValidatedScenarioBundle, ValidatedScenarioBundleV3, load_any_bundle};
use ba_engine::{
    DEFAULT_MAX_MONTE_CARLO_RUNS, EngineError, SimulationLimits, compare_v3, derive_run_seed_v3,
    replay_v3, replay_v3_with_limits, simulate_monte_carlo_v3, simulate_monte_carlo_v3_with_limits,
    simulate_trace_v3, simulate_trace_v3_with_limits,
};
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
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    }
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn v3_per_run_seed_vector_is_stable() {
    let bundle = bundle("v3_atomic_cross_target");
    assert_eq!(
        hex(derive_run_seed_v3(&bundle, 42, 0)),
        "d2a936840f0a7ddf7235c9baaba1af4349dea4e8d3e370e00c172828b4ef6a87"
    );
    assert_ne!(
        derive_run_seed_v3(&bundle, 42, 0),
        derive_run_seed_v3(&bundle, 42, 1)
    );
}

#[test]
fn v3_monte_carlo_is_serial_and_fixed_seed_reproducible() {
    let bundle = bundle("v3_atomic_cross_target");
    let runs = NonZeroU64::new(128).expect("runs");
    let left = simulate_monte_carlo_v3(&bundle, runs, 42).expect("left");
    let right = simulate_monte_carlo_v3(&bundle, runs, 42).expect("right");
    assert_eq!(
        serde_json::to_vec(&left).expect("left JSON"),
        serde_json::to_vec(&right).expect("right JSON")
    );
    assert_eq!(left.rng.stream_derivation_version, "mc-run-stream-v1");
    assert_eq!(left.rng.rng_algorithm, "chacha8");
    assert_eq!(left.sample_counts.total_runs, 128);
}

#[test]
fn v3_trace_replays_through_the_same_transition_authority() {
    let bundle = bundle("v3_atomic_cross_target");
    let trace = simulate_trace_v3(&bundle, 9).expect("trace");
    let replay = replay_v3(&bundle, &trace.replay_outcomes).expect("replay");
    assert_eq!(
        serde_json::to_value(&trace.events).expect("trace events"),
        serde_json::to_value(&replay.events).expect("replay events")
    );
    assert_eq!(
        trace.terminal_additional_primitive_recruitments,
        replay.terminal_additional_primitive_recruitments
    );
    assert_eq!(trace.terminal_resources, replay.terminal_resources);
}

#[test]
fn v3_comparison_covers_success_targets_prefixes_and_terminal_sets() {
    let bundle = bundle("v3_three_target_exact_small");
    let result = compare_v3(&bundle, NonZeroU64::new(32).expect("runs"), 7).expect("comparison");
    assert!(result.all_target_success.exact_within_monte_carlo_interval);
    assert_eq!(result.per_target.len(), 3);
    assert_eq!(result.ordered_prefixes.len(), 3);
    assert!(
        result.exact.terminal_owned_set_probabilities.len() < result.terminal_owned_sets.len(),
        "the exact support should omit at least one impossible mask"
    );
    assert_eq!(
        result.terminal_owned_sets.len(),
        8,
        "comparison must cover the complete numeric three-target mask domain"
    );
}

#[test]
fn provenance_only_mutation_changes_document_identity_not_streams_or_outcomes() {
    let original = bundle("v3_atomic_cross_target");
    let temporary = TempDir::new().expect("tempdir");
    fs::create_dir(temporary.path().join("rulesets")).expect("rulesets");
    fs::create_dir(temporary.path().join("rewards")).expect("rewards");
    let source = fs::read_to_string(workspace_path(
        "data/rulesets/jp_2026_07_29_provisional_v3.json",
    ))
    .expect("rules");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("JSON");
    value["provenance"]["sources"] = serde_json::json!([{
        "source_id": "inert_note",
        "source_category": "secondary_reference",
        "label": "Inert provisional note",
        "reference": "../../not-opened",
        "published_on": null,
        "retrieved_on": "2026-08-13",
        "content_sha256": null
    }]);
    fs::write(
        temporary.path().join("rulesets/rules.json"),
        serde_json::to_vec_pretty(&value).expect("render"),
    )
    .expect("rules");
    fs::copy(
        workspace_path("data/rewards/jp_2026_07_29_empty_v3.json"),
        temporary.path().join("rewards/rewards.json"),
    )
    .expect("rewards");
    let mutated = match load_any_bundle(
        temporary.path(),
        workspace_path("scenarios/golden/v3_atomic_cross_target.json"),
    )
    .expect("mutated bundle")
    {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    };
    assert_eq!(
        original.fingerprints().ruleset,
        mutated.fingerprints().ruleset
    );
    assert_ne!(
        original.fingerprints().ruleset_document,
        mutated.fingerprints().ruleset_document
    );
    assert_eq!(
        derive_run_seed_v3(&original, 42, 0),
        derive_run_seed_v3(&mutated, 42, 0)
    );
    assert_eq!(
        simulate_trace_v3(&original, 42)
            .expect("original trace")
            .replay_outcomes,
        simulate_trace_v3(&mutated, 42)
            .expect("mutated trace")
            .replay_outcomes
    );
}

#[test]
fn equivalent_probability_scales_preserve_behavior_identity_and_streams() {
    let original = bundle("v3_atomic_cross_target");
    let source = fs::read_to_string(workspace_path(
        "scenarios/golden/v3_atomic_cross_target.json",
    ))
    .expect("scenario");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("JSON");
    for table in value["cross_target_probability_tables"]
        .as_array_mut()
        .expect("tables")
    {
        let ordinary = &mut table["ordinary"];
        ordinary["denominator"] =
            serde_json::json!(ordinary["denominator"].as_u64().expect("denominator") * 10);
        for weight in ordinary["other_target_weights"]
            .as_array_mut()
            .expect("weights")
        {
            weight["weight"] = serde_json::json!(weight["weight"].as_u64().expect("weight") * 10);
        }
        for threshold in table["threshold_overrides"]
            .as_array_mut()
            .expect("thresholds")
        {
            threshold["denominator"] =
                serde_json::json!(threshold["denominator"].as_u64().expect("denominator") * 10);
            for weight in threshold["other_target_weights"]
                .as_array_mut()
                .expect("weights")
            {
                weight["weight"] =
                    serde_json::json!(weight["weight"].as_u64().expect("weight") * 10);
            }
        }
    }
    let temporary = TempDir::new().expect("tempdir");
    let path = temporary.path().join("scaled.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).expect("render")).expect("scenario");
    let scaled = match load_any_bundle(workspace_path("data"), path).expect("scaled bundle") {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    };
    assert_eq!(
        original.fingerprints().scenario,
        scaled.fingerprints().scenario
    );
    assert_ne!(
        original.fingerprints().scenario_document,
        scaled.fingerprints().scenario_document
    );
    assert_eq!(
        derive_run_seed_v3(&original, 7, 11),
        derive_run_seed_v3(&scaled, 7, 11)
    );
    assert_eq!(
        simulate_trace_v3(&original, 7)
            .expect("original")
            .replay_outcomes,
        simulate_trace_v3(&scaled, 7)
            .expect("scaled")
            .replay_outcomes
    );
}

#[test]
fn v3_concrete_execution_limits_fail_before_unbounded_work_or_trace_growth() {
    let bundle = bundle("v3_atomic_cross_target");
    let excessive_runs =
        NonZeroU64::new(DEFAULT_MAX_MONTE_CARLO_RUNS + 1).expect("positive excessive run count");
    assert!(matches!(
        simulate_monte_carlo_v3(&bundle, excessive_runs, 1),
        Err(EngineError::SimulationRunLimitExceeded { .. })
    ));

    let limits = SimulationLimits {
        max_trace_primitive_transitions: 5,
        ..SimulationLimits::default()
    };
    assert!(matches!(
        simulate_trace_v3_with_limits(&bundle, 1, limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "v3 trace",
            observed: 6,
            maximum: 5
        })
    ));
    let outcomes = simulate_trace_v3(&bundle, 1)
        .expect("valid trace")
        .replay_outcomes;
    assert!(matches!(
        replay_v3_with_limits(&bundle, &outcomes, limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "v3 replay",
            observed: 6,
            maximum: 5
        })
    ));

    let one_run = NonZeroU64::new(1).expect("one run");
    let per_run_limits = SimulationLimits {
        max_primitive_transitions_per_run: 5,
        max_total_primitive_transitions: 100,
        ..SimulationLimits::default()
    };
    assert!(matches!(
        simulate_monte_carlo_v3_with_limits(&bundle, one_run, 1, per_run_limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "v3 Monte Carlo run",
            observed: 6,
            maximum: 5
        })
    ));
    let total_limits = SimulationLimits {
        max_primitive_transitions_per_run: 100,
        max_total_primitive_transitions: 5,
        ..SimulationLimits::default()
    };
    assert!(matches!(
        simulate_monte_carlo_v3_with_limits(&bundle, one_run, 1, total_limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "v3 Monte Carlo total",
            observed: 6,
            maximum: 5
        })
    ));
}
