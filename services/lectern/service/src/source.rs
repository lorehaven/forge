//! Where the books come from: a Warehouse files storage laid out by `anvil docs release`.
//!
//! ```text
//! <book>/latest.json                       the manifest of the newest release, plus where its files are
//! <book>/<version>/manifest.json
//! <book>/<version>/<book>-<version>.tar.gz
//! ```

use quench_core::async_trait;
use quench_serde::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// A failed read from the source, as text for a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError(pub String);

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SourceError {}

/// Read access to the storage. Warehouse in service, a map in tests.
#[async_trait]
pub trait Source: Send + Sync {
    /// The names of the books: the directories at the storage root.
    async fn books(&self) -> Result<Vec<String>, SourceError>;

    /// A file's bytes; `None` when it is not there.
    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, SourceError>;
}

/// The token the service holds while it is good.
#[derive(Debug)]
struct Grant {
    token_url: String,
    client_id: String,
    client_secret: String,
    cached: Mutex<Option<(String, Instant)>>,
}

#[derive(Debug, Clone)]
pub struct WarehouseSource {
    base: String,
    storage: String,
    grant: Option<Arc<Grant>>,
    http: quench_client::transport::Client,
}

impl WarehouseSource {
    /// A source for `storage` at `warehouse_url`. With `gatehouse_url` and `client_secret` both given, requests
    /// carry a `client_credentials` token for `client_id`; without them they carry nothing.
    #[must_use]
    pub fn new(
        warehouse_url: &str,
        storage: &str,
        credentials: Option<(&str, &str, &str)>,
        tls_verify: bool,
    ) -> Self {
        Self {
            base: warehouse_url.trim_end_matches('/').to_string(),
            storage: storage.to_string(),
            grant: credentials.map(|(gatehouse_url, client_id, client_secret)| {
                Arc::new(Grant {
                    token_url: format!("{}/api/v1/token", gatehouse_url.trim_end_matches('/')),
                    client_id: client_id.to_string(),
                    client_secret: client_secret.to_string(),
                    cached: Mutex::new(None),
                })
            }),
            http: quench_client::transport::Client::builder()
                .timeout(Duration::from_secs(120))
                .danger_accept_invalid_certs(!tls_verify)
                .build()
                .unwrap_or_default(),
        }
    }

    fn files_url(&self) -> String {
        format!("{}/api/v1/files/{}", self.base, self.storage)
    }

    async fn get(&self, url: &str) -> Result<quench_client::transport::Response, SourceError> {
        let mut request = self.http.get(url);
        if let Some(grant) = &self.grant {
            request = request.bearer_auth(self.token(grant).await?);
        }
        request
            .send()
            .await
            .map_err(|e| SourceError(format!("{url}: {e}")))
    }

    /// The current token, or a fresh one when it is within 30 seconds of running out.
    async fn token(&self, grant: &Grant) -> Result<String, SourceError> {
        let fresh = grant
            .cached
            .lock()
            .await
            .as_ref()
            .filter(|(_, expires)| *expires > Instant::now())
            .map(|(token, _)| token.clone());
        if let Some(token) = fresh {
            return Ok(token);
        }

        let response = self
            .http
            .post(&grant.token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", grant.client_id.as_str()),
                ("client_secret", grant.client_secret.as_str()),
            ])
            .send()
            .await
            .map_err(|e| SourceError(format!("{}: {e}", grant.token_url)))?;
        if !response.status().is_success() {
            return Err(SourceError(format!(
                "{} answered {}",
                grant.token_url,
                response.status().as_u16()
            )));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|e| SourceError(format!("{}: {e}", grant.token_url)))?;
        let token = body
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| SourceError(format!("{} sent no access_token", grant.token_url)))?
            .to_string();
        let lifetime = body
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(60)
            .saturating_sub(30)
            .max(1);
        *grant.cached.lock().await = Some((
            token.clone(),
            Instant::now() + Duration::from_secs(lifetime),
        ));
        Ok(token)
    }
}

#[async_trait]
impl Source for WarehouseSource {
    async fn books(&self) -> Result<Vec<String>, SourceError> {
        let url = self.files_url();
        let response = self.get(&url).await?;
        if !response.status().is_success() {
            return Err(SourceError(format!(
                "{url} answered {}",
                response.status().as_u16()
            )));
        }
        let listing: Value = response
            .json()
            .await
            .map_err(|e| SourceError(format!("{url}: {e}")))?;
        let mut names: Vec<String> = listing
            .get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| entry.get("kind").and_then(Value::as_str) == Some("directory"))
                    .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                    .filter(|name| !name.starts_with('.'))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        Ok(names)
    }

    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, SourceError> {
        let url = format!(
            "{}/file?path={}",
            self.files_url(),
            quench_serde::urlencoded::percent_encode(path)
        );
        let response = self.get(&url).await?;
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(SourceError(format!(
                "{url} answered {}",
                response.status().as_u16()
            )));
        }
        response
            .bytes()
            .await
            .map(|bytes| Some(bytes.to_vec()))
            .map_err(|e| SourceError(format!("{url}: {e}")))
    }
}
