use serde::Serialize;

use crate::CoreError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnershipMask(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct TargetIndex(u8);

impl TargetIndex {
    pub fn new(index: usize, target_count: usize) -> Result<Self, CoreError> {
        validate_target_count(target_count)?;
        if index >= target_count {
            return Err(CoreError::InvalidTransition {
                message: "target index is outside the configured targets".to_owned(),
            });
        }
        let value = u8::try_from(index).map_err(|_| CoreError::ArithmeticOverflow {
            context: "converting validated target index",
        })?;
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl OwnershipMask {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn from_raw(raw: u8, target_count: usize) -> Result<Self, CoreError> {
        let allowed = Self::all(target_count)?.0;
        if raw & !allowed == 0 {
            Ok(Self(raw))
        } else {
            Err(CoreError::InvalidTransition {
                message: "ownership mask contains a bit outside the configured targets".to_owned(),
            })
        }
    }

    pub fn all(target_count: usize) -> Result<Self, CoreError> {
        validate_target_count(target_count)?;
        let shift = u32::try_from(target_count).map_err(|_| CoreError::ArithmeticOverflow {
            context: "converting ownership target count",
        })?;
        let upper = 1_u8
            .checked_shl(shift)
            .ok_or(CoreError::ArithmeticOverflow {
                context: "constructing ownership target range",
            })?;
        Ok(Self(upper - 1))
    }

    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    pub fn contains(self, index: usize) -> Result<bool, CoreError> {
        Ok(self.0 & bit(index)? != 0)
    }

    pub fn insert(&mut self, index: usize) -> Result<bool, CoreError> {
        let bit = bit(index)?;
        let inserted = self.0 & bit == 0;
        self.0 |= bit;
        Ok(inserted)
    }

    pub fn insert_target(&mut self, index: TargetIndex) -> bool {
        let bit = 1_u8 << index.get();
        let inserted = self.0 & bit == 0;
        self.0 |= bit;
        inserted
    }

    #[must_use]
    pub fn contains_target(self, index: TargetIndex) -> bool {
        self.0 & (1_u8 << index.get()) != 0
    }

    pub fn is_complete(self, target_count: usize) -> Result<bool, CoreError> {
        Ok(self == Self::all(target_count)?)
    }

    pub fn iter_owned(
        self,
        target_count: usize,
    ) -> Result<impl Iterator<Item = TargetIndex>, CoreError> {
        validate_target_count(target_count)?;
        Ok((0..target_count).filter_map(move |index| {
            let target = TargetIndex(index as u8);
            self.contains_target(target).then_some(target)
        }))
    }

    pub fn iter_unowned(
        self,
        target_count: usize,
    ) -> Result<impl Iterator<Item = TargetIndex>, CoreError> {
        validate_target_count(target_count)?;
        Ok((0..target_count).filter_map(move |index| {
            let target = TargetIndex(index as u8);
            (!self.contains_target(target)).then_some(target)
        }))
    }
}

fn bit(index: usize) -> Result<u8, CoreError> {
    let shift = u32::try_from(index).map_err(|_| CoreError::ArithmeticOverflow {
        context: "converting target ownership bit index",
    })?;
    1_u8.checked_shl(shift)
        .filter(|value| *value <= 0b1000)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "constructing four-target ownership bit",
        })
}

fn validate_target_count(target_count: usize) -> Result<(), CoreError> {
    if (1..=4).contains(&target_count) {
        Ok(())
    } else {
        Err(CoreError::InternalInvariant {
            message: "ownership masks support exactly one through four targets".to_owned(),
        })
    }
}
