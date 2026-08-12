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
    /// Emit a complete schema-v2 scenario template to stdout.
    Template {
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

fn parse_target_count(value: &str) -> Result<u8, String> {
    match value {
        "1" => Ok(1),
        "2" => Ok(2),
        _ => Err("target count must be 1 or 2".to_owned()),
    }
}
