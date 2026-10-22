use crate::domain::recurrence::Recurrence;
use crate::domain::task::{self, NewTask, TaskUpdate};
use crate::routers::api_error::{from_domain, json};
use crate::routers::authz::can;
use crate::routers::{OptionalClaims, actor};
use chrono::{DateTime, Utc};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Query, Response, delete, get, http::StatusCode, post, put,
};
use quench_starter::http::domain::api_error::{ApiError, json_error};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TaskInput {
    content: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    sort_order: i32,
    #[serde(default)]
    due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    remind_at: Option<DateTime<Utc>>,
    #[serde(default)]
    label_ids: Vec<String>,
    #[serde(default)]
    recurrence: Option<Recurrence>,
}

#[derive(Debug, Deserialize)]
pub struct TaskFilter {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    label_id: Option<String>,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    completed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TaskUpdateInput {
    content: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    is_completed: bool,
    #[serde(default)]
    sort_order: i32,
    #[serde(default)]
    due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    remind_at: Option<DateTime<Utc>>,
    #[serde(default)]
    label_ids: Vec<String>,
    #[serde(default)]
    recurrence: Option<Recurrence>,
}

/// Validates a task's repeat rule against its due date; `Err` is the 400 message. A rule needs a due
/// date because completing the task rolls that date forward - without one there is nothing to count from.
fn check_recurrence(
    recurrence: Option<Recurrence>,
    due_at: Option<DateTime<Utc>>,
) -> Result<Option<Recurrence>, &'static str> {
    let Some(recurrence) = recurrence else {
        return Ok(None);
    };
    if due_at.is_none() {
        return Err("a repeating task needs a due date");
    }
    recurrence.validated().map(Some)
}

#[get("/api/v1/tasks")]
pub async fn list(
    Query(filter): Query<TaskFilter>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can(claims.as_ref(), &config, "read") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no read access to daybook",
        ));
    }
    let user = actor(claims.as_ref());
    let tasks = task::list(
        &db,
        &user,
        filter.project_id.as_deref(),
        filter.label_id.as_deref(),
        filter.all,
        filter.completed,
    )
    .await
    .map_err(from_domain)?;
    Ok(json(StatusCode::OK, &tasks))
}

#[post("/api/v1/tasks")]
pub async fn create(
    Json(body): Json<TaskInput>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can(claims.as_ref(), &config, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to daybook",
        ));
    }
    let user = actor(claims.as_ref());
    if body.content.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "content is required"));
    }
    let recurrence = match check_recurrence(body.recurrence, body.due_at) {
        Ok(recurrence) => recurrence,
        Err(message) => return Ok(json_error(StatusCode::BAD_REQUEST, message)),
    };
    let input = NewTask {
        content: body.content.trim().to_string(),
        description: body.description,
        project_id: body.project_id,
        sort_order: body.sort_order,
        due_at: body.due_at,
        remind_at: body.remind_at,
        label_ids: body.label_ids,
        recurrence,
    };
    let task = task::create(&db, &user, &input)
        .await
        .map_err(from_domain)?;
    Ok(json(StatusCode::CREATED, &task))
}

#[put("/api/v1/tasks/{id}")]
pub async fn update(
    Path(id): Path<String>,
    Json(body): Json<TaskUpdateInput>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can(claims.as_ref(), &config, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to daybook",
        ));
    }
    let user = actor(claims.as_ref());
    if body.content.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "content is required"));
    }
    let recurrence = match check_recurrence(body.recurrence, body.due_at) {
        Ok(recurrence) => recurrence,
        Err(message) => return Ok(json_error(StatusCode::BAD_REQUEST, message)),
    };
    let input = TaskUpdate {
        content: body.content.trim().to_string(),
        description: body.description,
        project_id: body.project_id,
        is_completed: body.is_completed,
        sort_order: body.sort_order,
        due_at: body.due_at,
        remind_at: body.remind_at,
        label_ids: body.label_ids,
        recurrence,
    };
    task::update(&db, &user, &id, &input)
        .await
        .map_err(from_domain)?
        .map_or_else(
            || Ok(json_error(StatusCode::NOT_FOUND, "task not found")),
            |task| Ok(json(StatusCode::OK, &task)),
        )
}

#[delete("/api/v1/tasks/{id}")]
pub async fn remove(
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !can(claims.as_ref(), &config, "write") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no write access to daybook",
        ));
    }
    let user = actor(claims.as_ref());
    if task::delete(&db, &user, &id).await.map_err(from_domain)? {
        Ok(Response::new(StatusCode::NO_CONTENT))
    } else {
        Ok(json_error(StatusCode::NOT_FOUND, "task not found"))
    }
}

pub fn register_routes() {
    let _ = list as fn(_, _, _, _) -> _;
    let _ = create as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
}
