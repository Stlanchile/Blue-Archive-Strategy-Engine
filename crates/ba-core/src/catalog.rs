use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::CoreError;
use crate::fingerprint::SemanticFingerprint;
use crate::fs_secure::{DirectoryEntrySnapshot, PinnedDirectory, is_json_candidate};
use crate::id::{RewardScheduleId, RulesetId};
use crate::model::{CompiledRuleset, CompiledStrategy, RewardSchedule, ValidatedScenario};
use crate::model_v3::{CompiledRulesetV3, CompiledStrategyV3, ValidatedScenarioV3};
use crate::profile::{DOCUMENT_SCHEMA_VERSION_V2, DOCUMENT_SCHEMA_VERSION_V3, DocumentProfile};
use crate::provenance_v3::ProvenanceStatusV3;
use crate::reward_schedule_v3::RewardScheduleV3;
use crate::schema::{
    DocumentKind, RawRewardScheduleV2, RawRewardScheduleV3, RawRulesetV2, RawRulesetV3,
    RawScenarioV2, RawScenarioV3, VerificationStatus,
};
use crate::strict_json::BufferedDocument;

#[derive(Debug)]
pub struct Catalog {
    rulesets: BTreeMap<RulesetId, Arc<CompiledRuleset>>,
    reward_schedules: BTreeMap<RewardScheduleId, Arc<RewardSchedule>>,
    rulesets_v3: BTreeMap<RulesetId, Arc<CompiledRulesetV3>>,
    reward_schedules_v3: BTreeMap<RewardScheduleId, Arc<RewardScheduleV3>>,
    ruleset_paths: BTreeMap<RulesetId, PathBuf>,
    reward_paths: BTreeMap<RewardScheduleId, PathBuf>,
    ruleset_paths_v3: BTreeMap<RulesetId, PathBuf>,
    reward_paths_v3: BTreeMap<RewardScheduleId, PathBuf>,
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
        let mut rulesets_v3 = BTreeMap::new();
        let mut ruleset_paths = BTreeMap::new();
        let mut ruleset_paths_v3 = BTreeMap::new();
        let mut all_ruleset_ids = BTreeSet::new();
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
            let (id, profile) = match document.dispatch().schema_version {
                DOCUMENT_SCHEMA_VERSION_V2 => {
                    let raw: RawRulesetV2 = document.parse_typed()?;
                    let ruleset = Arc::new(CompiledRuleset::from_raw(raw, Some(&path))?);
                    let id = ruleset.id().clone();
                    rulesets.insert(id.clone(), ruleset);
                    ruleset_paths.insert(id.clone(), path.clone());
                    (id, DocumentProfile::V2)
                }
                DOCUMENT_SCHEMA_VERSION_V3 => {
                    let raw: RawRulesetV3 = document.parse_typed()?;
                    let ruleset = Arc::new(CompiledRulesetV3::from_raw(raw, Some(&path))?);
                    let id = ruleset.id().clone();
                    rulesets_v3.insert(id.clone(), ruleset);
                    ruleset_paths_v3.insert(id.clone(), path.clone());
                    (id, DocumentProfile::V3)
                }
                _ => {
                    return Err(CoreError::InternalInvariant {
                        message: "dispatch admitted an unsupported ruleset schema".to_owned(),
                    });
                }
            };
            if !all_ruleset_ids.insert(id.clone()) {
                return Err(CoreError::validation(
                    Some(&path),
                    format!("duplicate catalog ruleset ID {id}"),
                ));
            }
            let _ = profile;
        }
        observer(CatalogLoadStage::RulesetsLoaded);

        let mut reward_schedules = BTreeMap::new();
        let mut reward_schedules_v3 = BTreeMap::new();
        let mut reward_paths = BTreeMap::new();
        let mut reward_paths_v3 = BTreeMap::new();
        let mut all_reward_ids = BTreeSet::new();
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
            let id = match document.dispatch().schema_version {
                DOCUMENT_SCHEMA_VERSION_V2 => {
                    let raw: RawRewardScheduleV2 = document.parse_typed()?;
                    let rewards = Arc::new(RewardSchedule::from_raw(raw, Some(&path))?);
                    let id = rewards.id().clone();
                    reward_schedules.insert(id.clone(), rewards);
                    reward_paths.insert(id.clone(), path.clone());
                    id
                }
                DOCUMENT_SCHEMA_VERSION_V3 => {
                    let raw: RawRewardScheduleV3 = document.parse_typed()?;
                    let rewards = Arc::new(RewardScheduleV3::from_raw(raw, Some(&path))?);
                    let id = rewards.id().clone();
                    reward_schedules_v3.insert(id.clone(), rewards);
                    reward_paths_v3.insert(id.clone(), path.clone());
                    id
                }
                _ => {
                    return Err(CoreError::InternalInvariant {
                        message: "dispatch admitted an unsupported reward schema".to_owned(),
                    });
                }
            };
            if !all_reward_ids.insert(id.clone()) {
                return Err(CoreError::validation(
                    Some(&path),
                    format!("duplicate catalog reward schedule ID {id}"),
                ));
            }
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
            rulesets_v3,
            reward_schedules_v3,
            ruleset_paths,
            reward_paths,
            ruleset_paths_v3,
            reward_paths_v3,
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
    pub fn rulesets_v3(&self) -> &BTreeMap<RulesetId, Arc<CompiledRulesetV3>> {
        &self.rulesets_v3
    }

    #[must_use]
    pub fn reward_schedules_v3(&self) -> &BTreeMap<RewardScheduleId, Arc<RewardScheduleV3>> {
        &self.reward_schedules_v3
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
    pub fn ruleset_v3(&self, id: &RulesetId) -> Option<&CompiledRulesetV3> {
        self.rulesets_v3.get(id).map(AsRef::as_ref)
    }

    #[must_use]
    pub fn reward_schedule_v3(&self, id: &RewardScheduleId) -> Option<&RewardScheduleV3> {
        self.reward_schedules_v3.get(id).map(AsRef::as_ref)
    }

    #[must_use]
    pub fn ruleset_path(&self, id: &RulesetId) -> Option<&Path> {
        self.ruleset_paths.get(id).map(PathBuf::as_path)
    }

    #[must_use]
    pub fn reward_schedule_path(&self, id: &RewardScheduleId) -> Option<&Path> {
        self.reward_paths.get(id).map(PathBuf::as_path)
    }

    #[must_use]
    pub fn ruleset_path_v3(&self, id: &RulesetId) -> Option<&Path> {
        self.ruleset_paths_v3.get(id).map(PathBuf::as_path)
    }

    #[must_use]
    pub fn reward_schedule_path_v3(&self, id: &RewardScheduleId) -> Option<&Path> {
        self.reward_paths_v3.get(id).map(PathBuf::as_path)
    }

    #[must_use]
    pub fn contains_v3(&self) -> bool {
        !self.rulesets_v3.is_empty() || !self.reward_schedules_v3.is_empty()
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
        raw_scenario: RawScenarioV2,
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
    pub fn compiled_strategy(&self) -> &CompiledStrategy {
        self.scenario.compiled_strategy()
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedScenarioBundleV3 {
    scenario: Arc<ValidatedScenarioV3>,
    ruleset: Arc<CompiledRulesetV3>,
    reward_schedule: Arc<RewardScheduleV3>,
    fingerprints: BundleFingerprints,
    source_paths: SourcePaths,
}

impl ValidatedScenarioBundleV3 {
    pub fn from_programmatic(
        raw_scenario: RawScenarioV3,
        ruleset: CompiledRulesetV3,
        reward_schedule: RewardScheduleV3,
    ) -> Result<Self, CoreError> {
        let scenario =
            ValidatedScenarioV3::from_raw(raw_scenario, &ruleset, &reward_schedule, None)?;
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
                scenario: PathBuf::from("<programmatic-scenario-v3>"),
                ruleset: PathBuf::from("<programmatic-ruleset-v3>"),
                reward_schedule: PathBuf::from("<programmatic-reward-schedule-v3>"),
            },
        })
    }

    #[must_use]
    pub fn scenario(&self) -> &ValidatedScenarioV3 {
        &self.scenario
    }

    #[must_use]
    pub fn ruleset(&self) -> &CompiledRulesetV3 {
        &self.ruleset
    }

    #[must_use]
    pub fn reward_schedule(&self) -> &RewardScheduleV3 {
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
    pub fn compiled_strategy(&self) -> &CompiledStrategyV3 {
        self.scenario.compiled_strategy()
    }
}

#[derive(Debug, Clone)]
pub enum AnyValidatedScenarioBundle {
    V2(ValidatedScenarioBundle),
    V3(ValidatedScenarioBundleV3),
}

impl AnyValidatedScenarioBundle {
    #[must_use]
    pub const fn profile(&self) -> DocumentProfile {
        match self {
            Self::V2(_) => DocumentProfile::V2,
            Self::V3(_) => DocumentProfile::V3,
        }
    }

    #[must_use]
    pub const fn as_v2(&self) -> Option<&ValidatedScenarioBundle> {
        match self {
            Self::V2(bundle) => Some(bundle),
            Self::V3(_) => None,
        }
    }

    #[must_use]
    pub const fn as_v3(&self) -> Option<&ValidatedScenarioBundleV3> {
        match self {
            Self::V2(_) => None,
            Self::V3(bundle) => Some(bundle),
        }
    }
}

pub fn load_any_bundle(
    data_dir: impl AsRef<Path>,
    scenario_path: impl AsRef<Path>,
) -> Result<AnyValidatedScenarioBundle, CoreError> {
    let scenario_document = BufferedDocument::read(scenario_path)?;
    load_any_buffered_bundle(data_dir, &scenario_document)
}

pub fn load_any_buffered_bundle(
    data_dir: impl AsRef<Path>,
    scenario_document: &BufferedDocument,
) -> Result<AnyValidatedScenarioBundle, CoreError> {
    let catalog = Catalog::load(data_dir)?;
    compile_any_buffered_bundle(&catalog, scenario_document)
}

pub fn compile_any_buffered_bundle(
    catalog: &Catalog,
    scenario_document: &BufferedDocument,
) -> Result<AnyValidatedScenarioBundle, CoreError> {
    if scenario_document.dispatch().kind != DocumentKind::Scenario {
        return Err(CoreError::validation(
            Some(scenario_document.path()),
            "analysis input must be a scenario document",
        ));
    }
    match scenario_document.dispatch().schema_version {
        DOCUMENT_SCHEMA_VERSION_V2 => {
            let parsed = parse_scenario_document(scenario_document)?;
            compile_parsed_scenario(catalog, parsed).map(AnyValidatedScenarioBundle::V2)
        }
        DOCUMENT_SCHEMA_VERSION_V3 => {
            let parsed = parse_scenario_document_v3(scenario_document)?;
            compile_parsed_scenario_v3(catalog, parsed).map(AnyValidatedScenarioBundle::V3)
        }
        _ => Err(CoreError::InternalInvariant {
            message: "dispatch admitted an unsupported scenario schema".to_owned(),
        }),
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
    raw: RawScenarioV2,
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
    let raw: RawScenarioV2 = scenario_document.parse_typed()?;
    let ruleset_lookup = RulesetId::new(raw.ruleset_id.clone())
        .map_err(|error| CoreError::validation(Some(scenario_path), error.to_string()))?;
    let reward_lookup = RewardScheduleId::new(raw.reward_schedule_id.clone())
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
            if catalog.rulesets_v3.contains_key(&ruleset_lookup) {
                return CoreError::validation(
                    Some(&scenario_path),
                    format!(
                        "schema-v2 scenario references schema-v3 ruleset {ruleset_lookup}; mixed-profile bundles are unsupported"
                    ),
                );
            }
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
            if catalog.reward_schedules_v3.contains_key(&reward_lookup) {
                return CoreError::validation(
                    Some(&scenario_path),
                    format!(
                        "schema-v2 scenario references schema-v3 reward schedule {reward_lookup}; mixed-profile bundles are unsupported"
                    ),
                );
            }
            CoreError::validation(
                Some(&scenario_path),
                format!(
                    "referenced reward schedule {reward_lookup} is absent from the complete catalog"
                ),
            )
        })?;
    let scenario =
        ValidatedScenario::from_raw(raw, &ruleset, &reward_schedule, Some(&scenario_path))?;
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
    })
}

struct ParsedScenarioDocumentV3 {
    path: PathBuf,
    raw: RawScenarioV3,
    ruleset_lookup: RulesetId,
    reward_lookup: RewardScheduleId,
}

fn parse_scenario_document_v3(
    scenario_document: &BufferedDocument,
) -> Result<ParsedScenarioDocumentV3, CoreError> {
    let path = scenario_document.path();
    let raw: RawScenarioV3 = scenario_document.parse_typed()?;
    let ruleset_lookup = RulesetId::new(raw.ruleset_id.clone())
        .map_err(|error| CoreError::validation(Some(path), error.to_string()))?;
    let reward_lookup = RewardScheduleId::new(raw.reward_schedule_id.clone())
        .map_err(|error| CoreError::validation(Some(path), error.to_string()))?;
    Ok(ParsedScenarioDocumentV3 {
        path: path.to_path_buf(),
        raw,
        ruleset_lookup,
        reward_lookup,
    })
}

fn compile_parsed_scenario_v3(
    catalog: &Catalog,
    parsed: ParsedScenarioDocumentV3,
) -> Result<ValidatedScenarioBundleV3, CoreError> {
    let ParsedScenarioDocumentV3 {
        path: scenario_path,
        raw,
        ruleset_lookup,
        reward_lookup,
    } = parsed;
    let ruleset = catalog
        .rulesets_v3
        .get(&ruleset_lookup)
        .cloned()
        .ok_or_else(|| {
            if catalog.rulesets.contains_key(&ruleset_lookup) {
                return CoreError::validation(
                    Some(&scenario_path),
                    format!(
                        "schema-v3 scenario references schema-v2 ruleset {ruleset_lookup}; mixed-profile bundles are unsupported"
                    ),
                );
            }
            CoreError::validation(
                Some(&scenario_path),
                format!("referenced ruleset {ruleset_lookup} is absent from the complete catalog"),
            )
        })?;
    let reward_schedule = catalog
        .reward_schedules_v3
        .get(&reward_lookup)
        .cloned()
        .ok_or_else(|| {
            if catalog.reward_schedules.contains_key(&reward_lookup) {
                return CoreError::validation(
                    Some(&scenario_path),
                    format!(
                        "schema-v3 scenario references schema-v2 reward schedule {reward_lookup}; mixed-profile bundles are unsupported"
                    ),
                );
            }
            CoreError::validation(
                Some(&scenario_path),
                format!(
                    "referenced reward schedule {reward_lookup} is absent from the complete catalog"
                ),
            )
        })?;
    let scenario = Arc::new(ValidatedScenarioV3::from_raw(
        raw,
        &ruleset,
        &reward_schedule,
        Some(&scenario_path),
    )?);
    let fingerprints = BundleFingerprints {
        scenario: scenario.behavior_fingerprint()?,
        ruleset: ruleset.behavior_fingerprint()?,
        reward_schedule: reward_schedule.behavior_fingerprint()?,
        scenario_document: scenario.document_fingerprint()?,
        ruleset_document: ruleset.document_fingerprint()?,
        reward_schedule_document: reward_schedule.document_fingerprint()?,
    };
    let ruleset_path = catalog
        .ruleset_paths_v3
        .get(&ruleset_lookup)
        .cloned()
        .ok_or(CoreError::InternalInvariant {
            message: "v3 ruleset catalog path missing after successful lookup".to_owned(),
        })?;
    let reward_path = catalog.reward_paths_v3.get(&reward_lookup).cloned().ok_or(
        CoreError::InternalInvariant {
            message: "v3 reward catalog path missing after successful lookup".to_owned(),
        },
    )?;
    Ok(ValidatedScenarioBundleV3 {
        scenario,
        ruleset,
        reward_schedule,
        fingerprints,
        source_paths: SourcePaths {
            scenario: scenario_path,
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
    pub behavior_fingerprint: SemanticFingerprint,
    pub document_fingerprint: SemanticFingerprint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_status: Option<VerificationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_status: Option<ProvenanceStatusV3>,
}

pub fn validate_document(
    data_dir: impl AsRef<Path>,
    document_path: impl AsRef<Path>,
) -> Result<ValidationReport, CoreError> {
    let path = document_path.as_ref();
    let document = BufferedDocument::read(path)?;
    match (document.dispatch().schema_version, document.dispatch().kind) {
        (DOCUMENT_SCHEMA_VERSION_V2, DocumentKind::Ruleset) => {
            let raw: RawRulesetV2 = document.parse_typed()?;
            let value = CompiledRuleset::from_raw(raw, Some(path))?;
            Ok(ValidationReport {
                valid: true,
                schema_version: value.schema_version(),
                document_type: DocumentKind::Ruleset.as_str().to_owned(),
                id: value.id().to_string(),
                behavior_fingerprint: value.behavior_fingerprint()?,
                document_fingerprint: value.document_fingerprint()?,
                verification_status: Some(value.provenance().verification_status),
                provenance_status: None,
            })
        }
        (DOCUMENT_SCHEMA_VERSION_V2, DocumentKind::RewardSchedule) => {
            let raw: RawRewardScheduleV2 = document.parse_typed()?;
            let value = RewardSchedule::from_raw(raw, Some(path))?;
            Ok(ValidationReport {
                valid: true,
                schema_version: value.schema_version(),
                document_type: DocumentKind::RewardSchedule.as_str().to_owned(),
                id: value.id().to_string(),
                behavior_fingerprint: value.behavior_fingerprint()?,
                document_fingerprint: value.document_fingerprint()?,
                verification_status: Some(value.provenance().verification_status),
                provenance_status: None,
            })
        }
        (DOCUMENT_SCHEMA_VERSION_V2, DocumentKind::Scenario) => {
            let bundle = load_buffered_bundle(data_dir, &document)?;
            Ok(ValidationReport {
                valid: true,
                schema_version: bundle.scenario().schema_version(),
                document_type: DocumentKind::Scenario.as_str().to_owned(),
                id: bundle.scenario().id().to_string(),
                behavior_fingerprint: bundle.fingerprints().scenario,
                document_fingerprint: bundle.fingerprints().scenario_document,
                verification_status: None,
                provenance_status: None,
            })
        }
        (DOCUMENT_SCHEMA_VERSION_V3, DocumentKind::Ruleset) => {
            let raw: RawRulesetV3 = document.parse_typed()?;
            let value = CompiledRulesetV3::from_raw(raw, Some(path))?;
            Ok(ValidationReport {
                valid: true,
                schema_version: value.schema_version(),
                document_type: DocumentKind::Ruleset.as_str().to_owned(),
                id: value.id().to_string(),
                behavior_fingerprint: value.behavior_fingerprint()?,
                document_fingerprint: value.document_fingerprint()?,
                verification_status: None,
                provenance_status: Some(value.provenance().provenance_status),
            })
        }
        (DOCUMENT_SCHEMA_VERSION_V3, DocumentKind::RewardSchedule) => {
            let raw: RawRewardScheduleV3 = document.parse_typed()?;
            let value = RewardScheduleV3::from_raw(raw, Some(path))?;
            Ok(ValidationReport {
                valid: true,
                schema_version: value.schema_version(),
                document_type: DocumentKind::RewardSchedule.as_str().to_owned(),
                id: value.id().to_string(),
                behavior_fingerprint: value.behavior_fingerprint()?,
                document_fingerprint: value.document_fingerprint()?,
                verification_status: None,
                provenance_status: Some(value.provenance().provenance_status),
            })
        }
        (DOCUMENT_SCHEMA_VERSION_V3, DocumentKind::Scenario) => {
            let bundle = load_any_buffered_bundle(data_dir, &document)?;
            let Some(bundle) = bundle.as_v3() else {
                return Err(CoreError::InternalInvariant {
                    message: "v3 scenario validation produced a v2 bundle".to_owned(),
                });
            };
            Ok(ValidationReport {
                valid: true,
                schema_version: bundle.scenario().schema_version(),
                document_type: DocumentKind::Scenario.as_str().to_owned(),
                id: bundle.scenario().id().to_string(),
                behavior_fingerprint: bundle.fingerprints().scenario,
                document_fingerprint: bundle.fingerprints().scenario_document,
                verification_status: None,
                provenance_status: None,
            })
        }
        _ => Err(CoreError::InternalInvariant {
            message: "dispatch admitted an unsupported document pair".to_owned(),
        }),
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
            workspace_path("data/rulesets/jp_2026_07_29_provisional_v2.json"),
            root.join("rulesets/rules.json"),
        )
        .expect("rules");
        fs::copy(
            workspace_path("data/rewards/jp_2026_07_29_empty_v2.json"),
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
                        .any(|id| id.as_str() == "jp_2026_07_29_provisional_v2")
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
