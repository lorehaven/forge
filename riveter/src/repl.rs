use crate::env::{current_env, env_list, env_set, env_show, manifest_path};
use crate::render::generate_manifests;
use std::path::Path;
use std::process::Command;

fn manifest_exists(env: &str) -> bool {
    Path::new(&manifest_path(env)).exists()
}

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

  render                Render manifests
  apply [--dry-run]     Apply manifests via kubectl
  delete                Delete manifests via kubectl

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
            generate_manifests(&env)?;
            ok(&format!("rendered {}", manifest_path(&env)));
        }

        "apply" | "a" => {
            let env = current_env()?;
            if !manifest_exists(&env) {
                warn("manifest not found — run `render` first");
            } else {
                let dry = args.contains(&"--dry-run");
                kubectl_apply(&env, dry)?;
                if dry {
                    ok("kubectl apply --dry-run succeeded");
                } else {
                    ok("kubectl apply succeeded");
                }
            }
        }

        "delete" | "del" | "d" => {
            let env = current_env()?;
            if !manifest_exists(&env) {
                warn("manifest not found — nothing to delete");
            } else {
                warn(&format!("deleting resources for env: {env}"));
                kubectl_delete(&env)?;
                ok("kubectl delete completed");
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

pub fn kubectl_apply(env: &str, dry: bool) -> anyhow::Result<()> {
    let mut cmd = Command::new("kubectl");
    cmd.arg("apply");
    if dry {
        cmd.arg("--dry-run=client");
    }
    cmd.arg("-f").arg(manifest_path(env));
    cmd.status()?;
    Ok(())
}

pub fn kubectl_delete(env: &str) -> anyhow::Result<()> {
    Command::new("kubectl")
        .args(["delete", "-f", &manifest_path(env)])
        .status()?;
    Ok(())
}
