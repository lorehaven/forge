//! Managing the git credential a project or a repository checks out with.
//!
//! There is no endpoint that returns a token, for the same reason `secrets`
//! has none: once written, a credential is only ever read by the checkout it
//! belongs to, so a stolen session can overwrite one but never read one back
//! out - which is visible, unlike a silent read.
//!
//! Project-scoped credentials live under `/projects/{id}/credentials` and
//! repo-scoped ones under `/repos/{id}/credentials` - registered into those
//! scopes by `projects::scope`/`repos::scope`, the same way `secrets` folds
//! its own repo routes into `repos::scope`.

use crate::credentials::store::{self, CredentialError, NewCredential, Scope};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, actor, json_error};
use crate::scheduler::repos;
use crate::secrets::crypto::CryptoError;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, put, web};
use quench_db::prelude::Db;
use serde::Deserialize;

/// The only kind `workspace::checkout` knows how to use today. Rejected here,
/// at write time, rather than let a caller store a kind that would silently
/// be ignored by every checkout that resolves it.
const HTTP_TOKEN: &str = "http_token";

#[derive(Deserialize)]
pub struct SetCredential {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub username: String,
    pub token: String,
}

fn default_kind() -> String {
    HTTP_TOKEN.to_string()
}

// ---------------------------------------------------------------------------
// Per project
// ---------------------------------------------------------------------------

#[put("/{project_id}/credentials")]
pub async fn put_project(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetCredential>,
    db: web::Data<Db>,
) -> impl Responder {
    let project_id = path.into_inner();
    if !can_on_project(&request, &db, &project_id, "write").await {
        return json_error(
            StatusCode::FORBIDDEN,
            "no write access to this project's credential",
        );
    }
    let actor = actor(&request).await;
    write(&db, Scope::Project(project_id), &body, &actor).await
}

#[get("/{project_id}/credentials")]
pub async fn show_project(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let project_id = path.into_inner();
    if !can_on_project(&request, &db, &project_id, "read").await {
        return json_error(
            StatusCode::FORBIDDEN,
            "no read access to this project's credential",
        );
    }
    show(&db, Scope::Project(project_id)).await
}

#[delete("/{project_id}/credentials")]
pub async fn delete_project(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let project_id = path.into_inner();
    if !can_on_project(&request, &db, &project_id, "write").await {
        return json_error(
            StatusCode::FORBIDDEN,
            "no write access to this project's credential",
        );
    }
    remove(&db, Scope::Project(project_id)).await
}

// ---------------------------------------------------------------------------
// Per repository
// ---------------------------------------------------------------------------

#[put("/{repo_id}/credentials")]
pub async fn put_repo(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetCredential>,
    db: web::Data<Db>,
) -> impl Responder {
    match repo_scope(&request, &db, &path, "write").await {
        Ok(scope) => {
            let actor = actor(&request).await;
            write(&db, scope, &body, &actor).await
        }
        Err(response) => response,
    }
}

#[get("/{repo_id}/credentials")]
pub async fn show_repo(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    match repo_scope(&request, &db, &path, "read").await {
        Ok(scope) => show(&db, scope).await,
        Err(response) => response,
    }
}

#[delete("/{repo_id}/credentials")]
pub async fn delete_repo(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    match repo_scope(&request, &db, &path, "write").await {
        Ok(scope) => remove(&db, scope).await,
        Err(response) => response,
    }
}

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

async fn repo_scope(
    request: &HttpRequest,
    db: &Db,
    repo_id: &str,
    action: &str,
) -> Result<Scope, HttpResponse> {
    match repos::read(db, repo_id).await {
        Ok(Some(repo)) => {
            if can_on_project(request, db, &repo.project_id, action).await {
                Ok(Scope::Repo(repo.id))
            } else {
                Err(json_error(
                    StatusCode::FORBIDDEN,
                    &format!("no {action} access to this repository's credential"),
                ))
            }
        }
        Ok(None) => Err(json_error(StatusCode::NOT_FOUND, "no such repository")),
        Err(error) => Err(ApiError::from(error).into_response()),
    }
}

async fn write(db: &Db, scope: Scope, body: &SetCredential, by: &str) -> HttpResponse {
    if body.kind != HTTP_TOKEN {
        return json_error(
            StatusCode::BAD_REQUEST,
            &format!(
                "unsupported credential kind '{}': only '{HTTP_TOKEN}' today",
                body.kind
            ),
        );
    }

    let key = match crate::secrets::crypto::SecretKey::from_env_named(store::KEY_VAR) {
        Ok(Some(key)) => key,
        Ok(None) => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                &CryptoError::NoKey {
                    var: store::KEY_VAR,
                }
                .to_string(),
            );
        }
        Err(error) => return json_error(StatusCode::SERVICE_UNAVAILABLE, &error.to_string()),
    };

    let new = NewCredential {
        name: &body.name,
        kind: &body.kind,
        username: &body.username,
        token: &body.token,
    };

    match store::put(db, &key, &scope, &new, by).await {
        Ok(credential) => HttpResponse::Ok().json(credential),
        Err(error) => credential_error(&error),
    }
}

async fn show(db: &Db, scope: Scope) -> HttpResponse {
    // `null`, not 404: having no credential yet is the ordinary case for most
    // projects and repositories, the same way `secrets::list` answers "none
    // set" with an empty 200 rather than an error.
    match store::show(db, &scope).await {
        Ok(credential) => HttpResponse::Ok().json(credential),
        Err(error) => credential_error(&error),
    }
}

async fn remove(db: &Db, scope: Scope) -> HttpResponse {
    match store::delete(db, &scope).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no credential set"),
        Err(error) => credential_error(&error),
    }
}

fn credential_error(error: &CredentialError) -> HttpResponse {
    let status = match error {
        CredentialError::BadName { .. }
        | CredentialError::BadMaterial
        | CredentialError::TooShort => StatusCode::BAD_REQUEST,
        // A missing or wrong key, or an unusable database, is the deployment's
        // problem rather than the caller's - and no retry of theirs fixes it.
        CredentialError::Crypto(_) => StatusCode::SERVICE_UNAVAILABLE,
        CredentialError::Queue(crate::scheduler::QueueError::NotPostgres) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        CredentialError::Queue(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };

    if status.is_server_error() {
        tracing::error!("credential store: {error}");
    }
    json_error(status, &error.to_string())
}
