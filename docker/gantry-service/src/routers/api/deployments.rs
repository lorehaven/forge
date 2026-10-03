//! Deployments: what can be stopped and started, whether it is, and plans for changing that.

use crate::domain::deployments::{self, Request};
use crate::domain::planner::PlanError;
use crate::domain::service::Gantry;
use crate::routers::api::{ApiError, OptionalClaims, actor, authz, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Json, Path, Response, get, http::StatusCode, post};
use serde::{Deserialize, Serialize};

fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/api/v1/deployments")]
pub async fn list(
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let visible: Vec<_> = deployments::all(&gantry)
        .await?
        .into_iter()
        .filter(|d| authz::can_on_target(claims.as_ref(), &config, &d.target, "read"))
        .collect();
    Ok(json_ok(&visible))
}

#[derive(Deserialize)]
pub struct PlanBody {
    /// `start` or `stop`.
    pub action: String,
    /// For a start: deployments to stop first, on top of any it conflicts with.
    #[serde(default)]
    pub stop: Vec<String>,
}

#[post("/api/v1/deployments/{name}/plan")]
pub async fn make_plan(
    Path(name): Path<String>,
    Json(body): Json<PlanBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let views = deployments::all(&gantry).await?;
    let Some(view) = views.iter().find(|d| d.name == name) else {
        return Ok(json_error(StatusCode::NOT_FOUND, "no such deployment"));
    };
    // Planning changes nothing, so reading is enough; what the plan does is checked when it is confirmed.
    if !authz::can_on_target(claims.as_ref(), &config, &view.target, "read") {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            "no read access to this deployment",
        ));
    }

    let request = match body.action.as_str() {
        "stop" => Request::Stop,
        "start" => Request::Start {
            also_stop: body.stop,
        },
        other => {
            return Ok(json_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!("'{other}' is not start or stop"),
            ));
        }
    };
    let (stored, _) = deployments::plan(&gantry, &name, &request, &actor(claims.as_ref()))
        .await
        .map_err(|e: PlanError| ApiError::from(e))?;
    Ok(Response::json(StatusCode::CREATED, &stored)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)))
}

pub fn register_routes() {
    let _ = list as fn(_, _, _) -> _;
    let _ = make_plan as fn(_, _, _, _, _) -> _;
}
