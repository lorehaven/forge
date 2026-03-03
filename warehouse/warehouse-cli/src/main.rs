mod api;
mod application;
mod cli;
mod config;
mod domain;
mod ui;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    ui::banner();
    let cli = cli::Cli::parse();
    application::run(cli).await
}
