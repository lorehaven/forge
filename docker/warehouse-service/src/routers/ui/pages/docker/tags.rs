use crate::routers::ui::common::{PageAuth, ui_login_redirect};
use quench_http::prelude::{Path, Response, get, http::StatusCode};
use quench_starter::common::routes::with_base_path;

#[get("/ui/docker/tags/{repository:.+}")]
pub async fn docker_tags(
    PageAuth(authenticated): PageAuth,
    Path(repository): Path<String>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT).header(
        "Location",
        with_base_path(&format!("/ui/docker/catalog?repo={repository}")),
    )
}

pub fn register_routes() {
    let _ = docker_tags as fn(_, _) -> _;
}
