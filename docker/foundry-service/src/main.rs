//! Foundry - installs the central migration catalog into Postgres.
//!
//! Designed to run as a Kubernetes Job (or init container): it resolves the
//! requested modules and their dependencies, applies every outstanding
//! migration in dependency order, and exits non-zero if anything fails.

mod config;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{ConfigInputs, RunConfig};
use quench_cli::prelude::{Tone, print_status};
use quench_db::prelude::{Catalog, MigrationPlan, MigrationRunner, PostgresDb};
use quench_db::runner::MigrationOutcome;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "foundry",
    about = "Install versioned database modules from the Forge migration catalog",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Config file describing what to install (default: config/install.toml).
    #[arg(short, long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Migration catalog directory (default: migrations).
    #[arg(long, global = true, value_name = "DIR")]
    catalog: Option<PathBuf>,

    /// Postgres connection string (default: $DATABASE_URL).
    #[arg(long, global = true, value_name = "URL")]
    database_url: Option<String>,

    /// Module to install as `module[@version][:schema]`; repeatable.
    /// Overrides the config file's install list.
    #[arg(short, long = "install", global = true, value_name = "SPEC")]
    installs: Vec<String>,

    /// Dedicated schema holding the ledger tables (default: foundry).
    #[arg(long, global = true, value_name = "SCHEMA")]
    ledger_schema: Option<String>,

    /// Table recording applied migrations (default: forge_migrations).
    #[arg(long, global = true, value_name = "TABLE")]
    ledger_table: Option<String>,

    /// Table recording installed module versions (default: forge_modules).
    #[arg(long, global = true, value_name = "TABLE")]
    module_table: Option<String>,

    /// Continue when an applied migration's SQL no longer matches the catalog.
    #[arg(long, global = true)]
    allow_drift: bool,

    /// Confirm a destructive command. `reset` refuses to run without it.
    #[arg(long, global = true)]
    yes: bool,
}

#[derive(Subcommand, Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Command {
    /// Resolve the plan and apply every outstanding migration.
    #[default]
    Apply,
    /// Show what `apply` would do, without writing anything.
    Plan,
    /// Show installed module versions against the catalog.
    Status,
    /// Check the catalog and resolve the plan without touching the database.
    Validate,
    /// Drop everything the plan owns and reinstall it from scratch.
    ///
    /// For development. Requires --yes, because it destroys data.
    Reset,
}

#[tokio::main]
async fn main() -> Result<()> {
    let log_level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(tracing::Level::INFO);
    tracing_subscriber::fmt().with_max_level(log_level).init();

    let cli = Cli::parse();
    let command = cli.command.unwrap_or_default();

    let config = RunConfig::resolve(ConfigInputs {
        config_path: cli.config.as_deref(),
        catalog: cli.catalog.as_deref(),
        database_url: cli.database_url.as_deref(),
        installs: &cli.installs,
        ledger_schema: cli.ledger_schema.as_deref(),
        ledger_table: cli.ledger_table.as_deref(),
        module_table: cli.module_table.as_deref(),
        database_optional: command == Command::Validate,
    })?;

    let catalog = Catalog::load(&config.catalog)
        .with_context(|| format!("failed to load catalog {}", config.catalog.display()))?;
    print_status(
        Tone::Info,
        "catalog",
        &format!(
            "{} module(s) from {}",
            catalog.len(),
            config.catalog.display()
        ),
    );

    let plan = MigrationPlan::resolve(&catalog, &config.installs)?;
    print_plan(&plan);

    let runner = MigrationRunner::new()
        .ledger_schema(&config.ledger_schema)
        .ledger_table(&config.ledger_table)
        .module_table(&config.module_table)
        .allow_drift(cli.allow_drift);

    match command {
        Command::Validate => {
            print_status(Tone::Success, "validate", "catalog and plan are consistent");
            Ok(())
        }
        Command::Plan => {
            let db = connect(&config).await?;
            let report = runner.dry_run(true).apply(&db, &plan).await?;
            print_report(&report);
            Ok(())
        }
        Command::Status => {
            let db = connect(&config).await?;
            print_installed(&runner, &db, &catalog).await
        }
        Command::Reset => {
            if !cli.yes {
                print_status(
                    Tone::Error,
                    "reset",
                    "refusing to drop schemas without --yes; this destroys data",
                );
                for schema in runner.resettable_schemas(&plan) {
                    print_status(
                        Tone::Warn,
                        "would drop",
                        &format!("schema {schema} (CASCADE)"),
                    );
                }
                std::process::exit(1);
            }

            let db = connect(&config).await?;
            let report = runner.reset(&db, &plan).await?;
            for schema in &report.schemas {
                print_status(Tone::Warn, "dropped", &format!("schema {schema}"));
            }
            print_status(
                Tone::Info,
                "reset",
                &format!(
                    "forgot {} recorded migration(s)",
                    report.forgotten_migrations
                ),
            );

            let report = runner.apply(&db, &plan).await?;
            print_report(&report);
            print_status(
                Tone::Success,
                "reset",
                &format!(
                    "{} migration(s) reinstalled",
                    report.count(MigrationOutcome::Applied)
                ),
            );
            Ok(())
        }
        Command::Apply => {
            let db = connect(&config).await?;
            let report = runner.apply(&db, &plan).await?;
            print_report(&report);
            print_status(
                Tone::Success,
                "done",
                &format!(
                    "{} applied, {} already up to date",
                    report.count(MigrationOutcome::Applied),
                    report.count(MigrationOutcome::Skipped)
                ),
            );
            Ok(())
        }
    }
}

async fn connect(config: &RunConfig) -> Result<PostgresDb> {
    let db = PostgresDb::new(&config.database_url)
        .await
        .context("failed to connect to the database")?;
    print_status(Tone::Success, "db", "connected to Postgres");
    Ok(db)
}

fn print_plan(plan: &MigrationPlan) {
    for (index, module) in plan.modules.iter().enumerate() {
        let origin = match &module.required_by {
            Some(parent) => format!(" (required by {parent})"),
            None => String::new(),
        };
        print_status(
            Tone::Info,
            "module",
            &format!(
                "{}. {} {} -> schema {}{origin}",
                index + 1,
                module.module,
                module.version,
                module.schema
            ),
        );
    }
    print_status(
        Tone::Info,
        "plan",
        &format!(
            "{} migration(s) across {} module instance(s)",
            plan.migrations.len(),
            plan.modules.len()
        ),
    );
}

fn print_report(report: &quench_db::prelude::ApplyReport) {
    for result in &report.results {
        let (tone, label) = match result.outcome {
            MigrationOutcome::Applied => (Tone::Success, "applied"),
            MigrationOutcome::Skipped => (Tone::Info, "current"),
            MigrationOutcome::Pending => (Tone::Warn, "pending"),
        };
        print_status(tone, label, &result.id);
    }

    if report.dry_run {
        print_status(
            Tone::Info,
            "dry-run",
            &format!(
                "{} migration(s) would be applied",
                report.count(MigrationOutcome::Pending)
            ),
        );
    }
}

async fn print_installed(
    runner: &MigrationRunner,
    db: &PostgresDb,
    catalog: &Catalog,
) -> Result<()> {
    let installed = runner.installed_modules(db).await?;
    if installed.is_empty() {
        print_status(Tone::Warn, "status", "no modules installed yet");
        return Ok(());
    }

    for (module, schema, version) in installed {
        let available = catalog
            .get(&module)
            .map(|m| m.version().to_string())
            .unwrap_or_else(|| "not in catalog".to_string());
        let tone = if available == version {
            Tone::Success
        } else {
            Tone::Warn
        };
        print_status(
            tone,
            "installed",
            &format!("{module} {version} in schema {schema} (catalog: {available})"),
        );
    }
    Ok(())
}
