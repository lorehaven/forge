//! The cluster as it is: kinds, objects, one object with its events, and a pod's log.
//!
//! Read-only, and only for a caller with the blanket `read`: a grant scoped to one package does not open the
//! whole cluster.

use crate::domain::cluster::ClusterError;
use crate::domain::cluster_actions::{self, Object};
use crate::domain::service::Gantry;
use crate::routers::api::actor;
use crate::routers::api::{ApiError, OptionalClaims, authz, json_error};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Json, Query, Response, get, http::StatusCode, post};
use quench_serde::{Deserialize, Serialize};

impl From<ClusterError> for ApiError {
    fn from(error: ClusterError) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, error.to_string())
    }
}

fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn denied() -> Response {
    json_error(StatusCode::FORBIDDEN, "no read access to the cluster")
}

/// `None` for an empty value: a form field left blank means "all".
fn some(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

#[get("/api/v1/cluster/kinds")]
pub async fn kinds(
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(denied());
    }
    Ok(json_ok(&gantry.cluster.browse_kinds().await?))
}

#[get("/api/v1/cluster/overview")]
pub async fn overview(
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(denied());
    }
    Ok(json_ok(&gantry.cluster.browse_overview().await?))
}

#[get("/api/v1/cluster/facade")]
pub async fn facade(
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(denied());
    }
    Ok(json_ok(&gantry.cluster.browse_facade().await?))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub kind: String,
    #[serde(default)]
    pub namespace: Option<String>,
}

#[get("/api/v1/cluster/objects")]
pub async fn objects(
    Query(query): Query<ListQuery>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(denied());
    }
    let namespace = some(query.namespace);
    Ok(json_ok(
        &gantry
            .cluster
            .browse_list(&query.kind, namespace.as_deref())
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct ObjectQuery {
    pub kind: String,
    #[serde(default)]
    pub namespace: Option<String>,
    pub name: String,
}

#[get("/api/v1/cluster/object")]
pub async fn object(
    Query(query): Query<ObjectQuery>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(denied());
    }
    let namespace = some(query.namespace);
    Ok(json_ok(
        &gantry
            .cluster
            .browse_object(&query.kind, namespace.as_deref(), &query.name)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub namespace: String,
    pub pod: String,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub tail: Option<u32>,
    #[serde(default)]
    pub previous: Option<bool>,
}

/// The most lines one request returns: a log is looked at, not downloaded.
const MAX_TAIL: u32 = 5000;

#[get("/api/v1/cluster/logs")]
pub async fn logs(
    Query(query): Query<LogQuery>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "read") {
        return Ok(denied());
    }
    let text = gantry
        .cluster
        .pod_log(
            &query.namespace,
            &query.pod,
            some(query.container).as_deref(),
            query.tail.unwrap_or(500).clamp(1, MAX_TAIL),
            query.previous.unwrap_or(false),
        )
        .await?;
    Ok(json_ok(&quench_serde::json!({ "log": text })))
}

fn created(operation: &crate::domain::operation::Operation) -> Response {
    Response::json(StatusCode::CREATED, operation)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn forbidden(action: &str) -> Response {
    json_error(
        StatusCode::FORBIDDEN,
        &format!("no {action} access to the cluster"),
    )
}

fn object_of(object: Object) -> Object {
    Object {
        namespace: some(object.namespace),
        ..object
    }
}

#[post("/api/v1/cluster/delete")]
pub async fn delete(
    Json(object): Json<Object>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "scale") {
        return Ok(forbidden("scale"));
    }
    let operation =
        cluster_actions::delete(&gantry, &object_of(object), &actor(claims.as_ref())).await?;
    Ok(created(&operation))
}

#[derive(Debug, Deserialize)]
pub struct ScaleBody {
    #[serde(flatten)]
    pub object: Object,
    pub replicas: u32,
}

#[post("/api/v1/cluster/scale")]
pub async fn scale(
    Json(body): Json<ScaleBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "scale") {
        return Ok(forbidden("scale"));
    }
    let operation = cluster_actions::scale(
        &gantry,
        &object_of(body.object),
        body.replicas,
        &actor(claims.as_ref()),
    )
    .await?;
    Ok(created(&operation))
}

#[derive(Debug, Deserialize)]
pub struct EditBody {
    #[serde(flatten)]
    pub object: Object,
    pub yaml: String,
}

#[post("/api/v1/cluster/edit")]
pub async fn edit(
    Json(body): Json<EditBody>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "deploy") {
        return Ok(forbidden("deploy"));
    }
    let operation = cluster_actions::edit(
        &gantry,
        &object_of(body.object),
        &body.yaml,
        &actor(claims.as_ref()),
    )
    .await?;
    Ok(created(&operation))
}

#[derive(Debug, Deserialize)]
pub struct ExecOpen {
    pub namespace: String,
    pub pod: String,
    #[serde(default)]
    pub container: Option<String>,
}

/// The namespaces a shell may be opened in, for the caller: none when they may not, or it is off.
#[get("/api/v1/cluster/exec/enabled")]
pub async fn exec_enabled(
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let on = authz::can(claims.as_ref(), &config, "deploy");
    json_ok(&quench_serde::json!({
        "namespaces": if on { gantry.settings.exec_namespaces.clone() } else { Vec::new() },
    }))
}

fn exec_error(error: &crate::domain::exec::ExecError) -> ApiError {
    let status = match error {
        crate::domain::exec::ExecError::Unknown => StatusCode::NOT_FOUND,
        crate::domain::exec::ExecError::Refused(_) => StatusCode::CONFLICT,
    };
    ApiError::new(status, error.to_string())
}

/// Opens a shell. Needs `deploy`, and a namespace the operator listed in `GANTRY_EXEC_NAMESPACES`.
#[post("/api/v1/cluster/exec")]
pub async fn exec_open(
    Json(body): Json<ExecOpen>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "deploy") {
        return Ok(forbidden("deploy"));
    }
    if !gantry.settings.exec_namespaces.contains(&body.namespace) {
        return Ok(json_error(
            StatusCode::FORBIDDEN,
            &format!(
                "shells are not enabled in {} (GANTRY_EXEC_NAMESPACES)",
                body.namespace
            ),
        ));
    }
    let who = crate::routers::api::actor(claims.as_ref());
    let container = some(body.container);
    let connection = gantry
        .cluster
        .exec_open(&body.namespace, &body.pod, container.as_deref())
        .await?;
    let id = gantry
        .exec
        .start(&who, &body.namespace, &body.pod, connection)
        .map_err(|e| exec_error(&e))?;
    quench_log::warn!("{who} opened a shell in {}/{}", body.namespace, body.pod);
    Ok(json_ok(&quench_serde::json!({ "id": id })))
}

#[derive(Debug, Deserialize)]
pub struct ExecInput {
    pub id: String,
    pub data: String,
}

#[post("/api/v1/cluster/exec/input")]
pub async fn exec_input(
    Json(body): Json<ExecInput>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "deploy") {
        return Ok(forbidden("deploy"));
    }
    gantry
        .exec
        .write(
            &body.id,
            &crate::routers::api::actor(claims.as_ref()),
            &body.data,
        )
        .await
        .map_err(|e| exec_error(&e))?;
    Ok(json_ok(&quench_serde::json!({})))
}

#[derive(Debug, Deserialize)]
pub struct ExecOutput {
    pub id: String,
    #[serde(default)]
    pub since: Option<u64>,
}

#[get("/api/v1/cluster/exec/output")]
pub async fn exec_output(
    Query(query): Query<ExecOutput>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "deploy") {
        return Ok(forbidden("deploy"));
    }
    let read = gantry
        .exec
        .read(
            &query.id,
            &crate::routers::api::actor(claims.as_ref()),
            query.since.unwrap_or(0),
        )
        .map_err(|e| exec_error(&e))?;
    Ok(json_ok(&read))
}

#[derive(Debug, Deserialize)]
pub struct ExecClose {
    pub id: String,
}

#[post("/api/v1/cluster/exec/close")]
pub async fn exec_close(
    Json(body): Json<ExecClose>,
    Inject(gantry): Inject<Gantry>,
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Result<Response, ApiError> {
    if !authz::can(claims.as_ref(), &config, "deploy") {
        return Ok(forbidden("deploy"));
    }
    gantry
        .exec
        .close(&body.id, &crate::routers::api::actor(claims.as_ref()));
    Ok(json_ok(&quench_serde::json!({})))
}

pub fn register_routes() {
    let _ = exec_enabled as fn(_, _, _) -> _;
    let _ = exec_open as fn(_, _, _, _) -> _;
    let _ = exec_input as fn(_, _, _, _) -> _;
    let _ = exec_output as fn(_, _, _, _) -> _;
    let _ = exec_close as fn(_, _, _, _) -> _;
    let _ = delete as fn(_, _, _, _) -> _;
    let _ = scale as fn(_, _, _, _) -> _;
    let _ = edit as fn(_, _, _, _) -> _;
    let _ = kinds as fn(_, _, _) -> _;
    let _ = overview as fn(_, _, _) -> _;
    let _ = facade as fn(_, _, _) -> _;
    let _ = objects as fn(_, _, _, _) -> _;
    let _ = object as fn(_, _, _, _) -> _;
    let _ = logs as fn(_, _, _, _) -> _;
}
