//! Managing what conveyor holds on a pipeline's behalf - write-only, since a
//! stolen session should overwrite a secret (visible) rather than read it back.

use crate::routers::api::authz::{can_on_project, can_unscoped};
use crate::routers::api::{Actor, ApiError, OptionalClaims, json_error};
use crate::scheduler::repos;
use crate::secrets::store::{self, Scope, SecretError};
use crate::secrets::{CryptoError, SecretKey};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Json, Path, Response, delete, get, http::StatusCode, put};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SetSecret {
    pub value: String,
}

// --- Estate-wide - gated by the blanket grant only; no project to scope by. ---

#[put("/api/v1/secrets/{name}")]
pub async fn put_global(
    Actor(actor): Actor,
    OptionalClaims(claims): OptionalClaims,
    Path(name): Path<String>,
    Json(body): Json<SetSecret>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_unscoped(claims.as_ref(), &config, "write") {
        return json_error(
            StatusCode::FORBIDDEN,
            "no write access to estate-wide secrets",
        );
    }
    write(&db, Scope::Global, &name, &body.value, &actor).await
}

#[get("/api/v1/secrets")]
pub async fn list_global(
    OptionalClaims(claims): OptionalClaims,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_unscoped(claims.as_ref(), &config, "read") {
        return json_error(
            StatusCode::FORBIDDEN,
            "no read access to estate-wide secrets",
        );
    }
    read_names(&db, Scope::Global).await
}

#[delete("/api/v1/secrets/{name}")]
pub async fn delete_global(
    OptionalClaims(claims): OptionalClaims,
    Path(name): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_unscoped(claims.as_ref(), &config, "write") {
        return json_error(
            StatusCode::FORBIDDEN,
            "no write access to estate-wide secrets",
        );
    }
    remove(&db, Scope::Global, &name).await
}

// --- Per repository ---

#[put("/api/v1/repos/{repo_id}/secrets/{name}")]
pub async fn put_repo(
    Actor(actor): Actor,
    OptionalClaims(claims): OptionalClaims,
    Path((repo_id, name)): Path<(String, String)>,
    Json(body): Json<SetSecret>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match repo_scope(claims.as_ref(), &config, &db, &repo_id, "write").await {
        Ok(scope) => write(&db, scope, &name, &body.value, &actor).await,
        Err(response) => response,
    }
}

#[get("/api/v1/repos/{repo_id}/secrets")]
pub async fn list_repo(
    OptionalClaims(claims): OptionalClaims,
    Path(repo_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match repo_scope(claims.as_ref(), &config, &db, &repo_id, "read").await {
        Ok(scope) => read_names(&db, scope).await,
        Err(response) => response,
    }
}

#[delete("/api/v1/repos/{repo_id}/secrets/{name}")]
pub async fn delete_repo(
    OptionalClaims(claims): OptionalClaims,
    Path((repo_id, name)): Path<(String, String)>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match repo_scope(claims.as_ref(), &config, &db, &repo_id, "write").await {
        Ok(scope) => remove(&db, scope, &name).await,
        Err(response) => response,
    }
}

// --- Shared ---

async fn repo_scope(
    claims: Option<&quench_auth::domain::jwt::Claims>,
    config: &JwtConfig,
    db: &Db,
    repo_id: &str,
    action: &str,
) -> Result<Scope, Response> {
    match repos::read(db, repo_id).await {
        Ok(Some(repo)) => {
            if can_on_project(claims, config, db, &repo.project_id, action).await {
                Ok(Scope::Repo(repo.id))
            } else {
                Err(json_error(
                    StatusCode::FORBIDDEN,
                    &format!("no {action} access to this repository's secrets"),
                ))
            }
        }
        Ok(None) => Err(json_error(StatusCode::NOT_FOUND, "no such repository")),
        Err(error) => Err(ApiError::from(error).into_response()),
    }
}

async fn write(db: &Db, scope: Scope, name: &str, value: &str, by: &str) -> Response {
    let key = match SecretKey::from_env() {
        Ok(Some(key)) => key,
        Ok(None) => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                &CryptoError::NoKey {
                    var: "CONVEYOR_SECRET_KEY",
                }
                .to_string(),
            );
        }
        Err(error) => return json_error(StatusCode::SERVICE_UNAVAILABLE, &error.to_string()),
    };

    match store::put(db, &key, &scope, name, value, by).await {
        Ok(secret) => Response::json(StatusCode::OK, &secret)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => secret_error(&error),
    }
}

async fn read_names(db: &Db, scope: Scope) -> Response {
    match store::list(db, &scope).await {
        Ok(secrets) => Response::json(StatusCode::OK, &secrets)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => secret_error(&error),
    }
}

async fn remove(db: &Db, scope: Scope, name: &str) -> Response {
    match store::delete(db, &scope, name).await {
        Ok(true) => Response::new(StatusCode::NO_CONTENT),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no such secret"),
        Err(error) => secret_error(&error),
    }
}

fn secret_error(error: &SecretError) -> Response {
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

pub fn register_routes() {
    let _ = put_global as fn(_, _, _, _, _, _) -> _;
    let _ = list_global as fn(_, _, _) -> _;
    let _ = delete_global as fn(_, _, _, _) -> _;
    let _ = put_repo as fn(_, _, _, _, _, _) -> _;
    let _ = list_repo as fn(_, _, _, _) -> _;
    let _ = delete_repo as fn(_, _, _, _) -> _;
}
