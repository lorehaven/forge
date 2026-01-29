use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "riveter")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
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

#[derive(Subcommand, Debug)]
pub enum EnvCmd {
    List,
    Set { env: String },
    Show,
}
