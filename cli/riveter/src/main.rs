use anyhow::Result;
use clap::Parser;
use riveter::cli::{ApplyScope, Cli, Cmd, EnvCmd};
use riveter::env::{current_env, env_list, env_set, env_show};
use riveter::render::ResourceScope;
use riveter::render::generate_manifests_with_scope;
use riveter::repl::{kubectl_apply, kubectl_delete, ok, repl};

fn main() -> Result<()> {
    let cli = Cli::parse();

    let _ = match cli.cmd {
        Some(Cmd::Env { cmd }) => match cmd {
            EnvCmd::List => env_list(),
            EnvCmd::Set { env } => env_set(&env),
            EnvCmd::Show => env_show(),
        },
        Some(Cmd::Render { scope }) => {
            let env = current_env()?;
            let rendered = generate_manifests_with_scope(&env, map_apply_scope(scope))?;
            ok(&format!("rendered {}", rendered.path));
            Ok(())
        }
        Some(Cmd::Apply { dry_run, scope }) => {
            let env = current_env()?;
            kubectl_apply(&env, dry_run, map_apply_scope(scope)).map(|_| ())
        }
        Some(Cmd::Delete { scope }) => {
            let env = current_env()?;
            kubectl_delete(&env, map_apply_scope(scope)).map(|_| ())
        }
        Some(Cmd::Repl) | None => repl(),
    };

    Ok(())
}

fn map_apply_scope(value: ApplyScope) -> ResourceScope {
    match value {
        ApplyScope::Mutable => ResourceScope::Mutable,
        ApplyScope::Immutable => ResourceScope::Immutable,
        ApplyScope::All => ResourceScope::All,
    }
}
