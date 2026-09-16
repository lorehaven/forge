use async_trait::async_trait;
pub use common::assets;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{FromRequest, HttpError, Request, Response, get};
use quench_starter::common::routes::with_base_path;

pub mod chat;
pub mod common;
pub mod context_builder;
pub mod pages;

struct UiRoot(bool);

#[async_trait]
impl FromRequest for UiRoot {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(UiRoot(false));
        };
        Ok(UiRoot(common::is_ui_authenticated(req, &config).await))
    }
}

#[get("/ui")]
async fn root(UiRoot(authenticated): UiRoot) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(http::StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui/")]
async fn root_slash(UiRoot(authenticated): UiRoot) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(http::StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

pub fn register_routes() {
    let _ = root as fn(_) -> _;
    let _ = root_slash as fn(_) -> _;
    let _ = assets as fn(_) -> _;
    common::register_routes();
    chat::register_routes();
    pages::register_routes();
}
