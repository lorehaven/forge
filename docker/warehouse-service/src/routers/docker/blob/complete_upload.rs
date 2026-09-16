use crate::domain::docker_error;
use crate::routers::docker::{
    AppendError, DigestQuery, RawBody, append_body_to_upload, blob_path, upload_path,
    validate_digest,
};
use crate::utils::sha256::sha256_file;
use quench_http::prelude::{Path, Query, Response, http::StatusCode, put};
use quench_starter::http::domain::error;

// Pattern must match the sibling upload handlers byte-for-byte (quench-http keys routes by pattern string).
#[put("/v2/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(
    Path((name, uuid)): Path<(String, String)>,
    Query(query): Query<DigestQuery>,
    RawBody(body): RawBody,
) -> Response {
    let digest = &query.digest;

    if !validate_digest(digest) {
        return error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    }

    let Some(upload_file) = upload_path(&name, &uuid) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let Some(final_path) = blob_path(digest) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    };

    if tokio::fs::metadata(&final_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&upload_file).await;
        return Response::new(StatusCode::CREATED)
            .header("location", format!("/v2/{name}/blobs/{digest}"))
            .header("docker-content-digest", digest);
    }

    let current_size = match tokio::fs::metadata(&upload_file).await {
        Ok(m) => m.len(),
        Err(_) => {
            return error::response(
                StatusCode::NOT_FOUND,
                docker_error::BLOB_UNKNOWN,
                "blob upload unknown to registry",
            );
        }
    };

    match append_body_to_upload(&upload_file, current_size, body).await {
        Ok(_) => {}
        Err(AppendError::TooLarge(_)) => {
            return error::response(
                StatusCode::PAYLOAD_TOO_LARGE,
                error::UNSUPPORTED,
                "blob exceeds the maximum allowed size",
            );
        }
        Err(AppendError::Read) => {
            return error::response(
                StatusCode::BAD_REQUEST,
                error::UNSUPPORTED,
                "the upload was interrupted",
            );
        }
        Err(AppendError::Write) => {
            return error::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    }

    let computed = match sha256_file(&upload_file).await {
        Ok(hex) => format!("sha256:{hex}"),
        Err(_) => {
            if tokio::fs::metadata(&final_path).await.is_ok() {
                let _ = tokio::fs::remove_file(&upload_file).await;
                return Response::new(StatusCode::CREATED)
                    .header("location", format!("/v2/{name}/blobs/{digest}"))
                    .header("docker-content-digest", digest);
            }
            return error::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    if &computed != digest {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::BLOB_UNKNOWN,
            "digest invalid",
        );
    }

    let Some(final_parent) = final_path.parent() else {
        return error::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    };
    if tokio::fs::create_dir_all(final_parent).await.is_err() {
        return error::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    if tokio::fs::metadata(&final_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&upload_file).await;
        return Response::new(StatusCode::CREATED)
            .header("location", format!("/v2/{name}/blobs/{digest}"))
            .header("docker-content-digest", digest);
    }

    if let Err(err) = tokio::fs::rename(&upload_file, &final_path).await {
        if tokio::fs::metadata(&final_path).await.is_ok() {
            let _ = tokio::fs::remove_file(&upload_file).await;
        } else {
            let _ = err;
            return error::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    }

    Response::new(StatusCode::CREATED)
        .header("location", format!("/v2/{name}/blobs/{digest}"))
        .header("docker-content-digest", digest)
}

pub fn register_routes() {
    let _ = handle as fn(_, _, _) -> _;
}
