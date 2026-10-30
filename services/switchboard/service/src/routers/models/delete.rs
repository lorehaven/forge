use super::mod_impl::{GGUF_ROOTS, HF_ROOTS, OptionalClaims, can};
use super::store::get_store;
use super::types::DeleteModelRequest;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Form, Inject, Json, Response, post};
use std::path::{Component, Path};

#[post("/api/v1/models/delete")]
pub async fn delete_model(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Json(body): Json<DeleteModelRequest>,
) -> Response {
    if !can(claims.as_ref(), &config, "delete-model") {
        return Response::new(quench_http::http::StatusCode::FORBIDDEN);
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
        return Response::new(quench_http::http::StatusCode::FORBIDDEN);
    }

    let response = delete_model_path(&form.path).await;
    if response.status().is_success() {
        Response::html(
            quench_http::http::StatusCode::OK,
            r#"<div id="confirm-delete-modal" class="estimates-modal"></div>"#,
        )
        .header("HX-Trigger", "models-refresh")
    } else {
        response
    }
}

/// Whether `path` lies strictly below one of `roots`: no `..` components, and never a root itself.
#[must_use]
pub fn is_below_root(path: &Path, roots: &[String]) -> bool {
    !path.components().any(|c| c == Component::ParentDir)
        && roots
            .iter()
            .any(|root| path != Path::new(root) && path.starts_with(root))
}

/// The same check on resolved paths, so a symlink cannot lead out of the roots.
fn resolves_below_root(path: &Path, roots: &[String]) -> bool {
    let Ok(resolved) = path.canonicalize() else {
        return false;
    };
    let resolved_roots: Vec<String> = roots
        .iter()
        .filter_map(|root| Path::new(root).canonicalize().ok())
        .map(|root| root.to_string_lossy().into_owned())
        .collect();
    is_below_root(&resolved, &resolved_roots)
}

async fn delete_model_path(model_path: &str) -> Response {
    let path = Path::new(model_path);

    let is_valid_hf = is_below_root(path, &HF_ROOTS);
    let is_valid_gguf = is_below_root(path, &GGUF_ROOTS);

    if !is_valid_hf && !is_valid_gguf {
        return Response::text(
            quench_http::http::StatusCode::FORBIDDEN,
            "api_error_invalid_model_path",
        );
    }

    if !path.exists() {
        return Response::text(
            quench_http::http::StatusCode::NOT_FOUND,
            "api_error_model_not_found",
        );
    }

    if !(is_valid_hf && resolves_below_root(path, &HF_ROOTS)
        || is_valid_gguf && resolves_below_root(path, &GGUF_ROOTS))
    {
        return Response::text(
            quench_http::http::StatusCode::FORBIDDEN,
            "api_error_invalid_model_path",
        );
    }

    let res = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };

    match res {
        Ok(()) => {
            get_store().remove_model(model_path).await;
            Response::new(quench_http::http::StatusCode::OK)
        }
        Err(e) => {
            quench_log::error!("Failed to delete model {}: {}", model_path, e);
            Response::text(
                quench_http::http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_model_delete_failed",
            )
        }
    }
}
