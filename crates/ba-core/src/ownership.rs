use crate::CoreError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnershipMask(u8);

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
        match target_count {
            1 => Ok(Self(0b01)),
            2 => Ok(Self(0b11)),
            _ => Err(CoreError::InternalInvariant {
                message: "ownership masks support exactly one or two targets".to_owned(),
            }),
        }
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
}

fn bit(index: usize) -> Result<u8, CoreError> {
    let shift = u32::try_from(index).map_err(|_| CoreError::ArithmeticOverflow {
        context: "converting target ownership bit index",
    })?;
    1_u8.checked_shl(shift)
        .filter(|value| *value <= 0b10)
        .ok_or(CoreError::ArithmeticOverflow {
            context: "constructing two-target ownership bit",
        })
}
