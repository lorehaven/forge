//! Registering the repositories conveyor is willing to build.
//!
//! Registration is explicit and deliberate. Conveyor runs code that a
//! repository supplies, so "any repository a webhook mentions" is not an
//! acceptable answer to which ones it will build.

use crate::domain::Provider;
use crate::routers::api::{ApiError, actor, json_error};
use crate::scheduler::repos::{self, NewRepo};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, post, web};
use quench_db::prelude::Db;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct RegisterRepo {
    /// `github` or `generic`. Defaults to `github`.
    #[serde(default)]
    pub provider: Option<String>,
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    #[serde(default)]
    pub default_branch: Option<String>,
}

#[post("")]
pub async fn register(
    request: HttpRequest,
    body: web::Json<RegisterRepo>,
    db: web::Data<Db>,
) -> impl Responder {
    let provider = match body.provider.as_deref() {
        None => Provider::GitHub,
        Some(raw) => match Provider::parse(raw) {
            Some(provider) => provider,
            None => {
                return json_error(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    &format!("unknown provider '{raw}'"),
                );
            }
        },
    };

    if body.owner.trim().is_empty() || body.name.trim().is_empty() {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "owner and name are required",
        );
    }

    // Checked here rather than at checkout time: a clone url git would read as
    // an option should never be stored, let alone handed to a worker.
    if let Err(error) = crate::workspace::checkout::validate_url(&body.clone_url) {
        return json_error(actix_web::http::StatusCode::BAD_REQUEST, &error.to_string());
    }

    let new = NewRepo {
        provider,
        owner: body.owner.trim().to_string(),
        name: body.name.trim().to_string(),
        clone_url: body.clone_url.trim().to_string(),
        default_branch: body
            .default_branch
            .clone()
            .unwrap_or_else(|| "master".to_string()),
        registered_by: actor(&request),
    };

    match repos::create(&db, &new).await {
        Ok(repo) => HttpResponse::Created().json(repo),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[get("")]
pub async fn list(db: web::Data<Db>) -> impl Responder {
    match repos::list(&db).await {
        Ok(repos) => HttpResponse::Ok().json(repos),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[get("/{id}")]
pub async fn read(path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    match repos::read(&db, &path).await {
        Ok(Some(repo)) => HttpResponse::Ok().json(repo),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetEnabled {
    pub enabled: bool,
}

/// Turning a repository off keeps its history and stops it accepting triggers,
/// which is what you want for one that has gone bad rather than gone away.
#[post("/{id}/enabled")]
pub async fn set_enabled(
    path: web::Path<String>,
    body: web::Json<SetEnabled>,
    db: web::Data<Db>,
) -> impl Responder {
    match repos::set_enabled(&db, &path, body.enabled).await {
        Ok(Some(repo)) => HttpResponse::Ok().json(repo),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/{id}")]
pub async fn remove(path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    match repos::delete(&db, &path).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/repos")
        .service(register)
        .service(list)
        .service(read)
        .service(set_enabled)
        .service(remove)
        // Triggering lives here because the URL does. Declared beside this
        // scope instead, it would be shadowed: actix picks the first scope
        // whose prefix matches and does not fall through to the next.
        .service(super::runs::trigger)
        .service(super::secrets::list_repo)
        .service(super::secrets::put_repo)
        .service(super::secrets::delete_repo)
}
