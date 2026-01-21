use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dotenvy::from_path;
use minijinja::{Environment, Value, context};
use regex::Regex;
use serde_yaml::Value as YamlValue;
use std::{collections::HashMap, fs, path::Path, process::Command};

const OVERLAY_DIR: &str = "overlays";
const OUTPUT_DIR: &str = "manifests";
const ENV_FILE: &str = ".riveter-env";

/* ===================== CLI ===================== */

#[derive(Parser)]
#[command(name = "riveter")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
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
enum EnvCmd {
    List,
    Set { env: String },
    Show,
}

/* ===================== MAIN ===================== */

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

/* ===================== ENV MGMT ===================== */

fn env_list() -> Result<()> {
    let mut envs = Vec::new();
    for entry in fs::read_dir(OVERLAY_DIR)? {
        let entry = entry?;
        if entry.path().join("overlay.yaml").exists()
            && let Some(name) = entry.file_name().to_str()
        {
            envs.push(name.to_string());
        }
    }
    envs.sort();
    for e in envs {
        println!("{e}");
    }
    Ok(())
}

fn env_set(env: &str) -> Result<()> {
    let path = format!("{}/{}/overlay.yaml", OVERLAY_DIR, env);
    if !Path::new(&path).exists() {
        anyhow::bail!("overlay not found: {}", path);
    }
    fs::write(ENV_FILE, env)?;
    Ok(())
}

fn env_show() -> Result<()> {
    let env = current_env()?;
    println!("Current environment: {env}");
    Ok(())
}

fn current_env() -> Result<String> {
    Ok(fs::read_to_string(ENV_FILE)
        .context("No environment set. Run `riveter env set <env>`")?
        .trim()
        .to_string())
}

/* ===================== REPL ===================== */

fn manifest_exists(env: &str) -> bool {
    Path::new(&manifest_path(env)).exists()
}

fn ok(msg: &str) {
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

fn handle_repl_command(input: &str) -> Result<bool> {
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

fn repl() -> Result<()> {
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

/* ===================== KUBECTL ===================== */

fn manifest_path(env: &str) -> String {
    format!("{OUTPUT_DIR}/{env}-manifests.yaml")
}

fn kubectl_apply(env: &str, dry: bool) -> Result<()> {
    let mut cmd = Command::new("kubectl");
    cmd.arg("apply");
    if dry {
        cmd.arg("--dry-run=client");
    }
    cmd.arg("-f").arg(manifest_path(env));
    cmd.status()?;
    Ok(())
}

fn kubectl_delete(env: &str) -> Result<()> {
    Command::new("kubectl")
        .args(["delete", "-f", &manifest_path(env)])
        .status()?;
    Ok(())
}

/* ===================== MANIFEST RENDER ===================== */

fn load_env(env: &str) {
    let env_path = format!("{}/{}/.env", OVERLAY_DIR, env);
    if Path::new(&env_path).exists() {
        from_path(&env_path).ok();
    } else if Path::new(".env").exists() {
        from_path(".env").ok();
    }
}

fn load_embedded_templates(env: &mut Environment) {
    macro_rules! tpl {
        ($n:expr) => {
            env.add_template($n, include_str!(concat!("templates/", $n)))
                .unwrap()
        };
    }
    tpl!("cronjob.yaml.j2");
    tpl!("deployment.yaml.j2");
    tpl!("ingressroute.yaml.j2");
    tpl!("job.yaml.j2");
    tpl!("middleware.yaml.j2");
    tpl!("namespace.yaml.j2");
    tpl!("pv.yaml.j2");
    tpl!("pvc.yaml.j2");
    tpl!("service.yaml.j2");
    tpl!("serviceaccount.yaml.j2");
}

fn substitute(value: &mut YamlValue, env: &HashMap<String, String>, re: &Regex) {
    match value {
        YamlValue::String(s) => {
            *s = re
                .replace_all(s, |c: &regex::Captures| {
                    env.get(&c[1]).cloned().unwrap_or(c[0].to_string())
                })
                .into_owned();
        }
        YamlValue::Mapping(m) => m.values_mut().for_each(|v| substitute(v, env, re)),
        YamlValue::Sequence(s) => s.iter_mut().for_each(|v| substitute(v, env, re)),
        _ => {}
    }
}

fn generate_manifests(env_name: &str) -> Result<String> {
    load_env(env_name);

    let overlay_src = fs::read_to_string(format!("{OVERLAY_DIR}/{env_name}/overlay.yaml"))?;

    let mut overlay_jinja = Environment::new();
    overlay_jinja.set_loader(minijinja::path_loader(OVERLAY_DIR));
    overlay_jinja.add_global("env", env_name);

    let rendered_overlay = overlay_jinja.render_str(&overlay_src, Value::UNDEFINED)?;
    let mut data: YamlValue = serde_yaml::from_str(&rendered_overlay)?;

    let re = Regex::new(r"\$\{([^}]+)}")?;
    substitute(&mut data, &std::env::vars().collect(), &re);

    let resources = data["resources"]
        .as_sequence()
        .context("resources must be a list")?;

    let mut tpl_env = Environment::new();
    load_embedded_templates(&mut tpl_env);
    tpl_env.add_global("env", env_name);

    let mut out = Vec::new();
    for res in resources {
        let kind = res["kind"].as_str().context("kind missing")?;
        let tpl = format!("{}.yaml.j2", kind.to_lowercase());
        let y = tpl_env.get_template(&tpl)?.render(context! {
            data => &data,
            res => res,
            env => env_name,
        })?;
        out.push(y.trim().to_string());
    }

    fs::create_dir_all(OUTPUT_DIR)?;
    let path = manifest_path(env_name);
    fs::write(&path, strip_empty_lines(&(out.join("\n---\n") + "\n")))?;

    Ok(path)
}

fn strip_empty_lines(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
