//! Talking to conveyor.
//!
//! Basic auth against the realm, which is what every other CLI in the estate
//! uses for a machine account. The service verifies it the same way it verifies
//! a browser's token.

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub struct Client {
    base_url: String,
    credentials: Option<(String, String)>,
    http: reqwest::Client,
}

impl Client {
    pub fn new(
        url: Option<String>,
        username: Option<String>,
        password: Option<String>,
        insecure: bool,
    ) -> Result<Self> {
        let base_url = url
            .or_else(|| non_empty("CONVEYOR_URL"))
            .context(
                "conveyor's address is not set: pass --url or set CONVEYOR_URL, \
                 e.g. https://localhost:9443/conveyor",
            )?
            .trim_end_matches('/')
            .to_string();

        let username = username.or_else(|| non_empty("CONVEYOR_USERNAME"));
        let password = password.or_else(|| non_empty("CONVEYOR_PASSWORD"));
        let credentials = username.map(|user| (user, password.unwrap_or_default()));

        Ok(Self {
            base_url,
            credentials,
            http: reqwest::Client::builder()
                .danger_accept_invalid_certs(insecure || envmnt::is_or("CONVEYOR_INSECURE", false))
                .build()
                .context("could not build an HTTP client")?,
        })
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base_url)
    }

    fn authenticated(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.credentials {
            Some((user, password)) => request.basic_auth(user, Some(password)),
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
fn explain(status: reqwest::StatusCode, body: &str) -> String {
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

fn non_empty(key: &str) -> Option<String> {
    let value = envmnt::get_or(key, "");
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}
