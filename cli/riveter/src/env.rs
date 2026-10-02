use crate::config;
use anyhow::Context;
use quench_cli::prelude::{Tone, print_status};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

pub const OVERLAY_DIR: &str = "overlays";
pub const OUTPUT_DIR: &str = "manifests";

/// Where overlays are read from and manifests written, plus what a render adds to every resource.
///
/// Riveter works relative to the current directory; an installed package has no
/// such directory, so [`with_workspace`] points one command at a scratch tree
/// instead - without changing the process working directory, which every test
/// and thread would share.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    /// Replaces `overlays/`.
    pub overlays_dir: Option<PathBuf>,
    /// Replaces `manifests/`.
    pub output_dir: Option<PathBuf>,
    /// Replaces the overlay's `.env` file, so values never touch the disk.
    pub env_vars: Option<HashMap<String, String>>,
    /// Added to every rendered resource's `labels`, overriding the overlay's.
    pub resource_labels: BTreeMap<String, String>,
    /// Added to every rendered resource's `annotations`, overriding the overlay's.
    pub resource_annotations: BTreeMap<String, String>,
}

thread_local! {
    static WORKSPACE: RefCell<Option<Workspace>> = const { RefCell::new(None) };
}

/// Restores the previous workspace on drop, so a panic cannot leak one.
struct WorkspaceGuard(Option<Workspace>);

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        WORKSPACE.with(|w| *w.borrow_mut() = self.0.take());
    }
}

/// Runs `f` with `workspace` in effect for this thread.
pub fn with_workspace<T>(workspace: Workspace, f: impl FnOnce() -> T) -> T {
    let previous = WORKSPACE.with(|w| w.borrow_mut().replace(workspace));
    let _guard = WorkspaceGuard(previous);
    f()
}

fn from_workspace<T>(pick: impl FnOnce(&Workspace) -> Option<T>) -> Option<T> {
    WORKSPACE.with(|w| w.borrow().as_ref().and_then(pick))
}

/// The directory overlays are read from.
#[must_use]
pub fn overlay_dir() -> PathBuf {
    from_workspace(|w| w.overlays_dir.clone()).unwrap_or_else(|| PathBuf::from(OVERLAY_DIR))
}

/// The directory manifests are written to.
#[must_use]
pub fn output_dir() -> PathBuf {
    from_workspace(|w| w.output_dir.clone()).unwrap_or_else(|| PathBuf::from(OUTPUT_DIR))
}

/// Variables standing in for the overlay's `.env` file, if the workspace carries any.
#[must_use]
pub fn env_override() -> Option<HashMap<String, String>> {
    from_workspace(|w| w.env_vars.clone())
}

/// Labels and annotations to add to every rendered resource.
#[must_use]
pub fn resource_metadata() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    WORKSPACE.with(|w| {
        w.borrow().as_ref().map_or_else(Default::default, |w| {
            (w.resource_labels.clone(), w.resource_annotations.clone())
        })
    })
}

pub fn env_list() -> anyhow::Result<()> {
    let mut envs = Vec::new();
    for entry in fs::read_dir(overlay_dir())? {
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
    let path = overlay_dir().join(env).join("overlay.yaml");
    anyhow::ensure!(path.exists(), "overlay not found: {}", path.display());

    Ok(())
}

#[must_use]
pub fn manifest_path(env: &str) -> String {
    format!("{}/{env}-manifests.yaml", output_dir().display())
}
