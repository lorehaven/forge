use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Root config  (one per scope: ~/.config/warehouse/config.toml or .warehouse/config.toml)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RootConfig {
    #[serde(default)]
    pub docker: RootDockerConfig,
    #[serde(default)]
    pub crates: RootCratesConfig,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RootDockerConfig {
    pub current_registry: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RootCratesConfig {
    pub current_registry: Option<String>,
}

pub fn merge_root_config(global: RootConfig, local: RootConfig) -> RootConfig {
    RootConfig {
        docker: RootDockerConfig {
            current_registry: local
                .docker
                .current_registry
                .or(global.docker.current_registry),
        },
        crates: RootCratesConfig {
            current_registry: local
                .crates
                .current_registry
                .or(global.crates.current_registry),
        },
    }
}

// ---------------------------------------------------------------------------
// Per-registry config  (.warehouse/registries/<name>.toml)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct RegistryConfig {
    #[serde(default)]
    pub docker: RegistryDockerConfig,
    #[serde(default)]
    pub crates: RegistryCratesConfig,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct RegistryDockerConfig {
    pub url: String,
    #[serde(default = "default_docker_path")]
    pub path: String,
    pub service: Option<String>,
    #[serde(default)]
    pub insecure_tls: bool,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Crates registry config stored in a registry TOML file.
///
/// ```toml
/// [crates]
/// url = "https://registry.example.com"
/// token = "secret"
/// insecure_tls = false
/// ```
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct RegistryCratesConfig {
    /// Base URL of the registry, e.g. `https://registry.example.com`.
    /// The sparse index is expected at `<url>/index/` and the API at `<url>/api/v1/`.
    pub url: String,
    /// Bearer token used for authenticated operations (publish, yank, owners).
    pub token: Option<String>,
    /// Skip TLS certificate verification.
    #[serde(default)]
    pub insecure_tls: bool,
}

// ---------------------------------------------------------------------------
// Docker helpers
// ---------------------------------------------------------------------------

pub fn default_docker_path() -> String {
    "/v2".to_string()
}

pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/v2".to_string();
    }

    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{without_trailing}")
    }
}

pub fn derive_service_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let no_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host = no_scheme.split('/').next().unwrap_or_default().trim();

    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

pub fn api_url(reg: &RegistryConfig, endpoint: &str) -> Result<String> {
    let base = reg.docker.url.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("registry URL is empty");
    }

    let path = normalize_path(&reg.docker.path);
    let endpoint = endpoint.trim_start_matches('/');
    Ok(format!("{base}{path}/{endpoint}"))
}

// ---------------------------------------------------------------------------
// Crates helpers
// ---------------------------------------------------------------------------

/// Builds a full URL for a crates API endpoint.
/// `endpoint` should start with `/`, e.g. `/api/v1/crates?q=foo`.
pub fn crates_api_url(reg: &RegistryCratesConfig, endpoint: &str) -> Result<String> {
    let base = reg.url.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("crates registry URL is empty");
    }
    let endpoint = endpoint.trim_start_matches('/');
    Ok(format!("{base}/{endpoint}"))
}

/// Builds the sparse index URL for a given crate name.
/// Follows the crates.io prefix convention.
pub fn crates_index_url(reg: &RegistryCratesConfig, crate_name: &str) -> Result<String> {
    let base = reg.url.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("crates registry URL is empty");
    }
    let prefix = index_prefix(crate_name);
    Ok(format!("{base}/index/{prefix}/{crate_name}"))
}

/// Computes the sparse index directory prefix for a crate name,
/// matching the crates.io convention used by the server.
pub fn index_prefix(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.len() {
        0 => String::new(),
        1 => "1".to_string(),
        2 => "2".to_string(),
        3 => format!("3/{}", &lower[..1]),
        _ => format!("{}/{}", &lower[..2], &lower[2..4]),
    }
}

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

pub fn validate_registry_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("registry name cannot be empty");
    }
    if name.contains('/') || name.contains('\\') {
        bail!("registry name cannot contain path separators");
    }
    if name == "." || name == ".." {
        bail!("invalid registry name");
    }

    Ok(())
}
