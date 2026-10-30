//! The app's requests as calls to the API, and the answers as messages. Over whatever sends a request (the browser's
//! `fetch`, or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use gantry_app::{Msg, Request};
use gantry_core::format::encode;
use gantry_core::{
    Busy, DeploymentPlan, DeploymentView, Group, Info, Operation, PackageBody, PlanRequest,
    ResourceRef, ResourceYaml, StoredPlan, TargetView,
};
use quench_serde::json;
use quench_serde::{Deserialize, Serialize};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/api/v1/resources`); the transport adds the prefix.
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
    encode(s)
}

/// The service's path prefix from the page's own path: `/gantry/ui/home` is `/gantry`, `/ui/home` is no prefix.
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
    /// A read that did not work: the service's own words when it gave some (a cluster or a registry that cannot be
    /// reached is its 502, and the person should read why).
    fn read(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::NotFound => Msg::NotFound,
            Self::Other(status, body) => Msg::Failed(
                error_text(&body).unwrap_or_else(|| format!("the service answered {status}")),
            ),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }

    /// A change that did not work: the service's own words when it gave some.
    fn write(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::NotFound => Msg::Rejected("it is gone: there is no such thing".into()),
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

/// Just the id of what the service created.
#[derive(Deserialize)]
struct Created {
    id: String,
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

    /// A change that queues an operation: the app opens it.
    fn started(result: Result<Created, Failure>) -> Vec<Msg> {
        vec![match result {
            Ok(created) => Msg::Started(created.id),
            Err(failure) => failure.write(),
        }]
    }

    /// Plan a change and run the plan at once: what a start, a stop or a sync did before there were plans to look at. The
    /// service checks what the caller may do when the plan is confirmed.
    async fn plan_and_run<B: Serialize>(&self, plan_path: String, body: &B) -> Vec<Msg> {
        let stored: Created = match self.send("POST", plan_path, body).await {
            Ok(stored) => stored,
            Err(failure) => return vec![failure.write()],
        };
        let path = format!("/api/v1/plans/{}/confirm", segment(&stored.id));
        Self::started(self.send("POST", path, &quench_serde::json!({})).await)
    }

    async fn groups(&self) -> Result<Vec<Group>, Failure> {
        self.get("/api/v1/resources".into()).await
    }

    async fn operation(&self, id: &str) -> Msg {
        match self
            .get::<Operation>(format!("/api/v1/operations/{}", segment(id)))
            .await
        {
            Ok(operation) => Msg::Operation(Box::new(operation)),
            Err(failure) => failure.read(),
        }
    }

    /// What the host tells the app about a request. `Language` is the browser's own business.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Language(_) => vec![],
            Request::Startup => match self.get::<Info>("/api/v1/info".into()).await {
                Ok(info) => vec![Msg::Info(info)],
                Err(Failure::SignedOut) => vec![Msg::SignedOut],
                Err(_) => vec![],
            },
            Request::Packages => {
                let groups = match self.groups().await {
                    Ok(groups) => Msg::Groups(groups),
                    Err(failure) => return vec![failure.read()],
                };
                // The change in progress is a hint; not being allowed to see the history is not an error here.
                let busy = self
                    .get::<Vec<Operation>>("/api/v1/operations?limit=10".into())
                    .await
                    .ok()
                    .and_then(|list| list.into_iter().find(|op| !op.is_terminal()))
                    .map(|op| Busy {
                        id: op.id,
                        title: op.title,
                    });
                vec![groups, Msg::Busy(busy)]
            }
            Request::Package(name) => {
                let groups = match self.groups().await {
                    Ok(groups) => groups,
                    Err(failure) => return vec![failure.read()],
                };
                let missing = !groups.iter().any(|g| g.package == name);
                let mut out = vec![Msg::Groups(groups)];
                if missing {
                    out.push(Msg::NotFound);
                    return out;
                }
                // The deployments are a panel on the page: without them the page still shows its resources.
                let deployments = self
                    .get::<Vec<DeploymentView>>("/api/v1/deployments".into())
                    .await
                    .unwrap_or_default();
                out.push(Msg::Deployments(deployments));
                out
            }
            Request::Target(name) => {
                vec![match self
                    .get::<TargetView>(format!("/api/v1/targets/{}", segment(&name)))
                    .await
                {
                    Ok(target) => Msg::Target(Box::new(target)),
                    Err(failure) => failure.read(),
                }]
            }
            Request::Operations => {
                vec![match self
                    .get::<Vec<Operation>>("/api/v1/operations?limit=100".into())
                    .await
                {
                    Ok(list) => Msg::Operations(list),
                    Err(failure) => failure.read(),
                }]
            }
            Request::Operation(id) => vec![self.operation(&id).await],
            Request::Plan(id) => {
                vec![match self
                    .get::<StoredPlan>(format!("/api/v1/plans/{}", segment(&id)))
                    .await
                {
                    Ok(plan) => Msg::Plan(Box::new(plan)),
                    Err(failure) => failure.read(),
                }]
            }
            Request::Yaml(resource) => {
                let path = format!("/api/v1/resources/yaml?{}", yaml_query(&resource));
                vec![match self.get::<ResourceYaml>(path).await {
                    Ok(yaml) => Msg::Yaml(Box::new(yaml)),
                    Err(failure) => failure.read(),
                }]
            }
            Request::Start(name) => {
                self.plan_and_run(
                    format!("/api/v1/deployments/{}/plan", segment(&name)),
                    &DeploymentPlan {
                        action: "start".into(),
                        stop: vec![],
                    },
                )
                .await
            }
            Request::Stop(name) => {
                self.plan_and_run(
                    format!("/api/v1/deployments/{}/plan", segment(&name)),
                    &DeploymentPlan {
                        action: "stop".into(),
                        stop: vec![],
                    },
                )
                .await
            }
            Request::Sync { package, version } => {
                self.plan_and_run(
                    format!("/api/v1/targets/{}/plan", segment(&package)),
                    &PlanRequest { version },
                )
                .await
            }
            Request::ApplyResource(resource) => Self::started(
                self.send("POST", "/api/v1/resources/apply".into(), &resource.body())
                    .await,
            ),
            Request::ApplyMissing(package) => Self::started(
                self.send(
                    "POST",
                    "/api/v1/resources/apply".into(),
                    &PackageBody { package },
                )
                .await,
            ),
            Request::Refresh(package) => Self::started(
                self.send(
                    "POST",
                    "/api/v1/resources/refresh".into(),
                    &PackageBody { package },
                )
                .await,
            ),
            Request::DeleteResource(resource) => Self::started(
                self.send("POST", "/api/v1/resources/delete".into(), &resource.body())
                    .await,
            ),
            Request::Edit { resource, yaml } => Self::started(
                self.send(
                    "POST",
                    "/api/v1/resources/edit".into(),
                    &resource.edit_body(yaml),
                )
                .await,
            ),
            Request::Cancel(id) => {
                let path = format!("/api/v1/operations/{}/cancel", segment(&id));
                match self
                    .send::<_, quench_serde::Value>("POST", path, &quench_serde::json!({}))
                    .await
                {
                    // The cancel answers with the bare operation: the page wants its steps and log too.
                    Ok(_) => vec![self.operation(&id).await, Msg::Done("cancelling".into())],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Confirm(id) => {
                let path = format!("/api/v1/plans/{}/confirm", segment(&id));
                Self::started(self.send("POST", path, &quench_serde::json!({})).await)
            }
        }
    }
}

/// `package=ml&kind=ConfigMap&name=...&namespace=...&apiVersion=v1`: the API's names, percent-encoded.
fn yaml_query(resource: &ResourceRef) -> String {
    [
        ("package", &resource.package),
        ("apiVersion", &resource.api_version),
        ("kind", &resource.kind),
        ("namespace", &resource.namespace),
        ("name", &resource.name),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| format!("{key}={}", encode(value)))
    .collect::<Vec<_>>()
    .join("&")
}
