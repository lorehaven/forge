//! What can go wrong, and what the caller is told about it.
//!
//! The text of an internal failure goes to the log; the caller gets a short sentence that says nothing about the database,
//! the vault or the network.

use quench_http::prelude::{Response, http::StatusCode};
use quench_serde::Serialize;

#[derive(Debug, quench_core::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] quench_pg::Error),
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    /// The request is wrong; the text is for the person (never built from secrets).
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    /// Too many requests: try again after this many seconds.
    #[error("slow down")]
    Limited(u64),
    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize)]
struct Body<'a> {
    error: &'a str,
}

impl Error {
    /// The status and the sentence the caller sees.
    #[must_use]
    pub fn parts(&self) -> (StatusCode, String) {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
            Self::Forbidden => (StatusCode::FORBIDDEN, "not allowed".into()),
            Self::Invalid(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Self::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            Self::Limited(_) => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many requests, try again later".into(),
            ),
            // SQLSTATE class 22 is "data exception": the request carried something a column cannot hold (a NUL byte, a number
            // out of range). That is the caller's mistake, not ours.
            Self::Db(e) if e.code().is_some_and(|c| c.starts_with("22")) => {
                (StatusCode::BAD_REQUEST, "the request is not valid".into())
            }
            Self::Db(e) => {
                quench_log::error!("database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "something went wrong".into(),
                )
            }
            Self::Internal(e) => {
                quench_log::error!("internal error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "something went wrong".into(),
                )
            }
        }
    }

    /// The JSON error response.
    #[must_use]
    pub fn response(&self) -> Response {
        let (status, message) = self.parts();
        let mut response = Response::json(status, &Body { error: &message })
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
        if let Self::Limited(seconds) = self {
            response = response.header("retry-after", seconds.to_string());
        }
        response
    }
}

/// Unique-constraint violation (SQLSTATE 23505).
#[must_use]
pub fn is_unique_violation(error: &quench_pg::Error) -> bool {
    error.code() == Some("23505")
}

/// The current time in Unix seconds.
#[must_use]
pub fn now() -> i64 {
    quench_core::time::Time::now().timestamp()
}

/// A fresh identifier.
#[must_use]
pub fn new_id() -> String {
    quench_core::uuid::Uuid::new_v4().simple().to_string()
}
