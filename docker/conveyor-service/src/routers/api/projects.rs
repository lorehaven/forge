//! Conveyor's organisational tree: create, browse, rename, move and remove a
//! node.
//!
//! There is no separate "group" endpoint - a node is a node, whether or not it
//! has children or a repository attached.

use crate::routers::api::authz::{can_on_project, can_unscoped};
use crate::routers::api::{ApiError, json_error};
use crate::scheduler::projects::{self, DeleteOutcome, MoveOutcome, NewProject};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, patch, post, web};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};

/// A write on a root node needs the unscoped `conveyor:write` grant: there is
/// no ancestor above a root to hold a resource-scoped one against.
async fn can_write_under(request: &HttpRequest, db: &Db, parent_id: Option<&str>) -> bool {
    match parent_id {
        Some(parent_id) => can_on_project(request, db, parent_id, "write").await,
        None => can_unscoped(request, "write"),
    }
}

#[derive(Deserialize)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Serialize)]
pub struct ProjectView {
    #[serde(flatten)]
    pub project: crate::domain::Project,
    pub path: String,
}

#[post("")]
pub async fn create(
    request: HttpRequest,
    body: web::Json<CreateProject>,
    db: web::Data<Db>,
) -> impl Responder {
    if body.name.trim().is_empty() {
        return json_error(actix_web::http::StatusCode::BAD_REQUEST, "name is required");
    }

    if !can_write_under(&request, &db, body.parent_id.as_deref()).await {
        return json_error(actix_web::http::StatusCode::FORBIDDEN, "no write access here");
    }

    let new = NewProject {
        name: body.name.trim().to_string(),
        parent_id: body.parent_id.clone(),
    };

    match projects::create(&db, &new).await {
        Ok(project) => HttpResponse::Created().json(project),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[get("")]
pub async fn list(query: web::Query<ListQuery>, db: web::Data<Db>) -> impl Responder {
    match projects::list_children(&db, query.parent_id.as_deref()).await {
        Ok(projects) => HttpResponse::Ok().json(projects),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[get("/{id}")]
pub async fn read(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    if !can_on_project(&request, &db, &path, "read").await {
        return json_error(actix_web::http::StatusCode::FORBIDDEN, "no read access here");
    }

    match projects::read(&db, &path).await {
        Ok(Some(project)) => {
            let full_path = projects::full_path(&db, &project.id)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| project.name.clone());
            HttpResponse::Ok().json(ProjectView {
                project,
                path: full_path,
            })
        }
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such project"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateProject {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Moves the project to the root. Distinguishes "leave the parent alone"
    /// from "move it to the root" without the double-`Option` a plain
    /// `parent_id: Option<Option<String>>` field would need to tell "absent"
    /// from "explicitly null" apart.
    #[serde(default)]
    pub to_root: bool,
}

#[patch("/{id}")]
pub async fn update(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateProject>,
    db: web::Data<Db>,
) -> impl Responder {
    if !can_on_project(&request, &db, &path, "write").await {
        return json_error(actix_web::http::StatusCode::FORBIDDEN, "no write access here");
    }

    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return json_error(actix_web::http::StatusCode::BAD_REQUEST, "name cannot be empty");
        }
        match projects::rename(&db, &path, name.trim()).await {
            Ok(None) => {
                return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such project");
            }
            Err(error) => return ApiError::from(error).into_response(),
            Ok(Some(_)) => {}
        }
    }

    if body.to_root || body.parent_id.is_some() {
        let target_parent = if body.to_root {
            None
        } else {
            body.parent_id.as_deref()
        };

        if !can_write_under(&request, &db, target_parent).await {
            return json_error(
                actix_web::http::StatusCode::FORBIDDEN,
                "no write access to the destination",
            );
        }

        match projects::move_to(&db, &path, target_parent).await {
            Ok(MoveOutcome::Moved(_)) => {}
            Ok(MoveOutcome::NotFound) => {
                return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such project");
            }
            Ok(MoveOutcome::WouldCycle) => {
                return json_error(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    "a project cannot move under itself or one of its own descendants",
                );
            }
            Err(error) => return ApiError::from(error).into_response(),
        }
    }

    match projects::read(&db, &path).await {
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
    if !can_on_project(&request, &db, &path, "write").await {
        return json_error(actix_web::http::StatusCode::FORBIDDEN, "no write access here");
    }

    match projects::delete(&db, &path).await {
        Ok(DeleteOutcome::Deleted) => HttpResponse::NoContent().finish(),
        Ok(DeleteOutcome::NotFound) => {
            json_error(actix_web::http::StatusCode::NOT_FOUND, "no such project")
        }
        Ok(DeleteOutcome::HasChildren) => json_error(
            actix_web::http::StatusCode::CONFLICT,
            "this project still has child projects; move or remove them first",
        ),
        Ok(DeleteOutcome::HasRepo) => json_error(
            actix_web::http::StatusCode::CONFLICT,
            "a repository is still attached to this project; move or remove it first",
        ),
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
}
