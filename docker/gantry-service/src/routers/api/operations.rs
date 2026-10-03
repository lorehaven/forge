//! Operations: ask for a change, watch it, stop it. The API records and reads; the reconciler does the work.

use crate::domain::operation::Operation;
use crate::domain::service::{Gantry, SubmitError};
use crate::domain::steps::Plan;
use crate::routers::api::{ApiError, OptionalClaims, actor, authz, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Json, Path, Query, Response, get, http::StatusCode, post};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Submit {
    #[serde(default = "custom")]
    pub kind: String,
    pub title: String,
    pub plan: Plan,
}

fn custom() -> String {
    "custom".to_string()
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

/// An operation, with the runner's output as it is *now* while it is still running (the stored `log` is
/// only written when it ends).
#[derive(Serialize)]
struct Detail {
    #[serde(flatten)]
    operation: Operation,
    live_log: Option<String>,
}

fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn json_created<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::CREATED, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// A raw plan names no package, so only a blanket grant can submit one; the planner's endpoints (which
/// do) accept a grant scoped to a target.
#[post("/api/v1/operations")]
pub async fn submit(
    Json(body): Json<Submit>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "deploy") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no deploy access here"));
    }

    match gantry
        .submit(&body.kind, &body.title, body.plan, &actor(claims.as_ref()))
        .await
    {
        Ok(operation) => Ok(json_created(&operation)),
        Err(SubmitError::Invalid(reason)) => {
            Ok(json_error(StatusCode::UNPROCESSABLE_ENTITY, &reason))
        }
        Err(SubmitError::Store(error)) => Err(error.into()),
    }
}

#[get("/api/v1/operations")]
pub async fn list(
    Query(query): Query<ListQuery>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no read access here"));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    Ok(json_ok(&gantry.store.list(limit).await?))
}

#[get("/api/v1/operations/{id}")]
pub async fn read(
    Path(id): Path<String>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no read access here"));
    }
    let Some(operation) = gantry.store.get(&id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such operation"));
    };

    let live_log = if operation.state.is_terminal() {
        None
    } else if let Some(job) = &operation.runner_job {
        gantry.executor.log(job).await.ok()
    } else {
        None
    };
    Ok(json_ok(&Detail {
        operation,
        live_log,
    }))
}

#[post("/api/v1/operations/{id}/cancel")]
pub async fn cancel(
    Path(id): Path<String>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "deploy") {
        return Ok(json_error(StatusCode::FORBIDDEN, "no deploy access here"));
    }
    match gantry.cancel(&id).await? {
        Some(operation) => Ok(json_ok(&operation)),
        None => Ok(json_error(StatusCode::NOT_FOUND, "no such operation")),
    }
}

pub fn register_routes() {
    let _ = submit as fn(_, _, _, _) -> _;
    let _ = list as fn(_, _, _, _) -> _;
    let _ = read as fn(_, _, _, _) -> _;
    let _ = cancel as fn(_, _, _, _) -> _;
}
