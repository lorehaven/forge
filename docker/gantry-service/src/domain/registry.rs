//! What is published: the packages (overlays) Warehouse holds and their versions. The source of truth for
//! "what could be installed"; what is *running* comes from the cluster, never from here.

use crate::domain::settings::Settings;
use async_trait::async_trait;
use quench_client::{ClientCredentialsClient, HttpClient};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("the package registry: {0}")]
    Unavailable(String),
}

/// One published version of one package, as Warehouse describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVersion {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The namespace the package deploys into by default.
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub uploaded_by: String,
    /// The package's own `rivet.toml`, parsed.
    #[serde(default)]
    pub manifest: serde_json::Value,
}

impl PackageVersion {
    /// The `[[deployment]]`s the package declares, or `None` if the manifest is not readable. A package
    /// that declares none is one deployment named after it; that is the planner's concern, not this.
    pub fn deployments(&self) -> Vec<rivet_package::Deployment> {
        self.manifest
            .get("deployment")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }
}

#[async_trait]
pub trait Registry: Send + Sync {
    /// The newest non-yanked version of every package.
    async fn catalog(&self) -> Result<Vec<PackageVersion>, RegistryError>;
    /// Every version of one package, yanked ones included.
    async fn versions(&self, name: &str) -> Result<Vec<PackageVersion>, RegistryError>;
}

/// Semver precedence, then (for versions that differ only in build metadata, which semver ignores) the
/// build metadata as text - the same order Warehouse itself uses for "newest".
pub fn compare(a: &str, b: &str) -> Ordering {
    match (semver::Version::parse(a), semver::Version::parse(b)) {
        (Ok(x), Ok(y)) => x.cmp_precedence(&y).then_with(|| x.build.cmp(&y.build)),
        (Ok(_), Err(_)) => Ordering::Greater,
        (Err(_), Ok(_)) => Ordering::Less,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

/// The newest non-yanked version in a list.
pub fn newest(versions: &[PackageVersion]) -> Option<&PackageVersion> {
    versions
        .iter()
        .filter(|v| !v.yanked)
        .max_by(|a, b| compare(&a.version, &b.version))
}

pub fn build(settings: &Settings) -> Arc<dyn Registry> {
    if let Some(dir) = &settings.packages_dir {
        return Arc::new(DirRegistry { dir: dir.clone() });
    }
    if let Some(url) = &settings.warehouse_url {
        match WarehouseRegistry::connect(settings, url) {
            Ok(registry) => return Arc::new(registry),
            Err(reason) => tracing::error!("warehouse: {reason}"),
        }
    }
    tracing::warn!(
        "no package source: set WAREHOUSE_URL (with its client credentials) or GANTRY_PACKAGES_DIR; \
         the target list will be empty"
    );
    Arc::new(MemoryRegistry::default())
}

// ---------------------------------------------------------------- warehouse

pub struct WarehouseRegistry {
    client: Client,
}

enum Client {
    /// Gantry's own machine identity, exchanged for a token at Gatehouse.
    Authenticated(ClientCredentialsClient),
    /// No credentials configured: only for a Warehouse running with its auth switched off (local).
    Anonymous(HttpClient),
}

impl Client {
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        match self {
            Self::Authenticated(client) => client.get(path).await,
            Self::Anonymous(client) => client.get(path).await,
        }
    }
}

impl WarehouseRegistry {
    pub fn connect(settings: &Settings, url: &str) -> Result<Self, String> {
        let base = format!("{}/api/v1/rivets", url.trim_end_matches('/'));

        let (Some(gatehouse), Some(secret)) = (
            settings.gatehouse_url.as_deref(),
            settings.warehouse_client_secret.as_deref(),
        ) else {
            tracing::warn!(
                "warehouse: no client credentials (GATEHOUSE_URL, WAREHOUSE_CLIENT_SECRET); \
                 asking without a token, which only a Warehouse with auth off will answer"
            );
            let client = HttpClient::builder(&base)
                .tls_verify(settings.tls_verify)
                .build()
                .map_err(|e| e.to_string())?;
            return Ok(Self {
                client: Client::Anonymous(client),
            });
        };

        let client = ClientCredentialsClient::builder(&base)
            .token_url(&format!("{}/api/v1/token", gatehouse.trim_end_matches('/')))
            .client_id(&settings.warehouse_client_id)
            .client_secret(secret)
            .tls_verify(settings.tls_verify)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client: Client::Authenticated(client),
        })
    }
}

#[async_trait]
impl Registry for WarehouseRegistry {
    async fn catalog(&self) -> Result<Vec<PackageVersion>, RegistryError> {
        self.client
            .get("")
            .await
            .map_err(|e| RegistryError::Unavailable(e.to_string()))
    }

    async fn versions(&self, name: &str) -> Result<Vec<PackageVersion>, RegistryError> {
        self.client
            .get(&format!("/{name}"))
            .await
            .map_err(|e| RegistryError::Unavailable(e.to_string()))
    }
}

// ---------------------------------------------------------------- directory

/// Packages as `.rivet` files in a directory, for running without Warehouse: a developer's `riveter pack
/// --out` directory works as-is.
pub struct DirRegistry {
    pub dir: PathBuf,
}

impl DirRegistry {
    fn all(&self) -> Result<Vec<PackageVersion>, RegistryError> {
        let unavailable =
            |e: std::io::Error| RegistryError::Unavailable(format!("{}: {e}", self.dir.display()));
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(unavailable)? {
            let path = entry.map_err(unavailable)?.path();
            if path.extension().is_none_or(|ext| ext != "rivet") {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(unavailable)?;
            // A file that is not a valid package is skipped, loudly: one bad file must not hide the rest.
            let package = match rivet_package::Package::read(
                bytes.as_slice(),
                &rivet_package::Limits::default(),
            ) {
                Ok(package) => package,
                Err(error) => {
                    tracing::warn!("{}: not a valid package: {error}", path.display());
                    continue;
                }
            };
            found.push(PackageVersion {
                name: package.manifest.package.name.clone(),
                version: package.manifest.package.version.clone(),
                description: package.manifest.package.description.clone(),
                namespace: package.manifest.package.namespace.clone(),
                size_bytes: bytes.len() as u64,
                sha256: rivet_package::sha256_hex(&bytes),
                yanked: false,
                uploaded_by: "local".to_string(),
                manifest: serde_json::to_value(&package.manifest).unwrap_or_default(),
            });
        }
        Ok(found)
    }
}

#[async_trait]
impl Registry for DirRegistry {
    async fn catalog(&self) -> Result<Vec<PackageVersion>, RegistryError> {
        let all = self.all()?;
        let mut names: Vec<String> = all.iter().map(|p| p.name.clone()).collect();
        names.sort();
        names.dedup();
        Ok(names
            .into_iter()
            .filter_map(|name| {
                let versions: Vec<PackageVersion> =
                    all.iter().filter(|p| p.name == name).cloned().collect();
                newest(&versions).cloned()
            })
            .collect())
    }

    async fn versions(&self, name: &str) -> Result<Vec<PackageVersion>, RegistryError> {
        let mut versions: Vec<PackageVersion> =
            self.all()?.into_iter().filter(|p| p.name == name).collect();
        versions.sort_by(|a, b| compare(&b.version, &a.version));
        Ok(versions)
    }
}

// ---------------------------------------------------------------- memory

/// A registry a test fills in, and what a bare run has: empty.
#[derive(Default)]
pub struct MemoryRegistry {
    packages: Mutex<Vec<PackageVersion>>,
}

impl MemoryRegistry {
    pub fn publish(&self, name: &str, version: &str, namespace: Option<&str>) {
        self.publish_with(name, version, namespace, serde_json::Value::Null);
    }

    /// With a manifest, e.g. `{"deployment": [{"name": ..., "resources": [...]}]}`.
    pub fn publish_with(
        &self,
        name: &str,
        version: &str,
        namespace: Option<&str>,
        manifest: serde_json::Value,
    ) {
        self.packages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(PackageVersion {
                name: name.to_string(),
                version: version.to_string(),
                description: None,
                namespace: namespace.map(str::to_string),
                size_bytes: 0,
                sha256: String::new(),
                yanked: false,
                uploaded_by: "test".to_string(),
                manifest,
            });
    }

    pub fn yank(&self, name: &str, version: &str) {
        for package in self
            .packages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter_mut()
        {
            if package.name == name && package.version == version {
                package.yanked = true;
            }
        }
    }

    fn all(&self) -> Vec<PackageVersion> {
        self.packages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl Registry for MemoryRegistry {
    async fn catalog(&self) -> Result<Vec<PackageVersion>, RegistryError> {
        let all = self.all();
        let mut names: Vec<String> = all.iter().map(|p| p.name.clone()).collect();
        names.sort();
        names.dedup();
        Ok(names
            .into_iter()
            .filter_map(|name| {
                let versions: Vec<PackageVersion> =
                    all.iter().filter(|p| p.name == name).cloned().collect();
                newest(&versions).cloned()
            })
            .collect())
    }

    async fn versions(&self, name: &str) -> Result<Vec<PackageVersion>, RegistryError> {
        let mut versions: Vec<PackageVersion> =
            self.all().into_iter().filter(|p| p.name == name).collect();
        versions.sort_by(|a, b| compare(&b.version, &a.version));
        Ok(versions)
    }
}
