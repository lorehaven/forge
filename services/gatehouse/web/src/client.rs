//! The app's requests as calls to the service, and the answers as messages. Over whatever sends a request (the browser's
//! `fetch`, or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use gatehouse_app::{Msg, Request};
use gatehouse_core::{
    Account, AdminUser, AdminUsers, ConfirmEmail, Dashboard, MfaEnrollment, UnsubscribeInfo,
    segment,
};
use quench_serde::json;
use quench_serde::{Deserialize, Serialize};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/api/v1/ui/account`); the transport adds the prefix.
    pub path: String,
    pub body: Option<String>,
    /// Whether the body is an urlencoded form rather than JSON.
    pub form: bool,
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

/// What went wrong with a call, as the app says it.
enum Failure {
    SignedOut,
    Forbidden,
    NotFound,
    /// The service refused with a reason (`{"error": "<translation key>"}`).
    Refused(String),
    Other(u16),
    Offline(String),
}

impl Failure {
    fn msg(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::Forbidden => Msg::Forbidden,
            Self::NotFound => Msg::NotFound,
            Self::Refused(key) => Msg::Failed(key),
            Self::Other(status) => Msg::Failed(format!("the service answered {status}")),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }
}

#[derive(Deserialize)]
struct ApiError {
    error: String,
}

/// The `error` of the API's `{"error": "..."}`.
fn error_text(body: &str) -> Option<String> {
    json::from_str::<ApiError>(body).ok().map(|e| e.error)
}

/// The calls of one page.
pub struct Client<T: Transport> {
    transport: T,
}

impl<T: Transport> Client<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    async fn call(&self, request: HttpRequest) -> Result<String, Failure> {
        let response = self
            .transport
            .send(request)
            .await
            .map_err(Failure::Offline)?;
        match response.status {
            200..=299 => Ok(response.body),
            401 => Err(Failure::SignedOut),
            403 => Err(Failure::Forbidden),
            404 => Err(Failure::NotFound),
            400 | 409 | 422 => Err(error_text(&response.body)
                .map_or(Failure::Other(response.status), Failure::Refused)),
            status => Err(Failure::Other(status)),
        }
    }

    async fn get<R: Deserialize>(&self, path: String) -> Result<R, Failure> {
        let body = self
            .call(HttpRequest {
                method: "GET",
                path,
                body: None,
                form: false,
            })
            .await?;
        json::from_str(&body)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    /// Reads, with the answer turned into the message `into` makes of it.
    async fn read<R: Deserialize>(&self, path: String, into: impl FnOnce(R) -> Msg) -> Vec<Msg> {
        vec![match self.get::<R>(path).await {
            Ok(value) => into(value),
            Err(failure) => failure.msg(),
        }]
    }

    /// A link of an email stands for nothing when the service has never heard of it.
    async fn read_link<R: Deserialize>(
        &self,
        path: String,
        into: impl FnOnce(R) -> Msg,
    ) -> Vec<Msg> {
        vec![match self.get::<R>(path).await {
            Ok(value) => into(value),
            Err(Failure::NotFound) => Msg::LinkInvalid,
            Err(failure) => failure.msg(),
        }]
    }

    async fn post<B: Serialize>(&self, path: &str, body: &B) -> Result<String, Failure> {
        self.call(HttpRequest {
            method: "POST",
            path: path.to_string(),
            body: Some(json::to_string(body)),
            form: false,
        })
        .await
    }

    /// What the host tells the app about a request. `Language` is the browser's own business.
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Language(_) => vec![],
            Request::Dashboard => {
                self.read("/api/v1/ui/dashboard".into(), |d: Dashboard| {
                    Msg::Dashboard(Box::new(d))
                })
                .await
            }
            Request::Account => {
                self.read("/api/v1/ui/account".into(), |a: Account| {
                    Msg::Account(Box::new(a))
                })
                .await
            }
            Request::MfaSecret => {
                self.read("/api/v1/ui/mfa-enroll".into(), |e: MfaEnrollment| {
                    Msg::MfaSecret(Box::new(e))
                })
                .await
            }
            Request::MfaEnable(body) => {
                vec![match self.post("/api/v1/ui/mfa-enroll", &body).await {
                    Ok(_) => Msg::MfaEnabled,
                    // A wrong code is the person's to fix: the screen keeps its secret and says so.
                    Err(Failure::Refused(key)) => Msg::MfaRefused(key),
                    Err(failure) => failure.msg(),
                }]
            }
            Request::AdminUsers => {
                self.read("/api/v1/ui/admin/users".into(), |u: AdminUsers| {
                    Msg::AdminUsers(Box::new(u))
                })
                .await
            }
            Request::AdminUser(name) => {
                self.read(
                    format!("/api/v1/ui/admin/users/{}", segment(&name)),
                    |u: AdminUser| Msg::AdminUser(Box::new(u)),
                )
                .await
            }
            Request::ConfirmEmail(token) => {
                self.read_link(
                    format!("/api/v1/ui/confirm-email?token={}", segment(&token)),
                    |c: ConfirmEmail| Msg::ConfirmEmail(c),
                )
                .await
            }
            Request::UnsubscribeInfo(token) => {
                self.read_link(
                    format!("/api/v1/ui/unsubscribe?token={}", segment(&token)),
                    |i: UnsubscribeInfo| Msg::UnsubscribeInfo(i),
                )
                .await
            }
            Request::Unsubscribe(token) => vec![match self
                .call(HttpRequest {
                    method: "POST",
                    path: format!("/ui/unsubscribe?token={}", segment(&token)),
                    body: Some("unsubscribe=1".into()),
                    form: true,
                })
                .await
            {
                Ok(_) => Msg::Unsubscribed,
                Err(Failure::NotFound) => Msg::LinkInvalid,
                Err(failure) => failure.msg(),
            }],
        }
    }
}
