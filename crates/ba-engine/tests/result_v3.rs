use std::path::{Path, PathBuf};

use ba_core::{AnyValidatedScenarioBundle, load_any_bundle};
use ba_engine::{ExactSolverOptions, analyze_exact_v3};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn result_schema_three_has_explicit_authority_and_eleven_resources() {
    let bundle = match load_any_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/v3_three_target_exact_small.json"),
    )
    .expect("bundle")
    {
        AnyValidatedScenarioBundle::V3(bundle) => bundle,
        AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
    };
    let result = analyze_exact_v3(&bundle, ExactSolverOptions::default()).expect("exact");
    let value = serde_json::to_value(result).expect("JSON");
    assert_eq!(value["provenance"]["engine_semantics_version"], 3);
    assert_eq!(value["provenance"]["result_schema_version"], 3);
    assert_eq!(
        value["provenance"]["authority"]["cross_target_probabilities"],
        "scenario_document_user_authored"
    );
    assert!(
        value["provenance"]
            .get("analysis_verification_status")
            .is_none()
    );
    assert_eq!(
        value["expected_residual_resources"]
            .as_object()
            .expect("resources")
            .len(),
        11
    );
}
