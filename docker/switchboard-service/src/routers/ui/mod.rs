pub use common::assets;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::is_ui_authenticated;
use quench_http::prelude::{FromRequest, HttpError, Request, Response, get};
use quench_starter::common::routes::with_base_path;

pub mod common;
pub mod pages;

// Root redirects

pub(crate) struct UiAuthenticated(pub bool);

#[async_trait::async_trait]
impl FromRequest for UiAuthenticated {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(UiAuthenticated(false));
        };
        Ok(UiAuthenticated(is_ui_authenticated(req, &config).await))
    }
}

#[get("/ui")]
async fn root(UiAuthenticated(authenticated): UiAuthenticated) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(http::StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui/")]
async fn root_slash(UiAuthenticated(authenticated): UiAuthenticated) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(http::StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

pub fn register_routes() {
    let _ = root as fn(_) -> _;
    let _ = root_slash as fn(_) -> _;
    let _ = assets as fn(_) -> _;
    pages::register_routes();
}
