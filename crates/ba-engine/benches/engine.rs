use std::fs;
use std::hint::black_box;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ba_core::{Catalog, load_bundle, validate_document};
use ba_engine::{ExactSolverOptions, analyze_exact, simulate_monte_carlo};
use tempfile::TempDir;

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn measure<T>(name: &str, operation: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = operation();
    println!("{name}: {:?}", started.elapsed());
    black_box(value)
}

fn stage_synthetic() -> TempDir {
    let temp = TempDir::new().expect("synthetic benchmark tempdir");
    fs::create_dir_all(temp.path().join("rulesets")).expect("rulesets");
    fs::create_dir_all(temp.path().join("rewards")).expect("rewards");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/custom_ruleset.json"),
        temp.path().join("rulesets/rules.json"),
    )
    .expect("synthetic rules");
    fs::copy(
        workspace_path("tests/fixtures/schema_v2/custom_reward.json"),
        temp.path().join("rewards/rewards.json"),
    )
    .expect("synthetic rewards");
    temp
}

fn main() {
    measure("v2 ruleset read and validation", || {
        validate_document(
            workspace_path("data"),
            workspace_path("data/rulesets/jp_2026_07_29_provisional_v2.json"),
        )
        .expect("v2 ruleset validation")
    });
    measure("complete shipped catalog", || {
        Catalog::load(workspace_path("data")).expect("catalog")
    });

    for scenario in ["single_target_200", "dual_shared_200", "campaign_dual_310"] {
        let bundle = load_bundle(
            workspace_path("data"),
            workspace_path(&format!("scenarios/golden/{scenario}.json")),
        )
        .expect("bundle");
        let result = measure(&format!("{scenario} exact"), || {
            analyze_exact(&bundle, ExactSolverOptions::default()).expect("exact")
        });
        println!(
            "{scenario} deterministic counts: frontier={}, processed={}, expansions={}",
            result.solver_diagnostics.peak_boundary_frontier,
            result.solver_diagnostics.processed_states,
            result.solver_diagnostics.transition_expansions
        );
    }

    let monte_carlo_bundle = load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/single_target_200.json"),
    )
    .expect("Monte Carlo bundle");
    measure("serial Monte Carlo 10000", || {
        simulate_monte_carlo(
            &monte_carlo_bundle,
            NonZeroU64::new(10_000).expect("runs"),
            42,
        )
        .expect("Monte Carlo")
    });

    let synthetic = stage_synthetic();
    let synthetic_bundle = load_bundle(
        synthetic.path(),
        workspace_path("tests/fixtures/schema_v2/custom_scenario.json"),
    )
    .expect("synthetic bundle");
    measure("synthetic custom exact", || {
        analyze_exact(&synthetic_bundle, ExactSolverOptions::default()).expect("synthetic exact")
    });

    let guard_bundle = load_bundle(
        workspace_path("data"),
        workspace_path("scenarios/golden/campaign_dual_310.json"),
    )
    .expect("guard bundle");
    let near_guard = ExactSolverOptions {
        max_active_states: 201,
        max_processed_states: 66_813,
        max_transition_expansions: 72_549,
        ..ExactSolverOptions::default()
    };
    measure("near-guard exact", || {
        analyze_exact(&guard_bundle, near_guard).expect("near guard")
    });
    let over_guard = ExactSolverOptions {
        max_processed_states: 66_812,
        ..near_guard
    };
    measure("over-guard failure", || {
        analyze_exact(&guard_bundle, over_guard).expect_err("over guard")
    });
}
