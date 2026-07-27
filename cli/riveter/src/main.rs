use anyhow::Result;
use clap::{CommandFactory, Parser};
use riveter::cli::{ApplyScope, Cli, Cmd, EnvCmd};
use riveter::env::{env_list, env_set, env_show, resolve_env};
use riveter::render::ResourceScope;
use riveter::render::{Selector, generate_manifests_selected, list_resources};
use riveter::repl::{
    WaitPolicy, describe, kubectl_apply, kubectl_delete, kubectl_diff, note_skipped, ok,
    print_resource_list, prune, repl, report_prune, warn,
};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Some(Cmd::Env { cmd }) => match cmd {
            EnvCmd::List => env_list(),
            EnvCmd::Set { env } => env_set(&env),
            EnvCmd::Show => env_show(),
        },
        Some(Cmd::List { scope, targets }) => {
            let env = resolve_env(cli.env.as_deref())?;
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
            let env = resolve_env(cli.env.as_deref())?;
            let selector = Selector::parse(&targets)?;
            let rendered = generate_manifests_selected(&env, map_apply_scope(scope), &selector)?;
            ok(&format!(
                "rendered {} resource(s) to {}",
                rendered.resource_count, rendered.path
            ));
            if let Some(note) = note_skipped(&rendered) {
                warn(&note);
            }
            Ok(())
        }
        Some(Cmd::Apply {
            dry_run,
            no_wait,
            timeout,
            scope,
            targets,
        }) => {
            let env = resolve_env(cli.env.as_deref())?;
            let selector = Selector::parse(&targets)?;
            let wait = WaitPolicy {
                enabled: !no_wait,
                timeout_seconds: timeout,
            };
            let rendered = kubectl_apply(&env, dry_run, map_apply_scope(scope), &selector, wait)?;

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
        Some(Cmd::Diff { scope, targets }) => {
            let env = resolve_env(cli.env.as_deref())?;
            let selector = Selector::parse(&targets)?;
            let (rendered, differs) = kubectl_diff(&env, map_apply_scope(scope), &selector)?;

            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else if differs {
                warn("the cluster differs from these manifests");
            } else {
                ok("cluster matches these manifests");
            }
            Ok(())
        }
        Some(Cmd::Prune { dry_run }) => {
            let env = resolve_env(cli.env.as_deref())?;
            report_prune(&prune(&env, dry_run)?, dry_run);
            Ok(())
        }
        Some(Cmd::Delete { scope, targets }) => {
            let env = resolve_env(cli.env.as_deref())?;
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
        Some(Cmd::Help { command }) => print_help(command.as_deref()),
        Some(Cmd::Repl) | None => repl(),
    }
}

/// Mirrors the REPL's `help [command]`: the whole tree, or one command's
/// clap-generated detail.
fn print_help(topic: Option<&str>) -> Result<()> {
    let mut cmd = Cli::command();

    let Some(topic) = topic else {
        cmd.print_help()?;
        return Ok(());
    };

    if topic == "targets" || topic == "target" {
        println!("{}", riveter::help::targets());
        return Ok(());
    }

    // Accept the same aliases the help table advertises.
    let canonical =
        riveter::help::find_on(topic, riveter::help::Surface::Cli).map_or(topic, |c| c.name);
    match cmd.find_subcommand_mut(canonical) {
        Some(sub) => sub.print_help()?,
        None => anyhow::bail!(
            "{}",
            riveter::help::unknown_topic(topic, riveter::help::Surface::Cli)
        ),
    }

    Ok(())
}

const fn map_apply_scope(value: ApplyScope) -> ResourceScope {
    match value {
        ApplyScope::Mutable => ResourceScope::Mutable,
        ApplyScope::Immutable => ResourceScope::Immutable,
        ApplyScope::All => ResourceScope::All,
    }
}
