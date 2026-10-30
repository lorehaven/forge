//! The calls to the services from a page: the same operations the programs do over their blocking client, over
//! whatever sends a request (the browser's `fetch`, or a test's stand-in).

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;

use daybook_core::Data;
use daybook_host::{Op, Reply};
use daybook_net::NetError;
use daybook_net::api::{self, API, segment};

/// Sign-in goes through the page's own server (`daybook-service` passes it on to gatehouse), so the page only ever calls
/// the address it came from.
const LOGIN: &str = "/daybook/auth/login";
const REFRESH: &str = "/daybook/auth/refresh";
use quench_serde::{Map, Value, json};

/// A request to send. `bearer` is the access token when the call needs one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the server (`/daybook/api/v1/tasks`).
    pub path: String,
    pub bearer: Option<String>,
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

/// Where the refresh token is kept between visits.
pub trait Keep {
    fn load(&self) -> Option<String>;
    fn save(&self, token: Option<&str>);
}

/// A signed-in (or signed-out) page's session and calls.
pub struct Client<T: Transport, K: Keep> {
    transport: T,
    keep: K,
    access: RefCell<String>,
    refresh: RefCell<String>,
}

impl<T: Transport, K: Keep> Client<T, K> {
    /// With the refresh token the last visit kept, if there is one.
    pub fn new(transport: T, keep: K) -> Self {
        let refresh = keep.load().unwrap_or_default();
        Self {
            transport,
            keep,
            access: RefCell::default(),
            refresh: RefCell::new(refresh),
        }
    }

    #[must_use]
    pub fn signed_in(&self) -> bool {
        !self.refresh.borrow().is_empty()
    }

    async fn post(&self, path: &str, body: &Value) -> Result<HttpResponse, NetError> {
        self.transport
            .send(HttpRequest {
                method: "POST",
                path: path.to_string(),
                bearer: None,
                body: Some(json::to_string(body)),
            })
            .await
            .map_err(NetError::Network)
    }

    /// Takes the tokens of a sign-in or a refresh.
    fn adopt(&self, response: &HttpResponse) -> Result<(), NetError> {
        if response.status != 200 {
            return Err(api::sign_in_error(response.status, &response.body));
        }
        let value: Value =
            json::from_str(&response.body).map_err(|e| NetError::Network(e.to_string()))?;
        let (Some(access), Some(refresh)) = (
            value.get("access_token").and_then(Value::as_str),
            value.get("refresh_token").and_then(Value::as_str),
        ) else {
            return Err(NetError::Network("the sign-in answer has no tokens".into()));
        };
        *self.access.borrow_mut() = access.to_string();
        *self.refresh.borrow_mut() = refresh.to_string();
        self.keep.save(Some(refresh));
        Ok(())
    }

    async fn login(&self, username: &str, password: &str) -> Result<(), NetError> {
        let mut body = Map::new();
        body.insert("username", Value::String(username.into()));
        body.insert("password", Value::String(password.into()));
        let response = self.post(LOGIN, &Value::Object(body)).await?;
        self.adopt(&response)
    }

    async fn renew(&self) -> Result<(), NetError> {
        let spent = self.refresh.borrow().clone();
        if spent.is_empty() {
            return Err(NetError::Unauthorized);
        }
        let mut body = Map::new();
        body.insert("refresh_token", Value::String(spent));
        let response = self.post(REFRESH, &Value::Object(body)).await?;
        let result = self.adopt(&response);
        if result == Err(NetError::Unauthorized) {
            self.sign_out();
        }
        result
    }

    /// Forgets the session, here and in the storage.
    pub fn sign_out(&self) {
        self.access.borrow_mut().clear();
        self.refresh.borrow_mut().clear();
        self.keep.save(None);
    }

    /// A call to the service with the access token; a refused token is renewed once.
    async fn call(
        &self,
        method: &'static str,
        path: String,
        body: Option<String>,
    ) -> Result<HttpResponse, NetError> {
        let path = format!("{API}{path}");
        for attempt in 0..2 {
            if self.access.borrow().is_empty() && attempt == 0 {
                self.renew().await?;
            }
            let bearer = Some(self.access.borrow().clone());
            let response = self
                .transport
                .send(HttpRequest {
                    method,
                    path: path.clone(),
                    bearer,
                    body: body.clone(),
                })
                .await
                .map_err(NetError::Network)?;
            if response.status == 401 && attempt == 0 {
                self.renew().await?;
                continue;
            }
            return Ok(response);
        }
        Err(NetError::Unauthorized)
    }

    async fn read(&self, path: &str) -> Result<Value, NetError> {
        let response = self.call("GET", path.to_string(), None).await?;
        daybook_net::check_status(response.status)?;
        json::from_str(&response.body).map_err(|e| NetError::Network(e.to_string()))
    }

    async fn send(
        &self,
        method: &'static str,
        path: String,
        body: Option<&Value>,
    ) -> Result<Value, NetError> {
        let response = self.call(method, path, body.map(json::to_string)).await?;
        daybook_net::check_status(response.status)?;
        Ok(json::from_str(&response.body).unwrap_or(Value::Null))
    }

    /// Does one operation.
    ///
    /// # Errors
    /// [`NetError`].
    pub async fn execute(&self, op: Op) -> Result<Reply, NetError> {
        let missing = |what: &str| NetError::Network(format!("unreadable {what}"));
        Ok(match op {
            Op::Login {
                username, password, ..
            } => {
                self.login(&username, &password).await?;
                Reply::Done
            }
            Op::Logout => {
                self.sign_out();
                Reply::Done
            }
            Op::Load => {
                let projects = api::projects(&self.read("/projects").await?);
                let labels = api::labels(&self.read("/labels").await?);
                let tasks = api::tasks(&self.read("/tasks?all=true&completed=false").await?);
                Reply::Loaded(Box::new(Data {
                    projects,
                    labels,
                    tasks,
                }))
            }
            Op::CreateTask(body) => Reply::Task(Box::new(
                daybook_core::Task::from_value(
                    &self.send("POST", "/tasks".into(), Some(&body)).await?,
                )
                .ok_or_else(|| missing("task"))?,
            )),
            Op::UpdateTask(id, body) => Reply::Task(Box::new(
                daybook_core::Task::from_value(
                    &self
                        .send("PUT", format!("/tasks/{}", segment(&id)), Some(&body))
                        .await?,
                )
                .ok_or_else(|| missing("task"))?,
            )),
            Op::DeleteTask(id) => {
                self.send("DELETE", format!("/tasks/{}", segment(&id)), None)
                    .await?;
                Reply::Done
            }
            Op::CreateProject { name, sort_order } => Reply::Project(
                daybook_core::Project::from_value(
                    &self
                        .send(
                            "POST",
                            "/projects".into(),
                            Some(&api::project_body(&name, sort_order)),
                        )
                        .await?,
                )
                .ok_or_else(|| missing("list"))?,
            ),
            Op::RenameProject {
                id,
                name,
                sort_order,
            } => Reply::Project(
                daybook_core::Project::from_value(
                    &self
                        .send(
                            "PUT",
                            format!("/projects/{}", segment(&id)),
                            Some(&api::project_body(&name, sort_order)),
                        )
                        .await?,
                )
                .ok_or_else(|| missing("list"))?,
            ),
            Op::DeleteProject(id) => {
                self.send("DELETE", format!("/projects/{}", segment(&id)), None)
                    .await?;
                Reply::Done
            }
            Op::CreateLabel { name, color } => Reply::Label(
                daybook_core::Label::from_value(
                    &self
                        .send(
                            "POST",
                            "/labels".into(),
                            Some(&api::label_body(&name, &color)),
                        )
                        .await?,
                )
                .ok_or_else(|| missing("label"))?,
            ),
            Op::UpdateLabel { id, name, color } => Reply::Label(
                daybook_core::Label::from_value(
                    &self
                        .send(
                            "PUT",
                            format!("/labels/{}", segment(&id)),
                            Some(&api::label_body(&name, &color)),
                        )
                        .await?,
                )
                .ok_or_else(|| missing("label"))?,
            ),
            Op::DeleteLabel(id) => {
                self.send("DELETE", format!("/labels/{}", segment(&id)), None)
                    .await?;
                Reply::Done
            }
        })
    }
}
