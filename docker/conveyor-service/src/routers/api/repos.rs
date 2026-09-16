//! Registering the repositories conveyor is willing to build - explicit and deliberate.

use crate::domain::Provider;
use crate::routers::api::authz::{can_on_project, granted_project_ids};
use crate::routers::api::{Actor, ApiError, OptionalClaims, json_error};
use crate::scheduler::projects;
use crate::scheduler::repos::{self, NewRepo, RepoUpdate};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Response, delete, get, http::StatusCode, patch, post,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct RegisterRepo {
    /// `github` or `generic`. Defaults to `github`.
    #[serde(default)]
    pub provider: Option<String>,
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    #[serde(default)]
    pub default_branch: Option<String>,
    /// The project node this repo attaches to.
    pub project_id: String,
}

#[post("/api/v1/repos")]
pub async fn register(
    Actor(actor): Actor,
    OptionalClaims(claims): OptionalClaims,
    Json(body): Json<RegisterRepo>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let provider = match body.provider.as_deref() {
        None => Provider::GitHub,
        Some(raw) => match Provider::parse(raw) {
            Some(provider) => provider,
            None => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    &format!("unknown provider '{raw}'"),
                );
            }
        },
    };

    if body.owner.trim().is_empty() || body.name.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "owner and name are required");
    }

    if body.project_id.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "project_id is required");
    }

    // Checked here, not at checkout time - a url git would read as an option should never be stored.
    if let Err(error) = crate::workspace::checkout::validate_url(&body.clone_url) {
        return json_error(StatusCode::BAD_REQUEST, &error.to_string());
    }

    if !can_on_project(claims.as_ref(), &config, &db, &body.project_id, "write").await {
        return json_error(StatusCode::FORBIDDEN, "no write access to that project");
    }

    let new = NewRepo {
        provider,
        owner: body.owner.trim().to_string(),
        name: body.name.trim().to_string(),
        clone_url: body.clone_url.trim().to_string(),
        default_branch: body
            .default_branch
            .clone()
            .unwrap_or_else(|| "master".to_string()),
        registered_by: actor,
        project_id: body.project_id.clone(),
    };

    match repos::create(&db, &new).await {
        Ok(repo) => Response::json(StatusCode::CREATED, &repo)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Filters to what the caller may read rather than 403ing - like a directory listing shows what you can see.
#[get("/api/v1/repos")]
pub async fn list(OptionalClaims(claims): OptionalClaims, Inject(db): Inject<Db>) -> Response {
    let all = match repos::list(&db).await {
        Ok(repos) => repos,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let Some(claims) = claims else {
        // Auth disabled: no identity to scope by, so nothing is filtered.
        return Response::json(StatusCode::OK, &all)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    };

    if claims.can("conveyor", "read") {
        return Response::json(StatusCode::OK, &all)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    }

    let granted = granted_project_ids(&claims, "read");
    let visible = match projects::descendant_ids(&db, &granted).await {
        Ok(ids) => ids,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let repos: Vec<_> = all
        .into_iter()
        .filter(|repo| visible.contains(&repo.project_id))
        .collect();
    Response::json(StatusCode::OK, &repos)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/api/v1/repos/{id}")]
pub async fn read(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let repo = match repos::read(&db, &id).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(claims.as_ref(), &config, &db, &repo.project_id, "read").await {
        return json_error(StatusCode::FORBIDDEN, "no read access to this repository");
    }

    Response::json(StatusCode::OK, &repo)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[derive(Deserialize)]
pub struct UpdateRepo {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub clone_url: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// A partial update - absent fields are left alone. `provider` is not editable; it identifies the repo kind, not a property.
#[patch("/api/v1/repos/{id}")]
pub async fn update(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Json(body): Json<UpdateRepo>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let repo = match repos::read(&db, &id).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if body
        .owner
        .as_deref()
        .is_some_and(|owner| owner.trim().is_empty())
        || body
            .name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
    {
        return json_error(StatusCode::BAD_REQUEST, "owner and name cannot be empty");
    }

    if body
        .project_id
        .as_deref()
        .is_some_and(|project_id| project_id.trim().is_empty())
    {
        return json_error(StatusCode::BAD_REQUEST, "project_id cannot be empty");
    }

    if let Some(clone_url) = &body.clone_url
        && let Err(error) = crate::workspace::checkout::validate_url(clone_url)
    {
        return json_error(StatusCode::BAD_REQUEST, &error.to_string());
    }

    if !can_on_project(claims.as_ref(), &config, &db, &repo.project_id, "write").await {
        return json_error(StatusCode::FORBIDDEN, "no write access to this repository");
    }

    let target_project = body.project_id.as_deref().unwrap_or(&repo.project_id);

    // Moving to a different project needs write on both ends, like a project move does.
    if target_project != repo.project_id
        && !can_on_project(claims.as_ref(), &config, &db, target_project, "write").await
    {
        return json_error(
            StatusCode::FORBIDDEN,
            "no write access to the destination project",
        );
    }

    let changes = RepoUpdate {
        owner: body
            .owner
            .as_deref()
            .map_or_else(|| repo.owner.clone(), |owner| owner.trim().to_string()),
        name: body
            .name
            .as_deref()
            .map_or_else(|| repo.name.clone(), |name| name.trim().to_string()),
        clone_url: body
            .clone_url
            .as_deref()
            .map_or_else(|| repo.clone_url.clone(), |url| url.trim().to_string()),
        default_branch: body
            .default_branch
            .clone()
            .unwrap_or_else(|| repo.default_branch.clone()),
        project_id: target_project.to_string(),
        enabled: body.enabled.unwrap_or(repo.enabled),
    };

    match repos::update(&db, &id, &changes).await {
        Ok(Some(repo)) => Response::json(StatusCode::OK, &repo)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetEnabled {
    pub enabled: bool,
}

/// Turning a repository off keeps its history and stops it accepting triggers,
/// which is what you want for one that has gone bad rather than gone away.
#[post("/api/v1/repos/{id}/enabled")]
pub async fn set_enabled(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Json(body): Json<SetEnabled>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let repo = match repos::read(&db, &id).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(claims.as_ref(), &config, &db, &repo.project_id, "write").await {
        return json_error(StatusCode::FORBIDDEN, "no write access to this repository");
    }

    match repos::set_enabled(&db, &id, body.enabled).await {
        Ok(Some(repo)) => Response::json(StatusCode::OK, &repo)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/api/v1/repos/{id}")]
pub async fn remove(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let repo = match repos::read(&db, &id).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(claims.as_ref(), &config, &db, &repo.project_id, "write").await {
        return json_error(StatusCode::FORBIDDEN, "no write access to this repository");
    }

    match repos::delete(&db, &id).await {
        Ok(true) => Response::new(StatusCode::NO_CONTENT),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn register_routes() {
    let _ = register as fn(_, _, _, _, _) -> _;
    let _ = list as fn(_, _) -> _;
    let _ = read as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _, _) -> _;
    let _ = set_enabled as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
}
