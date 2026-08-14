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

/// The schema-v2 resource enum. This alias makes the input-profile boundary
/// explicit without changing the historical public `ResourceKind` surface.
pub type RawResourceKindV2 = ResourceKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawResourceKindV3 {
    Pyroxene,
    LimitedTenRecruitmentTickets,
    Eligma,
    AdvancedBdSelectors,
    AdvancedTechNoteSelectors,
    SuperiorTechNoteSelectors,
    GiftBoxes,
    KeystoneFragments,
    SecretTechNotes,
    SuperiorBdSelectors,
    HighGradeGiftBoxes,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcesV3 {
    pub pyroxene: u64,
    pub limited_ten_recruitment_tickets: u64,
    pub eligma: u64,
    pub advanced_bd_selectors: u64,
    pub advanced_tech_note_selectors: u64,
    pub superior_tech_note_selectors: u64,
    pub gift_boxes: u64,
    pub keystone_fragments: u64,
    pub secret_tech_notes: u64,
    pub superior_bd_selectors: u64,
    pub high_grade_gift_boxes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum LedgerResourceKind {
    Pyroxene = 0,
    LimitedTenRecruitmentTickets = 1,
    Eligma = 2,
    AdvancedBdSelectors = 3,
    AdvancedTechNoteSelectors = 4,
    SuperiorTechNoteSelectors = 5,
    GiftBoxes = 6,
    KeystoneFragments = 7,
    SecretTechNotes = 8,
    SuperiorBdSelectors = 9,
    HighGradeGiftBoxes = 10,
}

pub const RESOURCE_KIND_COUNT_V3: usize = 11;
pub const RESOURCE_KINDS_V3: [LedgerResourceKind; RESOURCE_KIND_COUNT_V3] = [
    LedgerResourceKind::Pyroxene,
    LedgerResourceKind::LimitedTenRecruitmentTickets,
    LedgerResourceKind::Eligma,
    LedgerResourceKind::AdvancedBdSelectors,
    LedgerResourceKind::AdvancedTechNoteSelectors,
    LedgerResourceKind::SuperiorTechNoteSelectors,
    LedgerResourceKind::GiftBoxes,
    LedgerResourceKind::KeystoneFragments,
    LedgerResourceKind::SecretTechNotes,
    LedgerResourceKind::SuperiorBdSelectors,
    LedgerResourceKind::HighGradeGiftBoxes,
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceLedger {
    values: [u64; RESOURCE_KIND_COUNT_V3],
}

impl ResourceLedger {
    #[must_use]
    pub const fn get(self, kind: LedgerResourceKind) -> u64 {
        self.values[kind as usize]
    }

    pub fn checked_add(
        &mut self,
        kind: LedgerResourceKind,
        quantity: u64,
    ) -> Result<(), CoreError> {
        let slot = &mut self.values[kind as usize];
        *slot = slot
            .checked_add(quantity)
            .ok_or(CoreError::ArithmeticOverflow {
                context: "adding resource ledger inventory",
            })?;
        Ok(())
    }

    pub fn checked_add_ledger(&mut self, other: Self) -> Result<(), CoreError> {
        for (kind, value) in other.iter_canonical() {
            self.checked_add(kind, value)?;
        }
        Ok(())
    }

    pub fn checked_sub_ledger(&mut self, other: Self) -> Result<(), CoreError> {
        for (kind, value) in other.iter_canonical() {
            let slot = &mut self.values[kind as usize];
            *slot = slot
                .checked_sub(value)
                .ok_or(CoreError::ArithmeticOverflow {
                    context: "subtracting resource ledger inventory",
                })?;
        }
        Ok(())
    }

    #[must_use]
    pub fn iter_canonical(self) -> impl ExactSizeIterator<Item = (LedgerResourceKind, u64)> {
        RESOURCE_KINDS_V3
            .into_iter()
            .map(move |kind| (kind, self.get(kind)))
    }

    #[must_use]
    pub const fn as_values(&self) -> &[u64; RESOURCE_KIND_COUNT_V3] {
        &self.values
    }
}

impl From<RawResourceKindV3> for LedgerResourceKind {
    fn from(value: RawResourceKindV3) -> Self {
        match value {
            RawResourceKindV3::Pyroxene => Self::Pyroxene,
            RawResourceKindV3::LimitedTenRecruitmentTickets => Self::LimitedTenRecruitmentTickets,
            RawResourceKindV3::Eligma => Self::Eligma,
            RawResourceKindV3::AdvancedBdSelectors => Self::AdvancedBdSelectors,
            RawResourceKindV3::AdvancedTechNoteSelectors => Self::AdvancedTechNoteSelectors,
            RawResourceKindV3::SuperiorTechNoteSelectors => Self::SuperiorTechNoteSelectors,
            RawResourceKindV3::GiftBoxes => Self::GiftBoxes,
            RawResourceKindV3::KeystoneFragments => Self::KeystoneFragments,
            RawResourceKindV3::SecretTechNotes => Self::SecretTechNotes,
            RawResourceKindV3::SuperiorBdSelectors => Self::SuperiorBdSelectors,
            RawResourceKindV3::HighGradeGiftBoxes => Self::HighGradeGiftBoxes,
        }
    }
}

impl From<ResourcesV3> for ResourceLedger {
    fn from(value: ResourcesV3) -> Self {
        Self {
            values: [
                value.pyroxene,
                value.limited_ten_recruitment_tickets,
                value.eligma,
                value.advanced_bd_selectors,
                value.advanced_tech_note_selectors,
                value.superior_tech_note_selectors,
                value.gift_boxes,
                value.keystone_fragments,
                value.secret_tech_notes,
                value.superior_bd_selectors,
                value.high_grade_gift_boxes,
            ],
        }
    }
}

impl From<ResourceLedger> for ResourcesV3 {
    fn from(value: ResourceLedger) -> Self {
        Self {
            pyroxene: value.get(LedgerResourceKind::Pyroxene),
            limited_ten_recruitment_tickets: value
                .get(LedgerResourceKind::LimitedTenRecruitmentTickets),
            eligma: value.get(LedgerResourceKind::Eligma),
            advanced_bd_selectors: value.get(LedgerResourceKind::AdvancedBdSelectors),
            advanced_tech_note_selectors: value.get(LedgerResourceKind::AdvancedTechNoteSelectors),
            superior_tech_note_selectors: value.get(LedgerResourceKind::SuperiorTechNoteSelectors),
            gift_boxes: value.get(LedgerResourceKind::GiftBoxes),
            keystone_fragments: value.get(LedgerResourceKind::KeystoneFragments),
            secret_tech_notes: value.get(LedgerResourceKind::SecretTechNotes),
            superior_bd_selectors: value.get(LedgerResourceKind::SuperiorBdSelectors),
            high_grade_gift_boxes: value.get(LedgerResourceKind::HighGradeGiftBoxes),
        }
    }
}

impl From<Resources> for ResourceLedger {
    fn from(value: Resources) -> Self {
        Self {
            values: [
                value.pyroxene,
                value.limited_ten_recruitment_tickets,
                value.eligma,
                value.advanced_bd_selectors,
                value.advanced_tech_note_selectors,
                value.superior_tech_note_selectors,
                value.gift_boxes,
                0,
                0,
                0,
                0,
            ],
        }
    }
}

impl ResourceLedger {
    #[must_use]
    pub fn v2_projection(self) -> Resources {
        Resources {
            pyroxene: self.get(LedgerResourceKind::Pyroxene),
            limited_ten_recruitment_tickets: self
                .get(LedgerResourceKind::LimitedTenRecruitmentTickets),
            eligma: self.get(LedgerResourceKind::Eligma),
            advanced_bd_selectors: self.get(LedgerResourceKind::AdvancedBdSelectors),
            advanced_tech_note_selectors: self.get(LedgerResourceKind::AdvancedTechNoteSelectors),
            superior_tech_note_selectors: self.get(LedgerResourceKind::SuperiorTechNoteSelectors),
            gift_boxes: self.get(LedgerResourceKind::GiftBoxes),
        }
    }
}

#[must_use]
pub const fn resource_kind_name_v3(kind: LedgerResourceKind) -> &'static str {
    match kind {
        LedgerResourceKind::Pyroxene => "pyroxene",
        LedgerResourceKind::LimitedTenRecruitmentTickets => "limited_ten_recruitment_tickets",
        LedgerResourceKind::Eligma => "eligma",
        LedgerResourceKind::AdvancedBdSelectors => "advanced_bd_selectors",
        LedgerResourceKind::AdvancedTechNoteSelectors => "advanced_tech_note_selectors",
        LedgerResourceKind::SuperiorTechNoteSelectors => "superior_tech_note_selectors",
        LedgerResourceKind::GiftBoxes => "gift_boxes",
        LedgerResourceKind::KeystoneFragments => "keystone_fragments",
        LedgerResourceKind::SecretTechNotes => "secret_tech_notes",
        LedgerResourceKind::SuperiorBdSelectors => "superior_bd_selectors",
        LedgerResourceKind::HighGradeGiftBoxes => "high_grade_gift_boxes",
    }
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
