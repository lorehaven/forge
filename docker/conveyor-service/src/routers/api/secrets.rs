//! Managing what conveyor holds on a pipeline's behalf.
//!
//! There is no endpoint that returns a value. Once written, a secret is only
//! ever read by a job that named it - so a stolen session cannot be used to
//! read the estate's tokens back out, only to overwrite them, which is visible.
//!
//! Repository secrets live under `/repos/{id}/secrets` and estate-wide ones
//! under `/secrets`. Two shapes rather than one `/{scope}/secrets` because a
//! wildcard first segment would collide with the `/repos` scope, and actix does
//! not fall through from a scope whose prefix matched.

use crate::routers::api::{ApiError, actor, json_error};
use crate::scheduler::repos;
use crate::secrets::store::{self, Scope, SecretError};
use crate::secrets::{CryptoError, SecretKey};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, put, web};
use quench_db::prelude::Db;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SetSecret {
    pub value: String,
}

// ---------------------------------------------------------------------------
// Estate-wide
// ---------------------------------------------------------------------------

#[put("/{name}")]
pub async fn put_global(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetSecret>,
    db: web::Data<Db>,
) -> impl Responder {
    write(&db, Scope::Global, &path, &body.value, &actor(&request)).await
}

#[get("")]
pub async fn list_global(db: web::Data<Db>) -> impl Responder {
    read_names(&db, Scope::Global).await
}

#[delete("/{name}")]
pub async fn delete_global(path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    remove(&db, Scope::Global, &path).await
}

pub fn scope() -> actix_web::Scope {
    web::scope("/secrets")
        .service(list_global)
        .service(put_global)
        .service(delete_global)
}

// ---------------------------------------------------------------------------
// Per repository
// ---------------------------------------------------------------------------
//
// Registered inside the `/repos` scope by `repos::scope`.

#[put("/{repo_id}/secrets/{name}")]
pub async fn put_repo(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<SetSecret>,
    db: web::Data<Db>,
) -> impl Responder {
    let (repo_id, name) = path.into_inner();
    match repo_scope(&db, &repo_id).await {
        Ok(scope) => write(&db, scope, &name, &body.value, &actor(&request)).await,
        Err(response) => response,
    }
}

#[get("/{repo_id}/secrets")]
pub async fn list_repo(path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    match repo_scope(&db, &path).await {
        Ok(scope) => read_names(&db, scope).await,
        Err(response) => response,
    }
}

#[delete("/{repo_id}/secrets/{name}")]
pub async fn delete_repo(path: web::Path<(String, String)>, db: web::Data<Db>) -> impl Responder {
    let (repo_id, name) = path.into_inner();
    match repo_scope(&db, &repo_id).await {
        Ok(scope) => remove(&db, scope, &name).await,
        Err(response) => response,
    }
}

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

async fn repo_scope(db: &Db, repo_id: &str) -> Result<Scope, HttpResponse> {
    match repos::read(db, repo_id).await {
        Ok(Some(repo)) => Ok(Scope::Repo(repo.id)),
        Ok(None) => Err(json_error(StatusCode::NOT_FOUND, "no such repository")),
        Err(error) => Err(ApiError::from(error).into_response()),
    }
}

async fn write(db: &Db, scope: Scope, name: &str, value: &str, by: &str) -> HttpResponse {
    let key = match SecretKey::from_env() {
        Ok(Some(key)) => key,
        Ok(None) => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                &CryptoError::NoKey.to_string(),
            );
        }
        Err(error) => return json_error(StatusCode::SERVICE_UNAVAILABLE, &error.to_string()),
    };

    match store::put(db, &key, &scope, name, value, by).await {
        Ok(secret) => HttpResponse::Ok().json(secret),
        Err(error) => secret_error(&error),
    }
}

async fn read_names(db: &Db, scope: Scope) -> HttpResponse {
    match store::list(db, &scope).await {
        Ok(secrets) => HttpResponse::Ok().json(secrets),
        Err(error) => secret_error(&error),
    }
}

async fn remove(db: &Db, scope: Scope, name: &str) -> HttpResponse {
    match store::delete(db, &scope, name).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no such secret"),
        Err(error) => secret_error(&error),
    }
}

fn secret_error(error: &SecretError) -> HttpResponse {
    let status = match error {
        SecretError::BadName { .. } | SecretError::TooShort => StatusCode::BAD_REQUEST,
        SecretError::Missing { .. } => StatusCode::NOT_FOUND,
        // A missing or wrong key, or an unusable database, is the deployment's
        // problem rather than the caller's - and no retry of theirs fixes it.
        SecretError::Crypto(_) => StatusCode::SERVICE_UNAVAILABLE,
        SecretError::Queue(crate::scheduler::QueueError::NotPostgres) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        SecretError::Queue(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };

    if status.is_server_error() {
        tracing::error!("secret store: {error}");
    }
    json_error(status, &error.to_string())
}
