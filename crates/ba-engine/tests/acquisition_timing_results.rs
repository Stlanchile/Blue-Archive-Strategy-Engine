mod timing_common;
use ba_engine::*;
use std::num::{NonZeroU64, NonZeroUsize};
use timing_common::*;

fn fields(value: &impl serde::Serialize, names: &[&str]) {
    let json = serde_json::to_value(value).unwrap();
    assert_eq!(json.as_object().unwrap().len(), names.len());
    let wire = serde_json::to_string(value).unwrap();
    let mut cursor = 0;
    for name in names {
        assert!(json.get(name).is_some(), "{name}");
        cursor += wire[cursor..].find(&format!("\"{name}\":")).unwrap() + name.len() + 3;
    }
}
#[test]
fn concrete_schema_four_field_sets_and_declaration_order_are_explicit() {
    let b = oracle_bundle();
    let e = exact(&b);
    let m = sampled(&b, 11, 42);
    let c = compare_v3_with_acquisition_timing(
        &b,
        NonZeroU64::new(11).unwrap(),
        42,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    for value in [
        &serde_json::to_value(&e).unwrap(),
        &serde_json::to_value(&m).unwrap(),
        &serde_json::to_value(&c).unwrap(),
    ] {
        assert_eq!(value["result_schema_version"], 4);
    }
    assert_eq!(e.analysis.provenance.result_schema_version, 3);
    assert_eq!(m.analysis.provenance.result_schema_version, 3);
    let envelope = [
        "result_schema_version",
        "engine_kind",
        "analysis",
        "acquisition_timing",
    ];
    fields(&e, &envelope);
    fields(&m, &envelope);
    fields(&c, &envelope);
    fields(&e.acquisition_timing, &["options", "targets"]);
    fields(&m.acquisition_timing, &["options", "targets"]);
    fields(&e.acquisition_timing.options, &["max_support_points"]);
    let ids = [
        "target_index",
        "target_id",
        "initially_owned",
        "acquired_by_terminal_probability",
        "not_acquired_by_terminal_probability",
    ];
    let exact_names = [ids.as_slice(), &["pmf", "cdf"]].concat();
    let sampled_names = [
        ids.as_slice(),
        &[
            "acquired_sample_count",
            "not_acquired_sample_count",
            "pmf",
            "cdf",
        ],
    ]
    .concat();
    for t in &e.acquisition_timing.targets {
        fields(t, &exact_names);
        for p in t.pmf.iter().chain(&t.cdf) {
            fields(
                p,
                &[
                    "additional_recruitment_count",
                    "absolute_campaign_recruitment_count",
                    "probability",
                ],
            );
        }
    }
    for t in &m.acquisition_timing.targets {
        fields(t, &sampled_names);
        for p in t.pmf.iter().chain(&t.cdf) {
            fields(
                p,
                &[
                    "additional_recruitment_count",
                    "absolute_campaign_recruitment_count",
                    "probability",
                    "sample_count",
                    "confidence_interval_95",
                ],
            );
            fields(&p.confidence_interval_95, &["lower", "upper"]);
        }
    }
    fields(
        &c.acquisition_timing,
        &["exact", "monte_carlo", "comparisons"],
    );
    for t in &c.acquisition_timing.comparisons {
        fields(
            t,
            &[
                "target_index",
                "target_id",
                "not_acquired_simulation_minus_exact",
                "exact_not_acquired_within_monte_carlo_interval",
                "points",
            ],
        );
        for p in &t.points {
            fields(
                p,
                &[
                    "additional_recruitment_count",
                    "absolute_campaign_recruitment_count",
                    "pmf_simulation_minus_exact",
                    "cdf_simulation_minus_exact",
                    "exact_pmf_within_monte_carlo_interval",
                    "exact_cdf_within_monte_carlo_interval",
                ],
            );
        }
    }
}
#[test]
fn options_are_validated_and_new_failures_are_engine_class() {
    assert_eq!(
        AcquisitionTimingOptions::default().max_support_points(),
        65536
    );
    assert!(AcquisitionTimingOptions::new(NonZeroUsize::new(65536).unwrap()).is_ok());
    let error = AcquisitionTimingOptions::new(NonZeroUsize::new(65537).unwrap()).unwrap_err();
    assert!(matches!(
        error,
        EngineError::InvalidAcquisitionTimingOptions { .. }
    ));
    assert_eq!(error.class(), EngineErrorClass::GuardOrInvariant);
    assert_eq!(
        EngineError::AcquisitionTimingSupportLimitExceeded {
            observed: 2,
            maximum: 1
        }
        .class(),
        EngineErrorClass::GuardOrInvariant
    );
}
#[test]
fn provenance_and_omitted_cycle_do_not_change_timing_or_run_streams() {
    let (r, w, s) = fixture_values();
    let original = compile(r.clone(), w.clone(), s.clone());
    let mut changed = r;
    changed["provenance"]["sources"] = serde_json::json!([{
        "source_id":"local_note", "source_category":"secondary_reference",
        "label":"inert \u{1b} text", "reference":"offline:test-only",
        "published_on":null, "retrieved_on":"2026-09-17", "content_sha256":null
    }]);
    let renamed = compile(changed, w.clone(), s.clone());
    assert_eq!(
        derive_run_seed_v3(&original, 42, 0),
        derive_run_seed_v3(&renamed, 42, 0)
    );
    assert_ne!(
        original.fingerprints().ruleset_document,
        renamed.fingerprints().ruleset_document
    );
    assert_legacy_eq(
        &exact(&original).acquisition_timing,
        &exact(&renamed).acquisition_timing,
    );
    assert_legacy_eq(
        &sampled(&original, 100, 42).acquisition_timing,
        &sampled(&renamed, 100, 42).acquisition_timing,
    );
    let (r, mut w, s) = fixture_values();
    w.as_object_mut().unwrap().remove("repeating_cycle");
    let omitted = compile(r, w, s);
    assert_eq!(
        original.fingerprints().reward_schedule_document,
        omitted.fingerprints().reward_schedule_document
    );
    assert_legacy_eq(&exact(&original), &exact(&omitted));
}
