use crate::domain::{RegistryConfig, service_url};
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

/// Where `/admin/...` lives.
///
/// Both GC endpoints are served by the warehouse service under its base path,
/// not by the docker registry root that serves `/v2` — the registry is mounted
/// outside the base-path scope. The crates URL is the one configured to point at
/// the service (a registry may carry the base path there rather than in
/// `base_path`), so it is what admin calls resolve against; a docker-only
/// registry falls back to the docker host.
pub fn admin_base_url(registry: &RegistryConfig) -> &str {
    if registry.crates.url.trim().is_empty() {
        &registry.docker.url
    } else {
        &registry.crates.url
    }
}

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
        let url = service_url(admin_base_url(registry), &registry.base_path, endpoint)?;
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
        let url = service_url(admin_base_url(registry), &registry.base_path, endpoint)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RegistryCratesConfig, RegistryDockerConfig};

    const CRATES_GC: &str = "/admin/crates/gc";
    const DOCKER_GC: &str = "/admin/docker/gc";

    fn registry(docker_url: &str, crates_url: &str, base_path: &str) -> RegistryConfig {
        RegistryConfig {
            base_path: base_path.to_string(),
            docker: RegistryDockerConfig {
                url: docker_url.to_string(),
                path: "/v2".to_string(),
                ..RegistryDockerConfig::default()
            },
            crates: RegistryCratesConfig {
                url: crates_url.to_string(),
                ..RegistryCratesConfig::default()
            },
            ..RegistryConfig::default()
        }
    }

    /// Both admin endpoints sit in the same server-side scope, so they have to
    /// resolve the same way. Resolving them differently is what made
    /// `admin gc --docker` 404 while `--crates` worked.
    #[test]
    fn both_gc_endpoints_resolve_against_the_same_base() {
        let reg = registry("https://example.net", "https://example.net/warehouse", "");

        let crates = service_url(admin_base_url(&reg), &reg.base_path, CRATES_GC).unwrap();
        let docker = service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap();

        assert_eq!(crates, "https://example.net/warehouse/admin/crates/gc");
        assert_eq!(docker, "https://example.net/warehouse/admin/docker/gc");
    }

    /// A registry may carry the service prefix in the crates URL instead of in
    /// `base_path` — the docker URL stays bare, because `/v2` is served outside
    /// the base-path scope.
    #[test]
    fn the_base_path_may_live_in_the_crates_url() {
        let reg = registry("https://example.net", "https://example.net/warehouse", "");

        assert_eq!(
            service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
            "https://example.net/warehouse/admin/docker/gc"
        );
    }

    /// ... or in `base_path`, with both URLs bare.
    #[test]
    fn the_base_path_may_live_in_the_base_path_field() {
        let reg = registry("https://example.net", "https://example.net", "/warehouse");

        assert_eq!(
            service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
            "https://example.net/warehouse/admin/docker/gc"
        );
    }

    #[test]
    fn a_service_at_the_root_needs_no_prefix() {
        let reg = registry(
            "https://registry.local:8443",
            "https://registry.local:8443",
            "",
        );

        assert_eq!(
            service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
            "https://registry.local:8443/admin/docker/gc"
        );
    }

    #[test]
    fn a_docker_only_registry_falls_back_to_the_docker_host() {
        let reg = registry("https://example.net", "", "/warehouse");

        assert_eq!(admin_base_url(&reg), "https://example.net");
        assert_eq!(
            service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
            "https://example.net/warehouse/admin/docker/gc"
        );
    }

    /// The docker registry itself is mounted at the server root, outside the
    /// base path — so admin resolution must not be reused for `/v2`.
    #[test]
    fn the_docker_registry_root_stays_unprefixed() {
        let reg = registry("https://example.net", "https://example.net/warehouse", "");

        assert_eq!(
            crate::domain::api_url(&reg, "/_catalog").unwrap(),
            "https://example.net/v2/_catalog"
        );
    }
}
