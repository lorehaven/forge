//! The app's requests as calls to the API, and the answers as messages. Over whatever sends a request (the browser's
//! `fetch`, or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use quench_serde::json;
use quench_serde::{Deserialize, Serialize};
use switchboard_app::{Msg, Request};
use switchboard_core::{Capabilities, DeleteBody, Gpu, Instance, LaunchBody, Model, ModelFilters};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/api/v1/gpu/status`); the transport adds the prefix.
    pub path: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// A future that is not sent between threads: a page has one.
pub type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// What sends a request.
pub trait Transport {
    /// # Errors
    /// The reason it could not be sent (no network).
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>>;
}

/// One path segment, percent-encoded.
#[must_use]
pub fn segment(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The service's path prefix from the page's own path: `/switchboard/ui/home` is `/switchboard`, `/ui/home` is no prefix.
#[must_use]
pub fn base_of(path: &str) -> String {
    let at = path
        .find("/ui/")
        .or_else(|| path.strip_suffix("/ui").map(str::len));
    at.map_or_else(String::new, |i| path[..i].trim_end_matches('/').to_string())
}

/// What went wrong with a call, as the app says it.
enum Failure {
    SignedOut,
    Other(u16, String),
    Offline(String),
}

impl Failure {
    /// A read that did not work.
    fn read(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::Other(403, _) => Msg::Failed(
                "this account may not see Switchboard: ask for access in gatehouse".into(),
            ),
            Self::Other(status, _) => Msg::Failed(format!("the service answered {status}")),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }

    /// A change that did not work: the service's own words when it gave some (it answers with translation keys, which the
    /// app translates).
    fn write(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::Other(403, body) if body.trim().is_empty() => {
                Msg::Rejected("this account may not do that: ask for access in gatehouse".into())
            }
            Self::Other(404, body) if body.trim().is_empty() => {
                Msg::Rejected("it is gone: someone removed it".into())
            }
            Self::Other(status, body) => Msg::Rejected(
                words(&body).unwrap_or_else(|| format!("the service answered {status}")),
            ),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }
}

#[derive(Deserialize)]
struct ApiError {
    error: String,
}

/// What the service said: its `{"error": "..."}`, or its plain text (a translation key), when it said anything short.
fn words(body: &str) -> Option<String> {
    if let Ok(e) = json::from_str::<ApiError>(body) {
        return Some(e.error);
    }
    let text = body.trim();
    (!text.is_empty() && text.len() <= 200 && !text.starts_with('<')).then(|| text.to_string())
}

/// The calls of one page.
pub struct Client<T: Transport> {
    transport: T,
}

impl<T: Transport> Client<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    async fn call(
        &self,
        method: &'static str,
        path: String,
        body: Option<String>,
    ) -> Result<String, Failure> {
        let response = self
            .transport
            .send(HttpRequest { method, path, body })
            .await
            .map_err(Failure::Offline)?;
        match response.status {
            200..=299 => Ok(response.body),
            401 => Err(Failure::SignedOut),
            status => Err(Failure::Other(status, response.body)),
        }
    }

    async fn get<R: Deserialize>(&self, path: &str) -> Result<R, Failure> {
        let body = self.call("GET", path.to_string(), None).await?;
        json::from_str(&body)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    async fn send<B: Serialize, R: Deserialize>(
        &self,
        method: &'static str,
        path: &str,
        body: &B,
    ) -> Result<R, Failure> {
        let text = self
            .call(method, path.to_string(), Some(json::to_string(body)))
            .await?;
        json::from_str(&text)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    /// What the host tells the app about a request. `Language` is the browser's own business.
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Language(_) => vec![],
            Request::Startup => vec![match self
                .get::<Capabilities>("/api/v1/vllm/capabilities")
                .await
            {
                Ok(caps) => Msg::Capabilities(caps),
                Err(failure) => failure.read(),
            }],
            Request::Gpu => vec![match self.get::<Gpu>("/api/v1/gpu/status").await {
                Ok(gpu) => Msg::Gpu(gpu),
                Err(failure) => failure.read(),
            }],
            Request::Models(filters) => {
                vec![match self
                    .send::<_, Vec<Model>>("POST", "/api/v1/models/list", &filters)
                    .await
                {
                    Ok(list) => Msg::Models(filters, list),
                    Err(failure) => failure.read(),
                }]
            }
            Request::Catalog => {
                // Every source: the launch form lists the models vLLM can run, wherever they are.
                let everything = ModelFilters {
                    source: "all".into(),
                    search: String::new(),
                    sort: "name_asc".into(),
                    quant: "ALL".into(),
                    context: "0".into(),
                    vllm_only: false,
                };
                vec![match self
                    .send::<_, Vec<Model>>("POST", "/api/v1/models/list", &everything)
                    .await
                {
                    Ok(list) => Msg::Catalog(list),
                    Err(failure) => failure.read(),
                }]
            }
            Request::DeleteModel(path) => {
                match self
                    .call(
                        "POST",
                        "/api/v1/models/delete".into(),
                        Some(json::to_string(&DeleteBody { path: path.clone() })),
                    )
                    .await
                {
                    Ok(_) => vec![Msg::ModelDeleted(path)],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Instances => {
                vec![
                    match self.get::<Vec<Instance>>("/api/v1/vllm/instances").await {
                        Ok(list) => Msg::Instances(list),
                        Err(failure) => failure.read(),
                    },
                ]
            }
            Request::Launch(body) => vec![launched(self.launch(&body).await)],
            Request::Stop(id) => {
                match self
                    .call(
                        "DELETE",
                        format!("/api/v1/vllm/instances/{}", segment(&id)),
                        None,
                    )
                    .await
                {
                    Ok(_) => vec![Msg::Stopped(id)],
                    Err(failure) => vec![failure.write()],
                }
            }
        }
    }

    async fn launch(&self, body: &LaunchBody) -> Result<(), Failure> {
        self.call(
            "POST",
            "/api/v1/vllm/instances".into(),
            Some(json::to_string(body)),
        )
        .await
        .map(|_| ())
    }
}

fn launched(result: Result<(), Failure>) -> Msg {
    match result {
        Ok(()) => Msg::Launched,
        Err(failure) => failure.write(),
    }
}
