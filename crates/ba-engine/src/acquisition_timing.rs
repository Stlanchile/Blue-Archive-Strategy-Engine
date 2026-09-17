//! Bounded sparse support for marginal observations; never part of a state key.
use crate::{EngineError, result_v4::*, simulation_v3::wilson_interval};
use ba_core::ValidatedScenarioBundleV3;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    num::NonZeroUsize,
};

pub const MAX_ACQUISITION_TIMING_SUPPORT_POINTS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AcquisitionTimingOptions {
    max_support_points: usize,
}

impl Default for AcquisitionTimingOptions {
    fn default() -> Self {
        Self {
            max_support_points: MAX_ACQUISITION_TIMING_SUPPORT_POINTS,
        }
    }
}

impl AcquisitionTimingOptions {
    pub fn new(max_support_points: NonZeroUsize) -> Result<Self, EngineError> {
        if max_support_points.get() > MAX_ACQUISITION_TIMING_SUPPORT_POINTS {
            return Err(EngineError::InvalidAcquisitionTimingOptions {
                requested: max_support_points.get(),
                maximum: MAX_ACQUISITION_TIMING_SUPPORT_POINTS,
            });
        }
        Ok(Self {
            max_support_points: max_support_points.get(),
        })
    }

    #[must_use]
    pub const fn max_support_points(self) -> usize {
        self.max_support_points
    }
}

pub(crate) struct SupportBudget {
    used: usize,
    options: AcquisitionTimingOptions,
}

impl SupportBudget {
    pub(crate) fn new(options: AcquisitionTimingOptions) -> Self {
        Self { used: 0, options }
    }
    fn reserve(&mut self) -> Result<(), EngineError> {
        if self.used >= self.options.max_support_points {
            return Err(EngineError::AcquisitionTimingSupportLimitExceeded {
                observed: self.used + 1,
                maximum: self.options.max_support_points,
            });
        }
        self.used += 1;
        Ok(())
    }
    pub(crate) fn entry<'a, T: Default>(
        &mut self,
        map: &'a mut BTreeMap<u64, T>,
        count: u64,
    ) -> Result<&'a mut T, EngineError> {
        match map.entry(count) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                self.reserve()?;
                Ok(entry.insert(T::default()))
            }
        }
    }
    fn insert(&mut self, support: &mut BTreeSet<u64>, count: u64) -> Result<(), EngineError> {
        if !support.contains(&count) {
            self.reserve()?;
            support.insert(count);
        }
        Ok(())
    }
}

pub(crate) fn newly_owned_target(
    pre: u8,
    post: u8,
    targets: usize,
) -> Result<Option<usize>, EngineError> {
    let new = post & !pre;
    if pre & !post != 0 || targets > 4 || post >> targets != 0 || new.count_ones() > 1 {
        return Err(EngineError::InternalInvariantViolation {
            message: "acquisition timing observed inconsistent ownership bits".to_owned(),
        });
    }
    Ok((new != 0).then(|| new.trailing_zeros() as usize))
}

pub(crate) fn compare_timing(
    bundle: &ValidatedScenarioBundleV3,
    exact: &ExactAcquisitionTimingV4,
    sampled: &MonteCarloAcquisitionTimingV4,
    runs: u64,
    options: AcquisitionTimingOptions,
) -> Result<Vec<TargetAcquisitionTimingComparisonV4>, EngineError> {
    let mut budget = SupportBudget::new(options);
    let mut comparisons = Vec::new();
    for (exact, sampled) in exact.targets.iter().zip(&sampled.targets) {
        let mut support = BTreeSet::new();
        for count in [
            0,
            bundle.compiled_strategy().max_additional_recruitments.get(),
        ]
        .into_iter()
        .chain(exact.pmf.iter().map(|p| p.additional_recruitment_count))
        .chain(sampled.pmf.iter().map(|p| p.additional_recruitment_count))
        {
            budget.insert(&mut support, count)?;
        }
        let mut points = Vec::with_capacity(support.len());
        let (mut ei, mut si) = (0, 0);
        let (mut ecdf, mut scdf) = (0.0, 0);
        for count in support {
            let (mut epmf, mut spmf) = (0.0, 0);
            if let Some(p) = exact
                .pmf
                .get(ei)
                .filter(|p| p.additional_recruitment_count == count)
            {
                epmf = p.probability;
                ecdf = exact.cdf[ei].probability;
                ei += 1;
            }
            if let Some(p) = sampled
                .pmf
                .get(si)
                .filter(|p| p.additional_recruitment_count == count)
            {
                spmf = p.sample_count;
                scdf = sampled.cdf[si].sample_count;
                si += 1;
            }
            let pi = wilson_interval(spmf, runs);
            let ci = wilson_interval(scdf, runs);
            points.push(AcquisitionTimingComparisonPointV4 {
                additional_recruitment_count: count,
                absolute_campaign_recruitment_count: bundle
                    .scenario()
                    .absolute_campaign_count(count)?,
                pmf_simulation_minus_exact: spmf as f64 / runs as f64 - epmf,
                cdf_simulation_minus_exact: scdf as f64 / runs as f64 - ecdf,
                exact_pmf_within_monte_carlo_interval: (pi.lower..=pi.upper).contains(&epmf),
                exact_cdf_within_monte_carlo_interval: (ci.lower..=ci.upper).contains(&ecdf),
            });
        }
        let interval = wilson_interval(sampled.not_acquired_sample_count, runs);
        comparisons.push(TargetAcquisitionTimingComparisonV4 {
            target_index: exact.target_index,
            target_id: exact.target_id.clone(),
            not_acquired_simulation_minus_exact: sampled.not_acquired_by_terminal_probability
                - exact.not_acquired_by_terminal_probability,
            exact_not_acquired_within_monte_carlo_interval: (interval.lower..=interval.upper)
                .contains(&exact.not_acquired_by_terminal_probability),
            points,
        });
    }
    Ok(comparisons)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_support_budget_is_shared_across_targets_and_checks_before_insertion() {
        let mut maps: [BTreeMap<u64, u64>; 4] = std::array::from_fn(|_| BTreeMap::new());
        let mut budget = SupportBudget::new(AcquisitionTimingOptions::default());
        for count in 0..16_384 {
            for map in &mut maps {
                *budget.entry(map, count).unwrap() += 1;
            }
        }
        *budget.entry(&mut maps[0], 0).unwrap() += 1;
        assert_eq!(maps[0][&0], 2);
        assert!(matches!(
            budget.entry(&mut maps[0], 16_384),
            Err(EngineError::AcquisitionTimingSupportLimitExceeded {
                observed: 65537,
                maximum: 65536
            })
        ));
        assert_eq!(maps.iter().map(BTreeMap::len).sum::<usize>(), 65536);
    }
    #[test]
    fn inconsistent_kernel_ownership_facts_are_typed_internal_errors() {
        for (pre, post, n) in [(1, 0, 2), (0, 3, 2), (0, 4, 2), (0, 0, 5)] {
            assert!(matches!(
                newly_owned_target(pre, post, n),
                Err(EngineError::InternalInvariantViolation { .. })
            ));
        }
    }
}
