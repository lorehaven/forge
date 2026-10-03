//! `riveter secrets sync`: an overlay's `.env`, as a Kubernetes Secret.
//!
//! A `.env` is gitignored and `pack` never includes it, so nothing that reads a package - Warehouse, a pod
//! installing one - can see the values an install needs. This puts them in the cluster, as one Secret per
//! overlay, written with the caller's own `kubectl` access and read later by whatever installs the package
//! (Gantry's runner Job mounts it as `--env-file`).
//!
//! The values are handled as little as possible: they go to `kubectl` over standard input, never in an
//! argument; the type that reports what was done has no field for a value, so nothing here can print one;
//! and the Secret carries a hash of what it holds, so a later install can tell the values have changed
//! without reading them.

use crate::render::parse_dotenv;
use anyhow::{Context as _, Result, bail, ensure};
use base64::Engine as _;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The Secret an overlay's values are kept in: `gantry-values-<package>`.
#[must_use]
pub fn secret_name(package: &str) -> String {
    format!("gantry-values-{package}")
}

/// The one key the file is stored under.
pub const KEY: &str = "env";
/// Label naming the package a Secret holds values for.
pub const LABEL: &str = "riveter.forge/values-for";
/// Annotation holding the SHA-256 of the stored file.
pub const HASH_ANNOTATION: &str = "riveter.forge/values-sha256";

/// What to sync, and where to.
#[derive(Debug, Clone, Default)]
pub struct SyncRequest {
    /// The overlays to sync, by name.
    pub overlays: Vec<String>,
    /// Every overlay that has a `.env`.
    pub all: bool,
    /// The namespace the Secrets go in - Gantry's, so its runner can mount them.
    pub namespace: String,
    /// A kubectl context other than the current one.
    pub context: Option<String>,
    /// Report what would be synced without touching the cluster.
    pub dry_run: bool,
}

/// What was synced. Names only: there is deliberately nowhere to put a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    /// The overlay, which is also the package name.
    pub package: String,
    /// The Secret written.
    pub secret: String,
    /// Where.
    pub namespace: String,
    /// The variables it holds, by name, sorted.
    pub variables: Vec<String>,
}

/// The overlays under `overlays_dir` that have both an `overlay.yaml` and a `.env`, sorted.
pub fn overlays_with_values(overlays_dir: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(overlays_dir)
        .with_context(|| format!("failed to read {}", overlays_dir.display()))?
    {
        let path = entry?.path();
        if path.join("overlay.yaml").is_file()
            && path.join(".env").is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            found.push(name.to_string());
        }
    }
    found.sort();
    Ok(found)
}

/// The Secret for `package`, as the manifest sent to `kubectl apply`.
#[must_use]
pub fn manifest(package: &str, namespace: &str, contents: &[u8]) -> serde_json::Value {
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "type": "Opaque",
        "metadata": {
            "name": secret_name(package),
            "namespace": namespace,
            // Not `app.kubernetes.io/managed-by: riveter`: `prune` selects on that label, and a Secret
            // in another environment's namespace must never look like something it owns.
            "labels": { LABEL: package },
            "annotations": { HASH_ANNOTATION: hex::encode(Sha256::digest(contents)) },
        },
        "data": { KEY: base64::engine::general_purpose::STANDARD.encode(contents) },
    })
}

/// Sends a manifest to `kubectl apply -f -` over standard input.
fn apply(manifest: &serde_json::Value, context: Option<&str>) -> Result<()> {
    let mut command = Command::new("kubectl");
    if let Some(context) = context {
        command.args(["--context", context]);
    }
    let mut child = command
        .args(["apply", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not run kubectl")?;

    child
        .stdin
        .take()
        .context("kubectl's stdin was not available")?
        .write_all(manifest.to_string().as_bytes())?;

    let out = child.wait_with_output()?;
    ensure!(
        out.status.success(),
        "kubectl could not apply the Secret:\n{}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

/// Syncs the requested overlays' `.env` files into Secrets.
pub fn sync(request: &SyncRequest, overlays_dir: &Path) -> Result<Vec<Synced>> {
    ensure!(
        request.all == request.overlays.is_empty(),
        "name the overlays to sync, or pass --all - not both, not neither"
    );
    ensure!(
        !request.namespace.trim().is_empty(),
        "a namespace is needed"
    );

    let packages = if request.all {
        let found = overlays_with_values(overlays_dir)?;
        ensure!(
            !found.is_empty(),
            "no overlay under {} has a .env to sync",
            overlays_dir.display()
        );
        found
    } else {
        request.overlays.clone()
    };

    let mut synced = Vec::new();
    for package in &packages {
        let dir: PathBuf = overlays_dir.join(package);
        ensure!(
            dir.join("overlay.yaml").is_file(),
            "overlay not found: {}",
            dir.join("overlay.yaml").display()
        );
        ensure!(
            rivet_package::manifest::is_valid_name(package),
            "`{package}` cannot be a package name (a lowercase DNS-1123 label), so it has no Secret to sync to"
        );
        let env_path = dir.join(".env");
        if !env_path.is_file() {
            bail!(
                "{} has no .env, so there are no values to sync",
                dir.display()
            );
        }

        let contents = std::fs::read(&env_path)
            .with_context(|| format!("failed to read {}", env_path.display()))?;
        let mut variables: Vec<String> = parse_dotenv(&String::from_utf8_lossy(&contents))
            .into_keys()
            .collect();
        variables.sort();
        ensure!(
            !variables.is_empty(),
            "{} defines no variables",
            env_path.display()
        );

        if !request.dry_run {
            apply(
                &manifest(package, &request.namespace, &contents),
                request.context.as_deref(),
            )
            .with_context(|| format!("syncing {package}"))?;
        }

        synced.push(Synced {
            package: package.clone(),
            secret: secret_name(package),
            namespace: request.namespace.clone(),
            variables,
        });
    }
    Ok(synced)
}
