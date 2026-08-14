use serde::Serialize;

pub const DOCUMENT_SCHEMA_VERSION_V2: u64 = 2;
pub const DOCUMENT_SCHEMA_VERSION_V3: u64 = 3;
pub const STRATEGY_SCHEMA_VERSION_V2: u64 = 1;
pub const STRATEGY_SCHEMA_VERSION_V3: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentProfile {
    V2,
    V3,
}

impl DocumentProfile {
    #[must_use]
    pub const fn schema_version(self) -> u64 {
        match self {
            Self::V2 => DOCUMENT_SCHEMA_VERSION_V2,
            Self::V3 => DOCUMENT_SCHEMA_VERSION_V3,
        }
    }

    pub fn from_schema_version(schema_version: u64) -> Option<Self> {
        match schema_version {
            DOCUMENT_SCHEMA_VERSION_V2 => Some(Self::V2),
            DOCUMENT_SCHEMA_VERSION_V3 => Some(Self::V3),
            _ => None,
        }
    }
}
