use super::mod_impl::{GGUF_ROOTS, HF_ROOTS, is_admin};
use super::store::get_store;
use super::types::DeleteModelRequest;
use actix_web::web::Json;
use actix_web::{HttpResponse, Responder, post, web};
use quench_srv::prelude::JwtConfig;
use std::path::Path;

#[post("/delete")]
pub async fn delete_model(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    body: Json<DeleteModelRequest>,
) -> impl Responder {
    if !is_admin(&req, &config) {
        return HttpResponse::Forbidden().finish();
    }

    let path = Path::new(&body.path);

    // Security check: ensure the path is within HF_ROOTS or GGUF_ROOTS
    let is_valid_hf = HF_ROOTS.iter().any(|root| path.starts_with(root));
    let is_valid_gguf = GGUF_ROOTS.iter().any(|root| path.starts_with(root));

    if !is_valid_hf && !is_valid_gguf {
        return HttpResponse::Forbidden().body("Invalid model path");
    }

    if !path.exists() {
        return HttpResponse::NotFound().body("Model not found on disk");
    }

    let res = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };

    match res {
        Ok(_) => {
            get_store().remove_model(&body.path).await;
            HttpResponse::Ok().finish()
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}
