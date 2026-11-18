//! The app's requests as calls to the API, and the answers as messages. Over whatever sends a request (the browser's
//! `fetch`, or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use mailroom_app::{ListQuery, Msg, Request};
use mailroom_core::admin::{AdminAccount, AuditEntry, OutboxItem, Page, Settings, Stats};
use mailroom_core::{Account, Folder, Me, MessageList, MessageView, Preset, TestResult};
use quench_serde::json;
use quench_serde::{Deserialize, Serialize};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/api/v1/accounts`); the transport adds the prefix.
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

/// One path segment or query value, percent-encoded.
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

/// The service's path prefix from the page's own path: `/mailroom/ui/home` is `/mailroom`, `/ui/home` is no prefix.
#[must_use]
pub fn base_of(path: &str) -> String {
    let at = path
        .find("/ui/")
        .or_else(|| path.strip_suffix("/ui").map(str::len));
    at.map_or_else(String::new, |i| path[..i].trim_end_matches('/').to_string())
}

/// The query string of a message list.
#[must_use]
pub fn list_path(query: &ListQuery) -> String {
    let mut parts = Vec::new();
    if let Some(folder) = &query.folder {
        parts.push(format!("folder={}", segment(folder)));
    }
    if let Some(role) = &query.role {
        parts.push(format!("role={}", segment(role)));
    }
    if !query.text.trim().is_empty() {
        parts.push(format!("q={}", segment(query.text.trim())));
    }
    if query.unread {
        parts.push("unread=1".to_string());
    }
    parts.push(format!("offset={}", query.offset));
    parts.push("limit=50".to_string());
    format!("/api/v1/messages?{}", parts.join("&"))
}

/// What went wrong with a call, as the app says it.
enum Failure {
    SignedOut,
    NotFound,
    Other(u16, String),
    Offline(String),
}

impl Failure {
    /// A read that did not work.
    fn read(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::NotFound => Msg::NotFound,
            Self::Other(403, _) => {
                Msg::Failed("this account may not use Mailroom: ask for access in gatehouse".into())
            }
            Self::Other(502, body) => Msg::Failed(
                error_text(&body).unwrap_or_else(|| "the mail server did not answer".into()),
            ),
            Self::Other(status, _) => Msg::Failed(format!("the service answered {status}")),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }

    /// A change that did not work: the service's own words when it gave some.
    fn write(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::NotFound => Msg::Rejected("it is gone".into()),
            Self::Other(status, body) => Msg::Rejected(
                error_text(&body).unwrap_or_else(|| format!("the service answered {status}")),
            ),
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

impl<T: Transport> std::fmt::Debug for Client<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

#[allow(
    clippy::future_not_send,
    reason = "a page runs on the browser's one thread: `Transport::send` answers a `Boxed` future, which is not `Send`"
)]
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
            404 => Err(Failure::NotFound),
            status => Err(Failure::Other(status, response.body)),
        }
    }

    async fn get<R: Deserialize>(&self, path: String) -> Result<R, Failure> {
        let body = self.call("GET", path, None).await?;
        json::from_str(&body)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    async fn send<B: Serialize, R: Deserialize>(
        &self,
        method: &'static str,
        path: String,
        body: &B,
    ) -> Result<R, Failure> {
        let text = self.call(method, path, Some(json::to_string(body))).await?;
        json::from_str(&text)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    /// A change that answers nothing the app needs.
    async fn act<B: Serialize>(
        &self,
        method: &'static str,
        path: String,
        body: Option<&B>,
    ) -> Result<(), Failure> {
        self.call(method, path, body.map(|b| json::to_string(b)))
            .await
            .map(|_| ())
    }

    async fn admin_accounts(&self, offset: u32) -> Vec<Msg> {
        vec![match self
            .get::<Page<AdminAccount>>(format!("/api/v1/admin/accounts?offset={offset}&limit=50"))
            .await
        {
            Ok(page) => Msg::AdminAccounts(page),
            Err(failure) => failure.read(),
        }]
    }

    async fn outbox(&self, status: &str, offset: u32) -> Vec<Msg> {
        let path = format!(
            "/api/v1/admin/outbox?status={}&offset={offset}&limit=50",
            segment(status)
        );
        vec![match self.get::<Page<OutboxItem>>(path).await {
            Ok(page) => Msg::Outbox(page),
            Err(failure) => failure.read(),
        }]
    }

    /// What the host tells the app about a request. `Language` is the browser's own business.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Startup => {
                let mut out = Vec::new();
                match self.get::<Me>("/api/v1/me".into()).await {
                    Ok(me) => out.push(Msg::Me(me)),
                    Err(Failure::SignedOut) => return vec![Msg::SignedOut],
                    Err(failure) => return vec![failure.read()],
                }
                if let Ok(presets) = self.get::<Vec<Preset>>("/api/v1/presets".into()).await {
                    out.push(Msg::Presets(presets));
                }
                match self.get::<Vec<Account>>("/api/v1/accounts".into()).await {
                    Ok(accounts) => out.push(Msg::Accounts(accounts)),
                    Err(failure) => out.push(failure.read()),
                }
                out
            }
            Request::Accounts => vec![match self
                .get::<Vec<Account>>("/api/v1/accounts".into())
                .await
            {
                Ok(accounts) => Msg::Accounts(accounts),
                Err(failure) => failure.read(),
            }],
            Request::Folders(account) => {
                let path = format!("/api/v1/accounts/{}/folders", segment(&account));
                vec![match self.get::<Vec<Folder>>(path).await {
                    Ok(folders) => Msg::Folders { account, folders },
                    Err(failure) => failure.read(),
                }]
            }
            Request::List(query) => vec![match self.get::<MessageList>(list_path(&query)).await {
                Ok(list) => Msg::Messages(list),
                Err(failure) => failure.read(),
            }],
            Request::Open(id) => vec![match self
                .get::<MessageView>(format!("/api/v1/messages/{}", segment(&id)))
                .await
            {
                Ok(view) => Msg::Message(Box::new(view)),
                Err(failure) => failure.read(),
            }],
            Request::Change(change) => {
                let moved = change.move_to.is_some() || change.delete;
                match self
                    .act("POST", "/api/v1/message-changes".into(), Some(&change))
                    .await
                {
                    Ok(()) => vec![if moved { Msg::Moved } else { Msg::Changed }],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Send(compose) => match self
                .act("POST", "/api/v1/outgoing".into(), Some(&compose))
                .await
            {
                Ok(()) => vec![Msg::Sent],
                Err(failure) => vec![failure.write()],
            },
            Request::Refresh(id) => {
                let path = format!("/api/v1/accounts/{}/refresh", segment(&id));
                match self.act::<()>("POST", path, None).await {
                    Ok(()) => vec![Msg::Changed],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::AddAccount(input) => vec![match self
                .send::<_, Account>("POST", "/api/v1/accounts".into(), &input)
                .await
            {
                Ok(account) => Msg::AccountSaved(Box::new(account)),
                Err(failure) => failure.write(),
            }],
            Request::UpdateAccount { id, input } => {
                let path = format!("/api/v1/accounts/{}", segment(&id));
                vec![match self.send::<_, Account>("PUT", path, &input).await {
                    Ok(account) => Msg::AccountSaved(Box::new(account)),
                    Err(failure) => failure.write(),
                }]
            }
            Request::TestAccount(input) => vec![match self
                .send::<_, TestResult>("POST", "/api/v1/account-tests".into(), &input)
                .await
            {
                Ok(result) => Msg::Tested(Box::new(result)),
                Err(failure) => failure.write(),
            }],
            Request::RemoveAccount(id) => {
                match self
                    .act::<()>("DELETE", format!("/api/v1/accounts/{}", segment(&id)), None)
                    .await
                {
                    Ok(()) => vec![Msg::AccountRemoved(id)],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::RemoveUpload(id) => {
                // Best effort: the page already forgot it, and unused uploads expire.
                let _ = self
                    .act::<()>("DELETE", format!("/api/v1/uploads/{}", segment(&id)), None)
                    .await;
                vec![]
            }
            Request::Stats => vec![
                match self.get::<Stats>("/api/v1/admin/stats".into()).await {
                    Ok(stats) => Msg::Stats(stats),
                    Err(failure) => failure.read(),
                },
            ],
            Request::Settings => vec![match self
                .get::<Settings>("/api/v1/admin/settings".into())
                .await
            {
                Ok(settings) => Msg::Settings(Box::new(settings)),
                Err(failure) => failure.read(),
            }],
            Request::SaveSettings(input) => {
                vec![match self
                    .send::<_, Settings>("PUT", "/api/v1/admin/settings".into(), &input)
                    .await
                {
                    Ok(settings) => Msg::SettingsSaved(Box::new(settings)),
                    Err(failure) => failure.write(),
                }]
            }
            Request::TestTransport => {
                let result = self
                    .call("POST", "/api/v1/admin/transport/test".into(), None)
                    .await;
                vec![match result.and_then(|body| {
                    json::from_str::<TestResult>(&body)
                        .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
                }) {
                    Ok(result) => Msg::TransportTested(Box::new(result)),
                    Err(failure) => failure.write(),
                }]
            }
            Request::AdminAccounts { offset } => self.admin_accounts(offset).await,
            Request::SetDisabled { id, disabled } => {
                let verb = if disabled { "disable" } else { "enable" };
                match self
                    .act::<()>(
                        "POST",
                        format!("/api/v1/admin/accounts/{}/{verb}", segment(&id)),
                        None,
                    )
                    .await
                {
                    Ok(()) => self.admin_accounts(0).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::AdminRemoveAccount(id) => {
                match self
                    .act::<()>(
                        "DELETE",
                        format!("/api/v1/admin/accounts/{}", segment(&id)),
                        None,
                    )
                    .await
                {
                    Ok(()) => self.admin_accounts(0).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Outbox { status, offset } => self.outbox(&status, offset).await,
            Request::Retry(id) => {
                match self
                    .act::<()>(
                        "POST",
                        format!("/api/v1/admin/outbox/{}/retry", segment(&id)),
                        None,
                    )
                    .await
                {
                    Ok(()) => self.outbox("", 0).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::RemoveQueued(id) => {
                match self
                    .act::<()>(
                        "DELETE",
                        format!("/api/v1/admin/outbox/{}", segment(&id)),
                        None,
                    )
                    .await
                {
                    Ok(()) => self.outbox("", 0).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Audit { offset } => {
                vec![match self
                    .get::<Page<AuditEntry>>(format!(
                        "/api/v1/admin/audit?offset={offset}&limit=50"
                    ))
                    .await
                {
                    Ok(page) => Msg::Audit(page),
                    Err(failure) => failure.read(),
                }]
            }
            Request::Language(_) => vec![],
        }
    }
}
