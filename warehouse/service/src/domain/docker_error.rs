use actix_web::{HttpResponse, http::StatusCode};
use serde::Serialize;
use serde_json::{Value, json};

pub const BLOB_UNKNOWN: &str = "BLOB_UNKNOWN";
pub const MANIFEST_UNKNOWN: &str = "MANIFEST_UNKNOWN";
pub const NAME_UNKNOWN: &str = "NAME_UNKNOWN";
pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const DENIED: &str = "DENIED";
pub const UNSUPPORTED: &str = "UNSUPPORTED";

#[derive(Serialize)]
struct DockerErrorBody {
    errors: Vec<DockerErrorEntry>,
}

#[derive(Serialize)]
struct DockerErrorEntry {
    code: &'static str,
    message: &'static str,
    detail: Value,
}

pub fn response(status: StatusCode, code: &'static str, message: &'static str) -> HttpResponse {
    response_with_detail(status, code, message, json!({}))
}

pub fn response_with_detail(
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    detail: Value,
) -> HttpResponse {
    HttpResponse::build(status).json(DockerErrorBody {
        errors: vec![DockerErrorEntry {
            code,
            message,
            detail,
        }],
    })
}
