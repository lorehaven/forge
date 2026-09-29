//! Following the results of a repository or project - the caller's own subscriptions only.
//! A project covers every repository nested beneath it. What is sent, and to whom, is
//! [`crate::notifications`]'s business.

use crate::notifications::subscriptions::{self, Scope};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{Actor, ApiError, OptionalClaims, json_error};
use crate::scheduler::{projects, repos};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Response, delete, get, http::StatusCode, put};
use serde::Serialize;

#[derive(Serialize)]
struct State {
    subscribed: bool,
}

fn state(status: StatusCode, subscribed: bool) -> Response {
    Response::json(status, &State { subscribed })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/api/v1/subscriptions")]
pub async fn list(Actor(username): Actor, Inject(db): Inject<Db>) -> Response {
    match subscriptions::list_for_user(&db, &username).await {
        Ok(found) => Response::json(StatusCode::OK, &found)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Following needs read access - it would otherwise tell someone how a project they can't see is doing.
#[put("/api/v1/repos/{id}/subscription")]
pub async fn follow_repo(
    Actor(username): Actor,
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let repo = match repos::read(&db, &id).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => return ApiError::from(error).into_response(),
    };
    if !can_on_project(claims.as_ref(), &config, &db, &repo.project_id, "read").await {
        return json_error(StatusCode::FORBIDDEN, "no read access to this repository");
    }

    match subscriptions::subscribe(&db, &username, Scope::Repo(&id)).await {
        Ok(created) => state(
            if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            true,
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Leaving needs no access check: someone who lost access can still stop the mail.
#[delete("/api/v1/repos/{id}/subscription")]
pub async fn unfollow_repo(
    Actor(username): Actor,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    match subscriptions::unsubscribe(&db, &username, Scope::Repo(&id)).await {
        Ok(_) => state(StatusCode::OK, false),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[put("/api/v1/projects/{id}/subscription")]
pub async fn follow_project(
    Actor(username): Actor,
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match projects::read(&db, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such project"),
        Err(error) => return ApiError::from(error).into_response(),
    }
    if !can_on_project(claims.as_ref(), &config, &db, &id, "read").await {
        return json_error(StatusCode::FORBIDDEN, "no read access here");
    }

    match subscriptions::subscribe(&db, &username, Scope::Project(&id)).await {
        Ok(created) => state(
            if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            true,
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/api/v1/projects/{id}/subscription")]
pub async fn unfollow_project(
    Actor(username): Actor,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    match subscriptions::unsubscribe(&db, &username, Scope::Project(&id)).await {
        Ok(_) => state(StatusCode::OK, false),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn register_routes() {
    let _ = list as fn(_, _) -> _;
    let _ = follow_repo as fn(_, _, _, _, _) -> _;
    let _ = unfollow_repo as fn(_, _, _) -> _;
    let _ = follow_project as fn(_, _, _, _, _) -> _;
    let _ = unfollow_project as fn(_, _, _) -> _;
}
