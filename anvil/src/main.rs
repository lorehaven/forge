use anvil::cli::{Cli, Commands, DockerCommands};
use anvil::commands;
use anvil::config;
use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let config = config::load_config()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            all,
            all_features,
            release,
            package,
        } => commands::build::build(all, all_features, release, package)?,
        Commands::Lint {
            all_targets,
            all_features,
            deny_warnings,
        } => commands::lint::lint(all_targets, all_features, deny_warnings)?,
        Commands::Format { check } => commands::lint::format(check)?,
        Commands::List { format } => commands::workspace::list(&format)?,
        Commands::Upgrade { incompatible } => commands::workspace::upgrade(incompatible)?,
        Commands::Audit => commands::workspace::audit()?,
        Commands::Machete => commands::workspace::machete()?,
        Commands::Test { all, package } => commands::build::test(all, package)?,
        Commands::Docker { command } => match command {
            DockerCommands::Build { package } => commands::docker::build(&config, &package)?,
            DockerCommands::Tag { package, registry } => {
                commands::docker::tag(&config, &package, &registry)?
            }
            DockerCommands::Push { package, registry } => {
                commands::docker::push(&config, &package, &registry)?
            }
            DockerCommands::Release { package, registry } => {
                commands::docker::release(&config, &package, &registry)?
            }
            DockerCommands::ReleaseAll { registry } => {
                commands::docker::release_all(config, &registry)?
            }
            DockerCommands::BuildAll => commands::docker::build_all(config)?,
        },
    }

    Ok(())
}
