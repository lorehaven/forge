//! Labels: create/list under a project, delete by their own id.

use crate::domain::label::{self, NewLabel};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, json_error};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, post, web};
use quench_db::prelude::Db;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateLabel {
    pub name: String,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "#888888".to_string()
}

#[post("")]
pub async fn create(
    request: HttpRequest,
    project_id: web::Path<String>,
    body: web::Json<CreateLabel>,
    db: web::Data<Db>,
) -> impl Responder {
    if body.name.trim().is_empty() {
        return json_error(actix_web::http::StatusCode::BAD_REQUEST, "name is required");
    }

    if !can_on_project(&request, &project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access here",
        );
    }

    let new = NewLabel {
        project_id: project_id.clone(),
        name: body.name.trim().to_string(),
        color: body.color.clone(),
    };

    match label::create(&db, &new).await {
        Ok(label) => HttpResponse::Created().json(label),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[get("")]
pub async fn list(
    request: HttpRequest,
    project_id: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    if !can_on_project(&request, &project_id, "read") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no read access here",
        );
    }

    match label::list_by_project(&db, &project_id).await {
        Ok(labels) => HttpResponse::Ok().json(labels),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope_under_project() -> actix_web::Scope {
    web::scope("/{id}/labels").service(create).service(list)
}

#[delete("/{id}")]
pub async fn remove(
    request: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    let label = match label::read(&db, &path).await {
        Ok(Some(label)) => label,
        Ok(None) => return json_error(actix_web::http::StatusCode::NOT_FOUND, "no such label"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    if !can_on_project(&request, &label.project_id, "write") {
        return json_error(
            actix_web::http::StatusCode::FORBIDDEN,
            "no write access to this label",
        );
    }

    match label::delete(&db, &path).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => json_error(actix_web::http::StatusCode::NOT_FOUND, "no such label"),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/labels").service(remove)
}
