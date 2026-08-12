use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ba_core::document::read_document_directory;
use ba_core::{
    Catalog, CoreError, DocumentKind, RewardScheduleId, RulesetId, ScenarioId,
    ValidatedScenarioBundle, compile_buffered_bundle, load_buffered_bundle, load_bundle,
    validate_document,
};
use ba_engine::{
    ExactSolverOptions, analyze_exact_detailed, compare, simulate_monte_carlo, simulate_trace,
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
                .and_then(|bundle| {
                    analyze_exact_detailed(&bundle, ExactSolverOptions::default())
                        .map_err(AppError::from)
                })
                .and_then(|value| render::exact(&value, format));
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
                    if trace {
                        simulate_trace(&bundle, seed)
                            .map(|value| SimulationOutput::Trace(Box::new(value)))
                            .map_err(AppError::from)
                    } else {
                        simulate_monte_carlo(&bundle, runs, seed)
                            .map(|value| SimulationOutput::Aggregate(Box::new(value)))
                            .map_err(AppError::from)
                    }
                })
                .and_then(|value| match value {
                    SimulationOutput::Trace(value) => render::trace(&value, format),
                    SimulationOutput::Aggregate(value) => render::monte_carlo(&value, format),
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
                    compare(&bundle, runs, seed).map_err(AppError::from)
                })
                .and_then(|value| render::comparison(&value, format));
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
                .and_then(|bundle| render::structured(&scenario_explanation(&bundle), format));
            (
                RenderMode {
                    format,
                    diagnostics: false,
                },
                result,
            )
        }
        ScenarioCommand::Template {
            scenario_id,
            ruleset,
            reward_schedule,
            target_count,
        } => {
            let result = scenario_template(
                data_dir,
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
    behavior_fingerprint: String,
    document_fingerprint: String,
    verification_status: Option<ba_core::VerificationStatus>,
}

#[derive(Debug, Serialize)]
struct ScenarioSummary {
    id: String,
    schema_version: u64,
    behavior_fingerprint: String,
    document_fingerprint: String,
}

fn catalog_list(
    data_dir: &Path,
    scenario_dir: Option<&Path>,
    selector: CatalogListSelector,
) -> Result<CatalogListOutput, AppError> {
    let catalog = Catalog::load(data_dir)?;
    let rulesets = matches!(
        selector,
        CatalogListSelector::All | CatalogListSelector::Rulesets
    )
    .then(|| {
        catalog
            .rulesets()
            .values()
            .map(|value| {
                Ok(CatalogDocumentSummary {
                    id: value.id().to_string(),
                    schema_version: value.schema_version(),
                    behavior_fingerprint: value.behavior_fingerprint()?.to_hex(),
                    document_fingerprint: value.document_fingerprint()?.to_hex(),
                    verification_status: Some(value.provenance().verification_status),
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()
    })
    .transpose()?;
    let reward_schedules = matches!(
        selector,
        CatalogListSelector::All | CatalogListSelector::RewardSchedules
    )
    .then(|| {
        catalog
            .reward_schedules()
            .values()
            .map(|value| {
                Ok(CatalogDocumentSummary {
                    id: value.id().to_string(),
                    schema_version: value.schema_version(),
                    behavior_fingerprint: value.behavior_fingerprint()?.to_hex(),
                    document_fingerprint: value.document_fingerprint()?.to_hex(),
                    verification_status: Some(value.provenance().verification_status),
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()
    })
    .transpose()?;
    let scenarios = matches!(
        selector,
        CatalogListSelector::All | CatalogListSelector::Scenarios
    )
    .then(|| scenario_summaries(data_dir, scenario_dir, &catalog))
    .transpose()?;
    Ok(CatalogListOutput {
        output_schema_version: 1,
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
    let mut by_id = BTreeMap::new();
    for directory in directories {
        for document in read_document_directory(&directory)? {
            if document.dispatch().kind != DocumentKind::Scenario {
                return Err(CoreError::validation(
                    Some(document.path()),
                    "scenario catalog contains a non-scenario document",
                )
                .into());
            }
            let bundle = compile_buffered_bundle(catalog, &document)?;
            let id = bundle.scenario().id().to_string();
            let summary = ScenarioSummary {
                id: id.clone(),
                schema_version: bundle.scenario().schema_version(),
                behavior_fingerprint: bundle.fingerprints().scenario.to_hex(),
                document_fingerprint: bundle.fingerprints().scenario_document.to_hex(),
            };
            if by_id.insert(id.clone(), summary).is_some() {
                return Err(CoreError::validation(
                    Some(document.path()),
                    format!("duplicate catalog scenario ID {id}"),
                )
                .into());
            }
        }
    }
    Ok(by_id.into_values().collect())
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
            let value = catalog.ruleset(&id).ok_or_else(|| {
                CoreError::validation(None, format!("catalog ruleset {id} was not found"))
            })?;
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
        }
        CatalogInspectKind::RewardSchedules => {
            let id = RewardScheduleId::new(id)
                .map_err(|error| CoreError::validation(None, error.to_string()))?;
            let value = catalog.reward_schedule(&id).ok_or_else(|| {
                CoreError::validation(None, format!("catalog reward schedule {id} was not found"))
            })?;
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
        }
        CatalogInspectKind::Scenarios => {
            let path = resolve_scenario(data_dir, scenario_dir, Path::new(id));
            let bundle = compile_catalog_scenario(&catalog, scenario_dir, Path::new(id), path)?;
            Ok(scenario_explanation(&bundle))
        }
    }
}

fn compile_catalog_scenario(
    catalog: &Catalog,
    scenario_dir: Option<&Path>,
    supplied: &Path,
    resolved: PathBuf,
) -> Result<ValidatedScenarioBundle, CoreError> {
    match ba_core::document::BufferedDocument::read(&resolved)
        .and_then(|document| compile_buffered_bundle(catalog, &document))
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
                let bundle = compile_buffered_bundle(catalog, &document)?;
                if bundle.scenario().id().as_str() == requested {
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
) -> Result<ValidatedScenarioBundle, CoreError> {
    match load_bundle(data_dir, &resolved) {
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
                let bundle = load_buffered_bundle(data_dir, &document)?;
                if bundle.scenario().id().as_str() == requested {
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

fn scenario_template(
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
