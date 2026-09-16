use super::mod_impl::{GGUF_ROOTS, HF_ROOTS, OptionalClaims, can};
use super::store::get_store;
use super::types::DeleteModelRequest;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Form, Inject, Json, Response, post};
use std::path::Path;

#[post("/api/v1/models/delete")]
pub async fn delete_model(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Json(body): Json<DeleteModelRequest>,
) -> Response {
    if !can(claims.as_ref(), &config, "delete-model") {
        return Response::new(http::StatusCode::FORBIDDEN);
    }

    delete_model_path(&body.path).await
}

#[post("/api/v1/models/delete-form")]
pub async fn delete_model_form(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Form(form): Form<DeleteModelRequest>,
) -> Response {
    if !can(claims.as_ref(), &config, "delete-model") {
        return Response::new(http::StatusCode::FORBIDDEN);
    }

    let response = delete_model_path(&form.path).await;
    if response.status().is_success() {
        Response::html(
            http::StatusCode::OK,
            r#"<div id="confirm-delete-modal" class="estimates-modal"></div>"#,
        )
        .header("HX-Trigger", "models-refresh")
    } else {
        response
    }
}

async fn delete_model_path(model_path: &str) -> Response {
    let path = Path::new(model_path);

    let is_valid_hf = HF_ROOTS.iter().any(|root| path.starts_with(root));
    let is_valid_gguf = GGUF_ROOTS.iter().any(|root| path.starts_with(root));

    if !is_valid_hf && !is_valid_gguf {
        return Response::text(http::StatusCode::FORBIDDEN, "api_error_invalid_model_path");
    }

    if !path.exists() {
        return Response::text(http::StatusCode::NOT_FOUND, "api_error_model_not_found");
    }

    let res = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };

    match res {
        Ok(_) => {
            get_store().remove_model(model_path).await;
            Response::new(http::StatusCode::OK)
        }
        Err(e) => {
            tracing::error!("Failed to delete model {}: {}", model_path, e);
            Response::text(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_model_delete_failed",
            )
        }
    }
}
