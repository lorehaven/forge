use anyhow::Context;
use std::fs;
use std::path::Path;

pub const OVERLAY_DIR: &str = "overlays";
pub const OUTPUT_DIR: &str = "manifests";
const ENV_FILE: &str = ".riveter-env";

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
    let path = format!("{}/{}/overlay.yaml", OVERLAY_DIR, env);
    if !Path::new(&path).exists() {
        anyhow::bail!("overlay not found: {}", path);
    }
    fs::write(ENV_FILE, env)?;
    Ok(())
}

pub fn env_show() -> anyhow::Result<()> {
    let env = current_env()?;
    println!("Current environment: {env}");
    Ok(())
}

pub fn current_env() -> anyhow::Result<String> {
    Ok(fs::read_to_string(ENV_FILE)
        .context("No environment set. Run `riveter env set <env>`")?
        .trim()
        .to_string())
}

pub fn manifest_path(env: &str) -> String {
    format!("{OUTPUT_DIR}/{env}-manifests.yaml")
}
