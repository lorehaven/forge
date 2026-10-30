use crate::domain::docker_error;
use crate::routers::docker::{
    AppendError, DigestQuery, RawBody, append_body_to_upload, blob_path, upload_path,
    validate_digest,
};
use crate::utils::sha256::sha256_file;
use quench_http::prelude::{Path, Query, Response, http::StatusCode, put};
use quench_starter::http::domain::error;

/// `201` for a blob that is (now) in the store.
fn created(name: &str, digest: &str) -> Response {
    Response::new(StatusCode::CREATED)
        .header("location", format!("/v2/{name}/blobs/{digest}"))
        .header("docker-content-digest", digest)
}

fn internal_error() -> Response {
    error::response(
        StatusCode::INTERNAL_SERVER_ERROR,
        error::UNSUPPORTED,
        "internal server error",
    )
}

/// What a failed append to the upload file means to the client.
fn append_error_response(err: &AppendError) -> Response {
    match err {
        AppendError::TooLarge(_) => error::response(
            StatusCode::PAYLOAD_TOO_LARGE,
            error::UNSUPPORTED,
            "blob exceeds the maximum allowed size",
        ),
        AppendError::Read => error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "the upload was interrupted",
        ),
        AppendError::Write => internal_error(),
    }
}

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
        return created(&name, digest);
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

    if let Err(err) = append_body_to_upload(&upload_file, current_size, body).await {
        return append_error_response(&err);
    }

    let computed = if let Ok(hex) = sha256_file(&upload_file).await {
        format!("sha256:{hex}")
    } else {
        if tokio::fs::metadata(&final_path).await.is_ok() {
            let _ = tokio::fs::remove_file(&upload_file).await;
            return created(&name, digest);
        }
        return internal_error();
    };

    if &computed != digest {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::BLOB_UNKNOWN,
            "digest invalid",
        );
    }

    let Some(final_parent) = final_path.parent() else {
        return internal_error();
    };
    if tokio::fs::create_dir_all(final_parent).await.is_err() {
        return internal_error();
    }

    if tokio::fs::metadata(&final_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&upload_file).await;
        return created(&name, digest);
    }

    if let Err(err) = tokio::fs::rename(&upload_file, &final_path).await {
        if tokio::fs::metadata(&final_path).await.is_ok() {
            let _ = tokio::fs::remove_file(&upload_file).await;
        } else {
            let _ = err;
            return internal_error();
        }
    }

    created(&name, digest)
}

pub fn register_routes() {
    let _ = handle as fn(_, _, _) -> _;
}
