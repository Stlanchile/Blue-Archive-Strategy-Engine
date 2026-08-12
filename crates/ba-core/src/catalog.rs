use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::CoreError;
use crate::fingerprint::SemanticFingerprint;
use crate::fs_secure::{DirectoryEntrySnapshot, PinnedDirectory, is_json_candidate};
use crate::id::{RewardScheduleId, RulesetId};
use crate::model::{CompiledRuleset, CompiledStrategy, RewardSchedule, ValidatedScenario};
use crate::schema::{
    DocumentKind, RawRewardScheduleV1, RawRewardScheduleV2, RawRulesetV1, RawRulesetV2,
    RawScenarioV1, RawScenarioV2, SCHEMA_VERSION_V1, SCHEMA_VERSION_V2, VerificationStatus,
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
        Self::load_observed(data_dir.as_ref(), |_| {})
    }

    fn load_observed(
        data_dir: &Path,
        mut observer: impl FnMut(CatalogLoadStage),
    ) -> Result<Self, CoreError> {
        let root = PinnedDirectory::open_ambient(data_dir)?;
        observer(CatalogLoadStage::RootPinned);
        let rules_name = OsStr::new("rulesets");
        let rewards_name = OsStr::new("rewards");
        let inspected_rules = root.inspect(rules_name)?;
        let inspected_rewards = root.inspect(rewards_name)?;
        observer(CatalogLoadStage::ChildrenInspected);
        let rules_directory = root.open_child_directory(rules_name, &inspected_rules)?;
        observer(CatalogLoadStage::RulesDirectoryOpened);
        let rewards_directory = root.open_child_directory(rewards_name, &inspected_rewards)?;
        observer(CatalogLoadStage::ChildrenOpened);

        let rules_snapshot = rules_directory.enumerate_catalog()?;
        let rewards_snapshot = rewards_directory.enumerate_catalog()?;
        observer(CatalogLoadStage::EntrySnapshotsCaptured);
        let mut rulesets = BTreeMap::new();
        let mut ruleset_paths = BTreeMap::new();
        for candidate in rules_snapshot
            .iter()
            .filter(|value| is_json_candidate(value))
        {
            let path = rules_directory.display_path().join(candidate.name());
            let document =
                BufferedDocument::from_bytes(&path, rules_directory.read_candidate(candidate)?)?;
            if document.dispatch().kind != DocumentKind::Ruleset {
                return Err(CoreError::validation(
                    Some(&path),
                    "rulesets catalog contains a non-ruleset document",
                ));
            }
            let ruleset = Arc::new(match document.dispatch().schema_version {
                SCHEMA_VERSION_V1 => {
                    let raw: RawRulesetV1 = document.parse_typed()?;
                    CompiledRuleset::from_raw_provisional(raw, Some(&path))?
                }
                SCHEMA_VERSION_V2 => {
                    let raw: RawRulesetV2 = document.parse_typed()?;
                    CompiledRuleset::from_raw_v2(raw, Some(&path))?
                }
                _ => {
                    return Err(CoreError::InternalInvariant {
                        message: "validated ruleset dispatch has an unknown version".to_owned(),
                    });
                }
            });
            let id = ruleset.id().clone();
            if rulesets.insert(id.clone(), ruleset).is_some() {
                return Err(CoreError::validation(
                    Some(&path),
                    format!("duplicate catalog ruleset ID {id}"),
                ));
            }
            ruleset_paths.insert(id, path);
        }
        observer(CatalogLoadStage::RulesetsLoaded);

        let mut reward_schedules = BTreeMap::new();
        let mut reward_paths = BTreeMap::new();
        for candidate in rewards_snapshot
            .iter()
            .filter(|value| is_json_candidate(value))
        {
            let path = rewards_directory.display_path().join(candidate.name());
            let document =
                BufferedDocument::from_bytes(&path, rewards_directory.read_candidate(candidate)?)?;
            if document.dispatch().kind != DocumentKind::RewardSchedule {
                return Err(CoreError::validation(
                    Some(&path),
                    "rewards catalog contains a non-reward-schedule document",
                ));
            }
            let rewards = Arc::new(match document.dispatch().schema_version {
                SCHEMA_VERSION_V1 => {
                    let raw: RawRewardScheduleV1 = document.parse_typed()?;
                    RewardSchedule::from_raw(raw, Some(&path))?
                }
                SCHEMA_VERSION_V2 => {
                    let raw: RawRewardScheduleV2 = document.parse_typed()?;
                    RewardSchedule::from_raw_v2(raw, Some(&path))?
                }
                _ => {
                    return Err(CoreError::InternalInvariant {
                        message: "validated reward dispatch has an unknown version".to_owned(),
                    });
                }
            });
            let id = rewards.id().clone();
            if reward_schedules.insert(id.clone(), rewards).is_some() {
                return Err(CoreError::validation(
                    Some(&path),
                    format!("duplicate catalog reward schedule ID {id}"),
                ));
            }
            reward_paths.insert(id, path);
        }
        observer(CatalogLoadStage::RewardsLoaded);

        observer(CatalogLoadStage::BeforeGenerationVerification);
        verify_catalog_generation(
            &root,
            &rules_directory,
            &rewards_directory,
            &rules_snapshot,
            &rewards_snapshot,
        )?;

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

    #[must_use]
    pub fn ruleset(&self, id: &RulesetId) -> Option<&CompiledRuleset> {
        self.rulesets.get(id).map(AsRef::as_ref)
    }

    #[must_use]
    pub fn reward_schedule(&self, id: &RewardScheduleId) -> Option<&RewardSchedule> {
        self.reward_schedules.get(id).map(AsRef::as_ref)
    }

    #[must_use]
    pub fn ruleset_path(&self, id: &RulesetId) -> Option<&Path> {
        self.ruleset_paths.get(id).map(PathBuf::as_path)
    }

    #[must_use]
    pub fn reward_schedule_path(&self, id: &RewardScheduleId) -> Option<&Path> {
        self.reward_paths.get(id).map(PathBuf::as_path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogLoadStage {
    RootPinned,
    ChildrenInspected,
    RulesDirectoryOpened,
    ChildrenOpened,
    EntrySnapshotsCaptured,
    RulesetsLoaded,
    RewardsLoaded,
    BeforeGenerationVerification,
}

fn verify_catalog_generation(
    root: &PinnedDirectory,
    rules_directory: &PinnedDirectory,
    rewards_directory: &PinnedDirectory,
    rules_snapshot: &[DirectoryEntrySnapshot],
    rewards_snapshot: &[DirectoryEntrySnapshot],
) -> Result<(), CoreError> {
    root.verify_unchanged()?;
    root.verify_child_identity(OsStr::new("rulesets"), rules_directory)?;
    root.verify_child_identity(OsStr::new("rewards"), rewards_directory)?;
    rules_directory.verify_unchanged()?;
    rewards_directory.verify_unchanged()?;

    if rules_directory.enumerate_catalog()? != rules_snapshot {
        return Err(CoreError::CatalogGenerationChanged {
            path: rules_directory.display_path().to_path_buf(),
            message: "ruleset catalog entry snapshot changed during loading".to_owned(),
        });
    }
    if rewards_directory.enumerate_catalog()? != rewards_snapshot {
        return Err(CoreError::CatalogGenerationChanged {
            path: rewards_directory.display_path().to_path_buf(),
            message: "reward catalog entry snapshot changed during loading".to_owned(),
        });
    }

    root.verify_unchanged()?;
    root.verify_child_identity(OsStr::new("rulesets"), rules_directory)?;
    root.verify_child_identity(OsStr::new("rewards"), rewards_directory)?;
    rules_directory.verify_unchanged()?;
    rewards_directory.verify_unchanged()
}

#[derive(Debug, Clone)]
pub struct SourcePaths {
    pub scenario: PathBuf,
    pub ruleset: PathBuf,
    pub reward_schedule: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BundleFingerprints {
    /// Behavior fingerprint used by the frozen Monte Carlo stream derivation.
    pub scenario: SemanticFingerprint,
    pub ruleset: SemanticFingerprint,
    pub reward_schedule: SemanticFingerprint,
    pub scenario_document: SemanticFingerprint,
    pub ruleset_document: SemanticFingerprint,
    pub reward_schedule_document: SemanticFingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleCompatibilityProfile {
    V1,
    V2,
}

#[derive(Debug, Clone)]
pub struct ValidatedScenarioBundle {
    scenario: Arc<ValidatedScenario>,
    ruleset: Arc<CompiledRuleset>,
    reward_schedule: Arc<RewardSchedule>,
    fingerprints: BundleFingerprints,
    source_paths: SourcePaths,
    profile: BundleCompatibilityProfile,
}

impl ValidatedScenarioBundle {
    pub fn from_programmatic(
        raw_scenario: RawScenarioV1,
        ruleset: CompiledRuleset,
        reward_schedule: RewardSchedule,
    ) -> Result<Self, CoreError> {
        let scenario = ValidatedScenario::from_raw(raw_scenario, &ruleset, &reward_schedule, None)?;
        let fingerprints = BundleFingerprints {
            scenario: scenario.behavior_fingerprint()?,
            ruleset: ruleset.behavior_fingerprint()?,
            reward_schedule: reward_schedule.behavior_fingerprint()?,
            scenario_document: scenario.document_fingerprint()?,
            ruleset_document: ruleset.document_fingerprint()?,
            reward_schedule_document: reward_schedule.document_fingerprint()?,
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
            profile: BundleCompatibilityProfile::V1,
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

    #[must_use]
    pub const fn profile(&self) -> BundleCompatibilityProfile {
        self.profile
    }

    #[must_use]
    pub fn compiled_strategy(&self) -> &CompiledStrategy {
        self.scenario.compiled_strategy()
    }
}

enum RawScenarioDocument {
    V1(RawScenarioV1),
    V2(RawScenarioV2),
}

impl RawScenarioDocument {
    const fn schema_version(&self) -> u64 {
        match self {
            Self::V1(_) => SCHEMA_VERSION_V1,
            Self::V2(_) => SCHEMA_VERSION_V2,
        }
    }

    fn ruleset_id(&self) -> &str {
        match self {
            Self::V1(value) => &value.ruleset_id,
            Self::V2(value) => &value.ruleset_id,
        }
    }

    fn reward_schedule_id(&self) -> &str {
        match self {
            Self::V1(value) => &value.reward_schedule_id,
            Self::V2(value) => &value.reward_schedule_id,
        }
    }
}

pub fn load_bundle(
    data_dir: impl AsRef<Path>,
    scenario_path: impl AsRef<Path>,
) -> Result<ValidatedScenarioBundle, CoreError> {
    let scenario_path = scenario_path.as_ref();
    let scenario_document = BufferedDocument::read(scenario_path)?;
    load_buffered_bundle(data_dir, &scenario_document)
}

pub fn load_buffered_bundle(
    data_dir: impl AsRef<Path>,
    scenario_document: &BufferedDocument,
) -> Result<ValidatedScenarioBundle, CoreError> {
    let parsed = parse_scenario_document(scenario_document)?;
    let catalog = Catalog::load(data_dir)?;
    compile_parsed_scenario(&catalog, parsed)
}

pub fn compile_buffered_bundle(
    catalog: &Catalog,
    scenario_document: &BufferedDocument,
) -> Result<ValidatedScenarioBundle, CoreError> {
    let parsed = parse_scenario_document(scenario_document)?;
    compile_parsed_scenario(catalog, parsed)
}

struct ParsedScenarioDocument {
    path: PathBuf,
    raw: RawScenarioDocument,
    ruleset_lookup: RulesetId,
    reward_lookup: RewardScheduleId,
}

fn parse_scenario_document(
    scenario_document: &BufferedDocument,
) -> Result<ParsedScenarioDocument, CoreError> {
    let scenario_path = scenario_document.path();
    if scenario_document.dispatch().kind != DocumentKind::Scenario {
        return Err(CoreError::validation(
            Some(scenario_path),
            "analysis input must be a scenario document",
        ));
    }
    let raw = match scenario_document.dispatch().schema_version {
        SCHEMA_VERSION_V1 => RawScenarioDocument::V1(scenario_document.parse_typed()?),
        SCHEMA_VERSION_V2 => RawScenarioDocument::V2(scenario_document.parse_typed()?),
        _ => {
            return Err(CoreError::InternalInvariant {
                message: "validated scenario dispatch has an unknown version".to_owned(),
            });
        }
    };
    let ruleset_lookup = RulesetId::new(raw.ruleset_id().to_owned())
        .map_err(|error| CoreError::validation(Some(scenario_path), error.to_string()))?;
    let reward_lookup = RewardScheduleId::new(raw.reward_schedule_id().to_owned())
        .map_err(|error| CoreError::validation(Some(scenario_path), error.to_string()))?;
    Ok(ParsedScenarioDocument {
        path: scenario_path.to_path_buf(),
        raw,
        ruleset_lookup,
        reward_lookup,
    })
}

fn compile_parsed_scenario(
    catalog: &Catalog,
    parsed: ParsedScenarioDocument,
) -> Result<ValidatedScenarioBundle, CoreError> {
    let ParsedScenarioDocument {
        path: scenario_path,
        raw,
        ruleset_lookup,
        reward_lookup,
    } = parsed;
    let ruleset = catalog
        .rulesets
        .get(&ruleset_lookup)
        .cloned()
        .ok_or_else(|| {
            CoreError::validation(
                Some(&scenario_path),
                format!("referenced ruleset {ruleset_lookup} is absent from the complete catalog"),
            )
        })?;
    let reward_schedule = catalog
        .reward_schedules
        .get(&reward_lookup)
        .cloned()
        .ok_or_else(|| {
            CoreError::validation(
                Some(&scenario_path),
                format!(
                    "referenced reward schedule {reward_lookup} is absent from the complete catalog"
                ),
            )
        })?;
    if raw.schema_version() == SCHEMA_VERSION_V1 && ruleset.schema_version() != SCHEMA_VERSION_V1 {
        return Err(CoreError::IncompatibleSchemaReference {
            scenario_schema_version: SCHEMA_VERSION_V1,
            referenced_kind: "ruleset",
            referenced_id: ruleset_lookup.to_string(),
            referenced_schema_version: ruleset.schema_version(),
            pointer: "/ruleset_id",
        });
    }
    if raw.schema_version() == SCHEMA_VERSION_V1
        && reward_schedule.schema_version() != SCHEMA_VERSION_V1
    {
        return Err(CoreError::IncompatibleSchemaReference {
            scenario_schema_version: SCHEMA_VERSION_V1,
            referenced_kind: "reward_schedule",
            referenced_id: reward_lookup.to_string(),
            referenced_schema_version: reward_schedule.schema_version(),
            pointer: "/reward_schedule_id",
        });
    }
    let (scenario, profile) = match raw {
        RawScenarioDocument::V1(raw) => (
            ValidatedScenario::from_raw(raw, &ruleset, &reward_schedule, Some(&scenario_path))?,
            BundleCompatibilityProfile::V1,
        ),
        RawScenarioDocument::V2(raw) => (
            ValidatedScenario::from_raw_v2(raw, &ruleset, &reward_schedule, Some(&scenario_path))?,
            BundleCompatibilityProfile::V2,
        ),
    };
    let scenario = Arc::new(scenario);
    let fingerprints = BundleFingerprints {
        scenario: scenario.behavior_fingerprint()?,
        ruleset: ruleset.behavior_fingerprint()?,
        reward_schedule: reward_schedule.behavior_fingerprint()?,
        scenario_document: scenario.document_fingerprint()?,
        ruleset_document: ruleset.document_fingerprint()?,
        reward_schedule_document: reward_schedule.document_fingerprint()?,
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
            scenario: scenario_path,
            ruleset: ruleset_path,
            reward_schedule: reward_path,
        },
        profile,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub schema_version: u64,
    pub document_type: String,
    pub id: String,
    pub fingerprint: SemanticFingerprint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior_fingerprint: Option<SemanticFingerprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_fingerprint: Option<SemanticFingerprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_status: Option<VerificationStatus>,
}

pub fn validate_document(
    data_dir: impl AsRef<Path>,
    document_path: impl AsRef<Path>,
) -> Result<ValidationReport, CoreError> {
    let path = document_path.as_ref();
    let document = BufferedDocument::read(path)?;
    match document.dispatch().kind {
        DocumentKind::Ruleset => {
            let value = match document.dispatch().schema_version {
                SCHEMA_VERSION_V1 => {
                    let raw: RawRulesetV1 = document.parse_typed()?;
                    CompiledRuleset::from_raw_provisional(raw, Some(path))?
                }
                SCHEMA_VERSION_V2 => {
                    let raw: RawRulesetV2 = document.parse_typed()?;
                    CompiledRuleset::from_raw_v2(raw, Some(path))?
                }
                _ => {
                    return Err(CoreError::InternalInvariant {
                        message: "validated ruleset dispatch has an unknown version".to_owned(),
                    });
                }
            };
            let is_v2 = value.schema_version() == SCHEMA_VERSION_V2;
            Ok(ValidationReport {
                valid: true,
                schema_version: value.schema_version(),
                document_type: DocumentKind::Ruleset.as_str().to_owned(),
                id: value.id().to_string(),
                fingerprint: value.document_fingerprint()?,
                behavior_fingerprint: is_v2.then(|| value.behavior_fingerprint()).transpose()?,
                document_fingerprint: is_v2.then(|| value.document_fingerprint()).transpose()?,
                verification_status: value
                    .provenance()
                    .map(|provenance| provenance.verification_status),
            })
        }
        DocumentKind::RewardSchedule => {
            let value = match document.dispatch().schema_version {
                SCHEMA_VERSION_V1 => {
                    let raw: RawRewardScheduleV1 = document.parse_typed()?;
                    RewardSchedule::from_raw(raw, Some(path))?
                }
                SCHEMA_VERSION_V2 => {
                    let raw: RawRewardScheduleV2 = document.parse_typed()?;
                    RewardSchedule::from_raw_v2(raw, Some(path))?
                }
                _ => {
                    return Err(CoreError::InternalInvariant {
                        message: "validated reward dispatch has an unknown version".to_owned(),
                    });
                }
            };
            let is_v2 = value.schema_version() == SCHEMA_VERSION_V2;
            Ok(ValidationReport {
                valid: true,
                schema_version: value.schema_version(),
                document_type: DocumentKind::RewardSchedule.as_str().to_owned(),
                id: value.id().to_string(),
                fingerprint: value.document_fingerprint()?,
                behavior_fingerprint: is_v2.then(|| value.behavior_fingerprint()).transpose()?,
                document_fingerprint: is_v2.then(|| value.document_fingerprint()).transpose()?,
                verification_status: value
                    .provenance()
                    .map(|provenance| provenance.verification_status),
            })
        }
        DocumentKind::Scenario => {
            let bundle = load_buffered_bundle(data_dir, &document)?;
            let is_v2 = bundle.profile() == BundleCompatibilityProfile::V2;
            Ok(ValidationReport {
                valid: true,
                schema_version: bundle.scenario().schema_version(),
                document_type: DocumentKind::Scenario.as_str().to_owned(),
                id: bundle.scenario().id().to_string(),
                fingerprint: bundle.fingerprints().scenario_document,
                behavior_fingerprint: is_v2.then_some(bundle.fingerprints().scenario),
                document_fingerprint: is_v2.then_some(bundle.fingerprints().scenario_document),
                verification_status: None,
            })
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::sync_channel;
    use std::thread;

    use tempfile::TempDir;

    use super::{Catalog, CatalogLoadStage};
    use crate::CoreError;

    fn workspace_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative)
    }

    fn minimal_catalog(root: &Path) {
        fs::create_dir_all(root.join("rulesets")).expect("rulesets");
        fs::create_dir_all(root.join("rewards")).expect("rewards");
        fs::copy(
            workspace_path("data/rulesets/jp_2026_07_29_provisional_v1.json"),
            root.join("rulesets/rules.json"),
        )
        .expect("rules");
        fs::copy(
            workspace_path("data/rewards/empty_v1.json"),
            root.join("rewards/rewards.json"),
        )
        .expect("rewards");
    }

    #[test]
    fn replacement_between_child_inspection_and_open_is_rejected_deterministically() {
        let temp = TempDir::new().expect("tempdir");
        minimal_catalog(temp.path());
        let root = temp.path().to_path_buf();
        let mutation_root = root.clone();
        let (trigger, wait) = sync_channel::<()>(0);
        let (finished, done) = sync_channel::<()>(0);
        let result = thread::scope(|scope| {
            scope.spawn(move || {
                wait.recv().expect("trigger");
                fs::rename(
                    mutation_root.join("rewards"),
                    mutation_root.join("rewards-old"),
                )
                .expect("rename rewards");
                fs::create_dir(mutation_root.join("rewards")).expect("replacement rewards");
                fs::copy(
                    mutation_root.join("rewards-old/rewards.json"),
                    mutation_root.join("rewards/rewards.json"),
                )
                .expect("replacement file");
                finished.send(()).expect("finished");
            });
            Catalog::load_observed(&root, |stage| {
                if stage == CatalogLoadStage::RulesDirectoryOpened {
                    trigger.send(()).expect("start mutation");
                    done.recv().expect("mutation done");
                }
            })
        });
        assert!(matches!(
            result,
            Err(CoreError::CatalogGenerationChanged { .. })
        ));
    }

    #[test]
    fn replacement_after_both_children_are_pinned_never_publishes_a_mixed_catalog() {
        let temp = TempDir::new().expect("tempdir");
        minimal_catalog(temp.path());
        let root = temp.path().to_path_buf();
        let mutation_root = root.clone();
        let (trigger, wait) = sync_channel::<()>(0);
        let (finished, done) = sync_channel::<()>(0);
        let result = thread::scope(|scope| {
            scope.spawn(move || {
                wait.recv().expect("trigger");
                fs::rename(
                    mutation_root.join("rulesets"),
                    mutation_root.join("rulesets-old"),
                )
                .expect("rename rulesets");
                fs::create_dir(mutation_root.join("rulesets")).expect("replacement rulesets");
                finished.send(()).expect("finished");
            });
            Catalog::load_observed(&root, |stage| {
                if stage == CatalogLoadStage::ChildrenOpened {
                    trigger.send(()).expect("start mutation");
                    done.recv().expect("mutation done");
                }
            })
        });
        assert!(matches!(
            result,
            Err(CoreError::CatalogGenerationChanged { .. })
        ));
    }

    #[test]
    fn candidate_replacement_after_snapshot_is_rejected_without_blocking() {
        let temp = TempDir::new().expect("tempdir");
        minimal_catalog(temp.path());
        let root = temp.path().to_path_buf();
        let mutation_root = root.clone();
        let (trigger, wait) = sync_channel::<()>(0);
        let (finished, done) = sync_channel::<()>(0);
        let result = thread::scope(|scope| {
            scope.spawn(move || {
                wait.recv().expect("trigger");
                let source = mutation_root.join("rulesets/rules.json");
                let moved = mutation_root.join("rulesets/rules-old.json");
                fs::rename(&source, &moved).expect("move candidate");
                symlink(&moved, &source).expect("replacement symlink");
                finished.send(()).expect("finished");
            });
            Catalog::load_observed(&root, |stage| {
                if stage == CatalogLoadStage::EntrySnapshotsCaptured {
                    trigger.send(()).expect("start mutation");
                    done.recv().expect("mutation done");
                }
            })
        });
        assert!(result.is_err());
    }

    #[test]
    fn root_metadata_mutation_before_publication_is_rejected() {
        let temp = TempDir::new().expect("tempdir");
        minimal_catalog(temp.path());
        let root = temp.path().to_path_buf();
        let mutation_root = root.clone();
        let (trigger, wait) = sync_channel::<()>(0);
        let (finished, done) = sync_channel::<()>(0);
        let result = thread::scope(|scope| {
            scope.spawn(move || {
                wait.recv().expect("trigger");
                fs::write(mutation_root.join("generation-marker"), b"changed")
                    .expect("change root metadata");
                finished.send(()).expect("finished");
            });
            Catalog::load_observed(&root, |stage| {
                if stage == CatalogLoadStage::RewardsLoaded {
                    trigger.send(()).expect("start mutation");
                    done.recv().expect("mutation done");
                }
            })
        });
        assert!(matches!(
            result,
            Err(CoreError::CatalogGenerationChanged { .. })
        ));
    }

    #[test]
    fn ambient_root_path_replacement_never_redirects_pinned_authority() {
        let temp = TempDir::new().expect("tempdir");
        let selected = temp.path().join("selected");
        minimal_catalog(&selected);
        let moved = temp.path().join("selected-original");
        let mutation_selected = selected.clone();
        let mutation_moved = moved.clone();
        let (trigger, wait) = sync_channel::<()>(0);
        let (finished, done) = sync_channel::<()>(0);
        let result = thread::scope(|scope| {
            scope.spawn(move || {
                wait.recv().expect("trigger");
                fs::rename(&mutation_selected, &mutation_moved).expect("move selected root");
                fs::create_dir(&mutation_selected).expect("replacement root");
                fs::create_dir(mutation_selected.join("rulesets")).expect("replacement rulesets");
                fs::create_dir(mutation_selected.join("rewards")).expect("replacement rewards");
                finished.send(()).expect("finished");
            });
            Catalog::load_observed(&selected, |stage| {
                if stage == CatalogLoadStage::RootPinned {
                    trigger.send(()).expect("start mutation");
                    done.recv().expect("mutation done");
                }
            })
        });
        match result {
            Ok(catalog) => {
                assert_eq!(catalog.rulesets().len(), 1);
                assert!(
                    catalog
                        .rulesets()
                        .keys()
                        .any(|id| id.as_str() == "jp_2026_07_29_provisional_v1")
                );
            }
            Err(CoreError::CatalogGenerationChanged { .. }) => {}
            Err(error) => panic!("unexpected root replacement result: {error}"),
        }
    }

    #[test]
    fn missing_child_during_final_verification_is_a_generation_change() {
        let temp = TempDir::new().expect("tempdir");
        minimal_catalog(temp.path());
        let root = temp.path().to_path_buf();
        let mutation_root = root.clone();
        let (trigger, wait) = sync_channel::<()>(0);
        let (finished, done) = sync_channel::<()>(0);
        let result = thread::scope(|scope| {
            scope.spawn(move || {
                wait.recv().expect("trigger");
                fs::rename(
                    mutation_root.join("rewards"),
                    mutation_root.join("rewards-removed"),
                )
                .expect("remove child name");
                finished.send(()).expect("finished");
            });
            Catalog::load_observed(&root, |stage| {
                if stage == CatalogLoadStage::BeforeGenerationVerification {
                    trigger.send(()).expect("start mutation");
                    done.recv().expect("mutation done");
                }
            })
        });
        assert!(matches!(
            result,
            Err(CoreError::CatalogGenerationChanged { .. })
        ));
    }
}
