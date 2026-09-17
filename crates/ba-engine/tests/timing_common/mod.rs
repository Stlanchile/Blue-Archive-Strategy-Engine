#![allow(dead_code)]
use ba_core::{
    AnyValidatedScenarioBundle, CompiledRulesetV3, RewardScheduleV3, ValidatedScenarioBundleV3,
    load_any_bundle,
};
use ba_engine::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn root(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}
pub fn shipped(path: &str) -> ValidatedScenarioBundleV3 {
    let AnyValidatedScenarioBundle::V3(bundle) = load_any_bundle(root("data"), root(path)).unwrap()
    else {
        panic!("v3")
    };
    bundle
}
pub fn fixture_values() -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let base = root("crates/ba-engine/tests/fixtures/acquisition_timing");
    let read = |p| serde_json::from_slice(&fs::read(base.join(p)).unwrap()).unwrap();
    (
        read("data/rulesets/rules.json"),
        read("data/rewards/rewards.json"),
        read("scenario.json"),
    )
}
pub fn compile(
    r: serde_json::Value,
    w: serde_json::Value,
    s: serde_json::Value,
) -> ValidatedScenarioBundleV3 {
    let rules = CompiledRulesetV3::from_raw(serde_json::from_value(r).unwrap(), None).unwrap();
    let rewards = RewardScheduleV3::from_raw(serde_json::from_value(w).unwrap(), None).unwrap();
    ValidatedScenarioBundleV3::from_programmatic(serde_json::from_value(s).unwrap(), rules, rewards)
        .unwrap()
}
pub fn oracle_bundle() -> ValidatedScenarioBundleV3 {
    let (r, w, s) = fixture_values();
    compile(r, w, s)
}
pub fn exact(bundle: &ValidatedScenarioBundleV3) -> ExactAcquisitionTimingResultV4 {
    analyze_exact_v3_with_acquisition_timing(
        bundle,
        ExactSolverOptions::default(),
        AcquisitionTimingOptions::default(),
    )
    .unwrap()
}
pub fn sampled(
    bundle: &ValidatedScenarioBundleV3,
    runs: u64,
    seed: u64,
) -> MonteCarloAcquisitionTimingResultV4 {
    simulate_monte_carlo_v3_with_acquisition_timing(
        bundle,
        std::num::NonZeroU64::new(runs).unwrap(),
        seed,
        SimulationLimits::default(),
        AcquisitionTimingOptions::default(),
    )
    .unwrap()
}
pub fn low_limit(n: usize) -> AcquisitionTimingOptions {
    AcquisitionTimingOptions::new(std::num::NonZeroUsize::new(n).unwrap()).unwrap()
}
pub fn observations(trace: &RunTraceResultV3) -> Vec<Option<u64>> {
    let mut times: Vec<_> = trace
        .context
        .ordered_target_ids
        .iter()
        .map(|id| {
            trace
                .context
                .initial_owned_targets
                .contains(id)
                .then_some(0)
        })
        .collect();
    for event in &trace.events {
        if let RunTraceEventV3::PrimitiveTransition(event) = event
            && event.target_newly_owned
        {
            let index = trace
                .context
                .ordered_target_ids
                .iter()
                .position(|id| Some(id) == event.acquired_target_id.as_ref())
                .unwrap();
            assert!(
                times[index]
                    .replace(event.additional_recruitment_count)
                    .is_none()
            );
        }
    }
    times
}
pub fn assert_legacy_eq(a: &impl serde::Serialize, b: &impl serde::Serialize) {
    assert_eq!(
        serde_json::to_string(a).unwrap(),
        serde_json::to_string(b).unwrap()
    );
}

pub fn long_single_values(
    horizon: u64,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let (mut r, w, mut s) = fixture_values();
    r["paid_single_cost"] = serde_json::json!(1);
    r["maximum_pre_recruitment_charge"] = serde_json::json!(horizon - 1);
    r["ordinary_featured_target_probability"] = serde_json::json!({"numerator":1,"denominator":2});
    r["threshold_overrides"] = serde_json::json!([{"pre_charge":horizon-1,"featured_target_probability":{"numerator":1,"denominator":1}}]);
    s["students"].as_array_mut().unwrap().truncate(1);
    s["banners"].as_array_mut().unwrap().truncate(1);
    s["targets"].as_array_mut().unwrap().truncate(1);
    s["cross_target_probability_tables"] = serde_json::json!([{"banner_id":"banner_a","ordinary":{"denominator":2,"other_target_weights":[]},"threshold_overrides":[{"pre_charge":horizon-1,"denominator":1,"other_target_weights":[]}]}]);
    s["initial_resources"]["limited_ten_recruitment_tickets"] = serde_json::json!(0);
    s["initial_resources"]["pyroxene"] = serde_json::json!(horizon);
    s["strategy"]["max_additional_recruitments"] = serde_json::json!(horizon);
    (r, w, s)
}
