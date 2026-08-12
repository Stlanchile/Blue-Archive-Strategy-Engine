use super::{REWARD_SCHEDULE_DOCUMENT_TYPE, RULESET_DOCUMENT_TYPE, SCENARIO_DOCUMENT_TYPE};

pub const SCHEMA_VERSION_V1: u64 = 1;
pub const SCHEMA_VERSION_V2: u64 = 2;

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
