use crate::domain::RegistryConfig;
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CratesGcReport {
    /// Number of `.crate` tarballs deleted (yanked or orphaned)
    pub deleted_crates: usize,
    /// Number of `.crate` tarballs kept
    pub kept_crates: usize,
    /// Number of index entries removed because their tarball was missing
    pub removed_index_entries: usize,
    /// Number of orphaned `owners.json` files deleted
    pub deleted_owner_files: usize,
    /// Number of empty directories removed
    pub removed_empty_dirs: usize,
}

#[derive(Debug, Deserialize)]
pub struct DockerGcReport {
    pub deleted: usize,
    pub kept: usize,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct AdminApi {
    client: reqwest::Client,
}

impl AdminApi {
    pub fn new(reg: &RegistryConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(reg.crates.insecure_tls || reg.docker.insecure_tls)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { client })
    }

    /// Call the crates garbage collection endpoint
    pub async fn run_crates_gc(
        &self,
        registry: &RegistryConfig,
        endpoint: &str,
    ) -> Result<CratesGcReport> {
        let url = format!("{}{}", registry.crates.url, endpoint);
        let mut headers = HeaderMap::new();

        if let Some(token) = &registry.crates.token {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .context("token contains invalid header characters")?;
            headers.insert(AUTHORIZATION, value);
        }

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .send()
            .await
            .with_context(|| format!("failed to send request to {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("request failed: {} {}", status, body);
        }

        let report: CratesGcReport = response
            .json()
            .await
            .context("failed to decode GC response")?;

        Ok(report)
    }

    /// Call the docker garbage collection endpoint
    pub async fn run_docker_gc(
        &self,
        registry: &RegistryConfig,
        endpoint: &str,
    ) -> Result<DockerGcReport> {
        let url = format!("{}{}", registry.docker.url, endpoint);
        let mut headers = HeaderMap::new();

        if let Some(token) = &registry.docker.username {
            let password = registry
                .docker
                .password
                .as_deref()
                .ok_or_else(|| anyhow!("missing password for docker auth"))?;
            let auth = format!("{}:{}", token, password);
            let encoded = STANDARD.encode(auth);
            let value = HeaderValue::from_str(&format!("Basic {}", encoded))
                .context("invalid auth header")?;
            headers.insert(AUTHORIZATION, value);
        }

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .send()
            .await
            .with_context(|| format!("failed to send request to {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("request failed: {} {}", status, body);
        }

        let report: DockerGcReport = response
            .json()
            .await
            .context("failed to decode GC response")?;

        Ok(report)
    }
}
