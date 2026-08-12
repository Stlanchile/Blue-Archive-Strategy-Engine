use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use ba_core::load_bundle;
use ba_engine::{
    DEFAULT_MAX_MONTE_CARLO_RUNS, EngineError, ExactSolverOptions, SimulationLimits, analyze_exact,
    compare, derive_run_seed, replay, replay_with_limits, simulate_monte_carlo,
    simulate_monte_carlo_with_limits, simulate_trace, simulate_trace_with_limits,
};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn bundle(name: &str) -> ba_core::ValidatedScenarioBundle {
    load_bundle(
        workspace_path("data"),
        workspace_path(&format!("scenarios/golden/{name}.json")),
    )
    .expect("shipped bundle")
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn per_run_seed_vectors_are_stable_and_indexed_independently() {
    let bundle = bundle("single_target_200");
    assert_eq!(
        hex(derive_run_seed(&bundle, 42, 0)),
        "2854b21b8943e8bf589c063dc3fb7fde782a305bdfb948f14377334c14ea04e9"
    );
    assert_eq!(
        hex(derive_run_seed(&bundle, 42, 1)),
        "dd3b7fef826ebc82af5092431d9e9a7c279988b0292b8cbd5c520263955221b5"
    );
    assert_ne!(
        derive_run_seed(&bundle, 42, 0),
        derive_run_seed(&bundle, 43, 0)
    );
}

#[test]
fn repeated_simulation_is_byte_for_byte_deterministic() {
    let bundle = bundle("dual_shared_200");
    let runs = NonZeroU64::new(512).expect("runs");
    let first = simulate_monte_carlo(&bundle, runs, 9876).expect("first");
    let second = simulate_monte_carlo(&bundle, runs, 9876).expect("second");
    assert_eq!(
        serde_json::to_vec(&first).expect("serialize"),
        serde_json::to_vec(&second).expect("serialize")
    );
    assert_eq!(first.rng.rng_algorithm, "chacha8");
    assert_eq!(first.rng.stream_derivation_version, "mc-run-stream-v1");
    assert_eq!(first.rng.run_count, 512);
}

#[test]
fn one_run_mean_intervals_are_null_and_wilson_is_present() {
    let result = simulate_monte_carlo(
        &bundle("charge_99_one"),
        NonZeroU64::new(1).expect("runs"),
        1,
    )
    .expect("simulation");
    assert!(
        result
            .estimation
            .expected_terminal_primitive_recruitments
            .confidence_interval_95
            .is_none()
    );
    assert!(
        result
            .estimation
            .expected_residual_resources
            .pyroxene
            .confidence_interval_95
            .is_none()
    );
    assert!(result.estimation.success_probability_interval_95.lower <= result.success_probability);
    assert!(result.success_probability <= result.estimation.success_probability_interval_95.upper);
    assert_eq!(
        result
            .estimation
            .probability_intervals_95
            .terminal_reason_probabilities
            .iter()
            .map(|entry| entry.sample_count)
            .sum::<u64>(),
        1
    );
}

#[test]
fn trace_replays_through_the_same_kernel() {
    let bundle = bundle("ticket_atomic");
    let trace = simulate_trace(&bundle, 77).expect("trace");
    let replayed = replay(&bundle, &trace.replay_outcomes).expect("replay");
    assert_eq!(
        trace.terminal_primitive_recruitments,
        replayed.terminal_primitive_recruitments
    );
    assert_eq!(
        trace.first_success_recruitment_count,
        replayed.first_success_recruitment_count
    );
    assert_eq!(trace.paid_pyroxene_spent, replayed.paid_pyroxene_spent);
    assert_eq!(
        trace.ticket_funded_primitive_recruitments,
        replayed.ticket_funded_primitive_recruitments
    );
    assert_eq!(trace.terminal_resources, replayed.terminal_resources);
    assert_eq!(trace.terminal_reason, replayed.terminal_reason);
    assert!(replayed.rng.is_none());
    assert!(replay(&bundle, &trace.replay_outcomes[..9]).is_err());
    let mut extra = trace.replay_outcomes.clone();
    extra.push(ba_core::RecruitOutcome::Miss);
    assert!(replay(&bundle, &extra).is_err());
}

#[test]
fn comparison_runs_exact_first_and_disagreement_is_informational() {
    let bundle = bundle("charge_99_one");
    let result = compare(&bundle, NonZeroU64::new(1).expect("runs"), 123)
        .expect("comparison still succeeds");
    assert_eq!(result.exact.success_probability, 0.5);
    assert!(
        result.monte_carlo.success_probability == 0.0
            || result.monte_carlo.success_probability == 1.0
    );
}

#[test]
fn exact_and_monte_carlo_share_initial_success_semantics() {
    let bundle = bundle("initial_success");
    let exact = analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact");
    let simulated =
        simulate_monte_carlo(&bundle, NonZeroU64::new(16).expect("runs"), 55).expect("simulation");
    let trace = simulate_trace(&bundle, 55).expect("trace");
    assert_eq!(exact.success_probability, 1.0);
    assert_eq!(simulated.success_probability, 1.0);
    assert_eq!(simulated.expected_terminal_primitive_recruitments, 0.0);
    assert_eq!(trace.terminal_primitive_recruitments, 0);
    assert_eq!(trace.first_success_recruitment_count, Some(0));
    assert!(trace.replay_outcomes.is_empty());
}

#[test]
fn concrete_execution_limits_fail_before_unbounded_work_or_trace_growth() {
    let bundle = bundle("ticket_atomic");
    let excessive_runs = NonZeroU64::new(DEFAULT_MAX_MONTE_CARLO_RUNS + 1).expect("positive runs");
    assert!(matches!(
        simulate_monte_carlo(&bundle, excessive_runs, 1),
        Err(EngineError::SimulationRunLimitExceeded { .. })
    ));

    let limits = SimulationLimits {
        max_trace_primitive_transitions: 5,
        ..SimulationLimits::default()
    };
    assert!(matches!(
        simulate_trace_with_limits(&bundle, 1, limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "trace",
            observed: 6,
            maximum: 5
        })
    ));
    let outcomes = vec![ba_core::RecruitOutcome::Pickup; 10];
    assert!(matches!(
        replay_with_limits(&bundle, &outcomes, limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "replay",
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
        simulate_monte_carlo_with_limits(&bundle, one_run, 1, per_run_limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "Monte Carlo run",
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
        simulate_monte_carlo_with_limits(&bundle, one_run, 1, total_limits),
        Err(EngineError::SimulationPrimitiveLimitExceeded {
            scope: "Monte Carlo total",
            observed: 6,
            maximum: 5
        })
    ));
}

#[test]
#[ignore = "statistical smoke check"]
fn monte_carlo_interval_contains_the_dual_exact_golden_at_scale() {
    let bundle = bundle("dual_shared_200");
    let exact = analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact");
    let simulated = simulate_monte_carlo(&bundle, NonZeroU64::new(100_000).expect("runs"), 2026)
        .expect("simulation");
    let interval = simulated.estimation.success_probability_interval_95;
    assert!((interval.lower..=interval.upper).contains(&exact.success_probability));
}
