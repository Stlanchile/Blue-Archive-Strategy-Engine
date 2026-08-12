use std::path::{Path, PathBuf};

use ba_core::load_bundle;
use ba_engine::{ExactSolverOptions, analyze_exact};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn analyze(name: &str) -> ba_engine::ExactAnalysisResult {
    let bundle = load_bundle(
        workspace_path("data"),
        workspace_path(&format!("scenarios/golden/{name}.json")),
    )
    .expect("golden bundle should validate");
    analyze_exact(&bundle, ExactSolverOptions::default()).expect("golden analysis should complete")
}

fn cdf_at(result: &ba_engine::ExactAnalysisResult, count: u64) -> f64 {
    result
        .first_success_cdf
        .iter()
        .find(|point| point.recruitment_count == count)
        .expect("requested CDF point should exist")
        .probability
}

#[test]
fn single_target_numeric_golden() {
    let result = analyze("single_target_200");
    assert!((cdf_at(&result, 99) - 0.501_143_539_420_088_3).abs() <= 1.0e-12);
    assert!((cdf_at(&result, 100) - 0.750_571_769_710_044_1).abs() <= 1.0e-12);
    assert!((cdf_at(&result, 199) - 0.875_571_115_868_841_5).abs() <= 1.0e-12);
    assert!((cdf_at(&result, 200) - 1.0).abs() <= 1.0e-12);
    assert!(
        (result
            .expected_first_success_recruitment_count_given_success
            .expect("success is certain")
            - 90.072_268_998_837_49)
            .abs()
            <= 1.0e-10
    );
}

#[test]
fn dual_shared_charge_numeric_golden() {
    let result = analyze("dual_shared_200");
    assert!((result.success_probability - 0.640_898_240_397_475_7).abs() <= 1.0e-12);
}

#[test]
fn approved_boundary_fixtures() {
    let initial = analyze("initial_success");
    assert_eq!(initial.success_probability, 1.0);
    assert_eq!(initial.expected_terminal_primitive_recruitments, 0.0);
    assert_eq!(cdf_at(&initial, 0), 1.0);
    assert_eq!(initial.solver_diagnostics.processed_states, 0);

    let half = analyze("charge_99_one");
    assert_eq!(half.success_probability, 0.5);

    let certain = analyze("charge_199_one");
    assert_eq!(certain.success_probability, 1.0);
    assert_eq!(certain.solver_diagnostics.transition_expansions, 1);

    let atomic = analyze("ticket_atomic");
    assert!((atomic.success_probability - 1.0).abs() <= 1.0e-12);
    assert!((atomic.expected_terminal_primitive_recruitments - 10.0).abs() <= 1.0e-12);
    assert!((atomic.expected_ticket_funded_primitive_recruitments - 10.0).abs() <= 1.0e-12);
}
