//! The app's requests as calls to the API, and the answers as messages. Over whatever sends a request (the browser's
//! `fetch`, or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use quench_serde::json;
use quench_serde::{Deserialize, Serialize};
use workbench_app::{Msg, Request};
use workbench_core::{
    Board, Comment, Issue, IssueView, Me, NewComment, NewLink, Project, RelatedIssues, Transition,
    User,
};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/api/v1/projects`); the transport adds the prefix.
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

/// The service's path prefix from the page's own path: `/workbench/ui/home` is `/workbench`, `/ui/home` is no prefix.
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
            Self::Other(403, _) => Msg::Failed(
                "this account may not see Workbench: ask for access in gatehouse".into(),
            ),
            Self::Other(status, _) => Msg::Failed(format!("the service answered {status}")),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }

    /// A change that did not work: the service's own words when it gave some.
    fn write(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::NotFound => Msg::Rejected("it is gone: someone removed it".into()),
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

    async fn board(&self, id: &str) -> Result<Board, Failure> {
        let project: Project = self
            .get(format!("/api/v1/projects/{}", segment(id)))
            .await?;
        let issues: Vec<Issue> = self
            .get(format!("/api/v1/projects/{}/issues", segment(id)))
            .await?;
        Ok(Board { project, issues })
    }

    async fn issue(&self, id: &str) -> Result<IssueView, Failure> {
        let issue: Issue = self.get(format!("/api/v1/issues/{}", segment(id))).await?;
        let project: Project = self
            .get(format!("/api/v1/projects/{}", segment(&issue.project_id)))
            .await?;
        let comments: Vec<Comment> = self
            .get(format!("/api/v1/issues/{}/comments", segment(id)))
            .await?;
        let links: RelatedIssues = self
            .get(format!("/api/v1/issues/{}/links", segment(id)))
            .await?;
        Ok(IssueView {
            issue,
            project,
            comments,
            links,
        })
    }

    /// The page of an issue again, after a change to it.
    async fn reopened(&self, id: &str) -> Vec<Msg> {
        vec![match self.issue(id).await {
            Ok(view) => Msg::Issue(Box::new(view)),
            Err(failure) => failure.read(),
        }]
    }

    /// What the host tells the app about a request. `Language` is the browser's own business.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Language(_) => vec![],
            Request::Startup => {
                let mut out = Vec::new();
                match self.get::<Me>("/api/v1/me".into()).await {
                    Ok(me) => out.push(Msg::Me(me.username)),
                    Err(Failure::SignedOut) => return vec![Msg::SignedOut],
                    Err(_) => {}
                }
                match self.get::<Vec<User>>("/api/v1/users".into()).await {
                    Ok(users) => out.push(Msg::Users(users)),
                    Err(Failure::SignedOut) => return vec![Msg::SignedOut],
                    Err(_) => {}
                }
                out
            }
            Request::Projects => vec![match self
                .get::<Vec<Project>>("/api/v1/projects".into())
                .await
            {
                Ok(list) => Msg::Projects(list),
                Err(failure) => failure.read(),
            }],
            Request::Board(id) => vec![match self.board(&id).await {
                Ok(board) => Msg::Board(Box::new(board)),
                Err(failure) => failure.read(),
            }],
            Request::Issue(id) => self.reopened(&id).await,
            Request::CreateProject(body) => {
                vec![
                    match self.send("POST", "/api/v1/projects".into(), &body).await {
                        Ok(project) => Msg::ProjectCreated(Box::new(project)),
                        Err(failure) => failure.write(),
                    },
                ]
            }
            Request::CreateIssue { project, body } => {
                let path = format!("/api/v1/projects/{}/issues", segment(&project));
                vec![match self.send("POST", path, &body).await {
                    Ok(issue) => Msg::IssueCreated(Box::new(issue)),
                    Err(failure) => failure.write(),
                }]
            }
            Request::Move { issue, status } => {
                let path = format!("/api/v1/issues/{}/transition", segment(&issue));
                match self
                    .send::<_, Issue>("POST", path, &Transition { status })
                    .await
                {
                    Ok(_) => vec![],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Save {
                issue,
                body,
                status,
            } => {
                let path = format!("/api/v1/issues/{}", segment(&issue));
                if let Err(failure) = self.send::<_, Issue>("PUT", path, &body).await {
                    return vec![failure.write()];
                }
                if let Some(status) = status {
                    let path = format!("/api/v1/issues/{}/transition", segment(&issue));
                    if let Err(failure) = self
                        .send::<_, Issue>("POST", path, &Transition { status })
                        .await
                    {
                        return vec![failure.write()];
                    }
                }
                self.reopened(&issue).await
            }
            Request::Comment { issue, body } => {
                let path = format!("/api/v1/issues/{}/comments", segment(&issue));
                match self
                    .send::<_, Comment>("POST", path, &NewComment { body })
                    .await
                {
                    Ok(_) => self.reopened(&issue).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::AddLink { issue, key, kind } => {
                let target: Issue = match self
                    .get(format!("/api/v1/issues?key={}", segment(&key)))
                    .await
                {
                    Ok(target) => target,
                    Err(Failure::NotFound) => {
                        return vec![Msg::Rejected(format!("there is no issue {key}"))];
                    }
                    Err(failure) => return vec![failure.write()],
                };
                let path = format!("/api/v1/issues/{}/links", segment(&issue));
                let body = NewLink {
                    linked_issue_id: target.id,
                    kind,
                };
                match self
                    .send::<_, quench_serde::Value>("POST", path, &body)
                    .await
                {
                    Ok(_) => self.reopened(&issue).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::RemoveLink { issue, link } => {
                match self
                    .call(
                        "DELETE",
                        format!("/api/v1/issue-links/{}", segment(&link)),
                        None,
                    )
                    .await
                {
                    Ok(_) => self.reopened(&issue).await,
                    Err(failure) => vec![failure.write()],
                }
            }
        }
    }
}
