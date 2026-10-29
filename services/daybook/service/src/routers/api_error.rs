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
        DaybookError::Sql(error) if error.is_unique_violation() => {
            ApiError::new(StatusCode::CONFLICT, "that name is already in use")
        }
        DaybookError::Sql(error) => {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
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
