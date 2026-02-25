mod application;
mod cli;
mod config;
mod crates_api;
mod docker_api;
mod domain;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    application::run(cli).await
}
