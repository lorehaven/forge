use actix_web::{HttpResponse, http::StatusCode};
use serde::Serialize;
use serde_json::{Value, json};

pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const DENIED: &str = "DENIED";
pub const UNSUPPORTED: &str = "UNSUPPORTED";

#[derive(Serialize)]
struct AuthErrorBody {
    errors: Vec<AuthErrorEntry>,
}

#[derive(Serialize)]
struct AuthErrorEntry {
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
    HttpResponse::build(status).json(AuthErrorBody {
        errors: vec![AuthErrorEntry {
            code,
            message,
            detail,
        }],
    })
}
