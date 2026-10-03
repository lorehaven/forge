//! Targets (packages and what runs of them) and plans: ask what a change would do, look
//! at it, confirm it. Confirming is what creates an operation.

use crate::domain::planner::{self, PlanError};
use crate::domain::service::Gantry;
use crate::routers::api::{ApiError, OptionalClaims, actor, authz, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Json, Path, Response, get, http::StatusCode, post};
use serde::{Deserialize, Serialize};

fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn json_created<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::CREATED, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn forbidden(action: &str) -> Response {
    json_error(
        StatusCode::FORBIDDEN,
        &format!("no {action} access to this target"),
    )
}

impl From<PlanError> for ApiError {
    fn from(error: PlanError) -> Self {
        let status = match &error {
            PlanError::UnknownTarget(_) => StatusCode::NOT_FOUND,
            PlanError::Refused(_) => StatusCode::UNPROCESSABLE_ENTITY,
            PlanError::Stale(_) => StatusCode::CONFLICT,
            // Warehouse or the cluster is down or refusing: the service is fine, what it needs is not.
            PlanError::Registry(_) | PlanError::Cluster(_) => StatusCode::BAD_GATEWAY,
            PlanError::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError::new(status, error.to_string())
    }
}

#[get("/api/v1/targets")]
pub async fn list(
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let visible: Vec<_> = planner::all(&gantry)
        .await?
        .into_iter()
        .filter(|target| authz::can_on_target(claims.as_ref(), &config, &target.name, "read"))
        .collect();
    Ok(json_ok(&visible))
}

#[derive(Serialize)]
struct Detail {
    #[serde(flatten)]
    target: crate::domain::targets::Target,
    versions: Vec<crate::domain::registry::PackageVersion>,
}

#[get("/api/v1/targets/{name}")]
pub async fn read(
    Path(name): Path<String>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can_on_target(claims.as_ref(), &config, &name, "read") {
        return Ok(forbidden("read"));
    }
    let Some(target) = planner::describe(&gantry, &name).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such target"));
    };
    let versions = gantry
        .registry
        .versions(&name)
        .await
        .map_err(PlanError::from)?;
    Ok(json_ok(&Detail { target, versions }))
}

#[derive(Deserialize, Default)]
pub struct PlanBody {
    #[serde(default)]
    pub version: Option<String>,
}

#[post("/api/v1/targets/{name}/plan")]
pub async fn make_plan(
    Path(name): Path<String>,
    Json(body): Json<PlanBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    // Planning changes nothing, so reading is enough; what the plan *does* is checked when it is confirmed.
    if !authz::can_on_target(claims.as_ref(), &config, &name, "read") {
        return Ok(forbidden("read"));
    }
    let (stored, _) = planner::plan(
        &gantry,
        &name,
        body.version.as_deref(),
        &actor(claims.as_ref()),
    )
    .await?;
    Ok(json_created(&stored))
}

#[get("/api/v1/plans/{id}")]
pub async fn read_plan(
    Path(id): Path<String>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(stored) = gantry.store.plan(&id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such plan"));
    };
    if !authz::can_on_target(claims.as_ref(), &config, &stored.target, "read") {
        return Ok(forbidden("read"));
    }
    Ok(json_ok(&stored))
}

#[post("/api/v1/plans/{id}/confirm")]
pub async fn confirm(
    Path(id): Path<String>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let Some(stored) = gantry.store.plan(&id).await? else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such plan"));
    };
    if let Err(reason) = authz::can_confirm(claims.as_ref(), &config, &stored) {
        return Ok(json_error(StatusCode::FORBIDDEN, &reason));
    }

    let operation = planner::confirm(&gantry, &id, &actor(claims.as_ref())).await?;
    Ok(json_created(&operation))
}

pub fn register_routes() {
    let _ = list as fn(_, _, _) -> _;
    let _ = read as fn(_, _, _, _) -> _;
    let _ = make_plan as fn(_, _, _, _, _) -> _;
    let _ = read_plan as fn(_, _, _, _) -> _;
    let _ = confirm as fn(_, _, _, _) -> _;
}
