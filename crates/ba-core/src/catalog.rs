use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::CoreError;
use crate::error::MAX_CATALOG_ENTRIES;
use crate::fingerprint::SemanticFingerprint;
use crate::id::{RewardScheduleId, RulesetId};
use crate::model::{CompiledRuleset, RewardSchedule, ValidatedScenario};
use crate::schema::{
    DocumentKind, RawRewardScheduleV1, RawRulesetV1, RawScenarioV1, SCHEMA_VERSION_V1,
};
use crate::strict_json::BufferedDocument;

#[derive(Debug)]
pub struct Catalog {
    rulesets: BTreeMap<RulesetId, Arc<CompiledRuleset>>,
    reward_schedules: BTreeMap<RewardScheduleId, Arc<RewardSchedule>>,
    ruleset_paths: BTreeMap<RulesetId, PathBuf>,
    reward_paths: BTreeMap<RewardScheduleId, PathBuf>,
}

impl Catalog {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, CoreError> {
        let data_dir = data_dir.as_ref();
        let ruleset_candidates = catalog_candidates(&data_dir.join("rulesets"))?;
        let reward_candidates = catalog_candidates(&data_dir.join("rewards"))?;

        let mut rulesets = BTreeMap::new();
        let mut ruleset_paths = BTreeMap::new();
        for path in ruleset_candidates {
            let document = BufferedDocument::read(&path)?;
            if document.dispatch().kind != DocumentKind::Ruleset {
                return Err(CoreError::validation(
                    Some(&path),
                    "rulesets catalog contains a non-ruleset document",
                ));
            }
            let raw: RawRulesetV1 = document.parse_typed()?;
            let ruleset = Arc::new(CompiledRuleset::from_raw_provisional(raw, Some(&path))?);
            let id = ruleset.id().clone();
            if rulesets.insert(id.clone(), ruleset).is_some() {
                return Err(CoreError::validation(
                    Some(&path),
                    format!("duplicate catalog ruleset ID {id}"),
                ));
            }
            ruleset_paths.insert(id, path);
        }

        let mut reward_schedules = BTreeMap::new();
        let mut reward_paths = BTreeMap::new();
        for path in reward_candidates {
            let document = BufferedDocument::read(&path)?;
            if document.dispatch().kind != DocumentKind::RewardSchedule {
                return Err(CoreError::validation(
                    Some(&path),
                    "rewards catalog contains a non-reward-schedule document",
                ));
            }
            let raw: RawRewardScheduleV1 = document.parse_typed()?;
            let rewards = Arc::new(RewardSchedule::from_raw(raw, Some(&path))?);
            let id = rewards.id().clone();
            if reward_schedules.insert(id.clone(), rewards).is_some() {
                return Err(CoreError::validation(
                    Some(&path),
                    format!("duplicate catalog reward schedule ID {id}"),
                ));
            }
            reward_paths.insert(id, path);
        }

        Ok(Self {
            rulesets,
            reward_schedules,
            ruleset_paths,
            reward_paths,
        })
    }

    #[must_use]
    pub fn rulesets(&self) -> &BTreeMap<RulesetId, Arc<CompiledRuleset>> {
        &self.rulesets
    }

    #[must_use]
    pub fn reward_schedules(&self) -> &BTreeMap<RewardScheduleId, Arc<RewardSchedule>> {
        &self.reward_schedules
    }
}

#[derive(Debug, Clone)]
pub struct SourcePaths {
    pub scenario: PathBuf,
    pub ruleset: PathBuf,
    pub reward_schedule: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BundleFingerprints {
    pub scenario: SemanticFingerprint,
    pub ruleset: SemanticFingerprint,
    pub reward_schedule: SemanticFingerprint,
}

#[derive(Debug, Clone)]
pub struct ValidatedScenarioBundle {
    scenario: Arc<ValidatedScenario>,
    ruleset: Arc<CompiledRuleset>,
    reward_schedule: Arc<RewardSchedule>,
    fingerprints: BundleFingerprints,
    source_paths: SourcePaths,
}

impl ValidatedScenarioBundle {
    pub fn from_programmatic(
        raw_scenario: RawScenarioV1,
        ruleset: CompiledRuleset,
        reward_schedule: RewardSchedule,
    ) -> Result<Self, CoreError> {
        let scenario = ValidatedScenario::from_raw(raw_scenario, &ruleset, &reward_schedule, None)?;
        let fingerprints = BundleFingerprints {
            scenario: scenario.fingerprint()?,
            ruleset: ruleset.fingerprint()?,
            reward_schedule: reward_schedule.fingerprint()?,
        };
        Ok(Self {
            scenario: Arc::new(scenario),
            ruleset: Arc::new(ruleset),
            reward_schedule: Arc::new(reward_schedule),
            fingerprints,
            source_paths: SourcePaths {
                scenario: PathBuf::from("<programmatic-scenario>"),
                ruleset: PathBuf::from("<programmatic-ruleset>"),
                reward_schedule: PathBuf::from("<programmatic-reward-schedule>"),
            },
        })
    }

    #[must_use]
    pub fn scenario(&self) -> &ValidatedScenario {
        &self.scenario
    }

    #[must_use]
    pub fn ruleset(&self) -> &CompiledRuleset {
        &self.ruleset
    }

    #[must_use]
    pub fn reward_schedule(&self) -> &RewardSchedule {
        &self.reward_schedule
    }

    #[must_use]
    pub const fn fingerprints(&self) -> &BundleFingerprints {
        &self.fingerprints
    }

    #[must_use]
    pub const fn source_paths(&self) -> &SourcePaths {
        &self.source_paths
    }
}

pub fn load_bundle(
    data_dir: impl AsRef<Path>,
    scenario_path: impl AsRef<Path>,
) -> Result<ValidatedScenarioBundle, CoreError> {
    let scenario_path = scenario_path.as_ref();
    let scenario_document = BufferedDocument::read(scenario_path)?;
    if scenario_document.dispatch().kind != DocumentKind::Scenario {
        return Err(CoreError::validation(
            Some(scenario_path),
            "analysis input must be a scenario document",
        ));
    }
    let raw: RawScenarioV1 = scenario_document.parse_typed()?;
    let ruleset_lookup = RulesetId::new(raw.ruleset_id.clone())
        .map_err(|error| CoreError::validation(Some(scenario_path), error.to_string()))?;
    let reward_lookup = RewardScheduleId::new(raw.reward_schedule_id.clone())
        .map_err(|error| CoreError::validation(Some(scenario_path), error.to_string()))?;

    let catalog = Catalog::load(data_dir)?;
    let ruleset = catalog
        .rulesets
        .get(&ruleset_lookup)
        .cloned()
        .ok_or_else(|| {
            CoreError::validation(
                Some(scenario_path),
                format!("referenced ruleset {ruleset_lookup} is absent from the complete catalog"),
            )
        })?;
    let reward_schedule = catalog
        .reward_schedules
        .get(&reward_lookup)
        .cloned()
        .ok_or_else(|| {
            CoreError::validation(
                Some(scenario_path),
                format!(
                    "referenced reward schedule {reward_lookup} is absent from the complete catalog"
                ),
            )
        })?;
    let scenario = Arc::new(ValidatedScenario::from_raw(
        raw,
        &ruleset,
        &reward_schedule,
        Some(scenario_path),
    )?);
    let fingerprints = BundleFingerprints {
        scenario: scenario.fingerprint()?,
        ruleset: ruleset.fingerprint()?,
        reward_schedule: reward_schedule.fingerprint()?,
    };
    let ruleset_path = catalog.ruleset_paths.get(&ruleset_lookup).cloned().ok_or(
        CoreError::InternalInvariant {
            message: "ruleset catalog path missing after successful lookup".to_owned(),
        },
    )?;
    let reward_path =
        catalog
            .reward_paths
            .get(&reward_lookup)
            .cloned()
            .ok_or(CoreError::InternalInvariant {
                message: "reward catalog path missing after successful lookup".to_owned(),
            })?;
    Ok(ValidatedScenarioBundle {
        scenario,
        ruleset,
        reward_schedule,
        fingerprints,
        source_paths: SourcePaths {
            scenario: scenario_path.to_path_buf(),
            ruleset: ruleset_path,
            reward_schedule: reward_path,
        },
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub schema_version: u64,
    pub document_type: String,
    pub id: String,
    pub fingerprint: SemanticFingerprint,
}

pub fn validate_document(
    data_dir: impl AsRef<Path>,
    document_path: impl AsRef<Path>,
) -> Result<ValidationReport, CoreError> {
    let path = document_path.as_ref();
    let document = BufferedDocument::read(path)?;
    match document.dispatch().kind {
        DocumentKind::Ruleset => {
            let raw: RawRulesetV1 = document.parse_typed()?;
            let value = CompiledRuleset::from_raw_provisional(raw, Some(path))?;
            Ok(ValidationReport {
                valid: true,
                schema_version: SCHEMA_VERSION_V1,
                document_type: DocumentKind::Ruleset.as_str().to_owned(),
                id: value.id().to_string(),
                fingerprint: value.fingerprint()?,
            })
        }
        DocumentKind::RewardSchedule => {
            let raw: RawRewardScheduleV1 = document.parse_typed()?;
            let value = RewardSchedule::from_raw(raw, Some(path))?;
            Ok(ValidationReport {
                valid: true,
                schema_version: SCHEMA_VERSION_V1,
                document_type: DocumentKind::RewardSchedule.as_str().to_owned(),
                id: value.id().to_string(),
                fingerprint: value.fingerprint()?,
            })
        }
        DocumentKind::Scenario => {
            let bundle = load_bundle(data_dir, path)?;
            Ok(ValidationReport {
                valid: true,
                schema_version: SCHEMA_VERSION_V1,
                document_type: DocumentKind::Scenario.as_str().to_owned(),
                id: bundle.scenario().id().to_string(),
                fingerprint: bundle.fingerprints().scenario,
            })
        }
    }
}

fn catalog_candidates(directory: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|source| CoreError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(CoreError::PathPolicy {
            path: directory.to_path_buf(),
            message: "catalog path must be a non-symlink directory".to_owned(),
        });
    }

    let reader = std::fs::read_dir(directory).map_err(|source| CoreError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut candidates = Vec::new();
    for item in reader {
        let item = item.map_err(|source| CoreError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = item.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        let entry_metadata = std::fs::symlink_metadata(&path).map_err(|source| CoreError::Io {
            path: path.clone(),
            source,
        })?;
        if entry_metadata.file_type().is_symlink() || !entry_metadata.file_type().is_file() {
            return Err(CoreError::PathPolicy {
                path,
                message: "a .json catalog entry must be a non-symlink regular file".to_owned(),
            });
        }
        candidates.push(path);
    }
    let observed = candidates.len();
    if observed > MAX_CATALOG_ENTRIES {
        return Err(CoreError::CatalogEntryLimitExceeded {
            directory: directory.to_path_buf(),
            observed,
            maximum: MAX_CATALOG_ENTRIES,
        });
    }
    candidates.sort();
    Ok(candidates)
}
