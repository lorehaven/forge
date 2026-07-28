//! Keeping what a job produced.
//!
//! A run's checkout is deleted when it finishes, so a build output that is only
//! recorded as a path is a record of something that no longer exists. Declared
//! artifacts are therefore uploaded to warehouse's file storage, and the row
//! conveyor keeps points at where they actually are.
//!
//! With no warehouse configured, nothing is recorded and the job says what it
//! produced and did not keep. A row promising an artifact conveyor cannot
//! produce is worse than no row.

use crate::domain::Artifact;
use crate::workspace::Workspace;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

/// The most a single artifact may be, before conveyor declines to move it.
///
/// A build that produces a two-gigabyte tarball wants a registry, not a file
/// store, and streaming it through this service would hold a worker and a
/// warehouse connection for as long as it took.
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
    /// Built from the environment, or `None` when this deployment keeps no
    /// artifacts.
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
                // The estate's internal certificates are its own; the same
                // allowance sage makes for switchboard.
                .danger_accept_invalid_certs(!envmnt::is_or("WAREHOUSE_TLS_VERIFY", true))
                .build()
                .unwrap_or_default(),
        })
    }

    /// Where a run's artifacts live, so two runs of the same commit do not
    /// overwrite each other.
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

        // The whole url, query included. Warehouse addresses a file by the
        // `path` parameter, so trimming it would give every artifact of every
        // run the same uri and identify none of them.
        Ok(url)
    }
}

/// One artifact, ready to record.
pub struct Collected {
    pub artifact: Artifact,
}

/// Uploads everything `job` declared and returns what was kept.
///
/// Errors are per artifact and do not fail the job: the build itself passed,
/// and losing a copy of its output is worth a warning rather than a red mark.
/// A path that escapes the checkout is the exception in spirit but not in
/// mechanism - it is refused, loudly, and the rest still go.
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
            // No store configured: nothing to keep, and the caller says so once
            // rather than once per path.
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
    // A pipeline can say `artifacts = ["../../etc/passwd"]`, and collecting it
    // would hand a repository author whatever the service account can read.
    // Checked before the file is opened, not after.
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

/// The name an artifact is stored under.
///
/// The file's own name, falling back to the declared path with separators
/// replaced - so `target/release/thing` and a directory of the same shape do
/// not collide inside one run.
fn file_name(resolved: &Path, declared: &str) -> String {
    resolved
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| declared.replace(['/', '\\'], "_"))
}
