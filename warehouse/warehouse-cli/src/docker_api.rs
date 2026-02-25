use crate::domain::{RegistryConfig, api_url, derive_service_from_url};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, WWW_AUTHENTICATE};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    repositories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    name: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Debug)]
struct BearerChallenge {
    realm: String,
    service: Option<String>,
}

pub struct DockerApi {
    client: reqwest::Client,
}

impl DockerApi {
    pub fn new(registry: &RegistryConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(registry.docker.insecure_tls)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { client })
    }

    pub async fn catalog(&self, registry: &RegistryConfig, n: usize) -> Result<Vec<String>> {
        let endpoint = format!("/_catalog?n={n}");
        let response: CatalogResponse = self.fetch_json(registry, &endpoint, None).await?;
        Ok(response.repositories)
    }

    pub async fn tags(
        &self,
        registry: &RegistryConfig,
        repository: &str,
        n: usize,
    ) -> Result<(String, Vec<String>)> {
        let endpoint = format!("/{repository}/tags/list?n={n}");
        let scope = format!("repository:{repository}:pull");
        let response: TagsResponse = self
            .fetch_json(registry, &endpoint, Some(scope.as_str()))
            .await?;
        Ok((response.name, response.tags))
    }

    async fn fetch_json<T: for<'de> Deserialize<'de>>(
        &self,
        registry: &RegistryConfig,
        endpoint: &str,
        scope: Option<&str>,
    ) -> Result<T> {
        let url = api_url(registry, endpoint)?;
        let initial = self.get_with_https_fallback(&url).await?;

        if initial.status().is_success() {
            return parse_json_response(initial).await;
        }

        if initial.status() != reqwest::StatusCode::UNAUTHORIZED {
            let status = initial.status();
            let body = initial.text().await.unwrap_or_default();
            bail!("request failed: {status} {body}");
        }

        let challenge = parse_bearer_challenge(initial.headers())
            .ok_or_else(|| anyhow!("registry returned 401 without a Bearer challenge"))?;

        let username = registry
            .docker
            .username
            .as_deref()
            .ok_or_else(|| anyhow!("missing username; run `warehouse docker login`"))?;
        let password = registry
            .docker
            .password
            .as_deref()
            .ok_or_else(|| anyhow!("missing password; run `warehouse docker login`"))?;

        let service = challenge
            .service
            .or_else(|| registry.docker.service.clone())
            .or_else(|| derive_service_from_url(&registry.docker.url))
            .ok_or_else(|| anyhow!("unable to determine token service"))?;

        let token = self
            .fetch_bearer_token(&challenge.realm, &service, scope, username, password)
            .await
            .context("failed to fetch auth token")?;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).context("invalid auth header")?,
        );

        let retry = self
            .get_with_https_fallback_with_headers(&url, headers)
            .await?;
        if retry.status().is_success() {
            return parse_json_response(retry).await;
        }

        let status = retry.status();
        let body = retry.text().await.unwrap_or_default();
        bail!("request failed after auth: {status} {body}")
    }

    async fn fetch_bearer_token(
        &self,
        realm: &str,
        service: &str,
        scope: Option<&str>,
        username: &str,
        password: &str,
    ) -> Result<String> {
        let mut token_url = format!("{realm}?service={service}");
        if let Some(scope) = scope {
            token_url.push_str("&scope=");
            token_url.push_str(scope);
        }

        let response = self
            .get_with_https_fallback_basic_auth(&token_url, username, password)
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("token request failed: {status} {body}");
        }

        let token = response
            .json::<TokenResponse>()
            .await
            .context("invalid token response body")?;
        Ok(token.token)
    }

    async fn get_with_https_fallback(&self, url: &str) -> Result<reqwest::Response> {
        match self.client.get(url).send().await {
            Ok(response) => Ok(response),
            Err(err) => {
                if should_try_https_fallback(url, &err)
                    && let Some(https_url) = to_https_url(url)
                {
                    return self
                        .client
                        .get(https_url)
                        .send()
                        .await
                        .context("http request failed; https fallback also failed");
                }

                Err(err).with_context(|| format!("error sending request for url ({url})"))
            }
        }
    }

    async fn get_with_https_fallback_with_headers(
        &self,
        url: &str,
        headers: HeaderMap,
    ) -> Result<reqwest::Response> {
        match self.client.get(url).headers(headers.clone()).send().await {
            Ok(response) => Ok(response),
            Err(err) => {
                if should_try_https_fallback(url, &err)
                    && let Some(https_url) = to_https_url(url)
                {
                    return self
                        .client
                        .get(https_url)
                        .headers(headers)
                        .send()
                        .await
                        .context("http request failed; https fallback also failed");
                }

                Err(err).with_context(|| format!("error sending request for url ({url})"))
            }
        }
    }

    async fn get_with_https_fallback_basic_auth(
        &self,
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<reqwest::Response> {
        match self
            .client
            .get(url)
            .basic_auth(username, Some(password))
            .send()
            .await
        {
            Ok(response) => Ok(response),
            Err(err) => {
                if should_try_https_fallback(url, &err)
                    && let Some(https_url) = to_https_url(url)
                {
                    return self
                        .client
                        .get(https_url)
                        .basic_auth(username, Some(password))
                        .send()
                        .await
                        .context("http token request failed; https fallback also failed");
                }

                Err(err).with_context(|| format!("error sending request for url ({url})"))
            }
        }
    }
}

fn parse_bearer_challenge(headers: &HeaderMap) -> Option<BearerChallenge> {
    let raw = headers.get(WWW_AUTHENTICATE)?.to_str().ok()?.trim();
    let raw = if let Some(rest) = raw.strip_prefix("Bearer ") {
        rest
    } else if let Some(rest) = raw.strip_prefix("bearer ") {
        rest
    } else {
        return None;
    };

    let mut realm = None;
    let mut service = None;

    for part in raw.split(',') {
        let (key, value) = part.trim().split_once('=')?;
        let unquoted = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "realm" => realm = Some(unquoted),
            "service" => service = Some(unquoted),
            _ => {}
        }
    }

    Some(BearerChallenge {
        realm: realm?,
        service,
    })
}

fn should_try_https_fallback(url: &str, err: &reqwest::Error) -> bool {
    url.starts_with("http://") && err.to_string().contains("invalid HTTP version parsed")
}

fn to_https_url(url: &str) -> Option<String> {
    url.strip_prefix("http://")
        .map(|rest| format!("https://{rest}"))
}

async fn parse_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T> {
    let status = response.status();
    response
        .json::<T>()
        .await
        .with_context(|| format!("failed to decode json response from {status}"))
}
