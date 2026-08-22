//! Talking to conveyor.
//!
//! Credentials go to gatehouse, once, at startup - `POST /api/v1/auth/login`,
//! the same resource-owner login every service's browser flow ends up at.
//! Every call to conveyor after that carries the bearer token gatehouse
//! handed back, never the password itself.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Debug)]
pub struct Client {
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

impl Client {
    pub async fn new(
        url: Option<String>,
        username: Option<String>,
        password: Option<String>,
        gatehouse_url: Option<String>,
        insecure: bool,
    ) -> Result<Self> {
        // Lowest-priority source: a flag beats an env var beats this. See
        // `crate::config` for why a malformed file is an error but a missing
        // one is not.
        let file = crate::config::FileConfig::load()?;

        let base_url = url
            .or_else(|| non_empty("CONVEYOR_URL"))
            .or_else(|| file.url.clone())
            .context(
                "conveyor's address is not set: pass --url or set CONVEYOR_URL, \
                 e.g. https://localhost:9443/conveyor",
            )?
            .trim_end_matches('/')
            .to_string();

        let username = username
            .or_else(|| non_empty("CONVEYOR_USERNAME"))
            .or_else(|| file.username.clone());
        let password = password
            .or_else(|| non_empty("CONVEYOR_PASSWORD"))
            .or_else(|| file.password.clone());

        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(
                insecure || envmnt::is_or("CONVEYOR_INSECURE", false) || file.insecure,
            )
            .build()
            .context("could not build an HTTP client")?;

        let token = match username {
            Some(username) => {
                let gatehouse_url = gatehouse_url
                    .or_else(|| non_empty("GATEHOUSE_URL"))
                    .or_else(|| file.gatehouse_url.clone())
                    .context(
                        "CONVEYOR_USERNAME is set but gatehouse's address is not: pass \
                         --gatehouse-url or set GATEHOUSE_URL, e.g. https://localhost:5443/gatehouse",
                    )?
                    .trim_end_matches('/')
                    .to_string();
                Some(
                    login(
                        &http,
                        &gatehouse_url,
                        &username,
                        &password.unwrap_or_default(),
                    )
                    .await?,
                )
            }
            None => None,
        };

        Ok(Self {
            base_url,
            token,
            http,
        })
    }

    /// A `Client` pointed at `base_url` with no token, bypassing the
    /// login flow entirely - for other modules' tests (e.g.
    /// `commands.rs`) that need a `Client` talking to a `wiremock` server
    /// but can't reach the private fields `Client::new`'s tests use
    /// directly, since they aren't in this module. Not `#[cfg(test)]`:
    /// the `tests/` integration binary links this crate as an ordinary
    /// dependency, where that flag is never set.
    pub fn for_tests(base_url: String) -> Self {
        Self::for_tests_with_token(base_url, None)
    }

    /// Like [`Client::for_tests`], but with an optional bearer token - for
    /// tests exercising `authenticated`'s branch that adds the
    /// `Authorization` header.
    pub fn for_tests_with_token(base_url: String, token: Option<String>) -> Self {
        Self {
            base_url,
            token,
            http: reqwest::Client::new(),
        }
    }

    /// Test-only accessor for `base_url` (private otherwise - callers go
    /// through [`Client::url`]).
    pub fn base_url_for_tests(&self) -> &str {
        &self.base_url
    }

    /// Test-only accessor for `token` (private otherwise).
    pub fn token_for_tests(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base_url)
    }

    fn authenticated(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self
            .authenticated(self.http.get(self.url(path)))
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        decode(response, path).await
    }

    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let response = self
            .authenticated(self.http.post(self.url(path)).json(body))
            .send()
            .await
            .with_context(|| format!("POST {path}"))?;
        decode(response, path).await
    }

    pub async fn put<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let response = self
            .authenticated(self.http.put(self.url(path)).json(body))
            .send()
            .await
            .with_context(|| format!("PUT {path}"))?;
        decode(response, path).await
    }

    pub async fn patch<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let response = self
            .authenticated(self.http.patch(self.url(path)).json(body))
            .send()
            .await
            .with_context(|| format!("PATCH {path}"))?;
        decode(response, path).await
    }

    /// For endpoints that answer with no body.
    pub async fn send_empty(&self, method: reqwest::Method, path: &str) -> Result<()> {
        let response = self
            .authenticated(self.http.request(method, self.url(path)))
            .send()
            .await
            .with_context(|| path.to_string())?;

        if response.status().is_success() {
            return Ok(());
        }
        bail!(explain(
            response.status(),
            &response.text().await.unwrap_or_default()
        ));
    }

    /// The raw response, for streaming.
    pub async fn stream(&self, path: &str) -> Result<reqwest::Response> {
        let response = self
            .authenticated(self.http.get(self.url(path)))
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;

        if response.status().is_success() {
            return Ok(response);
        }
        bail!(explain(
            response.status(),
            &response.text().await.unwrap_or_default()
        ));
    }
}

/// One password exchange against gatehouse's resource-owner login. Not
/// cached to disk - a CLI invocation is short-lived enough that logging in
/// once per run is the simplest thing that works.
async fn login(
    http: &reqwest::Client,
    gatehouse_url: &str,
    username: &str,
    password: &str,
) -> Result<String> {
    let response = http
        .post(format!("{gatehouse_url}/api/v1/auth/login"))
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await
        .context("failed to reach gatehouse to log in")?;

    if !response.status().is_success() {
        bail!(
            "gatehouse rejected {username}'s credentials (set CONVEYOR_USERNAME and CONVEYOR_PASSWORD)"
        );
    }

    let tokens: TokenResponse = response
        .json()
        .await
        .context("gatehouse's login response was not what was expected")?;
    Ok(tokens.access_token)
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response, path: &str) -> Result<T> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if !status.is_success() {
        bail!(explain(status, &body));
    }

    serde_json::from_str(&body)
        .with_context(|| format!("{path} answered with something unexpected: {body}"))
}

/// Turns a status and a body into one line worth reading.
///
/// Conveyor answers errors as `{"error": "..."}`, so the message it went to the
/// trouble of writing is what gets shown rather than the status code alone.
pub fn explain(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string))
        .unwrap_or_else(|| body.trim().to_string());

    if detail.is_empty() {
        return format!("conveyor answered {status}");
    }

    match status.as_u16() {
        401 => format!("{detail} (set CONVEYOR_USERNAME and CONVEYOR_PASSWORD)"),
        _ => detail,
    }
}

pub fn non_empty(key: &str) -> Option<String> {
    let value = envmnt::get_or(key, "");
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}
