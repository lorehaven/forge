//! Keeping what a job produced: uploaded to warehouse's file storage before the checkout is deleted.
//! With no warehouse configured, nothing is recorded rather than a row promising a file that isn't there.

use crate::domain::Artifact;
use crate::workspace::Workspace;
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use std::path::Path;

/// Above this, a build wants a registry, not a file store held open by a worker.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// What [`WarehouseStore::download`] will read: it serves summaries for a web page, not builds.
const MAX_DOWNLOAD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, quench_core::Error)]
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
    Unreachable(#[from] quench_client::transport::Error),

    #[error("`{uri}` is not in this warehouse")]
    Foreign { uri: String },

    #[error("`{uri}` is over the {MAX_DOWNLOAD_BYTES}-byte download limit")]
    TooBig { uri: String },
}

/// Where artifacts go.
#[derive(Debug, Clone)]
pub struct WarehouseStore {
    base_url: String,
    storage: String,
    credentials: Option<(String, String)>,
    http: quench_client::transport::Client,
}

impl WarehouseStore {
    /// `None` when this deployment keeps no artifacts.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let base_url = quench_core::env::get_or("WAREHOUSE_URL", "");
        let base_url = base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return None;
        }

        let username = quench_core::env::get_or("WAREHOUSE_TECH_USERNAME", "");
        let password = quench_core::env::get_or("WAREHOUSE_TECH_PASSWORD", "");
        let credentials =
            (!username.trim().is_empty()).then(|| (username.trim().to_string(), password));

        Some(Self {
            base_url: base_url.to_string(),
            storage: quench_core::env::get_or("CONVEYOR_ARTIFACT_STORAGE", "artifacts"),
            credentials,
            http: quench_client::transport::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                // Same allowance sage makes for switchboard's internal certs.
                .danger_accept_invalid_certs(!quench_core::env::is_or("WAREHOUSE_TLS_VERIFY", true))
                .build()
                .unwrap_or_default(),
        })
    }

    /// A store at `base_url` keeping files in `storage`, with basic-auth `credentials` if it wants them.
    #[must_use]
    pub fn new(base_url: &str, storage: &str, credentials: Option<(String, String)>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            storage: storage.to_string(),
            credentials,
            http: quench_client::transport::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    /// The bytes of something [`upload`](Self::upload) returned the address of.
    ///
    /// # Errors
    ///
    /// [`ArtifactError::Foreign`] for an address outside this warehouse, [`ArtifactError::Rejected`] for a
    /// non-success answer, [`ArtifactError::TooBig`] over 16 MiB, [`ArtifactError::Unreachable`] otherwise.
    pub async fn download(&self, uri: &str) -> Result<Vec<u8>, ArtifactError> {
        if !uri.starts_with(&self.base_url) {
            return Err(ArtifactError::Foreign {
                uri: uri.to_string(),
            });
        }
        let mut request = self.http.get(uri);
        if let Some((username, password)) = &self.credentials {
            request = request.basic_auth(username, Some(password));
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(ArtifactError::Rejected {
                path: uri.to_string(),
                status: response.status().as_u16(),
            });
        }
        let bytes = response.bytes().await?;
        if bytes.len() > MAX_DOWNLOAD_BYTES {
            return Err(ArtifactError::TooBig {
                uri: uri.to_string(),
            });
        }
        Ok(bytes.to_vec())
    }

    /// Scoped by run id so two runs of the same commit don't overwrite each other.
    fn remote_path(run_id: &str, name: &str) -> String {
        format!("conveyor/{run_id}/{name}")
    }

    async fn upload(&self, remote_path: &str, bytes: Vec<u8>) -> Result<String, ArtifactError> {
        let url = format!(
            "{}/api/v1/files/{}/file?path={}",
            self.base_url,
            self.storage,
            quench_serde::urlencoded::percent_encode(remote_path)
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

/// Where a `quench-test` run leaves its results, under the checkout's `target` directory, per build profile.
const TEST_REPORT_DIRS: [&str; 2] = ["target/debug/quench-test", "target/release/quench-test"];

/// The files of those directories worth keeping: the merged results, and the page that shows them.
const TEST_REPORT_FILES: [&str; 2] = ["summary.json", "index.html"];

/// What a job's runner must hand back to conveyor's workspace for [`collect`] and [`collect_test_reports`] to
/// find it: the declared artifacts, and the test reports a `quench-test` run leaves.
#[must_use]
pub fn exports(declared: &[String]) -> Vec<String> {
    let mut paths = declared.to_vec();
    for directory in TEST_REPORT_DIRS {
        for file in TEST_REPORT_FILES {
            paths.push(format!("{directory}/{file}"));
        }
    }
    paths
}

/// Keeps the results a `quench-test` run left in the checkout, whatever the job's outcome.
///
/// They matter most when the job failed. Nothing is declared for this: a job that ran quench-test tests has the
/// files, one that did not has nothing to keep. Scoped by job as well as run, since a run can have several test
/// jobs.
pub async fn collect_test_reports(
    store: Option<&WarehouseStore>,
    workspace: &Workspace,
    run_id: &str,
    job_id: &str,
) -> (Vec<Collected>, Vec<ArtifactError>) {
    let mut kept = Vec::new();
    let mut problems = Vec::new();
    let Some(store) = store else {
        return (kept, problems);
    };

    for directory in TEST_REPORT_DIRS {
        for file in TEST_REPORT_FILES {
            let declared = format!("{directory}/{file}");
            let Some(resolved) = workspace.resolve(&declared) else {
                continue;
            };
            let Ok(bytes) = tokio::fs::read(&resolved).await else {
                continue;
            };
            let digest = format!(
                "sha256:{}",
                quench_core::hex::encode(quench_crypto::hash::sha256(&bytes))
            );
            let remote = format!("conveyor/{run_id}/{job_id}/{file}");
            match store.upload(&remote, bytes).await {
                Ok(uri) => kept.push(Collected {
                    artifact: Artifact {
                        id: Uuid::new_v4().to_string(),
                        run_id: run_id.to_string(),
                        job_id: job_id.to_string(),
                        kind: "file".to_string(),
                        name: file.to_string(),
                        version: None,
                        uri,
                        digest: Some(digest),
                        created_at: Time::now(),
                    },
                }),
                Err(error) => problems.push(error),
            }
        }
    }

    (kept, problems)
}

/// One artifact, ready to record.
#[derive(Debug)]
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

    let digest = format!(
        "sha256:{}",
        quench_core::hex::encode(quench_crypto::hash::sha256(&bytes))
    );
    let size = bytes.len();
    let name = file_name(&resolved, declared);
    let remote = WarehouseStore::remote_path(run_id, &name);
    let uri = store.upload(&remote, bytes).await?;

    quench_log::info!("kept {declared} ({size} bytes) as {remote}");

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
            created_at: Time::now(),
        },
    }))
}

/// The file's own name, falling back to the declared path with separators replaced.
fn file_name(resolved: &Path, declared: &str) -> String {
    resolved
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| declared.replace(['/', '\\'], "_"), str::to_string)
}
