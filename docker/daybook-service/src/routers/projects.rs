use crate::domain::project::{self, NewProject};
use crate::routers::api_error::{from_domain, json};
use crate::routers::authz::can;
use crate::routers::{OptionalClaims, actor};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Response, delete, get, http::StatusCode, post, put,
};
use quench_serde::Deserialize;
use quench_starter::http::domain::api_error::{ApiError, json_error};

#[derive(Debug, Deserialize)]
pub struct ProjectInput {
    name: String,
    #[serde(default)]
    sort_order: i32,
}

#[get("/api/v1/projects")]
pub async fn list(
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
    let projects = project::list(&db, &user).await.map_err(from_domain)?;
    Ok(json(StatusCode::OK, &projects))
}

#[post("/api/v1/projects")]
pub async fn create(
    Json(body): Json<ProjectInput>,
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
    if body.name.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "name is required"));
    }
    let project = project::create(
        &db,
        &user,
        &NewProject {
            name: body.name.trim().to_string(),
            sort_order: body.sort_order,
        },
    )
    .await
    .map_err(from_domain)?;
    Ok(json(StatusCode::CREATED, &project))
}

#[put("/api/v1/projects/{id}")]
pub async fn update(
    Path(id): Path<String>,
    Json(body): Json<ProjectInput>,
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
    if body.name.trim().is_empty() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "name is required"));
    }
    let input = NewProject {
        name: body.name.trim().to_string(),
        sort_order: body.sort_order,
    };
    project::update(&db, &user, &id, &input)
        .await
        .map_err(from_domain)?
        .map_or_else(
            || Ok(json_error(StatusCode::NOT_FOUND, "project not found")),
            |project| Ok(json(StatusCode::OK, &project)),
        )
}

#[delete("/api/v1/projects/{id}")]
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
    if project::delete(&db, &user, &id)
        .await
        .map_err(from_domain)?
    {
        Ok(Response::new(StatusCode::NO_CONTENT))
    } else {
        Ok(json_error(StatusCode::NOT_FOUND, "project not found"))
    }
}

pub fn register_routes() {
    let _ = list as fn(_, _, _) -> _;
    let _ = create as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
}
