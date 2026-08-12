use std::num::NonZeroU64;

use serde::Serialize;

use crate::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ProbabilityRatio {
    numerator: u64,
    denominator: NonZeroU64,
}

impl ProbabilityRatio {
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, CoreError> {
        if denominator == 0 {
            return Err(CoreError::validation(
                None,
                "probability denominator must be nonzero",
            ));
        }
        if numerator > denominator {
            return Err(CoreError::validation(
                None,
                format!("probability numerator {numerator} exceeds denominator {denominator}"),
            ));
        }
        let divisor = gcd(numerator, denominator);
        let normalized_denominator = denominator / divisor;
        let normalized_numerator = numerator / divisor;
        let normalized_nonzero =
            NonZeroU64::new(normalized_denominator).ok_or(CoreError::InternalInvariant {
                message: "ratio normalization produced a zero denominator".to_owned(),
            })?;
        Ok(Self {
            numerator: normalized_numerator,
            denominator: normalized_nonzero,
        })
    }

    #[must_use]
    pub const fn numerator(self) -> u64 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(self) -> u64 {
        self.denominator.get()
    }

    #[must_use]
    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator.get() as f64
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.numerator == 0
    }

    #[must_use]
    pub fn is_one(self) -> bool {
        self.numerator == self.denominator.get()
    }

    #[must_use]
    pub fn complement(self) -> Self {
        Self {
            numerator: self.denominator.get() - self.numerator,
            denominator: self.denominator,
        }
    }
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

#[cfg(test)]
mod tests {
    use super::ProbabilityRatio;

    #[test]
    fn ratios_are_reduced_and_validated() {
        let ratio = ProbabilityRatio::new(50, 100).unwrap();
        assert_eq!(ratio.numerator(), 1);
        assert_eq!(ratio.denominator(), 2);
        assert_eq!(ratio.complement(), ratio);
        assert_eq!(
            ProbabilityRatio::new(0, u64::MAX).unwrap(),
            ProbabilityRatio::new(0, 1).unwrap()
        );
        assert_eq!(
            ProbabilityRatio::new(u64::MAX, u64::MAX).unwrap(),
            ProbabilityRatio::new(1, 1).unwrap()
        );
        let near_one = ProbabilityRatio::new(u64::MAX - 1, u64::MAX).unwrap();
        assert_eq!(near_one.complement().numerator(), 1);
        assert_eq!(near_one.complement().denominator(), u64::MAX);
        assert!(ProbabilityRatio::new(2, 1).is_err());
        assert!(ProbabilityRatio::new(0, 0).is_err());
    }
}
