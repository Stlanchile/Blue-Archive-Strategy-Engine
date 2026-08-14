use ba_core::{CompiledOutcomeDistribution, PrimitiveAcquisition, ProbabilityRatio, TargetIndex};

#[test]
fn joint_gcd_canonicalizes_equivalent_probability_tables() {
    let target = TargetIndex::new(1, 3).expect("target");
    let left = CompiledOutcomeDistribution::compile(
        ProbabilityRatio::new(7, 1000).expect("ratio"),
        1_000,
        &[(target, 7)],
    )
    .expect("left");
    let right = CompiledOutcomeDistribution::compile(
        ProbabilityRatio::new(7, 1000).expect("ratio"),
        10_000,
        &[(target, 70)],
    )
    .expect("right");
    assert_eq!(left, right);
    assert_eq!(left.denominator().get(), 1_000);
    assert_eq!(left.branches()[0].upper_exclusive, 7);
    assert_eq!(left.branches()[1].upper_exclusive, 14);
    assert_eq!(
        left.branches()[1].acquisition,
        PrimitiveAcquisition::OtherConfiguredTarget {
            target_index: target
        }
    );
    assert_eq!(
        left.branches().last().map(|branch| branch.upper_exclusive),
        Some(1_000)
    );
}

#[test]
fn categorical_validation_rejects_invalid_denominators_and_excess_mass() {
    let target = TargetIndex::new(1, 2).expect("target");
    assert!(
        CompiledOutcomeDistribution::compile(
            ProbabilityRatio::new(1, 2).expect("ratio"),
            999,
            &[(target, 0)]
        )
        .is_err()
    );
    assert!(
        CompiledOutcomeDistribution::compile(
            ProbabilityRatio::new(1, 1).expect("ratio"),
            1,
            &[(target, 1)]
        )
        .is_err()
    );
    assert!(
        CompiledOutcomeDistribution::compile(ProbabilityRatio::new(0, 1).expect("ratio"), 0, &[])
            .is_err()
    );
}
