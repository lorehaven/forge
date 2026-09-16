//! Comments: create/list under an issue, delete by their own id.

use crate::domain::comment::{self, NewComment};
use crate::domain::issue;
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, OptionalClaims, actor, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Json, Path, Response, delete, get, http::StatusCode, post};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateComment {
    pub body: String,
}

#[post("/api/v1/issues/{id}/comments")]
pub async fn create(
    Path(issue_id): Path<String>,
    Json(body): Json<CreateComment>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if body.body.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "body is required"));
    }

    let Some(issue) = issue::read(&db, &issue_id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this issue",
        ));
    }

    let new = NewComment {
        issue_id: issue.id,
        author: actor(claims.as_ref()),
        body: body.body.trim().to_string(),
    };

    let comment = comment::create(&db, &new).await?;
    Ok(json_created(&comment))
}

#[get("/api/v1/issues/{id}/comments")]
pub async fn list(
    Path(issue_id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(issue) = issue::read(&db, &issue_id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "read") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no read access to this issue",
        ));
    }

    let comments = comment::list_by_issue(&db, &issue.id).await?;
    Ok(json_ok(&comments))
}

#[delete("/api/v1/comments/{id}")]
pub async fn remove(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(comment) = comment::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such comment"));
    };

    let Some(issue) = issue::read(&db, &comment.issue_id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this comment",
        ));
    }

    match comment::delete(&db, &id).await? {
        true => Ok(Response::new(StatusCode::NO_CONTENT)),
        false => Ok(json_error(StatusCode::NOT_FOUND, "no such comment")),
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
