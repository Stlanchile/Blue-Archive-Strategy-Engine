use super::{REWARD_SCHEDULE_DOCUMENT_TYPE, RULESET_DOCUMENT_TYPE, SCENARIO_DOCUMENT_TYPE};
use crate::profile::DOCUMENT_SCHEMA_VERSION_V2;

pub const SCHEMA_VERSION: u64 = DOCUMENT_SCHEMA_VERSION_V2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Ruleset,
    RewardSchedule,
    Scenario,
}

impl DocumentKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ruleset => RULESET_DOCUMENT_TYPE,
            Self::RewardSchedule => REWARD_SCHEDULE_DOCUMENT_TYPE,
            Self::Scenario => SCENARIO_DOCUMENT_TYPE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDispatch {
    pub schema_version: u64,
    pub kind: DocumentKind,
}
