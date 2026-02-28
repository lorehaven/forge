use crate::env::{current_env, env_list, env_set, env_show};
use crate::render::{ResourceScope, generate_manifests_with_scope};
use anyhow::Context;
use std::process::Command;

pub fn ok(msg: &str) {
    println!("\x1b[32m✓\x1b[0m {msg}");
}

fn warn(msg: &str) {
    println!("\x1b[33m⚠\x1b[0m {msg}");
}

fn prompt() -> String {
    let env = current_env().unwrap_or_else(|_| "unset".into());
    format!("\x1b[1;34mriveter\x1b[0m(\x1b[1;32m{env}\x1b[0m)> ")
}

fn repl_help() {
    println!(
        "\
Commands:
  env
    list                List available environments
    set <env>           Set current environment
    show                Show current environment

  render [--scope mutable|immutable|all]
                        Render manifests
  apply [--dry-run] [--scope mutable|immutable|all]
                        Apply manifests via kubectl
  delete [--scope mutable|immutable|all]
                        Delete manifests via kubectl

  help                  Show this help
  exit | quit            Leave REPL
"
    );
}

fn handle_repl_command(input: &str) -> anyhow::Result<bool> {
    let args = input.split_whitespace().collect::<Vec<_>>();
    if args.is_empty() {
        return Ok(false);
    }

    match args[0] {
        "help" | "h" => {
            repl_help();
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

        "render" | "r" => {
            let env = current_env()?;
            let scope = parse_scope_arg(&args, ResourceScope::All)?;
            let rendered = generate_manifests_with_scope(&env, scope)?;
            ok(&format!("rendered {}", rendered.path));
        }

        "apply" | "a" => {
            let env = current_env()?;
            let dry = args.contains(&"--dry-run");
            let scope = parse_scope_arg(&args, ResourceScope::Mutable)?;
            let count = kubectl_apply(&env, dry, scope)?;
            if count == 0 {
                ok("no resources matched selected scope");
            } else if dry {
                ok("kubectl apply --dry-run succeeded");
            } else {
                ok("kubectl apply succeeded");
            }
        }

        "delete" | "del" | "d" => {
            let env = current_env()?;
            let scope = parse_scope_arg(&args, ResourceScope::Mutable)?;
            let count = kubectl_delete(&env, scope)?;
            if count > 0 {
                warn(&format!("deleted resources for env: {env}"));
                ok("kubectl delete completed");
            } else {
                ok("no resources matched selected scope");
            }
        }

        _ => {
            warn("unknown command — type `help`");
        }
    }

    Ok(false)
}

pub fn repl() -> anyhow::Result<()> {
    use rustyline::{DefaultEditor, error::ReadlineError};

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
                if handle_repl_command(line)? {
                    break;
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

pub fn kubectl_apply(env: &str, dry: bool, scope: ResourceScope) -> anyhow::Result<usize> {
    let rendered = generate_manifests_with_scope(env, scope)?;
    if rendered.resource_count == 0 {
        return Ok(0);
    }

    let mut cmd = Command::new("kubectl");
    cmd.arg("apply");
    if dry {
        cmd.arg("--dry-run=client");
    }
    let status = cmd.arg("-f").arg(rendered.path).status()?;
    anyhow::ensure!(status.success(), "kubectl apply failed");
    Ok(rendered.resource_count)
}

pub fn kubectl_delete(env: &str, scope: ResourceScope) -> anyhow::Result<usize> {
    let rendered = generate_manifests_with_scope(env, scope)?;
    if rendered.resource_count == 0 {
        return Ok(0);
    }

    let status = Command::new("kubectl")
        .args(["delete", "-f", &rendered.path])
        .status()?;
    anyhow::ensure!(status.success(), "kubectl delete failed");
    Ok(rendered.resource_count)
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
