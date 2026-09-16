use crate::domain::docker_error;
use crate::routers::docker::{AppendError, RawBody, append_body_to_upload, upload_path};
use quench_http::prelude::{Path, Response, http::StatusCode, patch};
use quench_starter::http::domain::error;

// Pattern must match the sibling upload handlers byte-for-byte (quench-http keys routes by pattern string).
#[patch("/v2/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(
    Path((name, uuid)): Path<(String, String)>,
    RawBody(body): RawBody,
) -> Response {
    let Some(file_path) = upload_path(&name, &uuid) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };

    let current_size = match tokio::fs::metadata(&file_path).await {
        Ok(m) => m.len(),
        Err(_) => {
            return error::response(
                StatusCode::NOT_FOUND,
                docker_error::BLOB_UNKNOWN,
                "blob upload unknown to registry",
            );
        }
    };

    let written = match append_body_to_upload(&file_path, current_size, body).await {
        Ok(written) => written,
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
    };

    if written == 0 {
        return error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "empty upload chunk",
        );
    }

    let new_size = current_size + written;

    Response::new(StatusCode::ACCEPTED)
        .header("range", format!("0-{}", new_size - 1))
        .header("docker-upload-uuid", &uuid)
        .header("location", format!("/v2/{}/blobs/uploads/{}", name, uuid))
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
