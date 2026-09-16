//! Projects: create, browse, edit and remove the flat container issues live in.

use crate::domain::project::{self, ProjectUpdate};
use crate::routers::api::authz::{can_on_project, can_unscoped, granted_project_ids};
use crate::routers::api::{ApiError, OptionalClaims, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Response, delete, get, http::StatusCode, post, put,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateProject {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[post("/api/v1/projects")]
pub async fn create(
    Json(body): Json<CreateProject>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if body.key.trim().is_empty() || body.name.trim().is_empty() {
        return Ok(json_error(
            StatusCode::BAD_REQUEST,
            "key and name are required",
        ));
    }

    // No project id yet to scope a grant against - only unscoped write works.
    if !can_unscoped(claims.as_ref(), &config, "write") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no write access here"));
    }

    let new = project::NewProject {
        key: body.key.trim().to_string(),
        name: body.name.trim().to_string(),
        description: body.description.clone(),
    };

    let project = project::create(&db, &new).await?;
    Ok(json_created(&project))
}

/// Filters to readable projects rather than 403ing when there's no blanket grant.
#[get("/api/v1/projects")]
pub async fn list(
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
) -> Result<Response, ApiError> {
    let all = project::list(&db).await?;

    let Some(claims) = claims else {
        // Auth disabled: no identity to scope by, so nothing is filtered.
        return Ok(json_ok(&all));
    };

    if claims.can("workbench", "read") {
        return Ok(json_ok(&all));
    }

    let granted = granted_project_ids(&claims, "read");
    let projects: Vec<_> = all
        .into_iter()
        .filter(|project| granted.contains(&project.id))
        .collect();
    Ok(json_ok(&projects))
}

#[get("/api/v1/projects/{id}")]
pub async fn read(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can_on_project(claims.as_ref(), &config, &id, "read") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no read access here"));
    }

    match project::read(&db, &id).await? {
        Some(project) => Ok(json_ok(&project)),
        None => Ok(json_error(StatusCode::NOT_FOUND, "no such project")),
    }
}

#[derive(Deserialize)]
pub struct UpdateProject {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[put("/api/v1/projects/{id}")]
pub async fn update(
    Path(id): Path<String>,
    Json(body): Json<UpdateProject>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if body.name.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "name cannot be empty"));
    }

    if !can_on_project(claims.as_ref(), &config, &id, "write") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no write access here"));
    }

    let changes = ProjectUpdate {
        name: body.name.trim().to_string(),
        description: body.description.clone(),
    };

    match project::update(&db, &id, &changes).await? {
        Some(project) => Ok(json_ok(&project)),
        None => Ok(json_error(StatusCode::NOT_FOUND, "no such project")),
    }
}

#[delete("/api/v1/projects/{id}")]
pub async fn remove(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can_on_project(claims.as_ref(), &config, &id, "write") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no write access here"));
    }

    match project::delete(&db, &id).await? {
        true => Ok(Response::new(StatusCode::NO_CONTENT)),
        false => Ok(json_error(StatusCode::NOT_FOUND, "no such project")),
    }
}

fn json_ok<T: serde::Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn json_created<T: serde::Serialize>(value: &T) -> Response {
    Response::json(StatusCode::CREATED, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = create as fn(_, _, _, _) -> _;
    let _ = list as fn(_, _) -> _;
    let _ = read as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
}
