use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "riveter")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    Env {
        #[command(subcommand)]
        cmd: EnvCmd,
    },
    Render,
    Apply {
        #[arg(long)]
        dry_run: bool,
    },
    Delete,
    Repl,
}

#[derive(Subcommand)]
pub enum EnvCmd {
    List,
    Set { env: String },
    Show,
}
