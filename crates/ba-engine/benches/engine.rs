use std::fs;
use std::hint::black_box;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ba_core::schema::RawRewardScheduleV3;
use ba_core::{
    AnyValidatedScenarioBundle, Catalog, CompiledOutcomeDistribution, ProbabilityRatio,
    RewardScheduleV3, TargetIndex, load_any_bundle, load_bundle, validate_document,
};
use ba_engine::{
    ExactSolverOptions, analyze_exact, analyze_exact_v3, simulate_monte_carlo,
    simulate_monte_carlo_v3,
};
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
    measure("v3 ruleset read and validation", || {
        validate_document(
            workspace_path("data"),
            workspace_path("data/rulesets/jp_2026_07_29_provisional_v3.json"),
        )
        .expect("v3 ruleset validation")
    });
    measure("v3 categorical compilation", || {
        CompiledOutcomeDistribution::compile(
            ProbabilityRatio::new(7, 1000).expect("ratio"),
            10_000,
            &[
                (TargetIndex::new(1, 4).expect("target"), 70),
                (TargetIndex::new(2, 4).expect("target"), 0),
                (TargetIndex::new(3, 4).expect("target"), 35),
            ],
        )
        .expect("categorical")
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

    for scenario in [
        "v3_three_target_exact_small",
        "v3_four_target_exact_small",
        "v3_atomic_cross_target",
    ] {
        let bundle = match load_any_bundle(
            workspace_path("data"),
            workspace_path(&format!("scenarios/golden/{scenario}.json")),
        )
        .expect("v3 bundle")
        {
            AnyValidatedScenarioBundle::V3(bundle) => bundle,
            AnyValidatedScenarioBundle::V2(_) => panic!("expected v3"),
        };
        let result = measure(&format!("{scenario} exact"), || {
            analyze_exact_v3(&bundle, ExactSolverOptions::default()).expect("v3 exact")
        });
        println!(
            "{scenario} deterministic counts: boundary={}, in-flight={}, processed={}, expansions={}",
            result.solver_diagnostics.peak_boundary_frontier,
            result.solver_diagnostics.peak_in_flight_frontier,
            result.solver_diagnostics.processed_states,
            result.solver_diagnostics.transition_expansions
        );
        if scenario == "v3_atomic_cross_target" {
            measure("v3 serial Monte Carlo 10000", || {
                simulate_monte_carlo_v3(&bundle, NonZeroU64::new(10_000).expect("runs"), 42)
                    .expect("v3 Monte Carlo")
            });
        }
    }

    let repeat_raw: RawRewardScheduleV3 = serde_json::from_value(serde_json::json!({
        "schema_version": 3,
        "document_type": "reward_schedule",
        "reward_schedule_id": "benchmark_repeat",
        "provenance": {
            "provenance_status": "provisional",
            "sources": [],
            "claim_bindings": []
        },
        "compatible_ruleset_ids": ["benchmark"],
        "initial_milestones": [],
        "repeating_cycle": {
            "starts_after_count": 390,
            "period": 200,
            "milestones": [{
                "offset": 20,
                "rewards": [{"resource": "eligma", "quantity": 1}]
            }]
        }
    }))
    .expect("repeat raw");
    let repeat = RewardScheduleV3::from_raw(repeat_raw, None).expect("repeat");
    measure("v3 repeat interval accumulation", || {
        repeat
            .resources_earned_between(1_000_000, 2_000_000)
            .expect("repeat interval")
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
