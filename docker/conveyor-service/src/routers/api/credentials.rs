//! Managing the git credential a project/repo checks out with - no endpoint returns a token (same
//! reasoning as `secrets`); a stolen session can overwrite one but never read it back.

use crate::credentials::store::{self, CredentialError, NewCredential, Scope};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{Actor, ApiError, OptionalClaims, json_error};
use crate::scheduler::repos;
use crate::secrets::crypto::CryptoError;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Json, Path, Response, delete, get, http::StatusCode, put};
use serde::Deserialize;

/// The only kind `workspace::checkout` knows how to use today; rejected at write time, not silently ignored later.
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

// --- Per project ---

#[put("/api/v1/projects/{project_id}/credentials")]
pub async fn put_project(
    Actor(actor): Actor,
    OptionalClaims(claims): OptionalClaims,
    Path(project_id): Path<String>,
    Json(body): Json<SetCredential>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_on_project(claims.as_ref(), &config, &db, &project_id, "write").await {
        return json_error(
            StatusCode::FORBIDDEN,
            "no write access to this project's credential",
        );
    }
    write(&db, Scope::Project(project_id), &body, &actor).await
}

#[get("/api/v1/projects/{project_id}/credentials")]
pub async fn show_project(
    OptionalClaims(claims): OptionalClaims,
    Path(project_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_on_project(claims.as_ref(), &config, &db, &project_id, "read").await {
        return json_error(
            StatusCode::FORBIDDEN,
            "no read access to this project's credential",
        );
    }
    show(&db, Scope::Project(project_id)).await
}

#[delete("/api/v1/projects/{project_id}/credentials")]
pub async fn delete_project(
    OptionalClaims(claims): OptionalClaims,
    Path(project_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_on_project(claims.as_ref(), &config, &db, &project_id, "write").await {
        return json_error(
            StatusCode::FORBIDDEN,
            "no write access to this project's credential",
        );
    }
    remove(&db, Scope::Project(project_id)).await
}

// --- Per repository ---

#[put("/api/v1/repos/{repo_id}/credentials")]
pub async fn put_repo(
    Actor(actor): Actor,
    OptionalClaims(claims): OptionalClaims,
    Path(repo_id): Path<String>,
    Json(body): Json<SetCredential>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match repo_scope(claims.as_ref(), &config, &db, &repo_id, "write").await {
        Ok(scope) => write(&db, scope, &body, &actor).await,
        Err(response) => response,
    }
}

#[get("/api/v1/repos/{repo_id}/credentials")]
pub async fn show_repo(
    OptionalClaims(claims): OptionalClaims,
    Path(repo_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match repo_scope(claims.as_ref(), &config, &db, &repo_id, "read").await {
        Ok(scope) => show(&db, scope).await,
        Err(response) => response,
    }
}

#[delete("/api/v1/repos/{repo_id}/credentials")]
pub async fn delete_repo(
    OptionalClaims(claims): OptionalClaims,
    Path(repo_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match repo_scope(claims.as_ref(), &config, &db, &repo_id, "write").await {
        Ok(scope) => remove(&db, scope).await,
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
                    &format!("no {action} access to this repository's credential"),
                ))
            }
        }
        Ok(None) => Err(json_error(StatusCode::NOT_FOUND, "no such repository")),
        Err(error) => Err(ApiError::from(error).into_response()),
    }
}

async fn write(db: &Db, scope: Scope, body: &SetCredential, by: &str) -> Response {
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
        Ok(credential) => Response::json(StatusCode::OK, &credential)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => credential_error(&error),
    }
}

async fn show(db: &Db, scope: Scope) -> Response {
    // `null`, not 404 - no credential yet is the ordinary case, like `secrets::list`'s empty 200.
    match store::show(db, &scope).await {
        Ok(credential) => Response::json(StatusCode::OK, &credential)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => credential_error(&error),
    }
}

async fn remove(db: &Db, scope: Scope) -> Response {
    match store::delete(db, &scope).await {
        Ok(true) => Response::new(StatusCode::NO_CONTENT),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no credential set"),
        Err(error) => credential_error(&error),
    }
}

fn credential_error(error: &CredentialError) -> Response {
    let status = match error {
        CredentialError::BadName { .. }
        | CredentialError::BadMaterial
        | CredentialError::TooShort => StatusCode::BAD_REQUEST,
        // The deployment's problem, not the caller's - no retry fixes it.
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

pub fn register_routes() {
    let _ = put_project as fn(_, _, _, _, _, _) -> _;
    let _ = show_project as fn(_, _, _, _) -> _;
    let _ = delete_project as fn(_, _, _, _) -> _;
    let _ = put_repo as fn(_, _, _, _, _, _) -> _;
    let _ = show_repo as fn(_, _, _, _) -> _;
    let _ = delete_repo as fn(_, _, _, _) -> _;
}
