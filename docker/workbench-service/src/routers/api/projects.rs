//! Projects: create, browse, edit and remove the flat container issues live
//! in.

use crate::domain::project::{self, ProjectUpdate};
use crate::routers::api::authz::{can_on_project, can_unscoped, granted_project_ids};
use crate::routers::api::{ApiError, claims, json_error};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, post, put, web};
use quench_db::prelude::Db;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateProject {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[post("")]
pub async fn create(
    request: HttpRequest,
    body: web::Json<CreateProject>,
    db: web::Data<Db>,
) -> impl Responder {
    if body.key.trim().is_empty() || body.name.trim().is_empty() {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "key and name are required",
        );
    }

    // A new project has no id yet to scope a grant against - only the
    // unscoped `workbench:write` grant can cover its creation.
    if !can_unscoped(&request, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access here",
        );
    }

    let new = project::NewProject {
        key: body.key.trim().to_string(),
        name: body.name.trim().to_string(),
        description: body.description.clone(),
    };

    match project::create(&db, &new).await {
        Ok(project) => HttpResponse::Created().json(project),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Filters to the projects the caller may read, rather than 403ing: a caller
/// with no blanket `workbench:read` still gets a (possibly empty) list scoped
/// to whatever projects they hold a resource-scoped grant on.
#[get("")]
pub async fn list(request: HttpRequest, db: web::Data<Db>) -> impl Responder {
    let all = match project::list(&db).await {
        Ok(projects) => projects,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let Some(claims) = claims(&request) else {
        // Auth disabled (the realm-wide dev switch): no identity to scope by,
        // so nothing is filtered - matches every other route's bypass.
        return HttpResponse::Ok().json(all);
    };

    if claims.can("workbench", "read") {
        return HttpResponse::Ok().json(all);
    }

    let granted = granted_project_ids(&claims, "read");
    let projects: Vec<_> = all
        .into_iter()
        .filter(|project| granted.contains(&project.id))
        .collect();
    HttpResponse::Ok().json(projects)
}

#[get("/{id}")]
pub async fn read(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    if !can_on_project(&request, &path, "read") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no read access here",
        );
    }

    match project::read(&db, &path).await {
        Ok(Some(project)) => HttpResponse::Ok().json(project),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such project"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateProject {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[put("/{id}")]
pub async fn update(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateProject>,
    db: web::Data<Db>,
) -> impl Responder {
    if body.name.trim().is_empty() {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "name cannot be empty",
        );
    }

    if !can_on_project(&request, &path, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access here",
        );
    }

    let changes = ProjectUpdate {
        name: body.name.trim().to_string(),
        description: body.description.clone(),
    };

    match project::update(&db, &path, &changes).await {
        Ok(Some(project)) => HttpResponse::Ok().json(project),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such project"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/{id}")]
pub async fn remove(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    if !can_on_project(&request, &path, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access here",
        );
    }

    match project::delete(&db, &path).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such project"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/projects")
        .service(create)
        .service(list)
        .service(read)
        .service(update)
        .service(remove)
        .service(super::issues::scope_under_project())
        .service(super::labels::scope_under_project())
}
