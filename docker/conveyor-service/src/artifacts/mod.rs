//! Keeping what a job produced: uploaded to warehouse's file storage before the checkout is deleted.
//! With no warehouse configured, nothing is recorded rather than a row promising a file that isn't there.

use crate::domain::Artifact;
use crate::workspace::Workspace;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

/// Above this, a build wants a registry, not a file store held open by a worker.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("`{path}` is outside the checkout")]
    Outside { path: String },

    #[error("`{path}` was declared as an artifact but the job did not produce it")]
    Missing { path: String },

    #[error("`{path}` is {size} bytes, over the {MAX_BYTES}-byte limit")]
    TooLarge { path: String, size: u64 },

    #[error("could not read `{path}`: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("warehouse rejected `{path}` ({status})")]
    Rejected { path: String, status: u16 },

    #[error("could not reach warehouse: {0}")]
    Unreachable(#[from] reqwest::Error),
}

/// Where artifacts go.
#[derive(Clone)]
pub struct WarehouseStore {
    base_url: String,
    storage: String,
    credentials: Option<(String, String)>,
    http: reqwest::Client,
}

impl WarehouseStore {
    /// `None` when this deployment keeps no artifacts.
    pub fn from_env() -> Option<Self> {
        let base_url = envmnt::get_or("WAREHOUSE_URL", "");
        let base_url = base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return None;
        }

        let username = envmnt::get_or("WAREHOUSE_TECH_USERNAME", "");
        let password = envmnt::get_or("WAREHOUSE_TECH_PASSWORD", "");
        let credentials =
            (!username.trim().is_empty()).then(|| (username.trim().to_string(), password));

        Some(Self {
            base_url: base_url.to_string(),
            storage: envmnt::get_or("CONVEYOR_ARTIFACT_STORAGE", "artifacts"),
            credentials,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                // Same allowance sage makes for switchboard's internal certs.
                .danger_accept_invalid_certs(!envmnt::is_or("WAREHOUSE_TLS_VERIFY", true))
                .build()
                .unwrap_or_default(),
        })
    }

    /// Scoped by run id so two runs of the same commit don't overwrite each other.
    fn remote_path(&self, run_id: &str, name: &str) -> String {
        format!("conveyor/{run_id}/{name}")
    }

    async fn upload(&self, remote_path: &str, bytes: Vec<u8>) -> Result<String, ArtifactError> {
        let url = format!(
            "{}/api/v1/files/{}/file?path={}",
            self.base_url,
            self.storage,
            urlencoding::encode(remote_path)
        );

        let mut request = self
            .http
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .body(bytes);

        if let Some((username, password)) = &self.credentials {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(ArtifactError::Rejected {
                path: remote_path.to_string(),
                status: response.status().as_u16(),
            });
        }

        // Query included: warehouse addresses the file by `path`, so trimming it would collide every artifact.
        Ok(url)
    }
}

/// One artifact, ready to record.
pub struct Collected {
    pub artifact: Artifact,
}

/// Uploads everything `job` declared; per-artifact errors don't fail the job (the build passed regardless).
pub async fn collect(
    store: Option<&WarehouseStore>,
    workspace: &Workspace,
    run_id: &str,
    job_id: &str,
    declared: &[String],
) -> (Vec<Collected>, Vec<ArtifactError>) {
    let mut kept = Vec::new();
    let mut problems = Vec::new();

    for path in declared {
        match collect_one(store, workspace, run_id, job_id, path).await {
            Ok(Some(collected)) => kept.push(collected),
            // No store configured; nothing to keep.
            Ok(None) => {}
            Err(error) => problems.push(error),
        }
    }

    (kept, problems)
}

async fn collect_one(
    store: Option<&WarehouseStore>,
    workspace: &Workspace,
    run_id: &str,
    job_id: &str,
    declared: &str,
) -> Result<Option<Collected>, ArtifactError> {
    // Checked before opening: a pipeline could declare `../../etc/passwd` otherwise.
    let resolved = workspace
        .resolve(declared)
        .ok_or_else(|| ArtifactError::Outside {
            path: declared.to_string(),
        })?;

    let metadata = tokio::fs::metadata(&resolved)
        .await
        .map_err(|_| ArtifactError::Missing {
            path: declared.to_string(),
        })?;

    if !metadata.is_file() {
        return Err(ArtifactError::Missing {
            path: declared.to_string(),
        });
    }

    if metadata.len() > MAX_BYTES {
        return Err(ArtifactError::TooLarge {
            path: declared.to_string(),
            size: metadata.len(),
        });
    }

    let Some(store) = store else {
        return Ok(None);
    };

    let bytes = tokio::fs::read(&resolved)
        .await
        .map_err(|source| ArtifactError::Unreadable {
            path: declared.to_string(),
            source,
        })?;

    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    let size = bytes.len();
    let name = file_name(&resolved, declared);
    let remote = store.remote_path(run_id, &name);
    let uri = store.upload(&remote, bytes).await?;

    tracing::info!("kept {declared} ({size} bytes) as {remote}");

    Ok(Some(Collected {
        artifact: Artifact {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.to_string(),
            job_id: job_id.to_string(),
            kind: "file".to_string(),
            name,
            version: None,
            uri,
            digest: Some(digest),
            created_at: Utc::now(),
        },
    }))
}

/// The file's own name, falling back to the declared path with separators replaced.
fn file_name(resolved: &Path, declared: &str) -> String {
    resolved
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| declared.replace(['/', '\\'], "_"))
}
