use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RootConfig {
    #[serde(default)]
    pub docker: RootDockerConfig,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RootDockerConfig {
    pub current_registry: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct RegistryConfig {
    #[serde(default)]
    pub docker: RegistryDockerConfig,
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

pub fn merge_root_config(global: RootConfig, local: RootConfig) -> RootConfig {
    RootConfig {
        docker: RootDockerConfig {
            current_registry: local
                .docker
                .current_registry
                .or(global.docker.current_registry),
        },
    }
}

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
