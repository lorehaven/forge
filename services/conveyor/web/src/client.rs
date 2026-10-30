//! The app's requests as calls to the API, and the answers as messages. Over whatever sends a request (the browser's `fetch`,
//! or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use conveyor_app::{Follow, Msg, Request};
use conveyor_core::format::segment;
use conveyor_core::{
    Access, Credential, KnownStage, Project, Repo, RepoChecks, Run, RunDetail, RunView, RunsPage,
    ScanFindings, ScanReport, Subscription,
};
use quench_serde::json;
use quench_serde::{Deserialize, Serialize};

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

/// The service's path prefix from the page's own path: `/conveyor/ui/home` is `/conveyor`, `/ui/home` is no prefix.
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
            Self::Other(403, _) => {
                Msg::Failed("this account may not see that: ask for access in gatehouse".into())
            }
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

/// `?project=..&page=..` for the runs reads.
fn scope_query(scope: Option<&str>, page: Option<u32>) -> String {
    let mut parts = Vec::new();
    if let Some(scope) = scope {
        parts.push(format!("project={}", segment(scope)));
    }
    if let Some(page) = page {
        parts.push(format!("page={page}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
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

    async fn post<R: Deserialize>(&self, path: String) -> Result<R, Failure> {
        let text = self.call("POST", path, None).await?;
        json::from_str(&text)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    // ── The reads the screens are made of ────────────────────────────────────

    async fn repos(&self) -> Result<Vec<Repo>, Failure> {
        self.get("/api/v1/repos".into()).await
    }

    async fn projects(&self) -> Result<Vec<Project>, Failure> {
        self.get("/api/v1/projects?all=true".into()).await
    }

    async fn recent(&self, scope: Option<&str>) -> Result<Vec<Run>, Failure> {
        self.get(format!("/api/v1/overview/runs{}", scope_query(scope, None)))
            .await
    }

    /// The repository `owner/name` among the visible ones.
    fn find(repos: &[Repo], owner: &str, name: &str) -> Option<Repo> {
        repos
            .iter()
            .find(|r| r.owner == owner && r.name == name)
            .cloned()
    }

    async fn run_view(&self, id: &str) -> Result<RunView, Failure> {
        let detail: RunDetail = self.get(format!("/api/v1/runs/{}", segment(id))).await?;
        // The repository may be gone (the run survives it) or out of reach: the page says "unknown repository".
        let repo = self
            .get::<Repo>(format!("/api/v1/repos/{}", segment(&detail.run.repo_id)))
            .await
            .ok();
        Ok(RunView {
            run: detail.run,
            jobs: detail.jobs,
            artifacts: detail.artifacts,
            repo,
        })
    }

    /// Reads that may fail without the page being wrong: only a lost session matters.
    async fn optional<R: Deserialize>(&self, path: String) -> Result<Option<R>, Failure> {
        match self.get(path).await {
            Ok(value) => Ok(Some(value)),
            Err(Failure::SignedOut) => Err(Failure::SignedOut),
            Err(_) => Ok(None),
        }
    }

    /// What the repositories pages show.
    async fn repos_page(&self) -> Vec<Msg> {
        let mut out = Vec::new();
        match self.optional::<Access>("/api/v1/access".into()).await {
            Ok(Some(access)) => out.push(Msg::Access(access)),
            Ok(None) => {}
            Err(failure) => return vec![failure.read()],
        }
        match self.projects().await {
            Ok(list) => out.push(Msg::Projects(list)),
            Err(failure) => return vec![failure.read()],
        }
        match self.repos().await {
            Ok(list) => out.push(Msg::Repos(list)),
            Err(failure) => return vec![failure.read()],
        }
        match self
            .optional::<Vec<Subscription>>("/api/v1/subscriptions".into())
            .await
        {
            Ok(Some(list)) => out.push(Msg::Subscriptions(list)),
            Ok(None) => {}
            Err(failure) => return vec![failure.read()],
        }
        out
    }

    /// The repositories again, after a change to them, with what to say.
    async fn reread(&self, mut first: Vec<Msg>) -> Vec<Msg> {
        match self.repos().await {
            Ok(list) => first.insert(0, Msg::Repos(list)),
            Err(failure) => first.push(failure.read()),
        }
        first
    }

    /// What the app asks for, as the messages the service's answers make. Streaming a log and the language are the browser's
    /// own business.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Language(_) | Request::OpenLog(_) | Request::CloseLog(_) => vec![],
            Request::Home { scope } => {
                let mut out = Vec::new();
                match self.projects().await {
                    Ok(list) => out.push(Msg::Projects(list)),
                    Err(failure) => return vec![failure.read()],
                }
                match self.repos().await {
                    Ok(list) => out.push(Msg::Repos(list)),
                    Err(failure) => return vec![failure.read()],
                }
                match self.recent(scope.as_deref()).await {
                    Ok(runs) => out.push(Msg::Recent { scope, runs }),
                    Err(failure) => return vec![failure.read()],
                }
                match self
                    .optional::<Vec<RepoChecks>>("/api/v1/scan".into())
                    .await
                {
                    Ok(Some(chips)) => out.push(Msg::Chips(chips)),
                    Ok(None) => {}
                    Err(failure) => return vec![failure.read()],
                }
                out
            }
            Request::Recent { scope } => vec![match self.recent(scope.as_deref()).await {
                Ok(runs) => Msg::Recent { scope, runs },
                Err(failure) => failure.read(),
            }],
            Request::History { scope, page } => {
                let mut out = Vec::new();
                if scope.is_some() {
                    match self.projects().await {
                        Ok(list) => out.push(Msg::Projects(list)),
                        Err(failure) => return vec![failure.read()],
                    }
                }
                match self.repos().await {
                    Ok(list) => out.push(Msg::Repos(list)),
                    Err(failure) => return vec![failure.read()],
                }
                let path = format!(
                    "/api/v1/overview/history{}",
                    scope_query(scope.as_deref(), Some(page))
                );
                match self.get::<RunsPage>(path).await {
                    Ok(page) => out.push(Msg::History(page)),
                    Err(failure) => return vec![failure.read()],
                }
                out
            }
            Request::Run(id) => vec![match self.run_view(&id).await {
                Ok(view) => Msg::Run(Box::new(view)),
                Err(failure) => failure.read(),
            }],
            Request::Restart(id) => {
                vec![match self
                    .post::<Run>(format!("/api/v1/runs/{}/restart", segment(&id)))
                    .await
                {
                    Ok(run) => Msg::Opened(run.id),
                    Err(failure) => failure.write(),
                }]
            }
            Request::RunNow { repo, scope } => {
                if let Err(failure) = self
                    .post::<Run>(format!("/api/v1/repos/{}/runs", segment(&repo)))
                    .await
                {
                    return vec![failure.write()];
                }
                vec![match self.recent(scope.as_deref()).await {
                    Ok(runs) => Msg::Recent { scope, runs },
                    Err(failure) => failure.read(),
                }]
            }
            Request::Repos => self.repos_page().await,
            Request::CreateRepo(body) => {
                match self
                    .send::<_, Repo>("POST", "/api/v1/repos".into(), &body)
                    .await
                {
                    Ok(_) => {
                        self.reread(vec![Msg::Done("ui_repos_ok_created".into())])
                            .await
                    }
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::SaveRepo { id, body } => {
                let path = format!("/api/v1/repos/{}", segment(&id));
                match self.send::<_, Repo>("PATCH", path, &body).await {
                    Ok(repo) => {
                        self.reread(vec![Msg::Saved {
                            owner: repo.owner,
                            name: repo.name,
                        }])
                        .await
                    }
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::DeleteRepo(id) => {
                match self
                    .call("DELETE", format!("/api/v1/repos/{}", segment(&id)), None)
                    .await
                {
                    Ok(_) => vec![Msg::Deleted],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Follow { target, on } => {
                let path = match &target {
                    Follow::Repo(id) => format!("/api/v1/repos/{}/subscription", segment(id)),
                    Follow::Project(id) => {
                        format!("/api/v1/projects/{}/subscription", segment(id))
                    }
                };
                match self
                    .call(if on { "PUT" } else { "DELETE" }, path, None)
                    .await
                {
                    Ok(_) => {
                        let key = if on {
                            "ui_follow_ok_followed"
                        } else {
                            "ui_follow_ok_unfollowed"
                        };
                        let mut out = Vec::new();
                        match self
                            .get::<Vec<Subscription>>("/api/v1/subscriptions".into())
                            .await
                        {
                            Ok(list) => out.push(Msg::Subscriptions(list)),
                            Err(failure) => return vec![failure.read()],
                        }
                        out.push(Msg::Done(key.into()));
                        out
                    }
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Launch { owner, name } => {
                let mut out = Vec::new();
                match self.optional::<Access>("/api/v1/access".into()).await {
                    Ok(Some(access)) => out.push(Msg::Access(access)),
                    Ok(None) => {}
                    Err(failure) => return vec![failure.read()],
                }
                let repos = match self.repos().await {
                    Ok(list) => list,
                    Err(failure) => return vec![failure.read()],
                };
                let found = Self::find(&repos, &owner, &name);
                out.push(Msg::Repos(repos));
                if let Some(repo) = found {
                    let path = format!("/api/v1/repos/{}/stages", segment(&repo.id));
                    match self.get::<Vec<KnownStage>>(path).await {
                        Ok(stages) => out.push(Msg::Stages {
                            repo: repo.id,
                            stages,
                        }),
                        Err(failure) => out.push(failure.read()),
                    }
                }
                out
            }
            Request::RunStages { repo, body } => {
                let path = format!("/api/v1/repos/{}/runs", segment(&repo));
                vec![match self.send::<_, Run>("POST", path, &body).await {
                    Ok(run) => Msg::Opened(run.id),
                    Err(failure) => failure.write(),
                }]
            }
            Request::Scan { owner, name } => self.scan(&owner, &name, None).await,
            Request::Check { owner, name, kind } => self.scan(&owner, &name, Some(&kind)).await,
            Request::Credentials => {
                let mut out = Vec::new();
                match self.projects().await {
                    Ok(list) => out.push(Msg::Projects(list)),
                    Err(failure) => return vec![failure.read()],
                }
                match self.repos().await {
                    Ok(list) => out.push(Msg::Repos(list)),
                    Err(failure) => return vec![failure.read()],
                }
                match self
                    .get::<Vec<Credential>>("/api/v1/credentials".into())
                    .await
                {
                    Ok(list) => out.push(Msg::Credentials(list)),
                    Err(failure) => return vec![failure.read()],
                }
                out
            }
        }
    }

    /// The scan of a repository and, for a check page of the security gate, the scanner's findings.
    async fn scan(&self, owner: &str, name: &str, kind: Option<&str>) -> Vec<Msg> {
        let repos = match self.repos().await {
            Ok(list) => list,
            Err(failure) => return vec![failure.read()],
        };
        let found = Self::find(&repos, owner, name);
        let mut out = vec![Msg::Repos(repos)];
        let Some(repo) = found else {
            return out;
        };
        let path = format!("/api/v1/repos/{}/scan", segment(&repo.id));
        match self.get::<ScanReport>(path).await {
            Ok(report) => out.push(Msg::Scan {
                repo: repo.id.clone(),
                report: Box::new(report),
            }),
            Err(failure) => {
                out.push(failure.read());
                return out;
            }
        }
        if kind == Some("security") {
            let path = format!("/api/v1/repos/{}/scan/findings", segment(&repo.id));
            match self.get::<ScanFindings>(path).await {
                Ok(found) => out.push(Msg::Findings {
                    repo: repo.id,
                    found: Some(Box::new(found)),
                }),
                Err(Failure::NotFound) => out.push(Msg::Findings {
                    repo: repo.id,
                    found: None,
                }),
                Err(failure) => out.push(failure.read()),
            }
        }
        out
    }
}
