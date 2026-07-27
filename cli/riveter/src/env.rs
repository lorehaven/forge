use crate::config;
use anyhow::Context;
use quench_cli::prelude::{Tone, print_status};
use std::fs;
use std::path::Path;

pub const OVERLAY_DIR: &str = "overlays";
pub const OUTPUT_DIR: &str = "manifests";

pub fn env_list() -> anyhow::Result<()> {
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

pub fn env_set(env: &str) -> anyhow::Result<()> {
    ensure_overlay_exists(env)?;

    let mut config = config::load_config()?;
    config.env.current = Some(env.to_string());
    config::save_config(&config)?;

    Ok(())
}

pub fn env_show() -> anyhow::Result<()> {
    let env = current_env()?;
    let source = if std::env::var_os(ENV_VAR).is_some() {
        " (from $RIVETER_ENV)"
    } else {
        ""
    };

    print_status(
        Tone::Info,
        "riveter",
        &format!("current environment: {env}{source}"),
    );
    Ok(())
}

/// Overrides the environment recorded by `env set` for one process.
pub const ENV_VAR: &str = "RIVETER_ENV";

/// Resolves the environment for a single invocation.
///
/// `--env` wins, then `$RIVETER_ENV`, then whatever `env set` recorded. The
/// recorded value is shared mutable state in the working directory — a second
/// terminal running `env set` retargets every other one — so anything that must
/// not be retargeted out from under it should name the environment explicitly.
pub fn resolve_env(explicit: Option<&str>) -> anyhow::Result<String> {
    let Some(env) = explicit else {
        return current_env();
    };

    let env = env.trim();
    anyhow::ensure!(!env.is_empty(), "--env needs an environment name");
    ensure_overlay_exists(env)?;

    Ok(env.to_string())
}

pub fn current_env() -> anyhow::Result<String> {
    if let Some(env) = std::env::var_os(ENV_VAR) {
        let env = env.to_string_lossy().trim().to_string();
        if !env.is_empty() {
            ensure_overlay_exists(&env)?;
            return Ok(env);
        }
    }

    let config = config::load_config()?;

    config.env.current.context(
        "No environment set. Run `riveter env set <env>`, pass `--env <env>`, or set $RIVETER_ENV",
    )
}

fn ensure_overlay_exists(env: &str) -> anyhow::Result<()> {
    let path = format!("{OVERLAY_DIR}/{env}/overlay.yaml");
    anyhow::ensure!(Path::new(&path).exists(), "overlay not found: {path}");

    Ok(())
}

#[must_use]
pub fn manifest_path(env: &str) -> String {
    format!("{OUTPUT_DIR}/{env}-manifests.yaml")
}
