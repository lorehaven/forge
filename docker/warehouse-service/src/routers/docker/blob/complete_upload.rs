use crate::domain::docker_error;
use crate::routers::docker::{DigestQuery, blob_path, upload_path, validate_digest};
use crate::utils::sha256::sha256_hex;
use actix_web::{HttpResponse, Responder, put, web};
use quench_srv::prelude::error;

#[put("/{name:.*}/blobs/uploads/{uuid}")]
pub async fn handle(
    path: web::Path<(String, String)>,
    query: web::Query<DigestQuery>,
    body: web::Bytes,
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

    if !upload_file.exists() {
        return error::response(
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
                return error::response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    error::UNSUPPORTED,
                    "internal server error",
                );
            }
        };

        if file.write_all(&body).await.is_err() {
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
        if file.sync_data().await.is_err() {
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
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
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    // Verify digest
    let computed = format!("sha256:{}", sha256_hex(&data));

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
