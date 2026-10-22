use crate::domain::label::{self, NewLabel};
use crate::routers::api_error::{from_domain, json};
use crate::routers::authz::can;
use crate::routers::{OptionalClaims, actor};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Response, delete, get, http::StatusCode, post, put,
};
use quench_starter::http::domain::api_error::{ApiError, json_error};
use serde::Deserialize;

/// Used when a client creates a label without choosing a colour.
const DEFAULT_COLOR: &str = "#888888";

#[derive(Debug, Deserialize)]
pub struct LabelInput {
    name: String,
    #[serde(default)]
    color: Option<String>,
}

/// `#rrggbb`, nothing else - clients render it directly.
fn valid_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Trims the name and defaults/validates the colour; `Err` is the 400 body.
fn parse(input: LabelInput) -> Result<NewLabel, &'static str> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("name is required");
    }
    let color = input.color.unwrap_or_else(|| DEFAULT_COLOR.to_string());
    if !valid_color(&color) {
        return Err("color must be #rrggbb");
    }
    Ok(NewLabel {
        name: name.to_string(),
        color: color.to_ascii_lowercase(),
    })
}

#[get("/api/v1/labels")]
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
    let labels = label::list(&db, &user).await.map_err(from_domain)?;
    Ok(json(StatusCode::OK, &labels))
}

#[post("/api/v1/labels")]
pub async fn create(
    Json(body): Json<LabelInput>,
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
    let input = match parse(body) {
        Ok(input) => input,
        Err(message) => return Ok(json_error(StatusCode::BAD_REQUEST, message)),
    };
    let label = label::create(&db, &user, &input)
        .await
        .map_err(from_domain)?;
    Ok(json(StatusCode::CREATED, &label))
}

#[put("/api/v1/labels/{id}")]
pub async fn update(
    Path(id): Path<String>,
    Json(body): Json<LabelInput>,
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
    let input = match parse(body) {
        Ok(input) => input,
        Err(message) => return Ok(json_error(StatusCode::BAD_REQUEST, message)),
    };
    label::update(&db, &user, &id, &input)
        .await
        .map_err(from_domain)?
        .map_or_else(
            || Ok(json_error(StatusCode::NOT_FOUND, "label not found")),
            |label| Ok(json(StatusCode::OK, &label)),
        )
}

#[delete("/api/v1/labels/{id}")]
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
    if label::delete(&db, &user, &id).await.map_err(from_domain)? {
        Ok(Response::new(StatusCode::NO_CONTENT))
    } else {
        Ok(json_error(StatusCode::NOT_FOUND, "label not found"))
    }
}

pub fn register_routes() {
    let _ = list as fn(_, _, _) -> _;
    let _ = create as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
}
