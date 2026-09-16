//! Typed links between issues: create/list under the owning issue, then
//! remove by the link's own id - mirrors `comments.rs`'s shape.

use crate::domain::issue;
use crate::domain::issue_link::{self, NewIssueLink};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, OptionalClaims, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Json, Path, Response, delete, get, http::StatusCode, post};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateIssueLink {
    pub linked_issue_id: String,
    pub kind: String,
}

#[post("/api/v1/issues/{id}/links")]
pub async fn create(
    Path(issue_id): Path<String>,
    Json(body): Json<CreateIssueLink>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !issue_link::is_valid_kind(&body.kind) {
        return Ok(json_error(
            StatusCode::BAD_REQUEST,
            &format!(
                "unknown kind '{}'; must be one of {:?}",
                body.kind,
                issue_link::KINDS
            ),
        ));
    }

    if body.linked_issue_id == issue_id {
        return Ok(json_error(
            StatusCode::BAD_REQUEST,
            "an issue cannot link to itself",
        ));
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

    let new = NewIssueLink {
        issue_id: issue.id,
        linked_issue_id: body.linked_issue_id.clone(),
        kind: body.kind.clone(),
    };

    let link = issue_link::create(&db, &new).await?;
    Ok(json_created(&link))
}

/// The three lists a detail page renders, resolved to key/title/status.
#[get("/api/v1/issues/{id}/links")]
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

    let related = issue_link::related(&db, &issue.id).await?;
    Ok(json_ok(&related))
}

#[delete("/api/v1/issue-links/{id}")]
pub async fn remove(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(link) = issue_link::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such link"));
    };

    let Some(issue) = issue::read(&db, &link.issue_id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this link",
        ));
    }

    match issue_link::delete(&db, &id).await? {
        true => Ok(Response::new(StatusCode::NO_CONTENT)),
        false => Ok(json_error(StatusCode::NOT_FOUND, "no such link")),
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
