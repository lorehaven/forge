//! Labels: create/list under a project, delete by their own id.

use crate::domain::label::{self, NewLabel};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, OptionalClaims, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Json, Path, Response, delete, get, http::StatusCode, post};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateLabel {
    pub name: String,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "#888888".to_string()
}

#[post("/api/v1/projects/{id}/labels")]
pub async fn create(
    Path(project_id): Path<String>,
    Json(body): Json<CreateLabel>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if body.name.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "name is required"));
    }

    if !can_on_project(claims.as_ref(), &config, &project_id, "write") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no write access here"));
    }

    let new = NewLabel {
        project_id: project_id.clone(),
        name: body.name.trim().to_string(),
        color: body.color.clone(),
    };

    let label = label::create(&db, &new).await?;
    Ok(json_created(&label))
}

#[get("/api/v1/projects/{id}/labels")]
pub async fn list(
    Path(project_id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can_on_project(claims.as_ref(), &config, &project_id, "read") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no read access here"));
    }

    let labels = label::list_by_project(&db, &project_id).await?;
    Ok(json_ok(&labels))
}

#[delete("/api/v1/labels/{id}")]
pub async fn remove(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(label) = label::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such label"));
    };

    if !can_on_project(claims.as_ref(), &config, &label.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this label",
        ));
    }

    match label::delete(&db, &id).await? {
        true => Ok(Response::new(StatusCode::NO_CONTENT)),
        false => Ok(json_error(StatusCode::NOT_FOUND, "no such label")),
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
    let _ = create as fn(_, _, _, _, _) -> _;
    let _ = list as fn(_, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
}
