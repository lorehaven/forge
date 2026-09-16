pub use common::assets;
use quench_http::prelude::{Response, get, http::StatusCode};
use quench_starter::common::routes::with_base_path;
use serde::Deserialize;

pub mod authz;
pub mod common;
pub mod pages;

#[derive(Deserialize)]
pub struct PageQuery {
    /// Selected crate name (or docker repository, or artifact program)
    pub repo: Option<String>,
    /// Selected version (or docker tag)
    pub tag: Option<String>,
    /// Selected platform tag - artifact catalog only; ignored elsewhere.
    pub platform: Option<String>,
}

// --- Root redirects ---

#[get("/ui")]
pub async fn root(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui/")]
pub async fn root_slash(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

// Docker redirects

#[get("/ui/docker")]
pub async fn docker_root(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/docker/catalog"))
}

#[get("/ui/docker/")]
pub async fn docker_root_slash(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/docker/catalog"))
}

// Crates redirects

#[get("/ui/crates")]
pub async fn crates_root(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/crates/catalog"))
}

#[get("/ui/crates/")]
pub async fn crates_root_slash(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/crates/catalog"))
}

// Files redirects

#[get("/ui/files")]
pub async fn files_root(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/files/storages"))
}

#[get("/ui/files/")]
pub async fn files_root_slash(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/files/storages"))
}

// Artifact redirects (`/apk` kept for old bookmarks)

#[get("/ui/artifacts")]
pub async fn artifacts_root(auth: common::PageAuth) -> Response {
    artifacts_redirect(auth)
}

#[get("/ui/artifacts/")]
pub async fn artifacts_root_slash(auth: common::PageAuth) -> Response {
    artifacts_redirect(auth)
}

#[get("/ui/apk")]
pub async fn apk_root(auth: common::PageAuth) -> Response {
    artifacts_redirect(auth)
}

#[get("/ui/apk/")]
pub async fn apk_root_slash(auth: common::PageAuth) -> Response {
    artifacts_redirect(auth)
}

fn artifacts_redirect(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/artifacts/catalog"))
}

pub fn register_routes() {
    let _ = root as fn(_) -> _;
    let _ = root_slash as fn(_) -> _;
    let _ = docker_root as fn(_) -> _;
    let _ = docker_root_slash as fn(_) -> _;
    let _ = crates_root as fn(_) -> _;
    let _ = crates_root_slash as fn(_) -> _;
    let _ = files_root as fn(_) -> _;
    let _ = files_root_slash as fn(_) -> _;
    let _ = artifacts_root as fn(_) -> _;
    let _ = artifacts_root_slash as fn(_) -> _;
    let _ = apk_root as fn(_) -> _;
    let _ = apk_root_slash as fn(_) -> _;
    let _ = assets as fn(_) -> _;
    common::register_routes();
    pages::register_routes();
}
