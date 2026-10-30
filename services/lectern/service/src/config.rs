//! What the service is told through its environment.

use std::time::Duration;

/// How often the storage is looked at for a newer release, unless `LECTERN_REFRESH_SECS` says otherwise.
const DEFAULT_REFRESH_SECS: u64 = 60;

#[derive(Debug, Clone)]
pub struct Config {
    /// Where Warehouse is (`WAREHOUSE_URL`, no trailing slash).
    pub warehouse_url: String,
    /// The token endpoint's host (`GATEHOUSE_URL`); with the secret below, the service's own identity.
    pub gatehouse_url: Option<String>,
    /// `WAREHOUSE_CLIENT_ID`, `lectern-warehouse` unless set.
    pub client_id: String,
    /// `WAREHOUSE_CLIENT_SECRET`. Without it the service asks without a token, which only a Warehouse with
    /// authentication off will answer.
    pub client_secret: Option<String>,
    /// The files storage the books live in (`LECTERN_STORAGE`, `docs`).
    pub storage: String,
    /// `LECTERN_REFRESH_SECS`.
    pub refresh: Duration,
    /// `BASE_PATH`, `/` or a path with a leading slash and none trailing.
    pub base_path: String,
    /// `WAREHOUSE_TLS_VERIFY`, on unless `false`.
    pub tls_verify: bool,
}

impl Config {
    /// # Errors
    ///
    /// When `WAREHOUSE_URL` is not set: there is nothing to serve without it.
    pub fn from_env() -> Result<Self, String> {
        let get = |name: &str| {
            let value = quench_core::env::get_or(name, "");
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        let warehouse_url = get("WAREHOUSE_URL").ok_or("WAREHOUSE_URL is not set")?;
        Ok(Self {
            warehouse_url: warehouse_url.trim_end_matches('/').to_string(),
            gatehouse_url: get("GATEHOUSE_URL").map(|url| url.trim_end_matches('/').to_string()),
            client_id: get("WAREHOUSE_CLIENT_ID")
                .unwrap_or_else(|| "lectern-warehouse".to_string()),
            client_secret: get("WAREHOUSE_CLIENT_SECRET"),
            storage: get("LECTERN_STORAGE").unwrap_or_else(|| "docs".to_string()),
            refresh: Duration::from_secs(
                get("LECTERN_REFRESH_SECS")
                    .and_then(|secs| secs.parse().ok())
                    .filter(|secs| *secs > 0)
                    .unwrap_or(DEFAULT_REFRESH_SECS),
            ),
            base_path: quench_starter::common::routes::normalize_base_path(
                &quench_core::env::get_or("BASE_PATH", "/"),
            ),
            tls_verify: quench_core::env::is_or("WAREHOUSE_TLS_VERIFY", true),
        })
    }
}
