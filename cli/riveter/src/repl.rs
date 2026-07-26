use crate::env::{current_env, env_list, env_set, env_show};
use crate::help;
use crate::render::{
    RenderedManifest, ResourceRef, ResourceScope, Selector, generate_manifests_selected,
    list_resources,
};
use anyhow::Context;
use quench_cli::prelude::{Tone, print_box_banner, print_status, repl_prompt};
use std::process::Command;

pub fn ok(msg: &str) {
    print_status(Tone::Success, "ok", msg);
}

fn warn(msg: &str) {
    print_status(Tone::Warn, "warn", msg);
}

fn prompt() -> String {
    let env = current_env().unwrap_or_else(|_| "unset".into());
    repl_prompt("riveter", &env)
}

/// Prints without panicking when stdout is a closed pipe (`help | head`).
fn print_block(text: &str) {
    use std::io::Write;

    let stdout = std::io::stdout();
    let _ = writeln!(stdout.lock(), "{text}");
}

fn repl_help(topic: Option<&str>) {
    let Some(topic) = topic else {
        print_block(&help::overview());
        return;
    };

    if topic == "targets" || topic == "target" {
        print_block(&help::targets());
    } else if let Some(cmd) = help::find_on(topic, help::Surface::Repl) {
        print_block(&help::detail(cmd));
    } else {
        warn(&help::unknown_topic(topic, help::Surface::Repl));
    }
}

fn handle_repl_command(input: &str) -> anyhow::Result<bool> {
    let args = input.split_whitespace().collect::<Vec<_>>();
    if args.is_empty() {
        return Ok(false);
    }

    match args[0] {
        "help" | "h" => {
            repl_help(args.get(1).copied());
        }

        "exit" | "quit" | "q" => {
            return Ok(true);
        }

        "env" if args.len() > 1 && args[1] == "list" => {
            env_list()?;
        }

        "env" if args.len() > 2 && args[1] == "set" => {
            env_set(args[2])?;
            ok(&format!("environment set to {}", args[2]));
        }

        "env" if args.len() > 1 && args[1] == "show" => {
            env_show()?;
        }

        "list" | "ls" => {
            let env = current_env()?;
            let scope = parse_scope_arg(&args, ResourceScope::All)?;
            let selector = parse_targets(&args)?;

            let resources: Vec<ResourceRef> = list_resources(&env)?
                .into_iter()
                .filter(|r| selector.matches(&r.kind, &r.name) && r.in_scope(scope))
                .collect();
            print_resource_list(&resources);
        }

        "render" | "r" => {
            let env = current_env()?;
            let scope = parse_scope_arg(&args, ResourceScope::All)?;
            let selector = parse_targets(&args)?;
            let rendered = generate_manifests_selected(&env, scope, &selector)?;
            ok(&format!(
                "rendered {} resource(s) to {}",
                rendered.resource_count, rendered.path
            ));
        }

        "apply" | "a" => {
            let env = current_env()?;
            let dry = args.contains(&"--dry-run");
            let scope = parse_scope_arg(&args, ResourceScope::Mutable)?;
            let selector = parse_targets(&args)?;

            let rendered = kubectl_apply(&env, dry, scope, &selector)?;
            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else {
                let verb = if dry { "would apply" } else { "applied" };
                ok(&format!(
                    "{verb} {} resource(s): {}",
                    rendered.resource_count,
                    describe(&rendered)
                ));
            }
        }

        "delete" | "del" | "d" => {
            let env = current_env()?;
            let scope = parse_scope_arg(&args, ResourceScope::Mutable)?;
            let selector = parse_targets(&args)?;

            let rendered = kubectl_delete(&env, scope, &selector)?;
            if rendered.resource_count == 0 {
                ok("no resources matched selected scope");
            } else {
                warn(&format!(
                    "deleted {} resource(s) for env {env}: {}",
                    rendered.resource_count,
                    describe(&rendered)
                ));
            }
        }

        _ => {
            warn("unknown command — type `help`");
        }
    }

    Ok(false)
}

fn error(msg: &str) {
    print_status(Tone::Error, "error", msg);
}

pub fn repl() -> anyhow::Result<()> {
    use rustyline::{DefaultEditor, error::ReadlineError};

    print_box_banner("Riveter REPL", "env-aware manifest commands");
    print_status(Tone::Info, "hint", "type `help` to list commands");

    let mut rl = DefaultEditor::new()?;

    loop {
        let prompt = prompt();
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                rl.add_history_entry(line)?;
                match handle_repl_command(line) {
                    Ok(exit) => {
                        if exit {
                            break;
                        }
                    }
                    Err(e) => {
                        error(&format!("{e:#}"));
                    }
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

pub fn kubectl_apply(
    env: &str,
    dry: bool,
    scope: ResourceScope,
    selector: &Selector,
) -> anyhow::Result<RenderedManifest> {
    let rendered = generate_manifests_selected(env, scope, selector)?;
    if rendered.resource_count == 0 {
        return Ok(rendered);
    }

    let mut cmd = Command::new("kubectl");
    cmd.arg("apply");
    if dry {
        cmd.arg("--dry-run=client");
    }
    let status = cmd.arg("-f").arg(&rendered.path).status()?;
    anyhow::ensure!(status.success(), "kubectl apply failed");
    Ok(rendered)
}

pub fn kubectl_delete(
    env: &str,
    scope: ResourceScope,
    selector: &Selector,
) -> anyhow::Result<RenderedManifest> {
    let rendered = generate_manifests_selected(env, scope, selector)?;
    if rendered.resource_count == 0 {
        return Ok(rendered);
    }

    let status = Command::new("kubectl")
        .args(["delete", "-f", &rendered.path])
        .status()?;
    anyhow::ensure!(status.success(), "kubectl delete failed");
    Ok(rendered)
}

/// `kind/name, kind/name` for reporting what a command touched.
pub fn describe(rendered: &RenderedManifest) -> String {
    rendered
        .selected
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn print_resource_list(resources: &[ResourceRef]) {
    use std::io::Write;

    if resources.is_empty() {
        warn("no resources matched");
        return;
    }

    let kind_width = resources.iter().map(|r| r.kind.len()).max().unwrap_or(4);
    let name_width = resources.iter().map(|r| r.name.len()).max().unwrap_or(4);

    // Written directly so a closed pipe (`riveter list | head`) ends the loop
    // instead of panicking the way `println!` does.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for res in resources {
        let lifecycle = if res.immutable { "immutable" } else { "mutable" };
        if writeln!(
            out,
            "  {:<kind_width$}  {:<name_width$}  {lifecycle}",
            res.kind, res.name
        )
        .is_err()
        {
            break;
        }
    }
}

/// Everything that is not the command word, a flag, or a flag's value is a
/// resource target.
fn parse_targets(args: &[&str]) -> anyhow::Result<Selector> {
    let mut targets = Vec::new();
    let mut idx = 1;

    while idx < args.len() {
        let arg = args[idx];
        if arg == "--scope" {
            idx += 2;
        } else if arg.starts_with('-') {
            idx += 1;
        } else {
            targets.push(arg.to_string());
            idx += 1;
        }
    }

    Selector::parse(&targets)
}

fn parse_scope_arg(args: &[&str], default: ResourceScope) -> anyhow::Result<ResourceScope> {
    let Some((idx, _)) = args.iter().enumerate().find(|(_, arg)| **arg == "--scope") else {
        return Ok(default);
    };

    let raw = args
        .get(idx + 1)
        .context("missing value for --scope (expected mutable|immutable|all)")?;

    match raw.to_ascii_lowercase().as_str() {
        "mutable" => Ok(ResourceScope::Mutable),
        "immutable" => Ok(ResourceScope::Immutable),
        "all" => Ok(ResourceScope::All),
        _ => anyhow::bail!("invalid --scope value `{raw}` (expected mutable|immutable|all)"),
    }
}
