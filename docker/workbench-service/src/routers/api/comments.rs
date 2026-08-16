//! Comments: create/list under an issue, delete by their own id.

use crate::domain::comment::{self, NewComment};
use crate::domain::issue;
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, actor, json_error};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, post, web};
use quench_db::prelude::Db;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateComment {
    pub body: String,
}

#[post("")]
pub async fn create(
    request: HttpRequest,
    issue_id: web::Path<String>,
    body: web::Json<CreateComment>,
    db: web::Data<Db>,
) -> impl Responder {
    if body.body.trim().is_empty() {
        return json_error(actix_web::http::StatusCode::BAD_REQUEST, "body is required");
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

    let new = NewComment {
        issue_id: issue.id,
        author: actor(&request).await,
        body: body.body.trim().to_string(),
    };

    match comment::create(&db, &new).await {
        Ok(comment) => HttpResponse::Created().json(comment),
        Err(error) => ApiError::from(error).into_response(),
    }
}

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

    match comment::list_by_issue(&db, &issue.id).await {
        Ok(comments) => HttpResponse::Ok().json(comments),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope_under_issue() -> actix_web::Scope {
    web::scope("/{id}/comments").service(create).service(list)
}

#[delete("/{id}")]
pub async fn remove(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let comment = match comment::read(&db, &path).await {
        Ok(Some(comment)) => comment,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such comment"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    let issue = match issue::read(&db, &comment.issue_id).await {
        Ok(Some(issue)) => issue,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such issue"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &issue.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this comment",
        );
    }

    match comment::delete(&db, &path).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such comment"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/comments").service(remove)
}
