use crate::domain::docker_error;
use crate::routers::docker::{DigestQuery, blob_path, upload_path, validate_digest};
use actix_web::{HttpResponse, Responder, put, web};
use sha2::{Digest, Sha256};

#[utoipa::path(
    put,
    operation_id = "complete_upload",
    tags = ["docker"],
    path = "/{name}/blobs/uploads/{uuid}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("uuid" = String, Path, description = "Upload UUID"),
        ("digest" = String, Query, description = "sha256 digest"),
    ),
    request_body(
        content = String,
        content_type = "application/octet-stream",
        description = "Optional final blob chunk",
    ),
    responses(
        (status = 201, description = "Upload completed successfully"),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Upload session not found"),
        (status = 416, description = "Requested range not satisfiable"),
        (status = 429, description = "Too many requests"),
    )
)]
#[put("/{name:.*}/blobs/uploads/{uuid}")]
pub async fn handle(
    path: web::Path<(String, String)>,
    query: web::Query<DigestQuery>,
    body: web::Bytes,
) -> impl Responder {
    let (name, uuid) = path.into_inner();
    let digest = &query.digest;

    if !validate_digest(digest) {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::UNSUPPORTED,
            "invalid digest",
        );
    }

    let Some(upload_file) = upload_path(&name, &uuid) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let Some(final_path) = blob_path(digest) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::UNSUPPORTED,
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

    if !upload_file.exists() {
        return docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob upload unknown to registry",
        );
    }

    // Append final chunk if present
    if !body.is_empty() {
        use tokio::io::AsyncWriteExt;
        let mut file = match tokio::fs::OpenOptions::new()
            .append(true)
            .open(&upload_file)
            .await
        {
            Ok(f) => f,
            Err(_) => {
                return docker_error::response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    docker_error::UNSUPPORTED,
                    "internal server error",
                );
            }
        };

        if file.write_all(&body).await.is_err() {
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
        if file.sync_data().await.is_err() {
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
    }

    // Read entire file
    let data = match tokio::fs::read(&upload_file).await {
        Ok(d) => d,
        Err(_) => {
            if tokio::fs::metadata(&final_path).await.is_ok() {
                return HttpResponse::Created()
                    .append_header(("Location", format!("/v2/{name}/blobs/{digest}")))
                    .append_header(("Docker-Content-Digest", digest.clone()))
                    .finish();
            }
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    // Verify digest
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let computed = format!("sha256:{:x}", hasher.finalize());

    if &computed != digest {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::BLOB_UNKNOWN,
            "digest invalid",
        );
    }

    let Some(final_parent) = final_path.parent() else {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    };
    if tokio::fs::create_dir_all(final_parent).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
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
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
    }

    HttpResponse::Created()
        .append_header(("Location", format!("/v2/{name}/blobs/{digest}")))
        .append_header(("Docker-Content-Digest", digest.clone()))
        .finish()
}
