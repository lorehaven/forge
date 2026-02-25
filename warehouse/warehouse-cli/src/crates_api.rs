use crate::domain::{RegistryCratesConfig, crates_api_url, crates_index_url};
use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SearchCrate {
    pub name: String,
    pub max_version: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    crates: Vec<SearchCrate>,
    meta: SearchMeta,
}

#[derive(Debug, Deserialize)]
struct SearchMeta {
    total: usize,
}

/// One line from the sparse index file — one entry per published version.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IndexRecord {
    pub name: String,
    pub vers: String,
    pub cksum: String,
    pub yanked: bool,
    #[serde(default)]
    pub rust_version: Option<String>,
    pub deps: Vec<IndexDep>,
    pub features: std::collections::HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IndexDep {
    pub name: String,
    pub req: String,
    pub optional: bool,
    pub kind: String,
    #[serde(default)]
    pub package: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OkResponse {
    ok: bool,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct CratesApi {
    client: reqwest::Client,
}

impl CratesApi {
    pub fn new(config: &RegistryCratesConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(config.insecure_tls)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { client })
    }

    /// `GET /api/v1/crates?q=<query>&per_page=<n>`
    pub async fn search(
        &self,
        config: &RegistryCratesConfig,
        query: &str,
        per_page: usize,
    ) -> Result<(Vec<SearchCrate>, usize)> {
        let endpoint = format!(
            "/api/v1/crates?q={}&per_page={}",
            urlencoding_simple(query),
            per_page
        );
        let url = crates_api_url(config, &endpoint)?;
        let resp = self.get_authed(config, &url).await?;
        ensure_success(&resp, &url)?;
        let body: SearchResponse = resp
            .json()
            .await
            .context("failed to decode search response")?;
        Ok((body.crates, body.meta.total))
    }

    /// Fetches the sparse index file for a crate and returns all version records.
    pub async fn versions(
        &self,
        config: &RegistryCratesConfig,
        crate_name: &str,
    ) -> Result<Vec<IndexRecord>> {
        let url = crates_index_url(config, crate_name)?;
        let resp = self.get_authed(config, &url).await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("crate '{}' not found", crate_name);
        }
        ensure_success(&resp, &url)?;

        let body = resp
            .text()
            .await
            .context("failed to read index response body")?;

        let records: Vec<IndexRecord> = body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();

        if records.is_empty() {
            bail!("no version records found for crate '{}'", crate_name);
        }

        Ok(records)
    }

    /// `DELETE /api/v1/crates/<name>/<version>/yank`
    pub async fn yank(
        &self,
        config: &RegistryCratesConfig,
        crate_name: &str,
        version: &str,
    ) -> Result<()> {
        let endpoint = format!("/api/v1/crates/{crate_name}/{version}/yank");
        let url = crates_api_url(config, &endpoint)?;

        let req = self
            .client
            .delete(&url)
            .headers(auth_headers(config)?)
            .build()
            .context("failed to build yank request")?;

        let resp = self
            .client
            .execute(req)
            .await
            .with_context(|| format!("request failed: DELETE {url}"))?;

        ensure_success(&resp, &url)?;
        let body: OkResponse = resp
            .json()
            .await
            .context("failed to decode yank response")?;

        if !body.ok {
            bail!("yank returned ok=false");
        }
        Ok(())
    }

    /// `PUT /api/v1/crates/<name>/<version>/unyank`
    pub async fn unyank(
        &self,
        config: &RegistryCratesConfig,
        crate_name: &str,
        version: &str,
    ) -> Result<()> {
        let endpoint = format!("/api/v1/crates/{crate_name}/{version}/unyank");
        let url = crates_api_url(config, &endpoint)?;

        let req = self
            .client
            .put(&url)
            .headers(auth_headers(config)?)
            .header("Content-Length", "0")
            .build()
            .context("failed to build unyank request")?;

        let resp = self
            .client
            .execute(req)
            .await
            .with_context(|| format!("request failed: PUT {url}"))?;

        ensure_success(&resp, &url)?;
        let body: OkResponse = resp
            .json()
            .await
            .context("failed to decode unyank response")?;

        if !body.ok {
            bail!("unyank returned ok=false");
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    async fn get_authed(
        &self,
        config: &RegistryCratesConfig,
        url: &str,
    ) -> Result<reqwest::Response> {
        self.client
            .get(url)
            .headers(auth_headers(config)?)
            .send()
            .await
            .with_context(|| format!("request failed: GET {url}"))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn auth_headers(config: &RegistryCratesConfig) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(token) = &config.token {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("token contains invalid header characters")?;
        headers.insert(AUTHORIZATION, value);
    }
    Ok(headers)
}

fn ensure_success(resp: &reqwest::Response, url: &str) -> Result<()> {
    if resp.status().is_success() {
        return Ok(());
    }
    bail!("request failed: {} {}", resp.status(), url)
}

/// Minimal percent-encoding for query string values (encodes everything except
/// unreserved characters). We avoid pulling in an extra dependency for this.
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}
