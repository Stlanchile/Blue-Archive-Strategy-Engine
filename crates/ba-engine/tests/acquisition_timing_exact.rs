mod timing_common;
use ba_core::*;
use ba_engine::*;
use timing_common::*;

#[test]
fn rational_path_enumeration_matches_sparse_marginals_and_partial_success() {
    let b = oracle_bundle();
    let report = exact(&b);
    let mut bins = [[0_u64; 4]; 2];
    let mut missing = [0_u64; 2];
    let mut complete = 0_u64;
    for mut code in 0..256_u64 {
        let mut draws = Vec::new();
        for _ in 0..4 {
            draws.push(match code % 4 {
                0 => PrimitiveAcquisition::CurrentFeaturedTarget,
                1 => PrimitiveAcquisition::OtherConfiguredTarget {
                    target_index: TargetIndex::new(1, 2).unwrap(),
                },
                _ => PrimitiveAcquisition::NoConfiguredTarget,
            });
            code /= 4;
        }
        let trace = replay_v3(&b, &draws).unwrap();
        let times = observations(&trace);
        for (index, time) in times.iter().enumerate() {
            if let Some(n) = time {
                bins[index][*n as usize - 1] = bins[index][*n as usize - 1].checked_add(1).unwrap();
            } else {
                missing[index] = missing[index].checked_add(1).unwrap();
            }
        }
        if times.iter().all(Option::is_some) {
            complete = complete.checked_add(1).unwrap();
        }
    }
    assert_eq!(complete, 110);
    assert_eq!(
        report.analysis.all_target_success_probability,
        110.0 / 256.0
    );
    for (index, target) in report.acquisition_timing.targets.iter().enumerate() {
        assert_eq!(bins[index], [64, 48, 36, 27]);
        assert_eq!(missing[index], 81);
        assert_eq!(target.acquired_by_terminal_probability, 175.0 / 256.0);
        assert_eq!(target.not_acquired_by_terminal_probability, 81.0 / 256.0);
        let mut total = 0;
        for (point, cdf) in target.pmf.iter().zip(&target.cdf) {
            let n = point.additional_recruitment_count as usize;
            total += bins[index][n - 1];
            assert_eq!(point.probability, bins[index][n - 1] as f64 / 256.0);
            assert_eq!(cdf.probability, total as f64 / 256.0);
        }
    }
    assert_legacy_eq(
        &report.analysis,
        &analyze_exact_v3(&b, ExactSolverOptions::default()).unwrap(),
    );
}

#[test]
fn distinct_first_acquisitions_merge_without_changing_future_state() {
    let b = oracle_bundle();
    let StrategyDecision::Act(action) = decide_v3(&b, &initial_world_v3(&b)).unwrap() else {
        panic!("action")
    };
    let (start, _) = begin_action_v3(&b, &initial_world_v3(&b), &action).unwrap();
    let f = PrimitiveAcquisition::CurrentFeaturedTarget;
    let n = PrimitiveAcquisition::NoConfiguredTarget;
    let o = PrimitiveAcquisition::OtherConfiguredTarget {
        target_index: TargetIndex::new(1, 2).unwrap(),
    };
    let advance = |draws: &[PrimitiveAcquisition]| {
        draws.iter().fold(start.clone(), |state, outcome| {
            apply_primitive_transition_v3(&b, &state, *outcome)
                .unwrap()
                .state
        })
    };
    assert_eq!(advance(&[f, n, f]), advance(&[n, n, f]));
    assert_eq!(advance(&[o, n, f, o]), advance(&[n, n, f, o]));
    let report = exact(&b);
    assert_eq!(
        report.acquisition_timing.targets[0].pmf[0].probability,
        0.25
    );
    assert_eq!(
        report.acquisition_timing.targets[0].pmf[2].probability,
        36.0 / 256.0
    );
    assert_legacy_eq(
        &report.analysis,
        &analyze_exact_v3(&b, ExactSolverOptions::default()).unwrap(),
    );
}

#[test]
fn initial_non_prefix_ownership_and_empty_support_and_absolute_endpoint() {
    let (r, w, mut s) = fixture_values();
    s["initial_owned_targets"] = serde_json::json!(["target_b"]);
    s["initial_recruitment_count"] = serde_json::json!(u64::MAX - 4);
    let report = exact(&compile(r.clone(), w.clone(), s.clone()));
    let initial = &report.acquisition_timing.targets[1];
    assert!(initial.initially_owned);
    assert_eq!(initial.pmf.len(), 1);
    assert_eq!(initial.pmf[0].additional_recruitment_count, 0);
    assert_eq!(initial.pmf[0].probability, 1.0);
    assert_eq!(
        report.acquisition_timing.targets[0].pmf[3].absolute_campaign_recruitment_count,
        u64::MAX
    );
    s["initial_resources"]["limited_ten_recruitment_tickets"] = serde_json::json!(0);
    let report = exact(&compile(r.clone(), w.clone(), s.clone()));
    assert!(report.acquisition_timing.targets[0].pmf.is_empty());
    assert_eq!(
        report.acquisition_timing.targets[0].not_acquired_by_terminal_probability,
        1.0
    );
    s["initial_owned_targets"] = serde_json::json!(["target_a", "target_b"]);
    assert!(
        exact(&compile(r, w, s))
            .acquisition_timing
            .targets
            .iter()
            .all(|t| t.pmf.len() == 1 && t.pmf[0].probability == 1.0)
    );
}

#[test]
fn support_insertion_guards_and_existing_solver_guards_are_effective() {
    let b = oracle_bundle();
    let run = |options, timing| analyze_exact_v3_with_acquisition_timing(&b, options, timing);
    assert!(run(ExactSolverOptions::default(), low_limit(8)).is_ok());
    assert!(matches!(
        run(ExactSolverOptions::default(), low_limit(7)),
        Err(EngineError::AcquisitionTimingSupportLimitExceeded {
            observed: 8,
            maximum: 7
        })
    ));
    assert!(matches!(
        run(
            ExactSolverOptions {
                max_transition_expansions: 1,
                ..Default::default()
            },
            low_limit(8)
        ),
        Err(EngineError::SolverTransitionLimitExceeded { .. })
    ));
    let (r, w, mut s) = fixture_values();
    s["initial_owned_targets"] = serde_json::json!(["target_a", "target_b"]);
    assert!(matches!(
        analyze_exact_v3_with_acquisition_timing(
            &compile(r, w, s),
            Default::default(),
            low_limit(1)
        ),
        Err(EngineError::AcquisitionTimingSupportLimitExceeded { .. })
    ));
}

#[test]
fn every_shipped_v3_example_retains_complete_legacy_result_and_conservation() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/v3_calibration.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let b = shipped(case["path"].as_str().unwrap());
        let timing = exact(&b);
        assert_legacy_eq(
            &timing.analysis,
            &analyze_exact_v3(&b, Default::default()).unwrap(),
        );
        for target in timing.acquisition_timing.targets {
            assert!(
                (target.acquired_by_terminal_probability
                    + target.not_acquired_by_terminal_probability
                    - 1.0)
                    .abs()
                    < 1e-12
            );
        }
    }
}

#[test]
fn one_target_timing_equals_v2_first_success() {
    let v3 = exact(&shipped(
        "scenarios/golden/v3_single_cross_target_zero.json",
    ));
    let v2 = analyze_exact(
        &load_bundle(
            root("data"),
            root("scenarios/golden/single_target_200.json"),
        )
        .unwrap(),
        Default::default(),
    )
    .unwrap();
    let left: Vec<_> = v3.acquisition_timing.targets[0]
        .pmf
        .iter()
        .map(|p| (p.additional_recruitment_count, p.probability))
        .collect();
    let right: Vec<_> = v2
        .first_success_pmf
        .iter()
        .map(|p| (p.recruitment_count, p.probability))
        .collect();
    assert_eq!(left, right);
}

#[test]
fn internally_positive_tails_survive_public_underflow() {
    let (r, w, s) = long_single_values(1200);
    let report = exact(&compile(r, w, s));
    let target = &report.acquisition_timing.targets[0];
    assert_eq!(target.pmf.len(), 1200);
    for count in 1022..=1074 {
        assert_eq!(
            target.pmf[count - 1].probability,
            f64::from_bits(1_u64 << (1074 - count)),
            "representable tail at draw {count}"
        );
    }
    assert_eq!(target.pmf[1074].probability, 0.0);
    assert_eq!(target.pmf[1199].probability, 0.0);
    assert_eq!(target.cdf[1199].probability, 1.0);
}

#[test]
fn representable_unacquired_tail_is_preserved_independently_of_rounded_cdf() {
    let (r, w, mut s) = long_single_values(1200);
    s["strategy"]["max_additional_recruitments"] = serde_json::json!(1074);
    let report = exact(&compile(r, w, s));
    let target = &report.acquisition_timing.targets[0];
    assert_eq!(target.acquired_by_terminal_probability, 1.0);
    assert_eq!(
        target.not_acquired_by_terminal_probability,
        f64::from_bits(1)
    );
    assert_eq!(target.pmf.last().unwrap().probability, f64::from_bits(1));
}

#[test]
fn funding_orders_action_fit_and_reward_boundaries_preserve_observations() {
    let (r, mut w, mut s) = fixture_values();
    s["initial_recruitment_count"] = serde_json::json!(1_000_000);
    s["initial_resources"]["pyroxene"] = serde_json::json!(240);
    s["strategy"]["max_additional_recruitments"] = serde_json::json!(8);
    w["initial_milestones"] =
        serde_json::json!([{"count":999,"rewards":[{"resource":"eligma","quantity":99}]}]);
    w["repeating_cycle"] = serde_json::json!({"starts_after_count":1000,"period":4,"milestones":[{"offset":2,"rewards":[{"resource":"limited_ten_recruitment_tickets","quantity":1},{"resource":"eligma","quantity":3}]}]});
    for priority in [["ticket_ten", "paid_single"], ["paid_single", "ticket_ten"]] {
        s["strategy"]["funding_priority"] = serde_json::json!(priority);
        let b = compile(r.clone(), w.clone(), s.clone());
        let report = exact(&b);
        assert_legacy_eq(
            &report.analysis,
            &analyze_exact_v3(&b, Default::default()).unwrap(),
        );
        let m = sampled(&b, 32, 42);
        assert_legacy_eq(
            &m.analysis,
            &simulate_monte_carlo_v3(&b, std::num::NonZeroU64::new(32).unwrap(), 42).unwrap(),
        );
        assert!(report.analysis.expected_milestone_rewards_acquired.eligma <= 6.0);
        for seed in 0..16 {
            let trace = simulate_trace_v3(&b, seed).unwrap();
            let m = sampled(&b, 1, seed);
            assert_eq!(
                observations(&trace),
                m.acquisition_timing
                    .targets
                    .iter()
                    .map(|t| t.pmf.first().map(|p| p.additional_recruitment_count))
                    .collect::<Vec<_>>()
            );
        }
    }
    s["strategy"]["max_additional_recruitments"] = serde_json::json!(3);
    s["initial_resources"]["pyroxene"] = serde_json::json!(0);
    assert!(
        exact(&compile(r, w, s))
            .acquisition_timing
            .targets
            .iter()
            .all(|t| t.pmf.is_empty())
    );
}

#[test]
fn enabled_timing_preserves_corrected_ticket_accounting_and_real_overflow() {
    let (r, mut w, mut s) = fixture_values();
    w["initial_milestones"] = serde_json::json!([{"count":2,"rewards":[{"resource":"limited_ten_recruitment_tickets","quantity":1}]}]);
    s["initial_resources"]["limited_ten_recruitment_tickets"] = serde_json::json!(u64::MAX);
    let b = compile(r.clone(), w.clone(), s.clone());
    assert!(
        analyze_exact_v3_with_acquisition_timing(&b, Default::default(), Default::default())
            .is_ok()
    );
    assert!(
        simulate_monte_carlo_v3_with_acquisition_timing(
            &b,
            std::num::NonZeroU64::new(2).unwrap(),
            42,
            Default::default(),
            Default::default()
        )
        .is_ok()
    );
    s["initial_resources"]["pyroxene"] = serde_json::json!(480);
    s["strategy"]["funding_priority"] = serde_json::json!(["paid_single", "ticket_ten"]);
    let b = compile(r, w, s);
    assert!(matches!(
        analyze_exact_v3_with_acquisition_timing(&b, Default::default(), Default::default()),
        Err(EngineError::ArithmeticOverflow { .. })
    ));
}

#[test]
fn scale_equivalent_large_denominators_and_impossible_branches_have_same_timing() {
    let (r, w, s) = fixture_values();
    let original = compile(r.clone(), w.clone(), s.clone());
    let mut r2 = r;
    let mut s2 = s;
    let scale = u64::MAX / 4;
    r2["ordinary_featured_target_probability"] =
        serde_json::json!({"numerator":scale,"denominator":scale*4});
    for table in s2["cross_target_probability_tables"]
        .as_array_mut()
        .unwrap()
    {
        table["ordinary"]["denominator"] = serde_json::json!(scale * 4);
        table["ordinary"]["other_target_weights"][0]["weight"] = serde_json::json!(scale);
    }
    let scaled = compile(r2, w, s2);
    assert_legacy_eq(
        &exact(&original).acquisition_timing,
        &exact(&scaled).acquisition_timing,
    );
    assert_legacy_eq(
        &sampled(&original, 32, 42).acquisition_timing,
        &sampled(&scaled, 32, 42).acquisition_timing,
    );
}

#[test]
fn enormous_authored_horizon_with_no_resources_stays_sparse() {
    let (r, w, mut s) = fixture_values();
    s["strategy"]["max_additional_recruitments"] = serde_json::json!(u64::MAX);
    s["initial_resources"]["limited_ten_recruitment_tickets"] = serde_json::json!(0);
    let b = compile(r, w, s);
    let report = exact(&b);
    assert!(
        report
            .acquisition_timing
            .targets
            .iter()
            .all(|t| t.pmf.is_empty())
    );
    let c = compare_v3_with_acquisition_timing(
        &b,
        std::num::NonZeroU64::new(1).unwrap(),
        42,
        Default::default(),
        Default::default(),
        low_limit(4),
    )
    .unwrap();
    for t in c.acquisition_timing.comparisons {
        assert_eq!(t.points.len(), 2);
        assert_eq!(t.points[1].absolute_campaign_recruitment_count, u64::MAX);
    }
}
