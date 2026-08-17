//! Typed links between issues: create/list under the owning issue, then
//! remove by the link's own id - mirrors `comments.rs`'s shape.

use crate::domain::issue;
use crate::domain::issue_link::{self, NewIssueLink};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, json_error};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, post, web};
use quench_db::prelude::Db;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateIssueLink {
    pub linked_issue_id: String,
    pub kind: String,
}

#[post("")]
pub async fn create(
    request: HttpRequest,
    issue_id: web::Path<String>,
    body: web::Json<CreateIssueLink>,
    db: web::Data<Db>,
) -> impl Responder {
    if !issue_link::is_valid_kind(&body.kind) {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            &format!(
                "unknown kind '{}'; must be one of {:?}",
                body.kind,
                issue_link::KINDS
            ),
        );
    }

    if body.linked_issue_id == *issue_id {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "an issue cannot link to itself",
        );
    }

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

    let new = NewIssueLink {
        issue_id: issue.id,
        linked_issue_id: body.linked_issue_id.clone(),
        kind: body.kind.clone(),
    };

    match issue_link::create(&db, &new).await {
        Ok(link) => HttpResponse::Created().json(link),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// The three lists (`blocks`, `blocked_by`, `relates_to`) a detail page
/// renders, each resolved to the linked issue's key/title/status.
#[get("")]
pub async fn list(
    request: HttpRequest,
    issue_id: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let issue = match issue::read(&db, &issue_id).await {
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

    match issue_link::related(&db, &issue.id).await {
        Ok(related) => HttpResponse::Ok().json(related),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope_under_issue() -> actix_web::Scope {
    web::scope("/{id}/links").service(create).service(list)
}

#[delete("/{id}")]
pub async fn remove(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let link = match issue_link::read(&db, &path).await {
        Ok(Some(link)) => link,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such link"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    let issue = match issue::read(&db, &link.issue_id).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this link",
        );
    }

    match issue_link::delete(&db, &path).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such link"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/issue-links").service(remove)
}
