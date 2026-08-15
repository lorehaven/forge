//! Registering the repositories conveyor is willing to build.
//!
//! Registration is explicit and deliberate. Conveyor runs code that a
//! repository supplies, so "any repository a webhook mentions" is not an
//! acceptable answer to which ones it will build.

use crate::domain::Provider;
use crate::routers::api::authz::{can_on_project, granted_project_ids};
use crate::routers::api::{ApiError, actor, claims, json_error};
use crate::scheduler::projects;
use crate::scheduler::repos::{self, NewRepo, RepoUpdate};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, patch, post, web};
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
pub async fn read(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let repo = match repos::read(&db, &path).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &db, &repo.project_id, "read").await {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no read access to this repository",
        );
    }

    HttpResponse::Ok().json(repo)
}

#[derive(Deserialize)]
pub struct UpdateRepo {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub clone_url: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// A partial update: every field is optional, and an absent one is left
/// exactly as it was - the same shape `UpdateProject` already gives `PATCH
/// /projects/{id}`, and what tells this apart from a `PUT` a caller could
/// only ever safely send by first reading the whole repository back. The
/// provider is not among the editable fields at all; it identifies what kind
/// of repository this is, not a property of it that an edit should flip.
#[patch("/{id}")]
pub async fn update(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateRepo>,
    db: web::Data<Db>,
) -> impl Responder {
    let repo = match repos::read(&db, &path).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if body
        .owner
        .as_deref()
        .is_some_and(|owner| owner.trim().is_empty())
        || body
            .name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
    {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "owner and name cannot be empty",
        );
    }

    if body
        .project_id
        .as_deref()
        .is_some_and(|project_id| project_id.trim().is_empty())
    {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "project_id cannot be empty",
        );
    }

    if let Some(clone_url) = &body.clone_url
        && let Err(error) = crate::workspace::checkout::validate_url(clone_url)
    {
        return json_error(actix_web::http::StatusCode::BAD_REQUEST, &error.to_string());
    }

    if !can_on_project(&request, &db, &repo.project_id, "write").await {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this repository",
        );
    }

    let target_project = body.project_id.as_deref().unwrap_or(&repo.project_id);

    // Moving a repository to a different project needs write on both ends,
    // the same as a project move does - otherwise a write grant on one
    // project alone would let it pull a repository in from a project the
    // caller has no access to.
    if target_project != repo.project_id
        && !can_on_project(&request, &db, target_project, "write").await
    {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to the destination project",
        );
    }

    let changes = RepoUpdate {
        owner: body
            .owner
            .as_deref()
            .map_or_else(|| repo.owner.clone(), |owner| owner.trim().to_string()),
        name: body
            .name
            .as_deref()
            .map_or_else(|| repo.name.clone(), |name| name.trim().to_string()),
        clone_url: body
            .clone_url
            .as_deref()
            .map_or_else(|| repo.clone_url.clone(), |url| url.trim().to_string()),
        default_branch: body
            .default_branch
            .clone()
            .unwrap_or_else(|| repo.default_branch.clone()),
        project_id: target_project.to_string(),
        enabled: body.enabled.unwrap_or(repo.enabled),
    };

    match repos::update(&db, &path, &changes).await {
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
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetEnabled>,
    db: web::Data<Db>,
) -> impl Responder {
    let repo = match repos::read(&db, &path).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &db, &repo.project_id, "write").await {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this repository",
        );
    }

    match repos::set_enabled(&db, &path, body.enabled).await {
        Ok(Some(repo)) => HttpResponse::Ok().json(repo),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/{id}")]
pub async fn remove(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let repo = match repos::read(&db, &path).await {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such repository");
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &db, &repo.project_id, "write").await {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this repository",
        );
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
        .service(update)
        .service(set_enabled)
        .service(remove)
        // Triggering lives here because the URL does. Declared beside this
        // scope instead, it would be shadowed: actix picks the first scope
        // whose prefix matches and does not fall through to the next.
        .service(super::runs::trigger)
        .service(super::secrets::list_repo)
        .service(super::secrets::put_repo)
        .service(super::secrets::delete_repo)
        .service(super::credentials::show_repo)
        .service(super::credentials::put_repo)
        .service(super::credentials::delete_repo)
}
