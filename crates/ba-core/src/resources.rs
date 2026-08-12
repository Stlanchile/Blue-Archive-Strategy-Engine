use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub pyroxene: u64,
    pub limited_ten_recruitment_tickets: u64,
    pub eligma: u64,
    pub advanced_bd_selectors: u64,
    pub advanced_tech_note_selectors: u64,
    pub superior_tech_note_selectors: u64,
    pub gift_boxes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Pyroxene,
    LimitedTenRecruitmentTickets,
    Eligma,
    AdvancedBdSelectors,
    AdvancedTechNoteSelectors,
    SuperiorTechNoteSelectors,
    GiftBoxes,
}

impl Resources {
    pub fn checked_add_kind(&mut self, kind: ResourceKind, quantity: u64) -> Result<(), CoreError> {
        let slot = self.slot_mut(kind);
        *slot = slot
            .checked_add(quantity)
            .ok_or(CoreError::ArithmeticOverflow {
                context: "adding resource inventory",
            })?;
        Ok(())
    }

    pub fn checked_add(&mut self, other: Self) -> Result<(), CoreError> {
        for (kind, value) in other.entries() {
            self.checked_add_kind(kind, value)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn get(self, kind: ResourceKind) -> u64 {
        match kind {
            ResourceKind::Pyroxene => self.pyroxene,
            ResourceKind::LimitedTenRecruitmentTickets => self.limited_ten_recruitment_tickets,
            ResourceKind::Eligma => self.eligma,
            ResourceKind::AdvancedBdSelectors => self.advanced_bd_selectors,
            ResourceKind::AdvancedTechNoteSelectors => self.advanced_tech_note_selectors,
            ResourceKind::SuperiorTechNoteSelectors => self.superior_tech_note_selectors,
            ResourceKind::GiftBoxes => self.gift_boxes,
        }
    }

    #[must_use]
    pub fn entries(self) -> [(ResourceKind, u64); 7] {
        [
            (ResourceKind::Pyroxene, self.pyroxene),
            (
                ResourceKind::LimitedTenRecruitmentTickets,
                self.limited_ten_recruitment_tickets,
            ),
            (ResourceKind::Eligma, self.eligma),
            (
                ResourceKind::AdvancedBdSelectors,
                self.advanced_bd_selectors,
            ),
            (
                ResourceKind::AdvancedTechNoteSelectors,
                self.advanced_tech_note_selectors,
            ),
            (
                ResourceKind::SuperiorTechNoteSelectors,
                self.superior_tech_note_selectors,
            ),
            (ResourceKind::GiftBoxes, self.gift_boxes),
        ]
    }

    fn slot_mut(&mut self, kind: ResourceKind) -> &mut u64 {
        match kind {
            ResourceKind::Pyroxene => &mut self.pyroxene,
            ResourceKind::LimitedTenRecruitmentTickets => &mut self.limited_ten_recruitment_tickets,
            ResourceKind::Eligma => &mut self.eligma,
            ResourceKind::AdvancedBdSelectors => &mut self.advanced_bd_selectors,
            ResourceKind::AdvancedTechNoteSelectors => &mut self.advanced_tech_note_selectors,
            ResourceKind::SuperiorTechNoteSelectors => &mut self.superior_tech_note_selectors,
            ResourceKind::GiftBoxes => &mut self.gift_boxes,
        }
    }
}
