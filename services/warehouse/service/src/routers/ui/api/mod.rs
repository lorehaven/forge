//! The JSON the browser client reads and writes, behind the session check the old pages had.
//!
//! These are the reads the old pages made straight from storage and the database. The machine-facing registry routes
//! (`/v2`, `/index`, `/api/v1`, `/admin`) are not touched.
//!
//! No session answers 401 (the client shows its sign-in link). A change that needs the blanket `warehouse:write` grant
//! answers 403 `api_error_forbidden` without it. An error is `{"error": "<text or translation key>"}`.

use crate::routers::ui::authz::{can_manage, ui_claims};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::async_trait;
use quench_http::prelude::{FromRequest, HttpError, Request, Response, get, http::StatusCode};
use quench_serde::Serialize;
use warehouse_core::{Features, Overview};

pub mod artifacts;
pub mod crates;
pub mod docker;
pub mod files;
pub mod rivets;

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
}

/// `{"error": text}` with `status`.
#[must_use]
pub fn error(status: StatusCode, text: &str) -> Response {
    Response::json(status, &ErrorBody { error: text })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// 200 with `value` as JSON.
#[must_use]
pub fn ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[must_use]
pub fn no_content() -> Response {
    Response::new(StatusCode::NO_CONTENT)
}

#[must_use]
pub fn internal() -> Response {
    error(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
}

/// The caller as the session cookie says (`None`: no usable session). With auth switched off the claims are an all-access
/// stand-in, as for the old pages.
#[derive(Debug)]
pub struct Session(pub Option<Claims>);

#[async_trait]
impl FromRequest for Session {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self(None));
        };
        Ok(Self(ui_claims(req, &config).await))
    }
}

impl Session {
    /// The claims, or the 401 to answer with.
    ///
    /// # Errors
    /// 401 when there is no session.
    pub fn require(self) -> Result<Claims, Response> {
        self.0
            .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "sign in"))
    }

    /// The claims of a caller who may change things, or the 401/403 to answer with.
    ///
    /// # Errors
    /// 401 without a session, 403 without the blanket write grant.
    pub fn manager(self) -> Result<Claims, Response> {
        let claims = self.require()?;
        if can_manage(&claims) {
            Ok(claims)
        } else {
            Err(error(StatusCode::FORBIDDEN, "api_error_forbidden"))
        }
    }
}

/// Who is asking and which registries are on: what the client needs before it draws anything.
#[get("/ui/api/overview")]
pub async fn overview(session: Session) -> Response {
    let claims = match session.require() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    ok(&Overview {
        username: claims.sub.clone(),
        can_manage: can_manage(&claims),
        features: Features {
            docker: crate::routers::docker_enabled(),
            crates: crate::routers::crates_enabled(),
            files: crate::routers::files_enabled(),
            artifacts: crate::routers::artifacts_enabled(),
            rivets: crate::routers::rivets_enabled(),
        },
    })
}

pub fn register_routes() {
    let _ = overview as fn(_) -> _;
    artifacts::register_routes();
    crates::register_routes();
    docker::register_routes();
    files::register_routes();
    rivets::register_routes();
}
