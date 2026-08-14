use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ba_core::document::read_document_directory;
use ba_core::{
    AnyValidatedScenarioBundle, Catalog, CoreError, DocumentKind, DocumentProfile,
    RewardScheduleId, RulesetId, ScenarioId, ValidatedScenarioBundle, ValidatedScenarioBundleV3,
    compile_any_buffered_bundle, load_any_buffered_bundle, load_any_bundle, validate_document,
};
use ba_engine::{
    ExactSolverOptions, analyze_exact_detailed, analyze_exact_v3, compare, compare_v3,
    simulate_monte_carlo, simulate_monte_carlo_v3, simulate_trace, simulate_trace_v3,
};
use rand_core::{OsRng, TryRngCore};
use serde::Serialize;

use crate::args::{
    CatalogCommand, CatalogInspectKind, CatalogListSelector, Cli, Command, OutputFormat,
    ScenarioCommand,
};
use crate::errors::AppError;
use crate::render;
use crate::resolve::{
    default_example_directory, default_golden_directory, is_bare_scenario_name, resolve_scenario,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct RenderMode {
    pub(crate) format: OutputFormat,
    pub(crate) diagnostics: bool,
}

pub(crate) fn execute(cli: Cli) -> (RenderMode, Result<String, AppError>) {
    let Cli {
        data_dir,
        scenario_dir,
        command,
    } = cli;
    match command {
        Command::Validate {
            document,
            format,
            diagnostics,
        } => {
            let result = validate_document(&data_dir, document)
                .map_err(AppError::from)
                .and_then(|value| render::validation(&value, format));
            (
                RenderMode {
                    format,
                    diagnostics,
                },
                result,
            )
        }
        Command::Analyze { scenario, format } => {
            let path = resolve_scenario(&data_dir, scenario_dir.as_deref(), &scenario);
            let result = load_scenario_bundle(&data_dir, scenario_dir.as_deref(), &scenario, path)
                .map_err(AppError::from)
                .and_then(|bundle| match bundle {
                    AnyValidatedScenarioBundle::V2(bundle) => {
                        analyze_exact_detailed(&bundle, ExactSolverOptions::default())
                            .map_err(AppError::from)
                            .and_then(|value| render::exact(&value, format))
                    }
                    AnyValidatedScenarioBundle::V3(bundle) => {
                        analyze_exact_v3(&bundle, ExactSolverOptions::default())
                            .map_err(AppError::from)
                            .and_then(|value| render::exact_v3(&value, format))
                    }
                });
            (
                RenderMode {
                    format,
                    diagnostics: false,
                },
                result,
            )
        }
        Command::Simulate {
            scenario,
            runs,
            seed,
            trace,
            format,
        } => {
            let path = resolve_scenario(&data_dir, scenario_dir.as_deref(), &scenario);
            let result = load_scenario_bundle(&data_dir, scenario_dir.as_deref(), &scenario, path)
                .map_err(AppError::from)
                .and_then(|bundle| {
                    if trace && runs.get() != 1 {
                        return Err(AppError::Usage(
                            "simulate --trace requires --runs 1".to_owned(),
                        ));
                    }
                    let seed = resolve_master_seed(seed)?;
                    match bundle {
                        AnyValidatedScenarioBundle::V2(bundle) if trace => {
                            simulate_trace(&bundle, seed)
                                .map(|value| SimulationOutput::Trace(Box::new(value)))
                                .map_err(AppError::from)
                        }
                        AnyValidatedScenarioBundle::V2(bundle) => {
                            simulate_monte_carlo(&bundle, runs, seed)
                                .map(|value| SimulationOutput::Aggregate(Box::new(value)))
                                .map_err(AppError::from)
                        }
                        AnyValidatedScenarioBundle::V3(bundle) if trace => {
                            simulate_trace_v3(&bundle, seed)
                                .map(|value| SimulationOutput::TraceV3(Box::new(value)))
                                .map_err(AppError::from)
                        }
                        AnyValidatedScenarioBundle::V3(bundle) => {
                            simulate_monte_carlo_v3(&bundle, runs, seed)
                                .map(|value| SimulationOutput::AggregateV3(Box::new(value)))
                                .map_err(AppError::from)
                        }
                    }
                })
                .and_then(|value| match value {
                    SimulationOutput::Trace(value) => render::trace(&value, format),
                    SimulationOutput::Aggregate(value) => render::monte_carlo(&value, format),
                    SimulationOutput::TraceV3(value) => render::trace_v3(&value, format),
                    SimulationOutput::AggregateV3(value) => render::monte_carlo_v3(&value, format),
                });
            (
                RenderMode {
                    format,
                    diagnostics: false,
                },
                result,
            )
        }
        Command::Compare {
            scenario,
            runs,
            seed,
            format,
        } => {
            let path = resolve_scenario(&data_dir, scenario_dir.as_deref(), &scenario);
            let result = load_scenario_bundle(&data_dir, scenario_dir.as_deref(), &scenario, path)
                .map_err(AppError::from)
                .and_then(|bundle| {
                    let seed = resolve_master_seed(seed)?;
                    match bundle {
                        AnyValidatedScenarioBundle::V2(bundle) => compare(&bundle, runs, seed)
                            .map_err(AppError::from)
                            .and_then(|value| render::comparison(&value, format)),
                        AnyValidatedScenarioBundle::V3(bundle) => compare_v3(&bundle, runs, seed)
                            .map_err(AppError::from)
                            .and_then(|value| render::comparison_v3(&value, format)),
                    }
                });
            (
                RenderMode {
                    format,
                    diagnostics: false,
                },
                result,
            )
        }
        Command::Catalog { command } => {
            execute_catalog(command, &data_dir, scenario_dir.as_deref())
        }
        Command::Scenario { command } => {
            execute_scenario(command, &data_dir, scenario_dir.as_deref())
        }
    }
}

enum SimulationOutput {
    Trace(Box<ba_engine::RunTraceResult>),
    Aggregate(Box<ba_engine::MonteCarloAnalysisResult>),
    TraceV3(Box<ba_engine::RunTraceResultV3>),
    AggregateV3(Box<ba_engine::MonteCarloAnalysisResultV3>),
}

fn execute_catalog(
    command: CatalogCommand,
    data_dir: &Path,
    scenario_dir: Option<&Path>,
) -> (RenderMode, Result<String, AppError>) {
    match command {
        CatalogCommand::List { selector, format } => {
            let result = catalog_list(data_dir, scenario_dir, selector)
                .and_then(|value| render::structured(&value, format));
            (
                RenderMode {
                    format,
                    diagnostics: false,
                },
                result,
            )
        }
        CatalogCommand::Inspect { kind, id, format } => {
            let result = catalog_inspect(data_dir, scenario_dir, kind, &id)
                .and_then(|value| render::structured(&value, format));
            (
                RenderMode {
                    format,
                    diagnostics: false,
                },
                result,
            )
        }
    }
}

fn execute_scenario(
    command: ScenarioCommand,
    data_dir: &Path,
    scenario_dir: Option<&Path>,
) -> (RenderMode, Result<String, AppError>) {
    match command {
        ScenarioCommand::Explain { scenario, format } => {
            let path = resolve_scenario(data_dir, scenario_dir, &scenario);
            let result = load_scenario_bundle(data_dir, scenario_dir, &scenario, path)
                .map_err(AppError::from)
                .and_then(|bundle| render::structured(&scenario_explanation_any(&bundle), format));
            (
                RenderMode {
                    format,
                    diagnostics: false,
                },
                result,
            )
        }
        ScenarioCommand::Template {
            schema_version,
            scenario_id,
            ruleset,
            reward_schedule,
            target_count,
        } => {
            let result = scenario_template(
                data_dir,
                schema_version,
                &scenario_id,
                &ruleset,
                &reward_schedule,
                target_count,
            )
            .and_then(|value| render::render_json(&value));
            (
                RenderMode {
                    format: OutputFormat::Json,
                    diagnostics: false,
                },
                result,
            )
        }
    }
}

#[derive(Debug, Serialize)]
struct CatalogListOutput {
    output_schema_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    rulesets: Option<Vec<CatalogDocumentSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reward_schedules: Option<Vec<CatalogDocumentSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenarios: Option<Vec<ScenarioSummary>>,
}

#[derive(Debug, Serialize)]
struct CatalogDocumentSummary {
    id: String,
    schema_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_profile: Option<DocumentProfile>,
    behavior_fingerprint: String,
    document_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_status: Option<ba_core::VerificationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance_status: Option<ba_core::ProvenanceStatusV3>,
}

#[derive(Debug, Serialize)]
struct ScenarioSummary {
    id: String,
    schema_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_profile: Option<DocumentProfile>,
    behavior_fingerprint: String,
    document_fingerprint: String,
}

fn catalog_list(
    data_dir: &Path,
    scenario_dir: Option<&Path>,
    selector: CatalogListSelector,
) -> Result<CatalogListOutput, AppError> {
    let catalog = Catalog::load(data_dir)?;
    let include_rulesets = matches!(
        selector,
        CatalogListSelector::All | CatalogListSelector::Rulesets
    );
    let include_rewards = matches!(
        selector,
        CatalogListSelector::All | CatalogListSelector::RewardSchedules
    );
    let include_scenarios = matches!(
        selector,
        CatalogListSelector::All | CatalogListSelector::Scenarios
    );
    let scenarios = include_scenarios
        .then(|| scenario_summaries(data_dir, scenario_dir, &catalog))
        .transpose()?;
    let emits_v3 = (include_rulesets && !catalog.rulesets_v3().is_empty())
        || (include_rewards && !catalog.reward_schedules_v3().is_empty())
        || scenarios
            .as_ref()
            .is_some_and(|values| values.iter().any(|value| value.schema_version == 3));
    let output_schema_version = if emits_v3 { 2 } else { 1 };
    let profile = |value| (output_schema_version == 2).then_some(value);
    let rulesets = include_rulesets
        .then(|| {
            catalog
                .rulesets()
                .values()
                .map(|value| {
                    Ok(CatalogDocumentSummary {
                        id: value.id().to_string(),
                        schema_version: value.schema_version(),
                        document_profile: profile(DocumentProfile::V2),
                        behavior_fingerprint: value.behavior_fingerprint()?.to_hex(),
                        document_fingerprint: value.document_fingerprint()?.to_hex(),
                        verification_status: Some(value.provenance().verification_status),
                        provenance_status: None,
                    })
                })
                .chain(catalog.rulesets_v3().values().map(|value| {
                    Ok(CatalogDocumentSummary {
                        id: value.id().to_string(),
                        schema_version: value.schema_version(),
                        document_profile: profile(DocumentProfile::V3),
                        behavior_fingerprint: value.behavior_fingerprint()?.to_hex(),
                        document_fingerprint: value.document_fingerprint()?.to_hex(),
                        verification_status: None,
                        provenance_status: Some(value.provenance().provenance_status),
                    })
                }))
                .collect::<Result<Vec<_>, CoreError>>()
        })
        .transpose()?;
    let reward_schedules = include_rewards
        .then(|| {
            catalog
                .reward_schedules()
                .values()
                .map(|value| {
                    Ok(CatalogDocumentSummary {
                        id: value.id().to_string(),
                        schema_version: value.schema_version(),
                        document_profile: profile(DocumentProfile::V2),
                        behavior_fingerprint: value.behavior_fingerprint()?.to_hex(),
                        document_fingerprint: value.document_fingerprint()?.to_hex(),
                        verification_status: Some(value.provenance().verification_status),
                        provenance_status: None,
                    })
                })
                .chain(catalog.reward_schedules_v3().values().map(|value| {
                    Ok(CatalogDocumentSummary {
                        id: value.id().to_string(),
                        schema_version: value.schema_version(),
                        document_profile: profile(DocumentProfile::V3),
                        behavior_fingerprint: value.behavior_fingerprint()?.to_hex(),
                        document_fingerprint: value.document_fingerprint()?.to_hex(),
                        verification_status: None,
                        provenance_status: Some(value.provenance().provenance_status),
                    })
                }))
                .collect::<Result<Vec<_>, CoreError>>()
        })
        .transpose()?;
    let scenarios = scenarios.map(|mut values| {
        for value in &mut values {
            value.document_profile = profile(if value.schema_version == 2 {
                DocumentProfile::V2
            } else {
                DocumentProfile::V3
            });
        }
        values
    });
    Ok(CatalogListOutput {
        output_schema_version,
        rulesets,
        reward_schedules,
        scenarios,
    })
}

fn scenario_summaries(
    data_dir: &Path,
    scenario_dir: Option<&Path>,
    catalog: &Catalog,
) -> Result<Vec<ScenarioSummary>, AppError> {
    let directories = scenario_dir.map_or_else(
        || {
            vec![
                default_golden_directory(data_dir),
                default_example_directory(data_dir),
            ]
        },
        |value| vec![value.to_path_buf()],
    );
    let mut seen_ids = BTreeMap::new();
    let mut summaries = Vec::new();
    for directory in directories {
        for document in read_document_directory(&directory)? {
            if document.dispatch().kind != DocumentKind::Scenario {
                return Err(CoreError::validation(
                    Some(document.path()),
                    "scenario catalog contains a non-scenario document",
                )
                .into());
            }
            let bundle = compile_any_buffered_bundle(catalog, &document)?;
            let (id, schema_version, fingerprints) = match &bundle {
                AnyValidatedScenarioBundle::V2(bundle) => (
                    bundle.scenario().id().to_string(),
                    bundle.scenario().schema_version(),
                    bundle.fingerprints(),
                ),
                AnyValidatedScenarioBundle::V3(bundle) => (
                    bundle.scenario().id().to_string(),
                    bundle.scenario().schema_version(),
                    bundle.fingerprints(),
                ),
            };
            if seen_ids.insert(id.clone(), ()).is_some() {
                return Err(CoreError::validation(
                    Some(document.path()),
                    format!("duplicate catalog scenario ID {id}"),
                )
                .into());
            }
            summaries.push(ScenarioSummary {
                id,
                schema_version,
                document_profile: None,
                behavior_fingerprint: fingerprints.scenario.to_hex(),
                document_fingerprint: fingerprints.scenario_document.to_hex(),
            });
        }
    }
    summaries.sort_by(|left, right| {
        left.schema_version
            .cmp(&right.schema_version)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(summaries)
}

fn catalog_inspect(
    data_dir: &Path,
    scenario_dir: Option<&Path>,
    kind: CatalogInspectKind,
    id: &str,
) -> Result<serde_json::Value, AppError> {
    let catalog = Catalog::load(data_dir)?;
    match kind {
        CatalogInspectKind::Rulesets => {
            let id = RulesetId::new(id)
                .map_err(|error| CoreError::validation(None, error.to_string()))?;
            if let Some(value) = catalog.ruleset(&id) {
                Ok(serde_json::json!({
                    "output_schema_version": 1,
                    "document_type": "ruleset",
                    "id": value.id(),
                    "schema_version": value.schema_version(),
                    "behavior_fingerprint": value.behavior_fingerprint()?,
                    "document_fingerprint": value.document_fingerprint()?,
                    "provenance": value.provenance(),
                    "mechanics": {
                        "paid_single_cost": value.paid_single_cost(),
                        "paid_single_action_size": value.paid_single_action_size(),
                        "ticket_action_size": value.ticket_action_size(),
                        "ordinary_pickup_probability": value.ordinary_pickup_probability(),
                        "maximum_pre_recruitment_charge": value.maximum_pre_recruitment_charge(),
                        "hit_reset_charge": value.hit_reset_charge(),
                        "miss_increment": value.miss_increment(),
                        "threshold_overrides": value.threshold_overrides(),
                    }
                }))
            } else {
                let value = catalog.ruleset_v3(&id).ok_or_else(|| {
                    CoreError::validation(None, format!("catalog ruleset {id} was not found"))
                })?;
                Ok(serde_json::json!({
                    "output_schema_version": 2,
                    "document_type": "ruleset",
                    "document_profile": "v3",
                    "id": value.id(),
                    "schema_version": value.schema_version(),
                    "behavior_fingerprint": value.behavior_fingerprint()?,
                    "document_fingerprint": value.document_fingerprint()?,
                    "provenance_status": value.provenance().provenance_status,
                    "provenance": value.provenance(),
                    "required_claim_groups": ba_core::RULESET_CLAIM_GROUPS_V3,
                    "mechanics": {
                        "paid_single_cost": value.paid_single_cost(),
                        "paid_single_action_size": value.paid_single_action_size(),
                        "ticket_action_size": value.ticket_action_size(),
                        "ordinary_featured_target_probability": value.ordinary_featured_target_probability(),
                        "maximum_pre_recruitment_charge": value.maximum_pre_recruitment_charge(),
                        "featured_hit_reset_charge": value.featured_hit_reset_charge(),
                        "non_featured_increment": value.non_featured_increment(),
                        "threshold_overrides": value.threshold_overrides(),
                    }
                }))
            }
        }
        CatalogInspectKind::RewardSchedules => {
            let id = RewardScheduleId::new(id)
                .map_err(|error| CoreError::validation(None, error.to_string()))?;
            if let Some(value) = catalog.reward_schedule(&id) {
                Ok(serde_json::json!({
                    "output_schema_version": 1,
                    "document_type": "reward_schedule",
                    "id": value.id(),
                    "schema_version": value.schema_version(),
                    "behavior_fingerprint": value.behavior_fingerprint()?,
                    "document_fingerprint": value.document_fingerprint()?,
                    "provenance": value.provenance(),
                    "compatible_ruleset_ids": value.compatible_ruleset_ids(),
                    "milestones": value.milestones(),
                }))
            } else {
                let value = catalog.reward_schedule_v3(&id).ok_or_else(|| {
                    CoreError::validation(
                        None,
                        format!("catalog reward schedule {id} was not found"),
                    )
                })?;
                Ok(serde_json::json!({
                    "output_schema_version": 2,
                    "document_type": "reward_schedule",
                    "document_profile": "v3",
                    "id": value.id(),
                    "schema_version": value.schema_version(),
                    "behavior_fingerprint": value.behavior_fingerprint()?,
                    "document_fingerprint": value.document_fingerprint()?,
                    "provenance_status": value.provenance().provenance_status,
                    "provenance": value.provenance(),
                    "required_claim_groups": ba_core::REWARD_SCHEDULE_CLAIM_GROUPS_V3,
                    "compatible_ruleset_ids": value.compatible_ruleset_ids(),
                    "initial_milestones": value.initial_milestones(),
                    "repeating_cycle": value.repeating_cycle(),
                }))
            }
        }
        CatalogInspectKind::Scenarios => {
            let path = resolve_scenario(data_dir, scenario_dir, Path::new(id));
            let bundle = compile_catalog_scenario(&catalog, scenario_dir, Path::new(id), path)?;
            Ok(scenario_explanation_any(&bundle))
        }
    }
}

fn compile_catalog_scenario(
    catalog: &Catalog,
    scenario_dir: Option<&Path>,
    supplied: &Path,
    resolved: PathBuf,
) -> Result<AnyValidatedScenarioBundle, CoreError> {
    match ba_core::document::BufferedDocument::read(&resolved)
        .and_then(|document| compile_any_buffered_bundle(catalog, &document))
    {
        Ok(bundle) => Ok(bundle),
        Err(original) => {
            let should_search_by_id = matches!(
                &original,
                CoreError::Io { source, .. }
                    if source.kind() == std::io::ErrorKind::NotFound
            ) && scenario_dir.is_some()
                && is_bare_scenario_name(supplied);
            if !should_search_by_id {
                return Err(original);
            }
            let requested = supplied.to_string_lossy();
            let Some(directory) = scenario_dir else {
                return Err(original);
            };
            let mut found = None;
            for document in read_document_directory(directory)? {
                if document.dispatch().kind != DocumentKind::Scenario {
                    return Err(CoreError::validation(
                        Some(document.path()),
                        "scenario directory contains a non-scenario document",
                    ));
                }
                let bundle = compile_any_buffered_bundle(catalog, &document)?;
                if any_scenario_id(&bundle) == requested {
                    if found.is_some() {
                        return Err(CoreError::validation(
                            Some(document.path()),
                            format!("duplicate scenario ID {requested}"),
                        ));
                    }
                    found = Some(bundle);
                }
            }
            found.ok_or(original)
        }
    }
}

fn load_scenario_bundle(
    data_dir: &Path,
    scenario_dir: Option<&Path>,
    supplied: &Path,
    resolved: PathBuf,
) -> Result<AnyValidatedScenarioBundle, CoreError> {
    match load_any_bundle(data_dir, &resolved) {
        Ok(bundle) => Ok(bundle),
        Err(original) => {
            let should_search_by_id = matches!(
                &original,
                CoreError::Io { source, .. }
                    if source.kind() == std::io::ErrorKind::NotFound
            ) && scenario_dir.is_some()
                && is_bare_scenario_name(supplied);
            if !should_search_by_id {
                return Err(original);
            }
            let requested = supplied.to_string_lossy();
            let mut found = None;
            let Some(directory) = scenario_dir else {
                return Err(original);
            };
            for document in read_document_directory(directory)? {
                if document.dispatch().kind != DocumentKind::Scenario {
                    return Err(CoreError::validation(
                        Some(document.path()),
                        "scenario directory contains a non-scenario document",
                    ));
                }
                let bundle = load_any_buffered_bundle(data_dir, &document)?;
                if any_scenario_id(&bundle) == requested {
                    if found.is_some() {
                        return Err(CoreError::validation(
                            Some(document.path()),
                            format!("duplicate scenario ID {requested}"),
                        ));
                    }
                    found = Some(bundle);
                }
            }
            found.ok_or(original)
        }
    }
}

fn any_scenario_id(bundle: &AnyValidatedScenarioBundle) -> &str {
    match bundle {
        AnyValidatedScenarioBundle::V2(bundle) => bundle.scenario().id().as_str(),
        AnyValidatedScenarioBundle::V3(bundle) => bundle.scenario().id().as_str(),
    }
}

fn scenario_explanation(bundle: &ValidatedScenarioBundle) -> serde_json::Value {
    serde_json::json!({
        "output_schema_version": 1,
        "document_type": "scenario_explanation",
        "scenario": {
            "id": bundle.scenario().id(),
            "schema_version": bundle.scenario().schema_version(),
            "behavior_fingerprint": bundle.fingerprints().scenario,
            "document_fingerprint": bundle.fingerprints().scenario_document,
        },
        "ruleset": {
            "id": bundle.ruleset().id(),
            "schema_version": bundle.ruleset().schema_version(),
            "behavior_fingerprint": bundle.fingerprints().ruleset,
            "document_fingerprint": bundle.fingerprints().ruleset_document,
            "provenance": bundle.ruleset().provenance(),
        },
        "reward_schedule": {
            "id": bundle.reward_schedule().id(),
            "schema_version": bundle.reward_schedule().schema_version(),
            "behavior_fingerprint": bundle.fingerprints().reward_schedule,
            "document_fingerprint": bundle.fingerprints().reward_schedule_document,
            "provenance": bundle.reward_schedule().provenance(),
        },
        "compiled_strategy": bundle.compiled_strategy(),
        "ordered_targets": bundle.scenario().targets(),
        "initial_resources": bundle.scenario().initial_resources(),
        "initial_charges": bundle.scenario().initial_charges(),
        "mechanics": {
            "paid_single_cost": bundle.ruleset().paid_single_cost(),
            "paid_single_action_size": bundle.ruleset().paid_single_action_size(),
            "ticket_action_size": bundle.ruleset().ticket_action_size(),
            "ordinary_pickup_probability": bundle.ruleset().ordinary_pickup_probability(),
            "maximum_pre_recruitment_charge": bundle.ruleset().maximum_pre_recruitment_charge(),
            "hit_reset_charge": bundle.ruleset().hit_reset_charge(),
            "miss_increment": bundle.ruleset().miss_increment(),
            "threshold_overrides": bundle.ruleset().threshold_overrides(),
        }
    })
}

fn scenario_explanation_any(bundle: &AnyValidatedScenarioBundle) -> serde_json::Value {
    match bundle {
        AnyValidatedScenarioBundle::V2(bundle) => scenario_explanation(bundle),
        AnyValidatedScenarioBundle::V3(bundle) => scenario_explanation_v3(bundle),
    }
}

fn scenario_explanation_v3(bundle: &ValidatedScenarioBundleV3) -> serde_json::Value {
    let probability_profiles = bundle
        .scenario()
        .probability_profiles()
        .iter()
        .map(|profile| {
            serde_json::json!({
                "banner_id": profile.banner_id,
                "ordinary": distribution_json(&profile.ordinary),
                "threshold_overrides": profile
                    .threshold_overrides
                    .iter()
                    .map(|(pre_charge, distribution)| serde_json::json!({
                        "pre_charge": pre_charge,
                        "distribution": distribution_json(distribution),
                    }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let charge_groups = bundle
        .scenario()
        .banners()
        .iter()
        .map(|banner| banner.charge_group_id.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let topology_kind = if charge_groups.len() == 1 {
        "shared"
    } else if charge_groups.len() == bundle.scenario().banners().len() {
        "independent"
    } else {
        "mixed"
    };
    serde_json::json!({
        "output_schema_version": 2,
        "document_type": "scenario_explanation",
        "document_profile": "v3",
        "scenario": {
            "id": bundle.scenario().id(),
            "schema_version": bundle.scenario().schema_version(),
            "behavior_fingerprint": bundle.fingerprints().scenario,
            "document_fingerprint": bundle.fingerprints().scenario_document,
            "authority": bundle.scenario().authority(),
        },
        "ruleset": {
            "id": bundle.ruleset().id(),
            "schema_version": bundle.ruleset().schema_version(),
            "behavior_fingerprint": bundle.fingerprints().ruleset,
            "document_fingerprint": bundle.fingerprints().ruleset_document,
            "provenance_status": bundle.ruleset().provenance().provenance_status,
            "provenance": bundle.ruleset().provenance(),
        },
        "reward_schedule": {
            "id": bundle.reward_schedule().id(),
            "schema_version": bundle.reward_schedule().schema_version(),
            "behavior_fingerprint": bundle.fingerprints().reward_schedule,
            "document_fingerprint": bundle.fingerprints().reward_schedule_document,
            "provenance_status": bundle.reward_schedule().provenance().provenance_status,
            "provenance": bundle.reward_schedule().provenance(),
            "schedule_kind": if bundle.reward_schedule().repeating_cycle().is_some() {
                "repeating"
            } else {
                "finite"
            },
            "finite_milestone_count": bundle.reward_schedule().initial_milestones().len(),
            "repeating_cycle": bundle.reward_schedule().repeating_cycle(),
            "effective_future_milestone_count": bundle.scenario().effective_milestones().len(),
            "effective_future_milestones": bundle.scenario().effective_milestones(),
        },
        "compiled_strategy": bundle.compiled_strategy(),
        "target_count": bundle.scenario().targets().len(),
        "ordered_targets": bundle.scenario().targets(),
        "ordered_banners": bundle
            .scenario()
            .targets()
            .iter()
            .map(|target| target.banner_id.to_string())
            .collect::<Vec<_>>(),
        "initial_resources": ba_core::ResourcesV3::from(bundle.scenario().initial_resources()),
        "initial_owned_targets": bundle.scenario().initial_owned_targets(),
        "initial_charges": bundle.scenario().initial_charges(),
        "initial_campaign_recruitment_count": bundle.scenario().initial_recruitment_count(),
        "maximum_additional_recruitments": bundle
            .compiled_strategy()
            .max_additional_recruitments
            .get(),
        "maximum_absolute_campaign_recruitment_count": bundle
            .scenario()
            .maximum_absolute_campaign_count(),
        "charge_group_topology": topology_kind,
        "probability_authority": "scenario_document_user_authored",
        "canonical_probability_profiles": probability_profiles,
        "mechanics": {
            "paid_single_cost": bundle.ruleset().paid_single_cost(),
            "paid_single_action_size": bundle.ruleset().paid_single_action_size(),
            "ticket_action_size": bundle.ruleset().ticket_action_size(),
            "ordinary_featured_target_probability": bundle
                .ruleset()
                .ordinary_featured_target_probability(),
            "maximum_pre_recruitment_charge": bundle
                .ruleset()
                .maximum_pre_recruitment_charge(),
            "featured_hit_reset_charge": bundle.ruleset().featured_hit_reset_charge(),
            "non_featured_increment": bundle.ruleset().non_featured_increment(),
            "threshold_overrides": bundle.ruleset().threshold_overrides(),
        }
    })
}

fn distribution_json(distribution: &ba_core::CompiledOutcomeDistribution) -> serde_json::Value {
    serde_json::json!({
        "denominator": distribution.denominator().get(),
        "branches": distribution
            .branches()
            .iter()
            .map(|branch| serde_json::json!({
                "outcome": branch.acquisition,
                "canonical_weight": branch.canonical_weight,
                "upper_exclusive": branch.upper_exclusive,
            }))
            .collect::<Vec<_>>(),
    })
}

fn scenario_template(
    data_dir: &Path,
    schema_version: u8,
    scenario_id: &str,
    ruleset_id: &str,
    reward_schedule_id: &str,
    target_count: u8,
) -> Result<serde_json::Value, AppError> {
    if schema_version == 3 {
        return scenario_template_v3(
            data_dir,
            scenario_id,
            ruleset_id,
            reward_schedule_id,
            target_count,
        );
    }
    if target_count > 2 {
        return Err(AppError::Usage(
            "schema v2 target count must be 1 or 2".to_owned(),
        ));
    }
    let scenario_id = ScenarioId::new(scenario_id)
        .map_err(|error| CoreError::validation(None, error.to_string()))?;
    let ruleset_id = RulesetId::new(ruleset_id)
        .map_err(|error| CoreError::validation(None, error.to_string()))?;
    let reward_schedule_id = RewardScheduleId::new(reward_schedule_id)
        .map_err(|error| CoreError::validation(None, error.to_string()))?;
    let catalog = Catalog::load(data_dir)?;
    let _ruleset = catalog.ruleset(&ruleset_id).ok_or_else(|| {
        CoreError::validation(None, format!("catalog ruleset {ruleset_id} was not found"))
    })?;
    let rewards = catalog
        .reward_schedule(&reward_schedule_id)
        .ok_or_else(|| {
            CoreError::validation(
                None,
                format!("catalog reward schedule {reward_schedule_id} was not found"),
            )
        })?;
    if !rewards.compatible_ruleset_ids().contains(&ruleset_id) {
        return Err(CoreError::validation(
            None,
            format!("reward schedule {reward_schedule_id} is incompatible with {ruleset_id}"),
        )
        .into());
    }

    let students = (1..=target_count)
        .map(|index| serde_json::json!({"student_id": format!("target_{index}")}))
        .collect::<Vec<_>>();
    let banners = (1..=target_count)
        .map(|index| {
            serde_json::json!({
                "banner_id": format!("banner_{index}"),
                "featured_student_id": format!("target_{index}"),
                "charge_group_id": format!("charge_group_{index}"),
            })
        })
        .collect::<Vec<_>>();
    let initial_charges = (1..=target_count)
        .map(|index| {
            serde_json::json!({
                "charge_group_id": format!("charge_group_{index}"),
                "pre_recruitment_charge": 0,
            })
        })
        .collect::<Vec<_>>();
    let targets = (1..=target_count)
        .map(|index| {
            serde_json::json!({
                "student_id": format!("target_{index}"),
                "banner_id": format!("banner_{index}"),
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "schema_version": 2,
        "document_type": "scenario",
        "scenario_id": scenario_id,
        "ruleset_id": ruleset_id,
        "reward_schedule_id": reward_schedule_id,
        "students": students,
        "banners": banners,
        "initial_charges": initial_charges,
        "initial_resources": {
            "pyroxene": 24000,
            "limited_ten_recruitment_tickets": 0,
            "eligma": 0,
            "advanced_bd_selectors": 0,
            "advanced_tech_note_selectors": 0,
            "superior_tech_note_selectors": 0,
            "gift_boxes": 0,
        },
        "initial_owned_targets": [],
        "strategy": {
            "strategy_schema_version": 1,
            "strategy_id": "sequential",
            "kind": "sequential_targets",
            "funding_priority": ["ticket_ten", "paid_single"],
            "max_total_recruitments": 200,
        },
        "targets": targets,
    }))
}

fn scenario_template_v3(
    data_dir: &Path,
    scenario_id: &str,
    ruleset_id: &str,
    reward_schedule_id: &str,
    target_count: u8,
) -> Result<serde_json::Value, AppError> {
    let scenario_id = ScenarioId::new(scenario_id)
        .map_err(|error| CoreError::validation(None, error.to_string()))?;
    let ruleset_id = RulesetId::new(ruleset_id)
        .map_err(|error| CoreError::validation(None, error.to_string()))?;
    let reward_schedule_id = RewardScheduleId::new(reward_schedule_id)
        .map_err(|error| CoreError::validation(None, error.to_string()))?;
    let catalog = Catalog::load(data_dir)?;
    let ruleset = catalog.ruleset_v3(&ruleset_id).ok_or_else(|| {
        if catalog.ruleset(&ruleset_id).is_some() {
            CoreError::validation(
                None,
                format!(
                    "schema v3 template requires a schema-v3 ruleset, but {ruleset_id} is schema v2"
                ),
            )
        } else {
            CoreError::validation(None, format!("catalog ruleset {ruleset_id} was not found"))
        }
    })?;
    let rewards = catalog.reward_schedule_v3(&reward_schedule_id).ok_or_else(|| {
        if catalog.reward_schedule(&reward_schedule_id).is_some() {
            CoreError::validation(
                None,
                format!(
                    "schema v3 template requires a schema-v3 reward schedule, but {reward_schedule_id} is schema v2"
                ),
            )
        } else {
            CoreError::validation(
                None,
                format!("catalog reward schedule {reward_schedule_id} was not found"),
            )
        }
    })?;
    if !rewards.compatible_ruleset_ids().contains(&ruleset_id) {
        return Err(CoreError::validation(
            None,
            format!("reward schedule {reward_schedule_id} is incompatible with {ruleset_id}"),
        )
        .into());
    }

    let students = (1..=target_count)
        .map(|index| serde_json::json!({"student_id": format!("target_{index}")}))
        .collect::<Vec<_>>();
    let banners = (1..=target_count)
        .map(|index| {
            serde_json::json!({
                "banner_id": format!("banner_{index}"),
                "featured_student_id": format!("target_{index}"),
                "charge_group_id": format!("charge_group_{index}"),
            })
        })
        .collect::<Vec<_>>();
    let initial_charges = (1..=target_count)
        .map(|index| {
            serde_json::json!({
                "charge_group_id": format!("charge_group_{index}"),
                "pre_recruitment_charge": 0,
            })
        })
        .collect::<Vec<_>>();
    let targets = (1..=target_count)
        .map(|index| {
            serde_json::json!({
                "student_id": format!("target_{index}"),
                "banner_id": format!("banner_{index}"),
            })
        })
        .collect::<Vec<_>>();
    let cross_target_probability_tables = (1..=target_count)
        .map(|banner_index| {
            let other_weights = (1..=target_count)
                .filter(|target_index| *target_index != banner_index)
                .map(|target_index| {
                    serde_json::json!({
                        "target_id": format!("target_{target_index}"),
                        "weight": 0,
                    })
                })
                .collect::<Vec<_>>();
            let thresholds = ruleset
                .threshold_overrides()
                .iter()
                .map(|(pre_charge, probability)| {
                    serde_json::json!({
                        "pre_charge": pre_charge,
                        "denominator": probability.denominator(),
                        "other_target_weights": other_weights.clone(),
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "banner_id": format!("banner_{banner_index}"),
                "ordinary": {
                    "denominator": ruleset
                        .ordinary_featured_target_probability()
                        .denominator(),
                    "other_target_weights": other_weights,
                },
                "threshold_overrides": thresholds,
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "schema_version": 3,
        "document_type": "scenario",
        "scenario_id": scenario_id,
        "ruleset_id": ruleset_id,
        "reward_schedule_id": reward_schedule_id,
        "authority": {
            "scenario": "user_authored",
            "banner_topology": "user_authored",
            "target_order": "user_authored",
            "initial_state": "user_authored",
            "cross_target_probabilities": "user_authored",
            "strategy": "user_authored",
        },
        "initial_recruitment_count": 0,
        "students": students,
        "banners": banners,
        "initial_charges": initial_charges,
        "initial_resources": {
            "pyroxene": 24000,
            "limited_ten_recruitment_tickets": 0,
            "eligma": 0,
            "advanced_bd_selectors": 0,
            "advanced_tech_note_selectors": 0,
            "superior_tech_note_selectors": 0,
            "gift_boxes": 0,
            "keystone_fragments": 0,
            "secret_tech_notes": 0,
            "superior_bd_selectors": 0,
            "high_grade_gift_boxes": 0,
        },
        "initial_owned_targets": [],
        "targets": targets,
        "cross_target_probability_tables": cross_target_probability_tables,
        "strategy": {
            "strategy_schema_version": 2,
            "strategy_id": "sequential_targets_v3",
            "kind": "sequential_targets",
            "funding_priority": ["ticket_ten", "paid_single"],
            "max_additional_recruitments": 200,
        },
    }))
}

fn resolve_master_seed(seed: Option<u64>) -> Result<u64, AppError> {
    resolve_master_seed_with(seed, || {
        OsRng
            .try_next_u64()
            .map_err(|error| format!("operating-system entropy is unavailable: {error}"))
    })
}

pub(crate) fn resolve_master_seed_with(
    seed: Option<u64>,
    entropy: impl FnOnce() -> Result<u64, String>,
) -> Result<u64, AppError> {
    match seed {
        Some(seed) => Ok(seed),
        None => entropy().map_err(AppError::Entropy),
    }
}
