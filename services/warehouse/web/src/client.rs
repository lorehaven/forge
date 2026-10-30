//! The app's requests as calls to the service's management API (`/ui/api`), and the answers as messages. Over whatever sends
//! a request (the browser's `fetch`, or a test's stand-in).

use std::future::Future;
use std::pin::Pin;

use quench_serde::json;
use quench_serde::{Deserialize, Serialize};
use warehouse_app::{Msg, Request};
use warehouse_core::fmt::encode;
use warehouse_core::tree::parent_dir;
use warehouse_core::{
    ArtifactEntry, ArtifactRef, Browse, CrateRef, CrateVersion, DockerTag, DynamicStorage, FileRef,
    ImageRef, Overview, RivetEntry, RivetRef, Storages,
};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/ui/api/overview`); the transport adds the prefix.
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

/// The service's path prefix from the page's own path: `/warehouse/ui/home` is `/warehouse`, `/ui/home` is no prefix.
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
                "this account may not see Warehouse: ask for access in gatehouse".into(),
            ),
            Self::Other(status, _) => Msg::Failed(format!("the service answered {status}")),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }

    /// A change that did not work: the service's own words when it gave some (an `api_error_*` key the app translates).
    fn write(self) -> Msg {
        match self {
            Self::SignedOut => Msg::SignedOut,
            Self::Other(status, body) => Msg::Rejected(
                error_text(&body).unwrap_or_else(|| format!("the service answered {status}")),
            ),
            Self::Offline(why) => Msg::Failed(format!("cannot reach the service: {why}")),
        }
    }

    /// Whether the service said the thing asked for is not there.
    const fn is_missing(&self) -> bool {
        matches!(self, Self::Other(400 | 404, _))
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
            status => Err(Failure::Other(status, response.body)),
        }
    }

    async fn get<R: Deserialize>(&self, path: String) -> Result<R, Failure> {
        let body = self.call("GET", path, None).await?;
        json::from_str(&body)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    /// A POST whose answer does not matter.
    async fn post<B: Serialize>(&self, path: String, body: &B) -> Result<(), Failure> {
        self.call("POST", path, Some(json::to_string(body)))
            .await
            .map(|_| ())
    }

    /// A POST that answers the storage it changed.
    async fn post_storage<B: Serialize>(
        &self,
        path: String,
        body: &B,
    ) -> Result<DynamicStorage, Failure> {
        let text = self.call("POST", path, Some(json::to_string(body))).await?;
        json::from_str(&text)
            .map_err(|e| Failure::Offline(format!("the answer is unreadable: {e}")))
    }

    async fn tags(&self, repo: &str) -> Msg {
        match self
            .get::<Vec<DockerTag>>(format!("/ui/api/docker/tags?repo={}", segment(repo)))
            .await
        {
            Ok(tags) => Msg::DockerTags {
                repo: repo.to_string(),
                tags,
            },
            // A repository that is not there has no tags.
            Err(failure) if failure.is_missing() => Msg::DockerTags {
                repo: repo.to_string(),
                tags: vec![],
            },
            Err(failure) => failure.read(),
        }
    }

    async fn versions(&self, name: &str) -> Msg {
        match self
            .get::<Vec<CrateVersion>>(format!("/ui/api/crates/{}/versions", segment(name)))
            .await
        {
            Ok(versions) => Msg::CrateVersions {
                name: name.to_string(),
                versions,
            },
            Err(failure) if failure.is_missing() => Msg::CrateVersions {
                name: name.to_string(),
                versions: vec![],
            },
            Err(failure) => failure.read(),
        }
    }

    async fn artifacts(&self) -> Msg {
        match self
            .get::<Vec<ArtifactEntry>>("/ui/api/artifacts".into())
            .await
        {
            Ok(list) => Msg::Artifacts(list),
            Err(failure) => failure.read(),
        }
    }

    async fn rivets(&self) -> Msg {
        match self.get::<Vec<RivetEntry>>("/ui/api/rivets".into()).await {
            Ok(list) => Msg::Rivets(list),
            Err(failure) => failure.read(),
        }
    }

    async fn storages(&self) -> Result<Msg, Failure> {
        self.get::<Storages>("/ui/api/files/storages".into())
            .await
            .map(Msg::Storages)
    }

    async fn browse(&self, storage: &str) -> Msg {
        match self
            .get::<Browse>(format!("/ui/api/files/browse?storage={}", segment(storage)))
            .await
        {
            Ok(browse) => Msg::Browse(Box::new(browse)),
            Err(failure) => failure.read(),
        }
    }

    /// The storages again, then `after` (a storage was changed).
    async fn storages_then(&self, after: Msg) -> Vec<Msg> {
        match self.storages().await {
            Ok(list) => vec![list, after],
            Err(failure) => vec![failure.write()],
        }
    }

    /// What the host tells the app about a request. `Language` is the browser's own business.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(&self, request: Request) -> Vec<Msg> {
        match request {
            Request::Language(_) => vec![],
            Request::Startup => vec![
                match self.get::<Overview>("/ui/api/overview".into()).await {
                    Ok(overview) => Msg::Overview(overview),
                    Err(failure) => failure.read(),
                },
            ],
            Request::DockerRepos => vec![match self
                .get::<Vec<String>>("/ui/api/docker/repositories".into())
                .await
            {
                Ok(list) => Msg::DockerRepos(list),
                Err(failure) => failure.read(),
            }],
            Request::DockerTags(repo) => vec![self.tags(&repo).await],
            Request::DeleteImage { repository, digest } => {
                let body = ImageRef {
                    repository: repository.clone(),
                    digest,
                };
                match self.post("/ui/api/docker/delete-image".into(), &body).await {
                    Ok(()) => vec![self.tags(&repository).await, Msg::ImageDeleted(repository)],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Crates => vec![
                match self.get::<Vec<String>>("/ui/api/crates".into()).await {
                    Ok(list) => Msg::Crates(list),
                    Err(failure) => failure.read(),
                },
            ],
            Request::CrateVersions(name) => vec![self.versions(&name).await],
            Request::SetCrateYank {
                name,
                version,
                yanked,
            } => {
                let path = if yanked { "yank" } else { "unyank" };
                let body = CrateRef {
                    name: name.clone(),
                    version,
                };
                match self.post(format!("/ui/api/crates/{path}"), &body).await {
                    Ok(()) => vec![self.versions(&name).await],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Artifacts => vec![self.artifacts().await],
            Request::SetArtifactYank {
                program,
                platform,
                version_code,
                yanked,
            } => {
                let path = if yanked { "yank" } else { "unyank" };
                let body = ArtifactRef {
                    program,
                    platform,
                    version_code,
                };
                match self.post(format!("/ui/api/artifacts/{path}"), &body).await {
                    Ok(()) => vec![self.artifacts().await],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Rivets => vec![self.rivets().await],
            Request::SetRivetYank {
                name,
                version,
                yanked,
            } => {
                let path = if yanked { "yank" } else { "unyank" };
                let body = RivetRef { name, version };
                match self.post(format!("/ui/api/rivets/{path}"), &body).await {
                    Ok(()) => vec![self.rivets().await],
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Storages => vec![match self.storages().await {
                Ok(list) => list,
                Err(failure) => failure.read(),
            }],
            Request::CreateStorage(body) => {
                match self
                    .post_storage("/ui/api/files/storages".into(), &body)
                    .await
                {
                    Ok(created) => self.storages_then(Msg::StorageSaved(created.name)).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::EditStorage { name, changes } => {
                let path = format!("/ui/api/files/storages/{}/edit", segment(&name));
                match self.post_storage(path, &changes).await {
                    Ok(saved) => self.storages_then(Msg::StorageSaved(saved.name)).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::DeleteStorage(name) => {
                let path = format!("/ui/api/files/storages/{}/delete", segment(&name));
                match self.call("POST", path, None).await {
                    Ok(_) => self.storages_then(Msg::StorageDeleted).await,
                    Err(failure) => vec![failure.write()],
                }
            }
            Request::Browse(storage) => vec![self.browse(&storage).await],
            Request::DeleteFile { storage, path } => {
                let parent = parent_dir(&path);
                let body = FileRef {
                    storage: storage.clone(),
                    path,
                };
                match self.post("/ui/api/files/delete-file".into(), &body).await {
                    Ok(()) => vec![
                        self.browse(&storage).await,
                        Msg::FileDeleted { storage, parent },
                    ],
                    Err(failure) => vec![failure.write()],
                }
            }
        }
    }
}
