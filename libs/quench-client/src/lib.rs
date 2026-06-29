use anyhow::{Context, Result, anyhow};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

pub type ClientResult<T> = std::result::Result<T, ClientError>;

#[derive(Clone)]
pub struct HttpClient {
    http: reqwest::Client,
    base_url: String,
}

impl HttpClient {
    pub fn new(base_url: &str) -> Result<Self> {
        Self::builder(base_url).build()
    }

    pub fn builder(base_url: &str) -> HttpClientBuilder {
        HttpClientBuilder {
            base_url: base_url.to_string(),
            tls_verify: true,
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to send GET request")?;

        self.handle_response::<T>(res).await
    }

    pub async fn post<S: Serialize, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;

        self.handle_response::<T>(res).await
    }

    pub async fn put<S: Serialize, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .put(&url)
            .json(body)
            .send()
            .await
            .context("Failed to send PUT request")?;

        self.handle_response::<T>(res).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .delete(&url)
            .send()
            .await
            .context("Failed to send DELETE request")?;

        self.handle_response::<T>(res).await
    }

    async fn handle_response<T: DeserializeOwned>(&self, res: reqwest::Response) -> Result<T> {
        let status = res.status();

        if !status.is_success() {
            let body = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("HTTP {}: {}", status, body));
        }

        res.json::<T>().await.context("Failed to parse response")
    }
}

pub struct HttpClientBuilder {
    base_url: String,
    tls_verify: bool,
}

impl HttpClientBuilder {
    pub fn tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    pub fn build(self) -> Result<HttpClient> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()?;

        Ok(HttpClient {
            http,
            base_url: self.base_url,
        })
    }
}

/// HTTP client with Basic authentication
#[derive(Clone)]
pub struct BasicAuthClient {
    client: HttpClient,
    username: String,
    password: String,
}

impl BasicAuthClient {
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self> {
        let client = HttpClient::new(base_url)?;
        Ok(Self {
            client,
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    pub fn builder(base_url: &str) -> BasicAuthClientBuilder {
        BasicAuthClientBuilder {
            base_url: base_url.to_string(),
            tls_verify: true,
            username: String::new(),
            password: String::new(),
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .context("Failed to send GET request")?;

        self.client.handle_response::<T>(res).await
    }

    pub async fn post<S: Serialize, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .post(&url)
            .basic_auth(&self.username, Some(&self.password))
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;

        self.client.handle_response::<T>(res).await
    }

    pub async fn put<S: Serialize, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .put(&url)
            .basic_auth(&self.username, Some(&self.password))
            .json(body)
            .send()
            .await
            .context("Failed to send PUT request")?;

        self.client.handle_response::<T>(res).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .delete(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .context("Failed to send DELETE request")?;

        self.client.handle_response::<T>(res).await
    }
}

pub struct BasicAuthClientBuilder {
    base_url: String,
    tls_verify: bool,
    username: String,
    password: String,
}

impl BasicAuthClientBuilder {
    pub fn username(mut self, username: &str) -> Self {
        self.username = username.to_string();
        self
    }

    pub fn password(mut self, password: &str) -> Self {
        self.password = password.to_string();
        self
    }

    pub fn tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    pub fn build(self) -> Result<BasicAuthClient> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()?;

        Ok(BasicAuthClient {
            client: HttpClient {
                http,
                base_url: self.base_url,
            },
            username: self.username,
            password: self.password,
        })
    }
}

/// Bearer token authentication client
#[derive(Clone)]
pub struct BearerAuthClient {
    client: HttpClient,
    token: String,
}

impl BearerAuthClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let client = HttpClient::new(base_url)?;
        Ok(Self {
            client,
            token: token.to_string(),
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("Failed to send GET request")?;

        self.client.handle_response::<T>(res).await
    }

    pub async fn post<S: Serialize, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T> {
        let url = format!("{}{}", self.client.base_url, path);
        let res = self
            .client
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .context("Failed to send POST request")?;

        self.client.handle_response::<T>(res).await
    }
}

pub mod prelude {
    pub use crate::{BasicAuthClient, BearerAuthClient, HttpClient};
}
