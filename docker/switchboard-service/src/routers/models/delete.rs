use super::mod_impl::{GGUF_ROOTS, HF_ROOTS, can};
use super::store::get_store;
use super::types::DeleteModelRequest;
use actix_web::web::Json;
use actix_web::{HttpResponse, Responder, http::header::ContentType, post, web};
use quench_auth::prelude::JwtConfig;
use std::path::Path;

#[post("/delete")]
pub async fn delete_model(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    body: Json<DeleteModelRequest>,
) -> impl Responder {
    if !can(&req, &config, "delete-model").await {
        return HttpResponse::Forbidden().finish();
    }

    delete_model_path(&body.path).await
}

#[post("/delete-form")]
pub async fn delete_model_form(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    form: web::Form<DeleteModelRequest>,
) -> impl Responder {
    if !can(&req, &config, "delete-model").await {
        return HttpResponse::Forbidden().finish();
    }

    match delete_model_path(&form.path).await {
        response if response.status().is_success() => HttpResponse::Ok()
            .content_type(ContentType::html())
            .append_header(("HX-Trigger", "models-refresh"))
            .body(r#"<div id="confirm-delete-modal" class="estimates-modal"></div>"#),
        response => response,
    }
}

async fn delete_model_path(model_path: &str) -> HttpResponse {
    let path = Path::new(model_path);

    // Security check: ensure the path is within HF_ROOTS or GGUF_ROOTS
    let is_valid_hf = HF_ROOTS.iter().any(|root| path.starts_with(root));
    let is_valid_gguf = GGUF_ROOTS.iter().any(|root| path.starts_with(root));

    if !is_valid_hf && !is_valid_gguf {
        return HttpResponse::Forbidden().body("api_error_invalid_model_path");
    }

    if !path.exists() {
        return HttpResponse::NotFound().body("api_error_model_not_found");
    }

    let res = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };

    match res {
        Ok(_) => {
            get_store().remove_model(model_path).await;
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            tracing::error!("Failed to delete model {}: {}", model_path, e);
            HttpResponse::InternalServerError().body("api_error_model_delete_failed")
        }
    }
}
