mod api;
mod application;
mod cli;
mod config;
mod domain;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    application::run(cli).await
}
