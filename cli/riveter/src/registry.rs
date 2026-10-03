//! The Warehouse rivet registry, as a client.

use anyhow::{Context as _, Result, bail, ensure};
use reqwest::blocking::Client;
use serde::Deserialize;

/// Where the registry is, and who to be.
#[derive(Debug)]
pub struct Registry {
    base: String,
    token: Option<String>,
    client: Client,
}

/// One version as the registry describes it.
#[derive(Debug, Clone, Deserialize)]
pub struct RemotePackage {
    /// Package name.
    pub name: String,
    /// Semver, build metadata included.
    pub version: String,
    /// One line, if the package has one.
    pub description: Option<String>,
    /// Default namespace.
    pub namespace: Option<String>,
    /// Compressed size.
    pub size_bytes: u64,
    /// Hex SHA-256 of the archive.
    pub sha256: String,
    /// Whether it has been withdrawn from `latest`.
    pub yanked: bool,
    /// Who published it.
    pub uploaded_by: String,
}

/// A downloaded, digest-verified archive.
#[derive(Debug)]
pub struct Fetched {
    /// The concrete version, even if `latest` was asked for.
    pub version: String,
    /// The archive.
    pub bytes: Vec<u8>,
}

/// Reads a variable, treating an empty one as unset.
fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Registry {
    /// A registry at `base` (Warehouse's URL including its base path, e.g.
    /// `https://host/warehouse`), authenticating with `token` if given.
    pub fn new(base: &str, token: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("failed to build an HTTP client")?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            token,
            client,
        })
    }

    /// Configured from the environment (and a `.env` in the working directory):
    ///
    /// - `RIVETER_WAREHOUSE_URL` - Warehouse including its base path.
    /// - `RIVETER_WAREHOUSE_TOKEN` - a bearer token, **or**
    /// - `RIVETER_GATEHOUSE_URL` with `RIVETER_CLIENT_ID` and `RIVETER_CLIENT_SECRET`,
    ///   exchanged for a token with the client-credentials grant.
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let base = var("RIVETER_WAREHOUSE_URL").context(
            "RIVETER_WAREHOUSE_URL is not set - point it at Warehouse, including its base path \
             (e.g. https://host/warehouse)",
        )?;

        let token = if let Some(token) = var("RIVETER_WAREHOUSE_TOKEN") {
            token
        } else {
            let (gatehouse, id, secret) = (
                var("RIVETER_GATEHOUSE_URL"),
                var("RIVETER_CLIENT_ID"),
                var("RIVETER_CLIENT_SECRET"),
            );
            let (Some(gatehouse), Some(id), Some(secret)) = (gatehouse, id, secret) else {
                bail!(
                    "no credentials: set RIVETER_WAREHOUSE_TOKEN, or RIVETER_GATEHOUSE_URL \
                     with RIVETER_CLIENT_ID and RIVETER_CLIENT_SECRET"
                );
            };
            client_credentials_token(&gatehouse, &id, &secret)?
        };

        Self::new(&base, Some(token))
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1/rivets{path}", self.base)
    }

    fn authorize(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    fn send(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response> {
        let response = self
            .authorize(request)
            .send()
            .context("could not reach Warehouse")?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let detail = response
            .json::<serde_json::Value>()
            .ok()
            .and_then(|body| {
                body.get("error")
                    .and_then(|e| e.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();
        match status.as_u16() {
            401 => bail!("Warehouse rejected the credentials (HTTP 401)"),
            403 => bail!(
                "Warehouse denied this action (HTTP 403): publishing needs the `warehouse:write` grant"
            ),
            404 => bail!("not found{}", suffix(&detail)),
            409 => bail!("already published{}", suffix(&detail)),
            code => bail!("Warehouse answered HTTP {code}{}", suffix(&detail)),
        }
    }

    /// Publishes `bytes` as `name`/`version`.
    pub fn publish(&self, name: &str, version: &str, bytes: Vec<u8>) -> Result<RemotePackage> {
        let request = self
            .client
            .put(self.url(&format!("/{name}/{version}")))
            .header("content-type", "application/zstd")
            .body(bytes);
        self.send(request)?
            .json()
            .context("unexpected response from Warehouse")
    }

    /// One version's record; `version` may be `latest`.
    pub fn describe(&self, name: &str, version: &str) -> Result<RemotePackage> {
        self.send(self.client.get(self.url(&format!("/{name}/{version}"))))?
            .json()
            .context("unexpected response from Warehouse")
    }

    /// Downloads `name`@`version` (`latest` allowed) and checks its digest
    /// against the record Warehouse holds for it.
    pub fn fetch(&self, name: &str, version: &str) -> Result<Fetched> {
        let record = self.describe(name, version)?;
        let response = self.send(
            self.client
                .get(self.url(&format!("/{name}/{}/download", record.version))),
        )?;
        let bytes = response
            .bytes()
            .context("download was interrupted")?
            .to_vec();

        let actual = rivet_package::sha256_hex(&bytes);
        ensure!(
            actual == record.sha256,
            "{name} {} downloaded with digest {actual}, but Warehouse records {}",
            record.version,
            record.sha256
        );
        Ok(Fetched {
            version: record.version,
            bytes,
        })
    }

    /// The newest published version of `name`, or `None` if Warehouse has never heard of it.
    pub fn latest(&self, name: &str) -> Result<Option<Fetched>> {
        match self.fetch(name, "latest") {
            Ok(fetched) => Ok(Some(fetched)),
            Err(error) if error.to_string().starts_with("not found") => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Every version of one package, newest first.
    pub fn versions(&self, name: &str) -> Result<Vec<RemotePackage>> {
        self.send(self.client.get(self.url(&format!("/{name}"))))?
            .json()
            .context("unexpected response from Warehouse")
    }

    /// The newest non-yanked version of every package.
    pub fn catalog(&self) -> Result<Vec<RemotePackage>> {
        self.send(self.client.get(self.url("")))?
            .json()
            .context("unexpected response from Warehouse")
    }
}

fn suffix(detail: &str) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

/// Exchanges a client id and secret for an access token at Gatehouse.
fn client_credentials_token(gatehouse: &str, id: &str, secret: &str) -> Result<String> {
    let response = Client::new()
        .post(format!("{}/api/v1/token", gatehouse.trim_end_matches('/')))
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", id),
            ("client_secret", secret),
        ])
        .send()
        .context("could not reach Gatehouse")?;
    ensure!(
        response.status().is_success(),
        "Gatehouse refused the client credentials (HTTP {})",
        response.status()
    );
    let body: serde_json::Value = response
        .json()
        .context("unexpected response from Gatehouse")?;
    body.get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .context("Gatehouse's response had no access_token")
}
