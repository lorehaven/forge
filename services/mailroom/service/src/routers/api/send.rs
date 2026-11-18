//! `POST /api/v1/send`: how a service sends mail.

use super::reply;
use crate::app::App;
use crate::error::Error;
use crate::outgoing;
use mailroom_core::SendRequest;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::async_trait;
use quench_http::prelude::{
    FromRequest, HttpError, Inject, Json, Limited, Request, Response, http::StatusCode, post,
};

/// The service that is calling: the subject of its token (a machine identity). With authentication off, `dev`.
#[derive(Debug)]
pub struct Sender(pub String);

#[async_trait]
impl FromRequest for Sender {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let config = req
            .container()
            .get::<JwtConfig>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if !config.auth_enabled {
            return Ok(Self("dev".into()));
        }
        req.extensions()
            .get::<Claims>()
            .map(|c| Self(c.sub.clone()))
            .ok_or_else(|| HttpError::status(StatusCode::UNAUTHORIZED, "sign in first"))
    }
}

#[post("/api/v1/send")]
pub async fn send(
    Inject(app): Inject<App>,
    Sender(source): Sender,
    Limited(Json(body)): Limited<Json<SendRequest>, { super::SEND_LIMIT }>,
) -> Response {
    let result = outgoing::send_service(&app, &source, &body).await;
    match result {
        Ok(queued) => {
            let status = if queued.status == "duplicate" {
                StatusCode::OK
            } else {
                StatusCode::ACCEPTED
            };
            Response::json(status, &queued)
                .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
        }
        Err(Error::Upstream(text)) => Response::json(
            StatusCode::SERVICE_UNAVAILABLE,
            &mailroom_core::ApiError { error: text },
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => reply::<()>(Err(e)),
    }
}

pub fn register_routes() {
    let _ = send as fn(_, _, _) -> _;
}
