use anvil::cli::{ArtifactCommands, Cli, Commands, ConfigCommands, DockerCommands};
use anvil::commands;
use anvil::config;
use anvil::util::{OutputMode, set_output_mode, stdout_is_data};
use quench_args::Parser;
use quench_cli::require::require_binary;
use quench_cli::terminal::print_box_banner;
use quench_core::{Context, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // A `--json` command's stdout is meant to be piped straight to a
    // consumer - keep anvil's own banner off it, same reasoning as
    // `run_command_json` skipping the status lines around the command itself.
    if !stdout_is_data(&args) {
        print_box_banner("Anvil CLI", "workspace build and release");
    }
    require_binary("cargo", "anvil shells out to it for nearly every command")?;
    let config = config::load_config(stdout_is_data(&args))?;
    let cli = Cli::parse();

    set_output_mode(if cli.silent {
        OutputMode::Silent
    } else if let Some(n) = cli.tail {
        OutputMode::Tail(n)
    } else {
        OutputMode::Full
    });

    match cli.command {
        Commands::Build {
            all,
            all_features,
            release,
            package,
        } => commands::build::build(all, all_features, release, package)?,
        Commands::Clean => commands::build::clean()?,
        Commands::Lint {
            all_targets,
            all_features,
            deny_warnings,
            json,
        } => commands::lint::lint(all_targets, all_features, deny_warnings, json)?,
        Commands::Format { check } => commands::lint::format(check)?,
        Commands::List { format } => commands::workspace::list(&format)?,
        Commands::Upgrade { incompatible } => commands::workspace::upgrade(incompatible)?,
        Commands::Audit { json } => commands::workspace::audit(json)?,
        Commands::Machete { json } => commands::workspace::machete(json)?,
        Commands::Deny { json } => commands::workspace::deny(json)?,
        Commands::SemverCheck {
            package,
            baseline_rev,
        } => commands::workspace::semver_check(&package, baseline_rev)?,
        Commands::Test {
            all,
            package,
            test_name,
            ignored,
            list,
            json,
        } => commands::build::test(all, package, test_name, ignored, list, json)?,
        Commands::Nextest {
            all,
            package,
            test_name,
            ignored,
            json,
        } => commands::build::nextest(all, package, test_name, ignored, json)?,
        Commands::Install { all, package } => commands::install::install(&config, package, all)?,
        Commands::Release {
            all,
            package,
            dry_run,
        } => commands::release::release(&config, package, all, dry_run)?,
        Commands::Run {
            package,
            serve,
            watch_interval_ms,
        } => commands::run::run(package.as_deref(), serve, watch_interval_ms)?,
        Commands::Docker { command } => match command {
            DockerCommands::Build { package } => commands::docker::build(&config, &package)?,
            DockerCommands::Tag { package } => {
                commands::docker::tag(&config, &package)?;
            }
            DockerCommands::Push { package } => {
                commands::docker::push(&config, &package)?;
            }
            DockerCommands::Release { package } => {
                commands::docker::release(&config, &package)?;
            }
            DockerCommands::ReleaseAll => {
                commands::docker::release_all(&config)?;
            }
            DockerCommands::BuildAll => commands::docker::build_all(&config)?,
        },
        Commands::Login {
            gatehouse_url,
            username,
            name,
            scope,
            days,
        } => commands::login::login(&config, gatehouse_url, username, name, scope, days)?,
        Commands::RegistryToken => {
            let dir = std::env::current_dir().context("finding the working directory")?;
            println!(
                "{}",
                commands::login::registry_token(&dir, |name| std::env::var(name).ok())?
            );
        }
        Commands::Artifact { command } => artifact(&config, command)?,
        Commands::Config { command } => match command {
            ConfigCommands::Check => commands::config_check::check()?,
        },
    }

    Ok(())
}

fn artifact(config: &config::Config, command: ArtifactCommands) -> Result<()> {
    match command {
        ArtifactCommands::Release {
            name,
            target,
            dry_run,
            allow_dirty,
            skip_existing,
        } => commands::artifact::release(
            config,
            &name,
            &commands::artifact::Options {
                target,
                dry_run,
                allow_dirty,
                skip_existing,
            },
        ),
        ArtifactCommands::List => commands::artifact::list(config, &mut std::io::stdout()),
    }
}
