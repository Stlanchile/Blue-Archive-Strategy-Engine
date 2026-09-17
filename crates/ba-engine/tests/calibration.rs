use std::path::{Path, PathBuf};

use ba_core::load_bundle;
use ba_engine::{
    DEFAULT_MAX_ACTIVE_STATES, DEFAULT_MAX_PROCESSED_STATES, DEFAULT_MAX_TRANSITION_EXPANSIONS,
    ExactSolverOptions, analyze_exact,
};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn every_shipped_scenario_matches_frozen_calibration_and_headroom() {
    let expected = [
        ("campaign_dual_310", 201, 200, 66_813, 72_549),
        ("charge_199_one", 1, 1, 3, 1),
        ("charge_99_one", 2, 1, 4, 2),
        ("dual_independent_200", 103, 102, 30_798, 30_398),
        ("dual_shared_200", 201, 200, 40_599, 40_199),
        ("initial_success", 0, 0, 0, 0),
        ("single_target_200", 2, 1, 600, 399),
        ("ticket_atomic", 10, 9, 57, 91),
    ];
    let mut observed_max_frontier = 0_usize;
    let mut observed_max_processed = 0_u64;
    let mut observed_max_expansions = 0_u64;
    for (name, boundary, in_flight, processed, expansions) in expected {
        let bundle = load_bundle(
            workspace_path("data"),
            workspace_path(&format!("scenarios/golden/{name}.json")),
        )
        .expect("shipped scenario validates");
        let result =
            analyze_exact(&bundle, ExactSolverOptions::default()).expect("calibration completes");
        let diagnostics = result.solver_diagnostics;
        assert_eq!(diagnostics.peak_boundary_frontier, boundary, "{name}");
        assert_eq!(diagnostics.peak_in_flight_frontier, in_flight, "{name}");
        assert_eq!(diagnostics.processed_states, processed, "{name}");
        assert_eq!(diagnostics.transition_expansions, expansions, "{name}");
        observed_max_frontier = observed_max_frontier.max(boundary).max(in_flight);
        observed_max_processed = observed_max_processed.max(processed);
        observed_max_expansions = observed_max_expansions.max(expansions);
    }
    assert!(DEFAULT_MAX_ACTIVE_STATES >= 4 * observed_max_frontier);
    assert!(DEFAULT_MAX_PROCESSED_STATES >= 4 * observed_max_processed);
    assert!(DEFAULT_MAX_TRANSITION_EXPANSIONS >= 4 * observed_max_expansions);
    assert_eq!(DEFAULT_MAX_ACTIVE_STATES, 65_536);
    assert_eq!(DEFAULT_MAX_PROCESSED_STATES, 1_048_576);
    assert_eq!(DEFAULT_MAX_TRANSITION_EXPANSIONS, 2_097_152);
}

#[test]
fn shipped_v3_scenarios_match_qualified_baseline_counts() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/v3_calibration.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let path = case["path"].as_str().unwrap();
        let ba_core::AnyValidatedScenarioBundle::V3(bundle) =
            ba_core::load_any_bundle(workspace_path("data"), workspace_path(path)).unwrap()
        else {
            panic!("v3")
        };
        let result = ba_engine::analyze_exact_v3(&bundle, ExactSolverOptions::default()).unwrap();
        assert_eq!(
            serde_json::to_value(result.solver_diagnostics).unwrap(),
            case["diagnostics"],
            "{path}"
        );
    }
}
