use std::num::NonZeroU64;

use serde::Serialize;

use crate::fingerprint::{CanonicalNode, object};
use crate::{CoreError, ProbabilityRatio, TargetIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrimitiveAcquisition {
    #[serde(rename = "current_featured_target_acquired")]
    CurrentFeaturedTarget,
    #[serde(rename = "other_configured_target_acquired")]
    OtherConfiguredTarget { target_index: TargetIndex },
    #[serde(rename = "no_configured_target_acquired")]
    NoConfiguredTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledOutcomeBranch {
    pub acquisition: PrimitiveAcquisition,
    pub canonical_weight: u64,
    pub upper_exclusive: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledOutcomeDistribution {
    denominator: NonZeroU64,
    branches: Vec<CompiledOutcomeBranch>,
    canonical_featured_weight: u64,
    canonical_other_weights: Vec<(TargetIndex, u64)>,
    canonical_residual_weight: u64,
}

impl CompiledOutcomeDistribution {
    pub fn compile(
        featured_probability: ProbabilityRatio,
        denominator: u64,
        ordered_other_weights: &[(TargetIndex, u64)],
    ) -> Result<Self, CoreError> {
        let denominator = NonZeroU64::new(denominator).ok_or_else(|| {
            CoreError::validation(None, "categorical denominator must be positive")
        })?;
        let featured_denominator = featured_probability.denominator();
        if denominator.get() % featured_denominator != 0 {
            return Err(CoreError::validation(
                None,
                format!(
                    "categorical denominator {} is not divisible by featured denominator {featured_denominator}",
                    denominator.get()
                ),
            ));
        }
        let featured_weight_u128 = u128::from(featured_probability.numerator())
            .checked_mul(u128::from(denominator.get() / featured_denominator))
            .ok_or(CoreError::ArithmeticOverflow {
                context: "scaling featured categorical weight",
            })?;
        let mut total = featured_weight_u128;
        for (_, weight) in ordered_other_weights {
            total =
                total
                    .checked_add(u128::from(*weight))
                    .ok_or(CoreError::ArithmeticOverflow {
                        context: "summing categorical weights",
                    })?;
        }
        if total > u128::from(denominator.get()) {
            return Err(CoreError::validation(
                None,
                format!(
                    "featured and other-target categorical weights {total} exceed denominator {}",
                    denominator.get()
                ),
            ));
        }
        let residual_u128 = u128::from(denominator.get()) - total;
        let featured_weight =
            u64::try_from(featured_weight_u128).map_err(|_| CoreError::ArithmeticOverflow {
                context: "converting featured categorical weight",
            })?;
        let residual_weight =
            u64::try_from(residual_u128).map_err(|_| CoreError::ArithmeticOverflow {
                context: "converting residual categorical weight",
            })?;

        let mut divisor = denominator.get();
        divisor = gcd(divisor, featured_weight);
        for (_, weight) in ordered_other_weights {
            divisor = gcd(divisor, *weight);
        }
        divisor = gcd(divisor, residual_weight);
        if divisor == 0 {
            return Err(CoreError::InternalInvariant {
                message: "joint categorical GCD became zero".to_owned(),
            });
        }

        let canonical_denominator =
            NonZeroU64::new(denominator.get() / divisor).ok_or(CoreError::InternalInvariant {
                message: "categorical normalization produced a zero denominator".to_owned(),
            })?;
        let canonical_featured_weight = featured_weight / divisor;
        let canonical_other_weights = ordered_other_weights
            .iter()
            .map(|(target, weight)| (*target, *weight / divisor))
            .collect::<Vec<_>>();
        let canonical_residual_weight = residual_weight / divisor;

        let mut cumulative = 0_u64;
        let mut branches = Vec::with_capacity(canonical_other_weights.len() + 2);
        push_branch(
            &mut branches,
            &mut cumulative,
            PrimitiveAcquisition::CurrentFeaturedTarget,
            canonical_featured_weight,
        )?;
        for (target_index, weight) in &canonical_other_weights {
            push_branch(
                &mut branches,
                &mut cumulative,
                PrimitiveAcquisition::OtherConfiguredTarget {
                    target_index: *target_index,
                },
                *weight,
            )?;
        }
        push_branch(
            &mut branches,
            &mut cumulative,
            PrimitiveAcquisition::NoConfiguredTarget,
            canonical_residual_weight,
        )?;
        if cumulative != canonical_denominator.get() || branches.is_empty() {
            return Err(CoreError::InternalInvariant {
                message: "canonical categorical endpoints do not conserve probability".to_owned(),
            });
        }

        Ok(Self {
            denominator: canonical_denominator,
            branches,
            canonical_featured_weight,
            canonical_other_weights,
            canonical_residual_weight,
        })
    }

    #[must_use]
    pub const fn denominator(&self) -> NonZeroU64 {
        self.denominator
    }

    #[must_use]
    pub fn branches(&self) -> &[CompiledOutcomeBranch] {
        &self.branches
    }

    #[must_use]
    pub fn contains(&self, acquisition: PrimitiveAcquisition) -> bool {
        self.branches
            .iter()
            .any(|branch| branch.acquisition == acquisition)
    }

    #[must_use]
    pub fn canonical_featured_weight(&self) -> u64 {
        self.canonical_featured_weight
    }

    #[must_use]
    pub fn canonical_other_weights(&self) -> &[(TargetIndex, u64)] {
        &self.canonical_other_weights
    }

    #[must_use]
    pub fn canonical_residual_weight(&self) -> u64 {
        self.canonical_residual_weight
    }

    #[must_use]
    pub fn canonical_node(&self) -> CanonicalNode {
        object([
            (
                "denominator",
                CanonicalNode::Integer(self.denominator.get()),
            ),
            (
                "featured_weight",
                CanonicalNode::Integer(self.canonical_featured_weight),
            ),
            (
                "other_target_weights",
                CanonicalNode::Array(
                    self.canonical_other_weights
                        .iter()
                        .map(|(target, weight)| {
                            object([
                                (
                                    "target_index",
                                    CanonicalNode::Integer(u64::from(target.get())),
                                ),
                                ("weight", CanonicalNode::Integer(*weight)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "residual_weight",
                CanonicalNode::Integer(self.canonical_residual_weight),
            ),
        ])
    }
}

fn push_branch(
    branches: &mut Vec<CompiledOutcomeBranch>,
    cumulative: &mut u64,
    acquisition: PrimitiveAcquisition,
    weight: u64,
) -> Result<(), CoreError> {
    if weight == 0 {
        return Ok(());
    }
    *cumulative = cumulative
        .checked_add(weight)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "constructing categorical cumulative endpoint",
        })?;
    branches.push(CompiledOutcomeBranch {
        acquisition,
        canonical_weight: weight,
        upper_exclusive: *cumulative,
    });
    Ok(())
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
