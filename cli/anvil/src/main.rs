use anvil::cli::{Cli, Commands, ConfigCommands, DockerCommands};
use anvil::commands;
use anvil::config;
use anvil::util::{OutputMode, set_output_mode};
use anyhow::Result;
use clap::Parser;
use quench_cli::require::require_binary;
use quench_cli::terminal::print_box_banner;

/// Cheap, parse-free check for a `--json` flag anywhere in argv.
///
/// The banner has to be decided before `Cli::parse()` runs, since clap exits
/// the process directly on `--help`/`--version`/a parse error without
/// returning - a real parse-then-check would silently drop the banner from
/// those paths too, not just from `--json` runs. Only the six `--json`-aware
/// commands ever accept that flag, so its presence anywhere in argv is
/// enough to know a JSON consumer is on the other end of stdout, without
/// needing to know which subcommand it belongs to.
fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // A `--json` command's stdout is meant to be piped straight to a
    // consumer - keep anvil's own banner off it, same reasoning as
    // `run_command_json` skipping the status lines around the command itself.
    if !wants_json(&args) {
        print_box_banner("Anvil CLI", "workspace build and release");
    }
    require_binary("cargo", "anvil shells out to it for nearly every command")?;
    let config = config::load_config(wants_json(&args))?;
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
        Commands::Config { command } => match command {
            ConfigCommands::Check => commands::config_check::check()?,
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::wants_json;

    #[test]
    fn true_when_json_flag_is_anywhere_in_argv() {
        let args = |s: &[&str]| s.iter().map(|a| (*a).to_string()).collect::<Vec<_>>();
        assert!(wants_json(&args(&["anvil", "lint", "--json"])));
        assert!(wants_json(&args(&["anvil", "audit", "--json"])));
    }

    #[test]
    fn false_without_a_json_flag() {
        let args = |s: &[&str]| s.iter().map(|a| (*a).to_string()).collect::<Vec<_>>();
        assert!(!wants_json(&args(&["anvil", "lint"])));
        assert!(!wants_json(&args(&["anvil", "--help"])));
        assert!(!wants_json(&args(&["anvil"])));
    }
}
