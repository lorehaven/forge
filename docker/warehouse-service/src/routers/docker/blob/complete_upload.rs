use crate::domain::docker_error;
use crate::routers::docker::{
    AppendError, DigestQuery, append_body_to_upload, blob_path, upload_path, validate_digest,
};
use crate::utils::sha256::sha256_file;
use actix_web::{HttpResponse, Responder, put, web};
use quench_starter::prelude::error;

#[put("/{name:.*}/blobs/uploads/{uuid}")]
pub async fn handle(
    path: web::Path<(String, String)>,
    query: web::Query<DigestQuery>,
    mut body: web::Payload,
) -> impl Responder {
    let (name, uuid) = path.into_inner();
    let digest = &query.digest;

    if !validate_digest(digest) {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    }

    let Some(upload_file) = upload_path(&name, &uuid) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let Some(final_path) = blob_path(digest) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    };

    if tokio::fs::metadata(&final_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&upload_file).await;
        return HttpResponse::Created()
            .append_header(("Location", format!("/v2/{name}/blobs/{digest}")))
            .append_header(("Docker-Content-Digest", digest.clone()))
            .finish();
    }

    let current_size = match tokio::fs::metadata(&upload_file).await {
        Ok(m) => m.len(),
        Err(_) => {
            return error::response(
                actix_web::http::StatusCode::NOT_FOUND,
                docker_error::BLOB_UNKNOWN,
                "blob upload unknown to registry",
            );
        }
    };

    // A monolithic push sends the whole blob in this final PUT with no prior
    // PATCH; stream it onto the upload file rather than buffering it.
    match append_body_to_upload(&upload_file, current_size, &mut body).await {
        Ok(_) => {}
        Err(AppendError::TooLarge(_)) => {
            return error::response(
                actix_web::http::StatusCode::PAYLOAD_TOO_LARGE,
                error::UNSUPPORTED,
                "blob exceeds the maximum allowed size",
            );
        }
        Err(AppendError::Read) => {
            return error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                error::UNSUPPORTED,
                "the upload was interrupted",
            );
        }
        Err(AppendError::Write) => {
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    }

    // Hash the assembled blob straight off disk - 64 KiB at a time, never the
    // whole layer in memory.
    let computed = match sha256_file(&upload_file).await {
        Ok(hex) => format!("sha256:{hex}"),
        Err(_) => {
            if tokio::fs::metadata(&final_path).await.is_ok() {
                return HttpResponse::Created()
                    .append_header(("Location", format!("/v2/{name}/blobs/{digest}")))
                    .append_header(("Docker-Content-Digest", digest.clone()))
                    .finish();
            }
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    if &computed != digest {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::BLOB_UNKNOWN,
            "digest invalid",
        );
    }

    let Some(final_parent) = final_path.parent() else {
        return error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    };
    if tokio::fs::create_dir_all(final_parent).await.is_err() {
        return error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    if tokio::fs::metadata(&final_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&upload_file).await;
        return HttpResponse::Created()
            .append_header(("Location", format!("/v2/{name}/blobs/{digest}")))
            .append_header(("Docker-Content-Digest", digest.clone()))
            .finish();
    }

    // Atomic move
    if let Err(err) = tokio::fs::rename(&upload_file, &final_path).await {
        if tokio::fs::metadata(&final_path).await.is_ok() {
            let _ = tokio::fs::remove_file(&upload_file).await;
        } else {
            let _ = err;
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    }

    HttpResponse::Created()
        .append_header(("Location", format!("/v2/{name}/blobs/{digest}")))
        .append_header(("Docker-Content-Digest", digest.clone()))
        .finish()
}
