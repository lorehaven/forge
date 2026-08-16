//! Issues: create and list under a project, then read, edit, transition,
//! remove and label by their own id.

use crate::domain::issue::{self, IssueUpdate, NewIssue};
use crate::domain::label;
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, actor, json_error};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, post, put, web};
use quench_db::prelude::Db;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateIssue {
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub assignee: Option<String>,
}

fn default_kind() -> String {
    "task".to_string()
}

fn default_priority() -> String {
    "medium".to_string()
}

#[post("")]
pub async fn create(
    request: HttpRequest,
    project_id: web::Path<String>,
    body: web::Json<CreateIssue>,
    db: web::Data<Db>,
) -> impl Responder {
    if body.title.trim().is_empty() {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "title is required",
        );
    }

    if !can_on_project(&request, &project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access here",
        );
    }

    let new = NewIssue {
        project_id: project_id.clone(),
        parent_id: body.parent_id.clone(),
        kind: body.kind.clone(),
        title: body.title.trim().to_string(),
        description: body.description.clone(),
        priority: body.priority.clone(),
        assignee: body.assignee.clone(),
        reporter: actor(&request).await,
    };

    match issue::create(&db, &new).await {
        Ok(issue) => HttpResponse::Created().json(issue),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// The board view's own query when `status` is set (one column at a time);
/// the plain list view's with it left `None`.
#[get("")]
pub async fn list(
    request: HttpRequest,
    project_id: web::Path<String>,
    query: web::Query<ListQuery>,
    db: web::Data<Db>,
) -> impl Responder {
    if !can_on_project(&request, &project_id, "read") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no read access here",
        );
    }

    match issue::list_by_project(&db, &project_id, query.status.as_deref()).await {
        Ok(issues) => HttpResponse::Ok().json(issues),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope_under_project() -> actix_web::Scope {
    web::scope("/{id}/issues").service(create).service(list)
}

// ---------------------------------------------------------------------------
// By issue id
// ---------------------------------------------------------------------------

#[get("/{id}")]
pub async fn read(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let issue = match issue::read(&db, &path).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue.project_id, "read") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no read access to this issue",
        );
    }

    HttpResponse::Ok().json(issue)
}

#[derive(Deserialize)]
pub struct UpdateIssue {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub kind: String,
    pub priority: String,
    #[serde(default)]
    pub assignee: Option<String>,
}

#[put("/{id}")]
pub async fn update(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateIssue>,
    db: web::Data<Db>,
) -> impl Responder {
    let issue = match issue::read(&db, &path).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if body.title.trim().is_empty() {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "title cannot be empty",
        );
    }

    if !can_on_project(&request, &issue.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this issue",
        );
    }

    let changes = IssueUpdate {
        title: body.title.trim().to_string(),
        description: body.description.clone(),
        kind: body.kind.clone(),
        priority: body.priority.clone(),
        assignee: body.assignee.clone(),
    };

    match issue::update(&db, &path, &changes).await {
        Ok(Some(issue)) => HttpResponse::Ok().json(issue),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct Transition {
    pub status: String,
}

#[post("/{id}/transition")]
pub async fn transition(
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<Transition>,
    db: web::Data<Db>,
) -> impl Responder {
    if !issue::is_valid_status(&body.status) {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            &format!(
                "unknown status '{}'; must be one of {:?}",
                body.status,
                issue::STATUSES
            ),
        );
    }

    let issue_row = match issue::read(&db, &path).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue_row.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this issue",
        );
    }

    match issue::transition(&db, &path, &body.status).await {
        Ok(Some(issue)) => HttpResponse::Ok().json(issue),
        Ok(None) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/{id}")]
pub async fn remove(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let issue = match issue::read(&db, &path).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this issue",
        );
    }

    match issue::delete(&db, &path).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Labels on an issue
// ---------------------------------------------------------------------------

#[get("/{id}/labels")]
pub async fn list_labels(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let issue = match issue::read(&db, &path).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue.project_id, "read") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no read access to this issue",
        );
    }

    match label::list_for_issue(&db, &issue.id).await {
        Ok(labels) => HttpResponse::Ok().json(labels),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[post("/{id}/labels/{label_id}")]
pub async fn attach_label(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    db: web::Data<Db>,
) -> impl Responder {
    let (issue_id, label_id) = path.into_inner();

    let issue = match issue::read(&db, &issue_id).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this issue",
        );
    }

    match label::attach(&db, &issue_id, &label_id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[delete("/{id}/labels/{label_id}")]
pub async fn detach_label(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    db: web::Data<Db>,
) -> impl Responder {
    let (issue_id, label_id) = path.into_inner();

    let issue = match issue::read(&db, &issue_id).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this issue",
        );
    }

    match label::detach(&db, &issue_id, &label_id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/issues")
        .service(read)
        .service(update)
        .service(transition)
        .service(remove)
        .service(list_labels)
        .service(attach_label)
        .service(detach_label)
        .service(super::comments::scope_under_issue())
}
