use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid {kind} identifier {value:?}; expected ASCII [A-Za-z0-9][A-Za-z0-9._-]{{0,127}}")]
pub struct IdError {
    kind: &'static str,
    value: String,
}

fn validate_id(kind: &'static str, value: String) -> Result<String, IdError> {
    let bytes = value.as_bytes();
    let first_valid = bytes.first().is_some_and(u8::is_ascii_alphanumeric);
    let rest_valid = bytes
        .iter()
        .skip(1)
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if first_valid && rest_valid && bytes.len() <= 128 {
        Ok(value)
    } else {
        Err(IdError { kind, value })
    }
}

macro_rules! define_id {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                validate_id($kind, value.into()).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

define_id!(RulesetId, "ruleset");
define_id!(RewardScheduleId, "reward schedule");
define_id!(ScenarioId, "scenario");
define_id!(StudentId, "student");
define_id!(BannerId, "banner");
define_id!(ChargeGroupId, "charge group");
define_id!(StrategyId, "strategy");

#[cfg(test)]
mod tests {
    use super::StudentId;

    #[test]
    fn identifier_contract() {
        for valid in ["A", "a.b-c_d9", &"x".repeat(128)] {
            assert!(StudentId::new(valid).is_ok(), "{valid}");
        }
        for invalid in ["", "-bad", "bad space", "é", &"x".repeat(129)] {
            assert!(StudentId::new(invalid).is_err(), "{invalid}");
        }
    }
}
