//! Resources of every kind, grouped by package, and the direct things that can be done to one.

use crate::domain::actions::{self, ActionError, Target};
use crate::domain::service::{Gantry, SubmitError};
use crate::routers::api::{ApiError, OptionalClaims, actor, authz, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Json, Query, Response, get, http::StatusCode, post};
use serde::{Deserialize, Serialize};

fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

impl From<ActionError> for ApiError {
    fn from(error: ActionError) -> Self {
        let status = match &error {
            ActionError::NotFound(_) => StatusCode::NOT_FOUND,
            ActionError::Refused(_) | ActionError::Submit(SubmitError::Invalid(_)) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            ActionError::Planner(_) | ActionError::Cluster(_) => StatusCode::BAD_GATEWAY,
            ActionError::Submit(SubmitError::Store(_)) | ActionError::Store(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        ApiError::new(status, error.to_string())
    }
}

fn forbidden(action: &str, package: &str) -> Response {
    json_error(
        StatusCode::FORBIDDEN,
        &format!("no {action} access to {package}"),
    )
}

#[get("/api/v1/resources")]
pub async fn list(
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    let visible: Vec<_> = actions::groups(&gantry)
        .await?
        .into_iter()
        .filter(|g| authz::can_on_target(claims.as_ref(), &config, &g.package, "read"))
        .collect();
    Ok(json_ok(&visible))
}

#[derive(Deserialize)]
pub struct YamlQuery {
    pub package: String,
    #[serde(rename = "apiVersion", default)]
    pub api_version: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub namespace: Option<String>,
    pub name: String,
}

#[get("/api/v1/resources/yaml")]
pub async fn yaml(
    Query(query): Query<YamlQuery>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can_on_target(claims.as_ref(), &config, &query.package, "read") {
        return Ok(forbidden("read", &query.package));
    }
    let target = Target {
        api_version: query.api_version,
        kind: query.kind,
        name: query.name,
        namespace: query.namespace.filter(|ns| !ns.is_empty()),
    };
    let text = actions::yaml(&gantry, &query.package, &target).await?;
    Ok(json_ok(&serde_json::json!({ "yaml": text })))
}

#[derive(Deserialize)]
pub struct ResourceBody {
    pub package: String,
    #[serde(flatten)]
    pub target: Option<Target>,
    #[serde(default)]
    pub yaml: Option<String>,
}

fn created(operation: &crate::domain::operation::Operation) -> Response {
    Response::json(StatusCode::CREATED, operation)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[post("/api/v1/resources/delete")]
pub async fn delete(
    Json(body): Json<ResourceBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can_on_target(claims.as_ref(), &config, &body.package, "scale") {
        return Ok(forbidden("scale", &body.package));
    }
    let Some(target) = body.target else {
        return Ok(json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "which resource? (kind, name)",
        ));
    };
    let operation =
        actions::delete(&gantry, &body.package, &target, &actor(claims.as_ref())).await?;
    Ok(created(&operation))
}

/// With a resource: apply that one from the package. Without: everything the package declares that is
/// missing.
#[post("/api/v1/resources/apply")]
pub async fn apply(
    Json(body): Json<ResourceBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can_on_target(claims.as_ref(), &config, &body.package, "scale") {
        return Ok(forbidden("scale", &body.package));
    }
    let operation = actions::apply(
        &gantry,
        &body.package,
        body.target.as_ref(),
        &actor(claims.as_ref()),
    )
    .await?;
    Ok(created(&operation))
}

#[post("/api/v1/resources/edit")]
pub async fn edit(
    Json(body): Json<ResourceBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can_on_target(claims.as_ref(), &config, &body.package, "deploy") {
        return Ok(forbidden("deploy", &body.package));
    }
    let (Some(target), Some(text)) = (body.target, body.yaml) else {
        return Ok(json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "kind, name and yaml are needed",
        ));
    };
    let operation = actions::edit(
        &gantry,
        &body.package,
        &target,
        &text,
        &actor(claims.as_ref()),
    )
    .await?;
    Ok(created(&operation))
}

#[post("/api/v1/resources/refresh")]
pub async fn refresh(
    Json(body): Json<ResourceBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can_on_target(claims.as_ref(), &config, &body.package, "read") {
        return Ok(forbidden("read", &body.package));
    }
    let operation = actions::refresh(&gantry, &body.package, &actor(claims.as_ref())).await?;
    Ok(created(&operation))
}

pub fn register_routes() {
    let _ = list as fn(_, _, _) -> _;
    let _ = yaml as fn(_, _, _, _) -> _;
    let _ = delete as fn(_, _, _, _) -> _;
    let _ = apply as fn(_, _, _, _) -> _;
    let _ = edit as fn(_, _, _, _) -> _;
    let _ = refresh as fn(_, _, _, _) -> _;
}
