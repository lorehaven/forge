use crate::domain::RegistryConfig;
use anyhow::{Context, Result, bail};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct FileStorageInfo {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Deserialize)]
struct StoragesResponse {
    storages: Vec<FileStorageInfo>,
}

#[derive(Debug, Deserialize)]
pub struct DirectoryEntry {
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListResponse {
    pub storage: String,
    pub path: String,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewResponse {
    pub storage: String,
    pub path: String,
    pub kind: String,
    pub content: String,
    pub truncated: bool,
}

pub struct FilesApi {
    client: reqwest::Client,
}

impl FilesApi {
    pub fn new(registry: &RegistryConfig) -> Result<Self> {
        let insecure = registry.docker.insecure_tls || registry.crates.insecure_tls;
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(insecure)
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self { client })
    }

    pub async fn storages(&self, registry: &RegistryConfig) -> Result<Vec<FileStorageInfo>> {
        let url = base_url(registry, "/api/v1/files/storages")?;
        let response = self.client.get(&url).send().await?;
        ensure_success(&response, &url)?;
        let body: StoragesResponse = response.json().await?;
        Ok(body.storages)
    }

    pub async fn list(
        &self,
        registry: &RegistryConfig,
        storage: &str,
        path: &str,
    ) -> Result<ListResponse> {
        let url = base_url(
            registry,
            &format!(
                "/api/v1/files/{storage}/entries?path={}",
                url_encode(path, true)
            ),
        )?;
        let response = self.client.get(&url).send().await?;
        ensure_success(&response, &url)?;
        Ok(response.json().await?)
    }

    pub async fn upload(
        &self,
        registry: &RegistryConfig,
        storage: &str,
        remote_path: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let url = base_url(
            registry,
            &format!(
                "/api/v1/files/{storage}/file?path={}",
                url_encode(remote_path, false)
            ),
        )?;
        let response = self
            .client
            .put(&url)
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(bytes)
            .send()
            .await?;
        ensure_success(&response, &url)
    }

    pub async fn preview(
        &self,
        registry: &RegistryConfig,
        storage: &str,
        path: &str,
    ) -> Result<PreviewResponse> {
        let url = base_url(
            registry,
            &format!(
                "/api/v1/files/{storage}/preview?path={}",
                url_encode(path, false)
            ),
        )?;
        let response = self.client.get(&url).send().await?;
        ensure_success(&response, &url)?;
        Ok(response.json().await?)
    }

    pub async fn mkdir(&self, registry: &RegistryConfig, storage: &str, path: &str) -> Result<()> {
        let url = base_url(
            registry,
            &format!(
                "/api/v1/files/{storage}/folder?path={}",
                url_encode(path, false)
            ),
        )?;
        let response = self.client.post(&url).send().await?;
        ensure_success(&response, &url)
    }

    pub async fn rmdir(&self, registry: &RegistryConfig, storage: &str, path: &str) -> Result<()> {
        let url = base_url(
            registry,
            &format!(
                "/api/v1/files/{storage}/folder?path={}",
                url_encode(path, false)
            ),
        )?;
        let response = self.client.delete(&url).send().await?;
        ensure_success(&response, &url)
    }

    pub async fn delete_file(
        &self,
        registry: &RegistryConfig,
        storage: &str,
        path: &str,
    ) -> Result<()> {
        let url = base_url(
            registry,
            &format!(
                "/api/v1/files/{storage}/file?path={}",
                url_encode(path, false)
            ),
        )?;
        let response = self.client.delete(&url).send().await?;
        ensure_success(&response, &url)
    }

    pub async fn bulk_delete(
        &self,
        registry: &RegistryConfig,
        storage: &str,
        paths: &[String],
    ) -> Result<()> {
        let url = base_url(registry, &format!("/api/v1/files/{storage}/bulk"))?;
        let response = self
            .client
            .delete(&url)
            .json(&serde_json::json!({ "paths": paths }))
            .send()
            .await?;
        ensure_success(&response, &url)
    }

    pub async fn download(
        &self,
        registry: &RegistryConfig,
        storage: &str,
        path: &str,
    ) -> Result<(Vec<u8>, Option<String>)> {
        let url = base_url(
            registry,
            &format!(
                "/api/v1/files/{storage}/download?path={}",
                url_encode(path, false)
            ),
        )?;
        let response = self.client.get(&url).send().await?;
        ensure_success(&response, &url)?;
        let filename = filename_from_content_disposition(
            response
                .headers()
                .get(CONTENT_DISPOSITION)
                .and_then(|value| value.to_str().ok()),
        );
        let data = response.bytes().await?.to_vec();
        Ok((data, filename))
    }

    pub async fn bulk_download(
        &self,
        registry: &RegistryConfig,
        storage: &str,
        paths: &[String],
    ) -> Result<Vec<u8>> {
        let url = base_url(registry, &format!("/api/v1/files/{storage}/bulk-download"))?;
        let response = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "paths": paths }))
            .send()
            .await?;
        ensure_success(&response, &url)?;
        Ok(response.bytes().await?.to_vec())
    }
}

fn filename_from_content_disposition(value: Option<&str>) -> Option<String> {
    let value = value?;
    for part in value.split(';') {
        let part = part.trim();
        if let Some(name) = part.strip_prefix("filename=") {
            return Some(name.trim_matches('"').to_string());
        }
    }
    None
}

fn ensure_success(response: &reqwest::Response, url: &str) -> Result<()> {
    if response.status().is_success() {
        return Ok(());
    }
    bail!("request failed: {} {}", response.status(), url)
}

fn base_url(registry: &RegistryConfig, endpoint: &str) -> Result<String> {
    let base = if !registry.docker.url.trim().is_empty() {
        registry.docker.url.trim().trim_end_matches('/')
    } else if !registry.crates.url.trim().is_empty() {
        registry.crates.url.trim().trim_end_matches('/')
    } else {
        bail!("registry URL is empty");
    };
    Ok(format!("{base}/{}", endpoint.trim_start_matches('/')))
}

fn url_encode(value: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b'/' if keep_slash => out.push('/'),
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

pub fn remote_path_for_upload(local_path: &str, remote_dir: Option<&str>) -> Result<String> {
    let filename = Path::new(local_path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow::anyhow!("cannot determine filename for {}", local_path))?;

    let path = if let Some(dir) = remote_dir {
        let dir = dir.trim().trim_matches('/');
        if dir.is_empty() {
            filename.to_string()
        } else {
            format!("{dir}/{filename}")
        }
    } else {
        filename.to_string()
    };

    Ok(path)
}
