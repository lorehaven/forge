//! Registering the repositories conveyor is willing to build.
//!
//! Registration is explicit and deliberate. Conveyor runs code that a
//! repository supplies, so "any repository a webhook mentions" is not an
//! acceptable answer to which ones it will build.

use crate::domain::Provider;
use crate::routers::api::authz::{can_on_project, granted_project_ids};
use crate::routers::api::{ApiError, actor, claims, json_error};
use crate::scheduler::projects;
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
    /// The project node this repo attaches to.
    pub project_id: String,
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

    if body.project_id.trim().is_empty() {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "project_id is required",
        );
    }

    // Checked here rather than at checkout time: a clone url git would read as
    // an option should never be stored, let alone handed to a worker.
    if let Err(error) = crate::workspace::checkout::validate_url(&body.clone_url) {
        return json_error(actix_web::http::StatusCode::BAD_REQUEST, &error.to_string());
    }

    if !can_on_project(&request, &db, &body.project_id, "write").await {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to that project",
        );
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
        registered_by: actor(&request).await,
        project_id: body.project_id.clone(),
    };

    match repos::create(&db, &new).await {
        Ok(repo) => HttpResponse::Created().json(repo),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Filters to the repos the caller may read, rather than 403ing: a caller
/// with no blanket `conveyor:read` still gets a (possibly empty) list scoped
/// to whatever projects they hold a resource-scoped grant on, the same way a
/// directory listing shows what you can see rather than refusing the whole
/// directory because you can't see everything in it.
#[get("")]
pub async fn list(request: HttpRequest, db: web::Data<Db>) -> impl Responder {
    let all = match repos::list(&db).await {
        Ok(repos) => repos,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let Some(claims) = claims(&request) else {
        // Auth disabled (the realm-wide dev switch): no identity to scope by,
        // so nothing is filtered - matches every other route's bypass.
        return HttpResponse::Ok().json(all);
    };

    if claims.can("conveyor", "read") {
        return HttpResponse::Ok().json(all);
    }

    let granted = granted_project_ids(&claims, "read");
    let visible = match projects::descendant_ids(&db, &granted).await {
        Ok(ids) => ids,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let repos: Vec<_> = all
        .into_iter()
        .filter(|repo| visible.contains(&repo.project_id))
        .collect();
    HttpResponse::Ok().json(repos)
}

#[get("/{id}")]
pub async fn read(request: HttpRequest, path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    let repo = match repos::read(&db, &path).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &db, &repo.project_id, "read").await {
        return json_error(actix_web::http::StatusCode::FORBIDDEN, "no read access to this repository");
    }

    HttpResponse::Ok().json(repo)
}

#[derive(Deserialize)]
pub struct SetEnabled {
    pub enabled: bool,
}

/// Turning a repository off keeps its history and stops it accepting triggers,
/// which is what you want for one that has gone bad rather than gone away.
#[post("/{id}/enabled")]
pub async fn set_enabled(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetEnabled>,
    db: web::Data<Db>,
) -> impl Responder {
    let repo = match repos::read(&db, &path).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &db, &repo.project_id, "write").await {
        return json_error(actix_web::http::StatusCode::FORBIDDEN, "no write access to this repository");
    }

    match repos::set_enabled(&db, &path, body.enabled).await {
        Ok(Some(repo)) => HttpResponse::Ok().json(repo),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/{id}")]
pub async fn remove(request: HttpRequest, path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    let repo = match repos::read(&db, &path).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &db, &repo.project_id, "write").await {
        return json_error(actix_web::http::StatusCode::FORBIDDEN, "no write access to this repository");
    }

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
