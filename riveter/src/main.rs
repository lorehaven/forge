use anyhow::Result;
use clap::Parser;
use riveter::cli::{Cli, Cmd, EnvCmd};
use riveter::env::{current_env, env_list, env_set, env_show};
use riveter::render::generate_manifests;
use riveter::repl::{kubectl_apply, kubectl_delete, ok, repl};

fn main() -> Result<()> {
    let cli = Cli::parse();

    let _ = match cli.cmd {
        Some(Cmd::Env { cmd }) => match cmd {
            EnvCmd::List => env_list(),
            EnvCmd::Set { env } => env_set(&env),
            EnvCmd::Show => env_show(),
        },
        Some(Cmd::Render) => {
            let env = current_env()?;
            let path = generate_manifests(&env)?;
            ok(&format!("rendered {path}"));
            Ok(())
        }
        Some(Cmd::Apply { dry_run }) => {
            let env = current_env()?;
            kubectl_apply(&env, dry_run)
        }
        Some(Cmd::Delete) => {
            let env = current_env()?;
            kubectl_delete(&env)
        }
        Some(Cmd::Repl) | None => repl(),
    };

    Ok(())
}
