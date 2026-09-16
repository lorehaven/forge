//! Conveyor's organisational tree: create, browse, rename, move, remove a node - no separate "group" endpoint.

use crate::routers::api::authz::{can_on_project, can_unscoped};
use crate::routers::api::{ApiError, OptionalClaims, json_error};
use crate::scheduler::projects::{self, DeleteOutcome, MoveOutcome, NewProject};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Query, Response, delete, get, http::StatusCode, patch, post,
};
use serde::{Deserialize, Serialize};

/// A write on a root node needs the unscoped grant - there's no ancestor to scope one against.
async fn can_write_under(
    claims: Option<&Claims>,
    config: &JwtConfig,
    db: &Db,
    parent_id: Option<&str>,
) -> bool {
    match parent_id {
        Some(parent_id) => can_on_project(claims, config, db, parent_id, "write").await,
        None => can_unscoped(claims, config, "write"),
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

#[post("/api/v1/projects")]
pub async fn create(
    OptionalClaims(claims): OptionalClaims,
    Json(body): Json<CreateProject>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if body.name.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "name is required");
    }

    if !can_write_under(claims.as_ref(), &config, &db, body.parent_id.as_deref()).await {
        return json_error(StatusCode::FORBIDDEN, "no write access here");
    }

    let new = NewProject {
        name: body.name.trim().to_string(),
        parent_id: body.parent_id.clone(),
    };

    match projects::create(&db, &new).await {
        Ok(project) => Response::json(StatusCode::CREATED, &project)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[get("/api/v1/projects")]
pub async fn list(Query(query): Query<ListQuery>, Inject(db): Inject<Db>) -> Response {
    match projects::list_children(&db, query.parent_id.as_deref()).await {
        Ok(projects) => Response::json(StatusCode::OK, &projects)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[get("/api/v1/projects/{id}")]
pub async fn read(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_on_project(claims.as_ref(), &config, &db, &id, "read").await {
        return json_error(StatusCode::FORBIDDEN, "no read access here");
    }

    match projects::read(&db, &id).await {
        Ok(Some(project)) => {
            let full_path = projects::full_path(&db, &project.id)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| project.name.clone());
            Response::json(
                StatusCode::OK,
                &ProjectView {
                    project,
                    path: full_path,
                },
            )
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
        }
        Ok(None) => json_error(StatusCode::NOT_FOUND, "no such project"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateProject {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Moves to root, distinguishing "leave parent alone" from "move to root" without a double-`Option` field.
    #[serde(default)]
    pub to_root: bool,
}

#[patch("/api/v1/projects/{id}")]
pub async fn update(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Json(body): Json<UpdateProject>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_on_project(claims.as_ref(), &config, &db, &id, "write").await {
        return json_error(StatusCode::FORBIDDEN, "no write access here");
    }

    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return json_error(StatusCode::BAD_REQUEST, "name cannot be empty");
        }
        match projects::rename(&db, &id, name.trim()).await {
            Ok(None) => {
                return json_error(StatusCode::NOT_FOUND, "no such project");
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

        if !can_write_under(claims.as_ref(), &config, &db, target_parent).await {
            return json_error(StatusCode::FORBIDDEN, "no write access to the destination");
        }

        match projects::move_to(&db, &id, target_parent).await {
            Ok(MoveOutcome::Moved(_)) => {}
            Ok(MoveOutcome::NotFound) => {
                return json_error(StatusCode::NOT_FOUND, "no such project");
            }
            Ok(MoveOutcome::WouldCycle) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    "a project cannot move under itself or one of its own descendants",
                );
            }
            Err(error) => return ApiError::from(error).into_response(),
        }
    }

    match projects::read(&db, &id).await {
        Ok(Some(project)) => Response::json(StatusCode::OK, &project)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "no such project"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/api/v1/projects/{id}")]
pub async fn remove(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if !can_on_project(claims.as_ref(), &config, &db, &id, "write").await {
        return json_error(StatusCode::FORBIDDEN, "no write access here");
    }

    match projects::delete(&db, &id).await {
        Ok(DeleteOutcome::Deleted) => Response::new(StatusCode::NO_CONTENT),
        Ok(DeleteOutcome::NotFound) => json_error(StatusCode::NOT_FOUND, "no such project"),
        Ok(DeleteOutcome::HasChildren) => json_error(
            StatusCode::CONFLICT,
            "this project still has child projects; move or remove them first",
        ),
        Ok(DeleteOutcome::HasRepo) => json_error(
            StatusCode::CONFLICT,
            "a repository is still attached to this project; move or remove it first",
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn register_routes() {
    let _ = create as fn(_, _, _, _) -> _;
    let _ = list as fn(_, _) -> _;
    let _ = read as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
}
