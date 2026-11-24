use crate::domain::DaybookError;
use quench_http::prelude::http::StatusCode;
use quench_starter::http::domain::api_error::ApiError;

#[must_use]
pub fn from_domain(error: DaybookError) -> ApiError {
    match error {
        DaybookError::NotPostgres => {
            ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "daybook requires Postgres")
        }
        DaybookError::Sql(error) if error.code() == Some("23503") => ApiError::new(
            StatusCode::BAD_REQUEST,
            "a referenced project or label does not exist for this user",
        ),
        // A value the database cannot store (a NUL byte, text that is not UTF-8, out of range, too long for its column or
        // index: SQLSTATE class 22, or 54000) is the caller's fault, not a 500 that quotes the database.
        DaybookError::Sql(error)
            if error
                .code()
                .is_some_and(|c| c.starts_with("22") || c == "54000") =>
        {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "a value in the request cannot be stored: it is too long, out of range, or holds a character that is not allowed",
            )
        }
        DaybookError::Sql(error) if error.is_unique_violation() => {
            ApiError::new(StatusCode::CONFLICT, "that name is already in use")
        }
        // The database's own words (a column, a constraint, a SQLSTATE) stay in the log.
        DaybookError::Sql(error) => {
            quench_log::error!("daybook: database error: {error}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

pub fn json<T: quench_serde::Serialize>(
    status: StatusCode,
    value: &T,
) -> quench_http::response::Response {
    quench_http::response::Response::json(status, value)
        .unwrap_or_else(|_| quench_http::response::Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}
