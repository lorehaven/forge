//! The app's requests as calls to the API, and the answers as messages. Over whatever sends a request (the browser's `fetch`,
//! or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use quench_serde::json;
use quench_serde::{Deserialize, Serialize};
use sage_app::{Msg, Request};
use sage_core::{
    ConversationItem, ConversationView, Created, EditBody, FileInfo, ModelsView, NewProject,
    Project, RegenerateBody, Resumed, SwitchBody, TurnStarted,
};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/api/v1/models`); the transport adds the prefix.
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

/// The service's path prefix from the page's own path: `/sage/ui/home` is `/sage`, `/ui/home` is no prefix.
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
            Self::NotFound => Msg::Failed("the service has nothing there".into()),
            Self::Other(403, _) => Msg::Failed("this account may not see that".into()),
            Self::Other(status, _) => Msg::Failed(format!("the service answered {status}")),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }

    /// A change that did not work: the service's own words when it gave some (it answers with an `api_error_*` code, which the
    /// app translates).
    fn write(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::NotFound => Msg::Rejected("it is gone: someone removed it".into()),
            Self::Other(403, _) => Msg::Rejected("this account may not do that".into()),
            Self::Other(status, body) => Msg::Rejected(if looks_like_code(&body) {
                body.trim().to_string()
            } else {
                format!("the service answered {status}")
            }),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }
}

/// The service's refusals are one `api_error_*` code.
fn looks_like_code(body: &str) -> bool {
    let body = body.trim();
    body.starts_with("api_error_") && body.len() < 80 && !body.contains(char::is_whitespace)
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

    fn parse<R: Deserialize>(body: &str) -> Result<R, Failure> {
        json::from_str(body).map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    async fn get<R: Deserialize>(&self, path: String) -> Result<R, Failure> {
        Self::parse(&self.call("GET", path, None).await?)
    }

    async fn send<B: Serialize, R: Deserialize>(
        &self,
        method: &'static str,
        path: String,
        body: &B,
    ) -> Result<R, Failure> {
        let text = self.call(method, path, Some(json::to_string(body))).await?;
        Self::parse(&text)
    }

    async fn conversation(&self, id: &str) -> Result<ConversationView, Failure> {
        self.get(format!("/api/v1/conversations/{id}")).await
    }

    /// The conversation as a message, a missing one being a new one.
    async fn conversation_msg(&self, id: &str) -> Msg {
        match self.conversation(id).await {
            Ok(view) => Msg::Conversation(Box::new(view)),
            Err(Failure::NotFound) => Msg::NoConversation(id.to_string()),
            Err(failure) => failure.read(),
        }
    }

    /// The turn that answers a conversation ending with the person's message, if there is one.
    async fn resume(&self, id: &str) -> Result<Option<Msg>, Failure> {
        let text = self
            .call("POST", format!("/api/v1/conversations/{id}/resume"), None)
            .await?;
        let resumed: Resumed = Self::parse(&text)?;
        Ok(resumed.id.map(Msg::TurnStarted))
    }

    async fn start(&self, path: &str, body: String) -> Msg {
        match self.call("POST", path.to_string(), Some(body)).await {
            Ok(text) => match Self::parse::<TurnStarted>(&text) {
                Ok(started) => Msg::TurnStarted(started.id),
                Err(failure) => failure.write(),
            },
            Err(failure) => failure.write(),
        }
    }

    /// What the app asks for, as the messages the service's answers make. Streaming the answer and the language are the browser's
    /// own business.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Language(_) | Request::StopStream => vec![],
            Request::Models => match self.get::<ModelsView>("/api/v1/models".into()).await {
                Ok(view) => vec![Msg::Models(view)],
                Err(failure) => vec![failure.read()],
            },
            Request::Sidebar => {
                let mut out = Vec::new();
                match self.get::<Vec<Project>>("/api/v1/projects".into()).await {
                    Ok(list) => out.push(Msg::Projects(list)),
                    Err(failure) => return vec![failure.read()],
                }
                match self
                    .get::<Vec<ConversationItem>>("/api/v1/conversations".into())
                    .await
                {
                    Ok(list) => out.push(Msg::Conversations(list)),
                    Err(failure) => return vec![failure.read()],
                }
                out
            }
            Request::ProjectFiles(project_id) => {
                match self
                    .get::<Vec<FileInfo>>(format!("/api/v1/files?project_id={project_id}"))
                    .await
                {
                    Ok(files) => vec![Msg::ProjectFiles { project_id, files }],
                    // A project that is not the person's has no files to show.
                    Err(Failure::NotFound | Failure::Other(403, _)) => vec![Msg::ProjectFiles {
                        project_id,
                        files: Vec::new(),
                    }],
                    Err(failure) => vec![failure.read()],
                }
            }
            Request::NewId => match self.get::<Created>("/api/v1/conversation-id".into()).await {
                Ok(created) => vec![Msg::NewId(created.id)],
                Err(failure) => vec![failure.read()],
            },
            Request::Conversation(id) => vec![self.conversation_msg(&id).await],
            Request::Send(body) => vec![self.start("/api/v1/turns", json::to_string(&body)).await],
            Request::Regenerate(message_id) => vec![
                self.start(
                    "/api/v1/turns/regenerate",
                    json::to_string(&RegenerateBody { message_id }),
                )
                .await,
            ],
            Request::Resume(id) => match self.resume(&id).await {
                Ok(Some(started)) => vec![started],
                Ok(None) => vec![],
                Err(failure) => vec![failure.write()],
            },
            Request::Edit {
                conversation_id,
                message_id,
                text,
            } => {
                let edited: Result<Created, Failure> = self
                    .send(
                        "POST",
                        format!("/api/v1/messages/{message_id}/edit"),
                        &EditBody { new_content: text },
                    )
                    .await;
                if let Err(failure) = edited {
                    return vec![failure.write()];
                }
                // The edited message is the tip now: show it, and have Sage answer it.
                let mut out = vec![self.conversation_msg(&conversation_id).await];
                match self.resume(&conversation_id).await {
                    Ok(Some(started)) => out.push(started),
                    Ok(None) => {}
                    Err(failure) => out.push(failure.write()),
                }
                out
            }
            Request::Switch {
                conversation_id,
                target,
            } => {
                let switched = self
                    .call(
                        "POST",
                        format!("/api/v1/conversations/{conversation_id}/switch"),
                        Some(json::to_string(&SwitchBody {
                            target_message_id: target,
                        })),
                    )
                    .await;
                match switched {
                    Ok(_) => vec![self.conversation_msg(&conversation_id).await],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::DeleteConversation(id) => {
                match self
                    .call("DELETE", format!("/api/v1/conversations/{id}"), None)
                    .await
                {
                    Ok(_) | Err(Failure::NotFound) => {
                        let mut out = vec![Msg::ConversationDeleted(id)];
                        if let Ok(list) = self
                            .get::<Vec<ConversationItem>>("/api/v1/conversations".into())
                            .await
                        {
                            out.push(Msg::Conversations(list));
                        }
                        out
                    }
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::DeleteFile(id) => {
                match self
                    .call("DELETE", format!("/api/v1/files/{id}"), None)
                    .await
                {
                    Ok(_) | Err(Failure::NotFound) => vec![Msg::FileGone(id)],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::CreateProject(body) => {
                match self
                    .send::<NewProject, Project>("POST", "/api/v1/projects".into(), &body)
                    .await
                {
                    Ok(project) => vec![Msg::ProjectCreated(project)],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::FileStatus(id) => {
                match self.get::<FileInfo>(format!("/api/v1/files/{id}")).await {
                    Ok(file) => vec![Msg::File(file)],
                    Err(Failure::NotFound) => vec![Msg::FileGone(id)],
                    Err(failure) => vec![failure.read()],
                }
            }
            Request::Reprocess(id) => {
                match self
                    .call("POST", format!("/api/v1/files/{id}/reprocess"), None)
                    .await
                {
                    // Accepted: the file is being processed again (the answer still shows the old state).
                    Ok(text) => match Self::parse::<FileInfo>(&text) {
                        Ok(mut file) => {
                            file.status = "processing".to_string();
                            file.error_message = None;
                            vec![Msg::File(file)]
                        }
                        Err(failure) => vec![failure.write()],
                    },
                    // Already processing, or an image that needs none: read where it stands.
                    Err(Failure::Other(409 | 422, _)) => {
                        match self.get::<FileInfo>(format!("/api/v1/files/{id}")).await {
                            Ok(file) => vec![Msg::File(file)],
                            Err(failure) => vec![failure.read()],
                        }
                    }
                    Err(failure) => vec![failure.write()],
                }
            }
        }
    }
}
