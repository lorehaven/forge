use anyhow::Result;
use clap::Parser;
use riveter::cli::{ApplyScope, Cli, Cmd, EnvCmd};
use riveter::env::{current_env, env_list, env_set, env_show};
use riveter::render::ResourceScope;
use riveter::render::{Selector, generate_manifests_selected, list_resources};
use riveter::repl::{describe, kubectl_apply, kubectl_delete, ok, print_resource_list, repl};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Some(Cmd::Env { cmd }) => match cmd {
            EnvCmd::List => env_list(),
            EnvCmd::Set { env } => env_set(&env),
            EnvCmd::Show => env_show(),
        },
        Some(Cmd::List { scope, targets }) => {
            let env = current_env()?;
            let scope = map_apply_scope(scope);
            let selector = Selector::parse(&targets)?;

            let resources = list_resources(&env)?
                .into_iter()
                .filter(|r| selector.matches(&r.kind, &r.name) && r.in_scope(scope))
                .collect::<Vec<_>>();

            print_resource_list(&resources);
            Ok(())
        }
        Some(Cmd::Render { scope, targets }) => {
            let env = current_env()?;
            let selector = Selector::parse(&targets)?;
            let rendered = generate_manifests_selected(&env, map_apply_scope(scope), &selector)?;
            ok(&format!(
                "rendered {} resource(s) to {}",
                rendered.resource_count, rendered.path
            ));
            Ok(())
        }
        Some(Cmd::Apply {
            dry_run,
            scope,
            targets,
        }) => {
            let env = current_env()?;
            let selector = Selector::parse(&targets)?;
            let rendered = kubectl_apply(&env, dry_run, map_apply_scope(scope), &selector)?;

            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else {
                let verb = if dry_run { "would apply" } else { "applied" };
                ok(&format!(
                    "{verb} {} resource(s): {}",
                    rendered.resource_count,
                    describe(&rendered)
                ));
            }
            Ok(())
        }
        Some(Cmd::Delete { scope, targets }) => {
            let env = current_env()?;
            let selector = Selector::parse(&targets)?;
            let rendered = kubectl_delete(&env, map_apply_scope(scope), &selector)?;

            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else {
                ok(&format!(
                    "deleted {} resource(s): {}",
                    rendered.resource_count,
                    describe(&rendered)
                ));
            }
            Ok(())
        }
        Some(Cmd::Repl) | None => repl(),
    }
}

const fn map_apply_scope(value: ApplyScope) -> ResourceScope {
    match value {
        ApplyScope::Mutable => ResourceScope::Mutable,
        ApplyScope::Immutable => ResourceScope::Immutable,
        ApplyScope::All => ResourceScope::All,
    }
}
