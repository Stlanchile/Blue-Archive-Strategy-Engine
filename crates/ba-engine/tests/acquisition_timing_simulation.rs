mod timing_common;
use ba_core::*;
use ba_engine::*;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use std::{collections::BTreeMap, num::NonZeroU64};
use timing_common::*;

#[test]
fn sampled_histograms_match_independently_driven_kernel_runs_and_trace_replay() {
    let b = oracle_bundle();
    let runs = 128;
    let report = sampled(&b, runs, 42);
    let mut bins = [BTreeMap::<u64, u64>::new(), BTreeMap::new()];
    for index in 0..runs {
        // Independent inverse CDF for the four equal elementary symbols.
        let mut rng = ChaCha8Rng::from_seed(derive_run_seed_v3(&b, 42, index));
        let outcomes: Vec<_> = (0..4)
            .map(|_| match rng.next_u64() % 4 {
                0 => PrimitiveAcquisition::CurrentFeaturedTarget,
                1 => PrimitiveAcquisition::OtherConfiguredTarget {
                    target_index: TargetIndex::new(1, 2).unwrap(),
                },
                _ => PrimitiveAcquisition::NoConfiguredTarget,
            })
            .collect();
        let replay = replay_v3(&b, &outcomes).unwrap();
        for (target, time) in observations(&replay).into_iter().enumerate() {
            if let Some(n) = time {
                *bins[target].entry(n).or_default() += 1;
            }
        }
    }
    for (target, bins) in report.acquisition_timing.targets.iter().zip(bins) {
        assert_eq!(
            target
                .pmf
                .iter()
                .map(|p| (p.additional_recruitment_count, p.sample_count))
                .collect::<BTreeMap<_, _>>(),
            bins
        );
        assert_eq!(
            target.acquired_sample_count + target.not_acquired_sample_count,
            runs
        );
    }
    for seed in 0..32 {
        let trace = simulate_trace_v3(&b, seed).unwrap();
        let replay = replay_v3(&b, &trace.replay_outcomes).unwrap();
        assert_eq!(observations(&trace), observations(&replay));
        let one = sampled(&b, 1, seed);
        for (time, target) in observations(&trace)
            .iter()
            .zip(&one.acquisition_timing.targets)
        {
            assert_eq!(
                *time,
                target.pmf.first().map(|p| p.additional_recruitment_count)
            );
            assert_eq!(target.acquired_sample_count, u64::from(time.is_some()));
        }
    }
    assert_legacy_eq(
        &report.analysis,
        &simulate_monte_carlo_v3(&b, NonZeroU64::new(runs).unwrap(), 42).unwrap(),
    );
}

#[test]
fn sampled_legacy_aggregates_stay_byte_identical_for_all_shipped_v3_cases() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/v3_calibration.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let b = shipped(case["path"].as_str().unwrap());
        let report = sampled(&b, 11, 42);
        assert_legacy_eq(
            &report.analysis,
            &simulate_monte_carlo_v3(&b, NonZeroU64::new(11).unwrap(), 42).unwrap(),
        );
    }
}

#[test]
fn comparison_aligns_absent_bins_and_held_cdfs_and_retains_legacy_analysis() {
    let b = oracle_bundle();
    let result = compare_v3_with_acquisition_timing(
        &b,
        NonZeroU64::new(1).unwrap(),
        42,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    assert_legacy_eq(
        &result.analysis,
        &compare_v3(&b, NonZeroU64::new(1).unwrap(), 42).unwrap(),
    );
    for ((comparison, exact), sampled) in result
        .acquisition_timing
        .comparisons
        .iter()
        .zip(&result.acquisition_timing.exact.targets)
        .zip(&result.acquisition_timing.monte_carlo.targets)
    {
        assert_eq!(
            comparison
                .points
                .iter()
                .map(|p| p.additional_recruitment_count)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        for p in &comparison.points {
            let e = exact
                .pmf
                .iter()
                .find(|e| e.additional_recruitment_count == p.additional_recruitment_count)
                .map_or(0.0, |e| e.probability);
            let s = sampled
                .pmf
                .iter()
                .find(|e| e.additional_recruitment_count == p.additional_recruitment_count)
                .map_or(0.0, |e| e.probability);
            let ec = exact
                .cdf
                .iter()
                .rev()
                .find(|e| e.additional_recruitment_count <= p.additional_recruitment_count)
                .map_or(0.0, |e| e.probability);
            let sc = sampled
                .cdf
                .iter()
                .rev()
                .find(|e| e.additional_recruitment_count <= p.additional_recruitment_count)
                .map_or(0.0, |e| e.probability);
            assert_eq!(p.pmf_simulation_minus_exact, s - e);
            assert_eq!(p.cdf_simulation_minus_exact, sc - ec);
        }
    }
}

#[test]
fn support_and_work_guards_fail_without_fallback_and_exact_runs_first() {
    let b = oracle_bundle();
    let runs = NonZeroU64::new(100).unwrap();
    assert!(matches!(
        simulate_monte_carlo_v3_with_acquisition_timing(
            &b,
            runs,
            42,
            Default::default(),
            low_limit(1)
        ),
        Err(EngineError::AcquisitionTimingSupportLimitExceeded { .. })
    ));
    assert!(matches!(
        simulate_monte_carlo_v3_with_acquisition_timing(
            &b,
            runs,
            42,
            SimulationLimits {
                max_total_primitive_transitions: 1,
                ..Default::default()
            },
            Default::default()
        ),
        Err(EngineError::SimulationPrimitiveLimitExceeded { .. })
    ));
    assert!(matches!(
        compare_v3_with_acquisition_timing(
            &b,
            runs,
            42,
            ExactSolverOptions {
                max_transition_expansions: 1,
                ..Default::default()
            },
            SimulationLimits {
                max_runs: 1,
                ..Default::default()
            },
            Default::default()
        ),
        Err(EngineError::SolverTransitionLimitExceeded { .. })
    ));
    // Both datasets fit in 8 keys; comparison needs 10 after adding time zero.
    assert!(matches!(
        compare_v3_with_acquisition_timing(
            &b,
            runs,
            42,
            Default::default(),
            Default::default(),
            low_limit(8)
        ),
        Err(EngineError::AcquisitionTimingSupportLimitExceeded {
            observed: 9,
            maximum: 8
        })
    ));
    assert!(
        compare_v3_with_acquisition_timing(
            &b,
            runs,
            42,
            Default::default(),
            Default::default(),
            low_limit(10)
        )
        .is_ok()
    );
}

#[test]
fn certain_initial_and_impossible_outcomes_have_inclusive_intervals() {
    let (r, w, mut s) = fixture_values();
    s["initial_resources"]["limited_ten_recruitment_tickets"] = serde_json::json!(0);
    s["initial_owned_targets"] = serde_json::json!(["target_b"]);
    let b = compile(r, w, s);
    let result = compare_v3_with_acquisition_timing(
        &b,
        NonZeroU64::new(11).unwrap(),
        42,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    assert!(result.acquisition_timing.comparisons.iter().all(|t| {
        t.exact_not_acquired_within_monte_carlo_interval
            && t.points.iter().all(|p| {
                p.exact_pmf_within_monte_carlo_interval && p.exact_cdf_within_monte_carlo_interval
            })
    }));
}
