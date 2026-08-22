use anyhow::Result;
use clap::Parser;
use warehouse_cli::{application, cli, ui};

#[tokio::main]
async fn main() -> Result<()> {
    ui::banner();
    let cli = cli::Cli::parse();
    application::run(cli).await
}
