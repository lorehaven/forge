//! Conveyor's command line.
//!
//! Everything here goes through the same HTTP API the UI uses, with one
//! exception: `validate` links conveyor's own parser in and needs no running
//! service, so a pipeline can be checked before it is ever pushed.

use anyhow::Result;
use clap::Parser;
use quench_cli::prelude::{Tone, print_status};

mod cli;
mod client;
mod commands;
mod config;

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();

    if let Err(error) = run().await {
        // One line, on stderr, and a non-zero exit - so this is usable as the
        // last line of a script.
        print_status(Tone::Error, "conveyor", &error.to_string());
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = cli::Cli::parse();

    // Checking a file needs no service, so it is handled before one is built -
    // otherwise `conveyor validate` would demand a URL it never uses.
    if let cli::Commands::Validate(args) = &cli.command {
        return commands::validate(args);
    }

    let client = client::Client::new(
        cli.url.clone(),
        cli.username.clone(),
        cli.password.clone(),
        cli.gatehouse_url.clone(),
        cli.insecure,
    )
    .await?;

    match &cli.command {
        cli::Commands::Repo { command } => commands::repo(&client, command).await,
        cli::Commands::Run(args) => commands::run(&client, args).await,
        cli::Commands::Runs(args) => commands::runs(&client, args).await,
        cli::Commands::Show(args) => commands::show(&client, args).await,
        cli::Commands::Logs(args) => commands::logs(&client, args).await,
        cli::Commands::Cancel(args) => commands::cancel(&client, args).await,
        cli::Commands::Secret { command } => commands::secret(&client, command).await,
        cli::Commands::Validate(_) => unreachable!("handled above"),
    }
}
