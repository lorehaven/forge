//! Share links of the management UI: making one, listing them, withdrawing one.
//!
//! Making a link is publishing, so it needs write access to the storage (an owner has it, a fixed storage needs the grant for
//! it). Everyone sees their own links; whoever may write everywhere sees all of them and can withdraw any. Opening a link is
//! a different route (`/share/{token}`, [`crate::routers::share`]) and needs no sign-in.

use super::{Session, error, internal, no_content, ok};
use crate::domain::share::{self, Share};
use crate::domain::{storage, storage_file};
use crate::routers::files::authz::{can_on_static_claims, can_on_storage_claims};
use crate::routers::files::{confined, relative, resolve};
use quench_auth::domain::jwt::Claims;
use quench_core::async_trait;
use quench_db::prelude::Db;
use quench_http::prelude::{FromRequest, HttpError, Request};
use quench_http::prelude::{Inject, Json, Path, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use warehouse_core::{NewShare, ShareView};

/// Where this service is reached from outside, for the links it hands out.
///
/// `PUBLIC_URL` if set, otherwise what the request says (`X-Forwarded-Proto` and `X-Forwarded-Host`, then `Host`) plus the
/// service's own path prefix. Empty when neither is known; the link is then only the part after the host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicBase(pub String);

impl PublicBase {
    /// The pieces put together. `configured` wins; `host` is `host[:port]`.
    #[must_use]
    pub fn from_parts(
        configured: &str,
        proto: Option<&str>,
        host: Option<&str>,
        base_path: &str,
    ) -> Self {
        let configured = configured.trim().trim_end_matches('/');
        if !configured.is_empty() {
            return Self(configured.to_string());
        }
        let prefix = base_path.trim().trim_matches('/');
        let prefix = if prefix.is_empty() {
            String::new()
        } else {
            format!("/{prefix}")
        };
        let host = host.map(str::trim).filter(|host| !host.is_empty());
        match host {
            // A comma list is what a chain of proxies makes of a header; the first is the one that faced the browser.
            Some(host) => {
                let host = host.split(',').next().unwrap_or(host).trim();
                let proto = proto
                    .and_then(|proto| proto.split(',').next())
                    .map_or("https", str::trim);
                let proto = if proto == "http" { "http" } else { "https" };
                Self(format!("{proto}://{host}{prefix}"))
            }
            None => Self(prefix),
        }
    }

    /// The whole link to a token.
    #[must_use]
    pub fn link(&self, token: &str) -> String {
        format!("{}/share/{token}", self.0)
    }
}

#[async_trait]
impl FromRequest for PublicBase {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let header = |name: &str| req.header(name).map(str::to_string);
        Ok(Self::from_parts(
            &quench_core::env::get_or("PUBLIC_URL", ""),
            header("x-forwarded-proto").as_deref(),
            header("x-forwarded-host")
                .or_else(|| header("host"))
                .as_deref(),
            &quench_core::env::get_or("BASE_PATH", ""),
        ))
    }
}

fn view(share: Share, base: &PublicBase) -> ShareView {
    ShareView {
        url: base.link(&share.token),
        id: share.id,
        token: share.token,
        storage: share.storage,
        path: share.path,
        is_dir: share.is_dir,
        created_by: share.created_by,
        created: share.created_at.format("%Y-%m-%d %H:%M UTC"),
    }
}

/// Whoever may write everywhere sees and withdraws every link.
fn sees_all(claims: &Claims) -> bool {
    claims.can("warehouse", "write")
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Only the links of this storage.
    #[serde(default)]
    pub storage: Option<String>,
}

/// The caller's links, newest first; everyone's for a caller who may write everywhere.
#[get("/ui/api/files/shares")]
pub async fn list(
    session: Session,
    base: PublicBase,
    Query(query): Query<ListQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    let claims = match session.require() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let by = (!sees_all(&claims)).then_some(claims.sub.as_str());
    match share::list(&db, by).await {
        Ok(shares) => ok(&shares
            .into_iter()
            .filter(|share| {
                query
                    .storage
                    .as_deref()
                    .is_none_or(|storage| share.storage == storage)
            })
            .map(|share| view(share, &base))
            .collect::<Vec<_>>()),
        Err(problem) => {
            quench_log::error!("UI share list failed: {problem}");
            internal()
        }
    }
}

/// Makes the link for a file or directory of a storage, or returns the one this person already made for it.
#[post("/ui/api/files/shares")]
pub async fn create(
    session: Session,
    base: PublicBase,
    Inject(db): Inject<Db>,
    Json(body): Json<NewShare>,
) -> Response {
    let claims = match session.require() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !crate::routers::files_enabled() {
        return error(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }
    // The same path rules as everywhere; empty is the storage itself.
    let path = body.path.trim_matches('/').to_string();
    if !path.is_empty() && relative(&path).is_err() {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_path");
    }
    let path = relative(&path).map_or(String::new(), |p| p.to_string_lossy().into_owned());

    let is_dir = match storage::read(&db, &body.storage).await {
        Ok(Some(found)) => {
            if !can_on_storage_claims(&claims, &found, "write") {
                return error(StatusCode::FORBIDDEN, "api_error_forbidden");
            }
            match dynamic_kind(&db, &found.name, &path).await {
                Ok(Some(is_dir)) => is_dir,
                Ok(None) => return error(StatusCode::NOT_FOUND, "api_error_file_not_found"),
                Err(response) => return response,
            }
        }
        Ok(None) => {
            let Some(found) = crate::routers::files::storage(&body.storage) else {
                return error(StatusCode::NOT_FOUND, "api_error_share_storage_missing");
            };
            if !can_on_static_claims(&claims, &found.name, "write") {
                return error(StatusCode::FORBIDDEN, "api_error_forbidden");
            }
            match static_kind(found, &path).await {
                Some(is_dir) => is_dir,
                None => return error(StatusCode::NOT_FOUND, "api_error_file_not_found"),
            }
        }
        Err(problem) => {
            quench_log::error!("UI share storage lookup failed: {problem}");
            return internal();
        }
    };

    match share::create(&db, &body.storage, &path, is_dir, &claims.sub).await {
        Ok(share) => ok(&view(share, &base)),
        Err(problem) => {
            quench_log::error!("UI share create failed: {problem}");
            internal()
        }
    }
}

/// `Some(true)` for a directory, `Some(false)` for a file, `None` for nothing.
async fn static_kind(storage: &crate::routers::files::Storage, path: &str) -> Option<bool> {
    if path.is_empty() {
        return Some(true);
    }
    let target = resolve(storage, path).ok()?;
    if !confined(&storage.root, &target).await {
        return None;
    }
    let metadata = tokio::fs::metadata(&target).await.ok()?;
    if metadata.is_file() {
        Some(false)
    } else {
        metadata.is_dir().then_some(true)
    }
}

/// A file if a row has exactly this path, a directory if any row lies below it.
async fn dynamic_kind(db: &Db, storage: &str, path: &str) -> Result<Option<bool>, Response> {
    if path.is_empty() {
        return Ok(Some(true));
    }
    match storage_file::read_file(db, storage, path).await {
        Ok(Some(_)) => return Ok(Some(false)),
        Ok(None) => {}
        Err(problem) => {
            quench_log::error!("UI share lookup failed: {problem}");
            return Err(internal());
        }
    }
    match storage_file::children(db, storage, path, 1).await {
        Ok(children) => Ok((!children.is_empty()).then_some(true)),
        Err(problem) => {
            quench_log::error!("UI share lookup failed: {problem}");
            Err(internal())
        }
    }
}

/// Withdraws a link: its creator may, and so may whoever writes everywhere. The link stops working at once.
#[post("/ui/api/files/shares/{id}/delete")]
pub async fn delete(session: Session, Inject(db): Inject<Db>, Path(id): Path<String>) -> Response {
    let claims = match session.require() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let found = match share::read(&db, &id).await {
        Ok(Some(found)) => found,
        Ok(None) => return error(StatusCode::NOT_FOUND, "api_error_share_not_found"),
        Err(problem) => {
            quench_log::error!("UI share lookup failed: {problem}");
            return internal();
        }
    };
    if found.created_by != claims.sub && !sees_all(&claims) {
        return error(StatusCode::FORBIDDEN, "api_error_forbidden");
    }
    match share::delete(&db, &id).await {
        Ok(true) => no_content(),
        Ok(false) => error(StatusCode::NOT_FOUND, "api_error_share_not_found"),
        Err(problem) => {
            quench_log::error!("UI share delete failed: {problem}");
            internal()
        }
    }
}

pub fn register_routes() {
    let _ = list as fn(_, _, _, _) -> _;
    let _ = create as fn(_, _, _, _) -> _;
    let _ = delete as fn(_, _, _) -> _;
}
