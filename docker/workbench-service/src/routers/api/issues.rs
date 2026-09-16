//! Issues: create and list under a project, then read, edit, transition,
//! remove and label by their own id.

use crate::domain::issue::{self, IssueUpdate, NewIssue};
use crate::domain::label;
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, OptionalClaims, actor, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Query, Response, delete, get, http::StatusCode, post, put,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateIssue {
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub estimate: Option<i32>,
}

fn default_kind() -> String {
    "task".to_string()
}

fn default_priority() -> String {
    "medium".to_string()
}

#[post("/api/v1/projects/{id}/issues")]
pub async fn create(
    Path(project_id): Path<String>,
    Json(body): Json<CreateIssue>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if body.title.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "title is required"));
    }

    if !can_on_project(claims.as_ref(), &config, &project_id, "write") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no write access here"));
    }

    let new = NewIssue {
        project_id: project_id.clone(),
        parent_id: body.parent_id.clone(),
        kind: body.kind.clone(),
        title: body.title.trim().to_string(),
        description: body.description.clone(),
        priority: body.priority.clone(),
        assignee: body.assignee.clone(),
        reporter: actor(claims.as_ref()),
        estimate: body.estimate,
    };

    let issue = issue::create(&db, &new).await?;
    Ok(json_created(&issue))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// Board view when `status` is set; plain list view with it `None`.
#[get("/api/v1/projects/{id}/issues")]
pub async fn list(
    Path(project_id): Path<String>,
    Query(query): Query<ListQuery>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can_on_project(claims.as_ref(), &config, &project_id, "read") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no read access here"));
    }

    let issues = issue::list_by_project(&db, &project_id, query.status.as_deref()).await?;
    Ok(json_ok(&issues))
}

// ---------------------------------------------------------------------------
// By issue id
// ---------------------------------------------------------------------------

#[get("/api/v1/issues/{id}")]
pub async fn read(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(issue) = issue::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "read") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no read access to this issue",
        ));
    }

    Ok(json_ok(&issue))
}

#[derive(Deserialize)]
pub struct UpdateIssue {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub kind: String,
    pub priority: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub estimate: Option<i32>,
}

#[put("/api/v1/issues/{id}")]
pub async fn update(
    Path(id): Path<String>,
    Json(body): Json<UpdateIssue>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(issue) = issue::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if body.title.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "title cannot be empty"));
    }

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this issue",
        ));
    }

    let changes = IssueUpdate {
        title: body.title.trim().to_string(),
        description: body.description.clone(),
        kind: body.kind.clone(),
        priority: body.priority.clone(),
        assignee: body.assignee.clone(),
        estimate: body.estimate,
    };

    match issue::update(&db, &id, &changes).await? {
        Some(issue) => Ok(json_ok(&issue)),
        None => Ok(json_error(StatusCode::NOT_FOUND, "no such issue")),
    }
}

#[derive(Deserialize)]
pub struct Transition {
    pub status: String,
}

#[post("/api/v1/issues/{id}/transition")]
pub async fn transition(
    Path(id): Path<String>,
    Json(body): Json<Transition>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !issue::is_valid_status(&body.status) {
        return Ok(json_error(
            StatusCode::BAD_REQUEST,
            &format!(
                "unknown status '{}'; must be one of {:?}",
                body.status,
                issue::STATUSES
            ),
        ));
    }

    let Some(issue_row) = issue::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue_row.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this issue",
        ));
    }

    match issue::transition(&db, &id, &body.status).await? {
        Some(issue) => Ok(json_ok(&issue)),
        None => Ok(json_error(StatusCode::NOT_FOUND, "no such issue")),
    }
}

#[delete("/api/v1/issues/{id}")]
pub async fn remove(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(issue) = issue::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this issue",
        ));
    }

    match issue::delete(&db, &id).await? {
        true => Ok(Response::new(StatusCode::NO_CONTENT)),
        false => Ok(json_error(StatusCode::NOT_FOUND, "no such issue")),
    }
}

// ---------------------------------------------------------------------------
// Labels on an issue
// ---------------------------------------------------------------------------

#[get("/api/v1/issues/{id}/labels")]
pub async fn list_labels(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(issue) = issue::read(&db, &id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "read") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no read access to this issue",
        ));
    }

    let labels = label::list_for_issue(&db, &issue.id).await?;
    Ok(json_ok(&labels))
}

#[post("/api/v1/issues/{id}/labels/{label_id}")]
pub async fn attach_label(
    Path((issue_id, label_id)): Path<(String, String)>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(issue) = issue::read(&db, &issue_id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this issue",
        ));
    }

    label::attach(&db, &issue_id, &label_id).await?;
    Ok(Response::new(StatusCode::NO_CONTENT))
}

#[delete("/api/v1/issues/{id}/labels/{label_id}")]
pub async fn detach_label(
    Path((issue_id, label_id)): Path<(String, String)>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(issue) = issue::read(&db, &issue_id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such issue"));
    };

    if !can_on_project(claims.as_ref(), &config, &issue.project_id, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to this issue",
        ));
    }

    label::detach(&db, &issue_id, &label_id).await?;
    Ok(Response::new(StatusCode::NO_CONTENT))
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
    let _ = list as fn(_, _, _, _, _) -> _;
    let _ = read as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _, _) -> _;
    let _ = transition as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
    let _ = list_labels as fn(_, _, _, _) -> _;
    let _ = attach_label as fn(_, _, _, _) -> _;
    let _ = detach_label as fn(_, _, _, _) -> _;
}
