use anyhow::Result;
use clap::Parser;
use foreman::cli::{Cli, Command};
use foreman::estate::Estate;
use foreman::repl::Picker;
use foreman::{commands, ui};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            ui::error("error", error.to_string());
            // The cause chain is where the useful half of a config error lives.
            for cause in error.chain().skip(1) {
                ui::error("", format!("  {cause}"));
            }
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8> {
    let cli = Cli::parse();

    // `foreman init` writes the file everything else needs, so it cannot
    // require one to already exist.
    if let Some(Command::Init { force }) = &cli.command {
        commands::init(*force)?;
        return Ok(0);
    }

    let estate = Estate::load()?;

    match cli.command {
        None => {
            // No verb at all is a start, and a bare service name is a start of
            // that service: `foreman conveyor` is what anyone who has read the
            // service list types first.
            estate.reject_unknown(&cli.bare)?;
            commands::start(&estate, &cli.bare)?;
        }
        Some(Command::Start { services }) => commands::start(&estate, &services)?,
        Some(Command::Stop { services }) => commands::stop(&estate, &services)?,
        Some(Command::Status) => commands::status(&estate)?,
        Some(Command::Logs { service }) => commands::logs(&estate, &service)?,
        Some(Command::Repl { services }) => {
            let mut picker = Picker::new(&estate);
            picker.seed(&services)?;
            picker.run()?;
        }
        Some(Command::Db) => commands::db(&estate)?,
        Some(Command::Reset) => commands::reset(&estate)?,
        Some(Command::Test { args }) => return Ok(commands::test(&estate, &args)? as u8),
        Some(Command::Run { task, services }) => commands::run_task(&estate, &task, &services)?,
        Some(Command::List) => commands::list(&estate)?,
        Some(Command::Env { service }) => commands::env(&estate, &service)?,
        Some(Command::Init { .. }) => unreachable!("handled before the estate loads"),
    }

    Ok(0)
}
