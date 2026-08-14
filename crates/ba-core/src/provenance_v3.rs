use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::CoreError;
use crate::fingerprint::{CanonicalNode, object};
use crate::schema::{RawClaimGroupV3, RawProvenanceStatusV3, RawProvenanceV3, RawSourceCategoryV3};

pub type ProvenanceStatusV3 = RawProvenanceStatusV3;
pub type SourceCategoryV3 = RawSourceCategoryV3;
pub type ClaimGroupV3 = RawClaimGroupV3;

pub const RULESET_CLAIM_GROUPS_V3: [ClaimGroupV3; 7] = [
    ClaimGroupV3::RecruitmentCost,
    ClaimGroupV3::OrdinaryFeaturedTargetProbability,
    ClaimGroupV3::ChargeThresholds,
    ClaimGroupV3::ChargeResetBehavior,
    ClaimGroupV3::ChargeCarryAndGroupScope,
    ClaimGroupV3::AtomicTenRecruitmentContinuation,
    ClaimGroupV3::LimitedTicketActionSizeAndEligibility,
];

pub const REWARD_SCHEDULE_CLAIM_GROUPS_V3: [ClaimGroupV3; 4] = [
    ClaimGroupV3::PeriodScopeAndReset,
    ClaimGroupV3::FirstTimeMilestones,
    ClaimGroupV3::RepeatingCycle,
    ClaimGroupV3::MilestoneTicketAwards,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceSubjectV3 {
    Ruleset,
    RewardSchedule,
}

impl ProvenanceSubjectV3 {
    fn required_claim_groups(self) -> &'static [ClaimGroupV3] {
        match self {
            Self::Ruleset => &RULESET_CLAIM_GROUPS_V3,
            Self::RewardSchedule => &REWARD_SCHEDULE_CLAIM_GROUPS_V3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceSourceV3 {
    pub source_id: String,
    pub source_category: SourceCategoryV3,
    pub label: String,
    pub reference: String,
    pub published_on: Option<String>,
    pub retrieved_on: String,
    pub content_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaimBindingV3 {
    pub claim_group: ClaimGroupV3,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceV3 {
    pub provenance_status: ProvenanceStatusV3,
    pub sources: Vec<ProvenanceSourceV3>,
    pub claim_bindings: Vec<ClaimBindingV3>,
}

impl ProvenanceV3 {
    pub fn from_raw(
        raw: RawProvenanceV3,
        subject: ProvenanceSubjectV3,
        path: Option<&Path>,
    ) -> Result<Self, CoreError> {
        let mut sources = Vec::with_capacity(raw.sources.len());
        let mut source_ids = BTreeSet::new();
        for source in raw.sources {
            validate_source_id(&source.source_id, path)?;
            if !source_ids.insert(source.source_id.clone()) {
                return Err(CoreError::validation(
                    path,
                    format!("duplicate provenance source ID {}", source.source_id),
                ));
            }
            if source.label.is_empty() || source.label.len() > 256 {
                return Err(CoreError::validation(
                    path,
                    "provenance source label must contain 1 through 256 UTF-8 bytes",
                ));
            }
            if source.reference.is_empty() || source.reference.len() > 2_048 {
                return Err(CoreError::validation(
                    path,
                    "provenance source reference must contain 1 through 2048 UTF-8 bytes",
                ));
            }
            if source
                .published_on
                .as_deref()
                .is_some_and(|value| !is_gregorian_date(value))
            {
                return Err(CoreError::validation(
                    path,
                    "provenance published_on must be a valid Gregorian YYYY-MM-DD date",
                ));
            }
            if !is_gregorian_date(&source.retrieved_on) {
                return Err(CoreError::validation(
                    path,
                    "provenance retrieved_on must be a valid Gregorian YYYY-MM-DD date",
                ));
            }
            if source.content_sha256.as_deref().is_some_and(|value| {
                value.len() != 64
                    || !value
                        .as_bytes()
                        .iter()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
            }) {
                return Err(CoreError::validation(
                    path,
                    "provenance content_sha256 must be 64 lowercase hexadecimal characters",
                ));
            }
            sources.push(ProvenanceSourceV3 {
                source_id: source.source_id,
                source_category: source.source_category,
                label: source.label,
                reference: source.reference,
                published_on: source.published_on,
                retrieved_on: source.retrieved_on,
                content_sha256: source.content_sha256,
            });
        }
        sources.sort_by(|left, right| left.source_id.cmp(&right.source_id));

        let source_by_id = sources
            .iter()
            .map(|source| (source.source_id.as_str(), source.source_category))
            .collect::<BTreeMap<_, _>>();
        let mut seen_groups = BTreeSet::new();
        let mut bindings = Vec::with_capacity(raw.claim_bindings.len());
        for binding in raw.claim_bindings {
            if !seen_groups.insert(binding.claim_group) {
                return Err(CoreError::validation(
                    path,
                    format!(
                        "duplicate provenance claim group {}",
                        binding.claim_group.as_str()
                    ),
                ));
            }
            if binding.source_ids.is_empty() {
                return Err(CoreError::validation(
                    path,
                    format!(
                        "claim group {} must reference at least one source",
                        binding.claim_group.as_str()
                    ),
                ));
            }
            let mut ids = BTreeSet::new();
            for source_id in binding.source_ids {
                if !source_by_id.contains_key(source_id.as_str()) {
                    return Err(CoreError::validation(
                        path,
                        format!(
                            "claim group {} references unknown source ID {source_id}",
                            binding.claim_group.as_str()
                        ),
                    ));
                }
                if !ids.insert(source_id.clone()) {
                    return Err(CoreError::validation(
                        path,
                        format!(
                            "claim group {} repeats source ID {source_id}",
                            binding.claim_group.as_str()
                        ),
                    ));
                }
            }
            bindings.push(ClaimBindingV3 {
                claim_group: binding.claim_group,
                source_ids: ids.into_iter().collect(),
            });
        }
        bindings.sort_by_key(|binding| binding.claim_group);

        if raw.provenance_status == ProvenanceStatusV3::SourceBacked {
            if sources.is_empty() {
                return Err(CoreError::validation(
                    path,
                    "source_backed provenance requires at least one source",
                ));
            }
            let required = subject.required_claim_groups();
            if bindings.len() != required.len()
                || !required.iter().all(|group| seen_groups.contains(group))
            {
                return Err(CoreError::validation(
                    path,
                    "source_backed provenance must bind every required claim group exactly once",
                ));
            }
            for binding in &bindings {
                if !binding.source_ids.iter().any(|source_id| {
                    source_by_id.get(source_id.as_str())
                        == Some(&SourceCategoryV3::FirstPartyOfficial)
                }) {
                    return Err(CoreError::validation(
                        path,
                        format!(
                            "source_backed claim group {} requires first-party official coverage",
                            binding.claim_group.as_str()
                        ),
                    ));
                }
            }
        }

        Ok(Self {
            provenance_status: raw.provenance_status,
            sources,
            claim_bindings: bindings,
        })
    }

    #[must_use]
    pub fn required_claim_groups(subject: ProvenanceSubjectV3) -> &'static [ClaimGroupV3] {
        subject.required_claim_groups()
    }
}

pub(crate) fn provenance_node_v3(provenance: &ProvenanceV3) -> CanonicalNode {
    object([
        (
            "claim_bindings",
            CanonicalNode::Array(
                provenance
                    .claim_bindings
                    .iter()
                    .map(|binding| {
                        object([
                            (
                                "claim_group",
                                CanonicalNode::String(binding.claim_group.as_str().to_owned()),
                            ),
                            (
                                "source_ids",
                                CanonicalNode::Array(
                                    binding
                                        .source_ids
                                        .iter()
                                        .map(|id| CanonicalNode::String(id.clone()))
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "provenance_status",
            CanonicalNode::String(provenance.provenance_status.as_str().to_owned()),
        ),
        (
            "sources",
            CanonicalNode::Array(
                provenance
                    .sources
                    .iter()
                    .map(|source| {
                        object([
                            (
                                "content_sha256",
                                source
                                    .content_sha256
                                    .as_ref()
                                    .map_or(CanonicalNode::Null, |value| {
                                        CanonicalNode::String(value.clone())
                                    }),
                            ),
                            ("label", CanonicalNode::String(source.label.clone())),
                            (
                                "published_on",
                                source
                                    .published_on
                                    .as_ref()
                                    .map_or(CanonicalNode::Null, |value| {
                                        CanonicalNode::String(value.clone())
                                    }),
                            ),
                            ("reference", CanonicalNode::String(source.reference.clone())),
                            (
                                "retrieved_on",
                                CanonicalNode::String(source.retrieved_on.clone()),
                            ),
                            (
                                "source_category",
                                CanonicalNode::String(source.source_category.as_str().to_owned()),
                            ),
                            ("source_id", CanonicalNode::String(source.source_id.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn validate_source_id(value: &str, path: Option<&Path>) -> Result<(), CoreError> {
    let bytes = value.as_bytes();
    let valid = bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.len() <= 128
        && bytes
            .iter()
            .skip(1)
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(CoreError::validation(
            path,
            format!(
                "invalid provenance source ID {value:?}; expected ASCII [A-Za-z0-9][A-Za-z0-9._-]{{0,127}}"
            ),
        ))
    }
}

fn is_gregorian_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let parse = |start: usize, end: usize| -> Option<u32> { value.get(start..end)?.parse().ok() };
    let Some(year) = parse(0, 4) else {
        return false;
    };
    let Some(month) = parse(5, 7) else {
        return false;
    };
    let Some(day) = parse(8, 10) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let maximum_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=maximum_day).contains(&day)
}
