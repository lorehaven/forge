use clap::{Parser, Subcommand, ValueEnum};

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
    Render {
        #[arg(long, value_enum, default_value_t = ApplyScope::All)]
        scope: ApplyScope,
    },
    Apply {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable)]
        scope: ApplyScope,
    },
    Delete {
        #[arg(long, value_enum, default_value_t = ApplyScope::Mutable)]
        scope: ApplyScope,
    },
    Repl,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ApplyScope {
    Mutable,
    Immutable,
    All,
}

#[derive(Subcommand, Debug)]
pub enum EnvCmd {
    List,
    Set { env: String },
    Show,
}
