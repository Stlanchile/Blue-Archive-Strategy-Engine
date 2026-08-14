use std::ffi::OsString;
use std::num::NonZeroU64;
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Parser)]
#[command(name = "ba-strategy", version, about = "Blue Archive Strategy Engine")]
pub(crate) struct Cli {
    #[arg(long, global = true, default_value = "./data")]
    pub(crate) data_dir: PathBuf,

    #[arg(long, global = true)]
    pub(crate) scenario_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Strictly validate one versioned JSON document.
    Validate {
        document: PathBuf,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
        /// Emit a versioned structured diagnostic envelope on failure.
        #[arg(long)]
        diagnostics: bool,
    },
    /// Exhaustively enumerate every modeled probability branch.
    Analyze {
        scenario: PathBuf,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Run Monte Carlo with an OS-random seed unless --seed is supplied.
    Simulate {
        scenario: PathBuf,
        #[arg(long)]
        runs: NonZeroU64,
        /// Reproduce a run with this master seed instead of using OS entropy.
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long)]
        trace: bool,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Compare exact analysis with OS-seeded or explicitly seeded Monte Carlo.
    Compare {
        scenario: PathBuf,
        #[arg(long)]
        runs: NonZeroU64,
        /// Reproduce a run with this master seed instead of using OS entropy.
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Inspect validated local catalogs.
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
    /// Explain or author a scenario without running analysis.
    Scenario {
        #[command(subcommand)]
        command: ScenarioCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum CatalogCommand {
    /// List validated catalog entries deterministically.
    List {
        #[arg(value_enum, default_value_t)]
        selector: CatalogListSelector,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Inspect one validated catalog entry.
    Inspect {
        #[arg(value_enum)]
        kind: CatalogInspectKind,
        id: String,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum CatalogListSelector {
    #[default]
    All,
    Rulesets,
    RewardSchedules,
    Scenarios,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CatalogInspectKind {
    Rulesets,
    RewardSchedules,
    Scenarios,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ScenarioCommand {
    /// Explain the compiled inputs and strategy without analysis or RNG.
    Explain {
        scenario: PathBuf,
        #[arg(long, value_enum, default_value_t)]
        format: OutputFormat,
    },
    /// Emit a complete schema-v2 or schema-v3 scenario template to stdout.
    Template {
        /// Select the scenario document profile; omitting this preserves schema v2.
        #[arg(long, default_value_t = 2, value_parser = parse_schema_version)]
        schema_version: u8,
        #[arg(long)]
        scenario_id: String,
        #[arg(long)]
        ruleset: String,
        #[arg(long)]
        reward_schedule: String,
        #[arg(long, value_parser = parse_target_count)]
        target_count: u8,
    },
}

pub(crate) fn requests_json(args: &[OsString]) -> bool {
    args.iter().enumerate().any(|(index, value)| {
        value == "--format=json"
            || (value == "--format" && args.get(index + 1).is_some_and(|next| next == "json"))
    })
}

pub(crate) fn prepare_template_args_for_clap(args: &mut [OsString]) -> Option<u8> {
    let explicit_v3 = args.iter().enumerate().any(|(index, argument)| {
        argument == "--schema-version=3"
            || (argument == "--schema-version"
                && args
                    .get(index + 1)
                    .is_some_and(|value| value.as_os_str() == "3"))
    });
    if !explicit_v3 {
        return None;
    }
    let scenario_index = args.iter().position(|argument| argument == "scenario")?;
    args.iter()
        .skip(scenario_index + 1)
        .position(|argument| argument == "template")?;
    for index in 0..args.len() {
        if args[index] == "--target-count" {
            let value = args.get(index + 1)?.to_str()?;
            let count = match value {
                "3" => 3,
                "4" => 4,
                _ => continue,
            };
            args[index + 1] = OsString::from("2");
            return Some(count);
        }
        let Some(value) = args[index]
            .to_str()
            .and_then(|argument| argument.strip_prefix("--target-count="))
        else {
            continue;
        };
        let count = match value {
            "3" => 3,
            "4" => 4,
            _ => continue,
        };
        args[index] = OsString::from("--target-count=2");
        return Some(count);
    }
    None
}

pub(crate) fn restore_template_target_count(cli: &mut Cli, override_count: Option<u8>) {
    let Some(override_count) = override_count else {
        return;
    };
    if let Command::Scenario {
        command:
            ScenarioCommand::Template {
                schema_version,
                target_count,
                ..
            },
    } = &mut cli.command
        && *schema_version == 3
    {
        *target_count = override_count;
    }
}

fn parse_target_count(value: &str) -> Result<u8, String> {
    match value {
        "1" => Ok(1),
        "2" => Ok(2),
        _ => Err("target count must be 1 or 2".to_owned()),
    }
}

fn parse_schema_version(value: &str) -> Result<u8, String> {
    match value {
        "2" => Ok(2),
        "3" => Ok(3),
        _ => Err("schema version must be 2 or 3".to_owned()),
    }
}
