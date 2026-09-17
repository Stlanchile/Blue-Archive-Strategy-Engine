use ba_core::ValidatedScenarioBundleV3;
use ba_engine::*;
use std::{hint::black_box, num::NonZeroU64, time::Instant};
#[path = "../../tests/timing_common/mod.rs"]
mod common;

fn alternating<A, B>(label: &str, disabled: impl Fn() -> A, enabled: impl Fn() -> B) {
    paired(label, ["disabled", "enabled"], disabled, enabled);
}
fn paired<A, B>(label: &str, names: [&str; 2], disabled: impl Fn() -> A, enabled: impl Fn() -> B) {
    black_box(disabled());
    black_box(enabled());
    let mut off = Vec::new();
    let mut on = Vec::new();
    for iteration in 0..3 {
        let mut run_off = || {
            let start = Instant::now();
            black_box(disabled());
            off.push(start.elapsed());
        };
        let mut run_on = || {
            let start = Instant::now();
            black_box(enabled());
            on.push(start.elapsed());
        };
        if iteration % 2 == 0 {
            run_off();
            run_on();
        } else {
            run_on();
            run_off();
        }
    }
    off.sort();
    on.sort();
    println!(
        "timing {label}: {}={:?} {}={:?}",
        names[0], off[1], names[1], on[1]
    );
}
fn case(label: &str, bundle: &ValidatedScenarioBundleV3) {
    let exact = || analyze_exact_v3(bundle, Default::default()).unwrap();
    let timing = || {
        analyze_exact_v3_with_acquisition_timing(bundle, Default::default(), Default::default())
            .unwrap()
    };
    alternating(&format!("{label} exact"), exact, timing);
    let result = timing();
    let d = &result.analysis.solver_diagnostics;
    println!(
        "counts {label} exact: boundary={} in_flight={} processed={} expansions={} keys={} json_bytes={}",
        d.peak_boundary_frontier,
        d.peak_in_flight_frontier,
        d.processed_states,
        d.transition_expansions,
        result
            .acquisition_timing
            .targets
            .iter()
            .map(|t| t.pmf.len())
            .sum::<usize>(),
        serde_json::to_vec_pretty(&result).unwrap().len() + 1
    );
    for n in [1, 10_000, 100_000] {
        let runs = NonZeroU64::new(n).unwrap();
        let off = || simulate_monte_carlo_v3(bundle, runs, 42).unwrap();
        let on = || {
            simulate_monte_carlo_v3_with_acquisition_timing(
                bundle,
                runs,
                42,
                Default::default(),
                Default::default(),
            )
            .unwrap()
        };
        alternating(&format!("{label} MC {n}"), off, on);
        let result = on();
        println!(
            "counts {label} MC {n}: primitives={} keys={} json_bytes={}",
            (result.analysis.expected_additional_primitive_recruitments * n as f64).round() as u64,
            result
                .acquisition_timing
                .targets
                .iter()
                .map(|t| t.pmf.len())
                .sum::<usize>(),
            serde_json::to_vec_pretty(&result).unwrap().len() + 1
        );
    }
    let runs = NonZeroU64::new(1).unwrap();
    alternating(
        &format!("{label} comparison sparse"),
        || compare_v3(bundle, runs, 42).unwrap(),
        || {
            compare_v3_with_acquisition_timing(
                bundle,
                runs,
                42,
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap()
        },
    );
    let result = compare_v3_with_acquisition_timing(
        bundle,
        runs,
        42,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    println!(
        "counts {label} comparison: points={} json_bytes={}",
        result
            .acquisition_timing
            .comparisons
            .iter()
            .map(|t| t.points.len())
            .sum::<usize>(),
        serde_json::to_vec_pretty(&result).unwrap().len() + 1
    );
}
pub fn run() {
    for name in [
        "single_target_v3",
        "dual_target_shared_paid_first_v3",
        "dual_target_independent_ticket_first_v3",
        "three_target_mixed_cross_acquisition_v3",
        "four_target_independent_simulation_v3",
    ] {
        case(
            name,
            &common::shipped(&format!("scenarios/examples/{name}.json")),
        );
    }
    case(
        "atomic_cross_target",
        &common::shipped("scenarios/golden/v3_atomic_cross_target.json"),
    );
    let oracle = common::oracle_bundle();
    case("merge_oracle", &oracle);
    let d = analyze_exact_v3(&oracle, Default::default())
        .unwrap()
        .solver_diagnostics;
    let guard = ExactSolverOptions {
        max_active_states: d.peak_boundary_frontier.max(d.peak_in_flight_frontier),
        max_processed_states: d.processed_states,
        max_transition_expansions: d.transition_expansions,
        ..Default::default()
    };
    alternating(
        "near guard",
        || analyze_exact_v3(&oracle, guard).unwrap(),
        || analyze_exact_v3_with_acquisition_timing(&oracle, guard, Default::default()).unwrap(),
    );
    let over = ExactSolverOptions {
        max_transition_expansions: d.transition_expansions - 1,
        ..guard
    };
    alternating(
        "one expansion over guard",
        || analyze_exact_v3(&oracle, over).unwrap_err(),
        || analyze_exact_v3_with_acquisition_timing(&oracle, over, Default::default()).unwrap_err(),
    );
    paired(
        "support lower caller cap success/failure (8/7)",
        ["fits_limit", "exceeds_limit"],
        || {
            analyze_exact_v3_with_acquisition_timing(
                &oracle,
                Default::default(),
                common::low_limit(8),
            )
            .unwrap()
        },
        || {
            analyze_exact_v3_with_acquisition_timing(
                &oracle,
                Default::default(),
                common::low_limit(7),
            )
            .unwrap_err()
        },
    );
    let (r, mut w, mut s) = common::fixture_values();
    s["initial_recruitment_count"] = serde_json::json!(1_000_000_000);
    s["strategy"]["max_additional_recruitments"] = serde_json::json!(8);
    w["repeating_cycle"] = serde_json::json!({"starts_after_count":1000,"period":4,"milestones":[{"offset":2,"rewards":[{"resource":"limited_ten_recruitment_tickets","quantity":1}]}]});
    case(
        "large_initial_small_repeat_window",
        &common::compile(r, w, s),
    );
    let (mut r, w, _) = common::fixture_values();
    let mut s: serde_json::Value = serde_json::from_slice(
        &std::fs::read(common::root(
            "scenarios/golden/v3_four_target_exact_small.json",
        ))
        .unwrap(),
    )
    .unwrap();
    s["ruleset_id"] = serde_json::json!("timing_oracle");
    s["reward_schedule_id"] = serde_json::json!("timing_empty");
    for charge in s["initial_charges"].as_array_mut().unwrap() {
        charge["pre_recruitment_charge"] = serde_json::json!(0);
    }
    r["ordinary_featured_target_probability"] = serde_json::json!({"numerator":1,"denominator":4});
    for table in s["cross_target_probability_tables"].as_array_mut().unwrap() {
        table["ordinary"]["denominator"] = serde_json::json!(8);
        for weight in table["ordinary"]["other_target_weights"]
            .as_array_mut()
            .unwrap()
        {
            weight["weight"] = serde_json::json!(1);
        }
        table["threshold_overrides"]
            .as_array_mut()
            .unwrap()
            .retain(|t| t["pre_charge"] == 199);
    }
    case("four_targets_five_categories", &common::compile(r, w, s));
}
