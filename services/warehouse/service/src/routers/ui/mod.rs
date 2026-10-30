use quench_http::prelude::{Path, Response, get, http::StatusCode};
use quench_starter::common::routes::with_base_path;

pub mod api;
pub mod authz;
pub mod common;
pub mod pages;

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

/// Links to a repository's tags (old bookmarks) land on the catalog with that repository open.
#[get("/ui/docker/tags/{repository:.+}")]
pub async fn docker_tags(
    common::PageAuth(authenticated): common::PageAuth,
    Path(repository): Path<String>,
) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT).header(
        "Location",
        with_base_path(&format!("/ui/docker/catalog?repo={repository}")),
    )
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

// Rivet redirects

#[get("/ui/rivets")]
pub async fn rivets_root(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/rivets/catalog"))
}

#[get("/ui/rivets/")]
pub async fn rivets_root_slash(common::PageAuth(authenticated): common::PageAuth) -> Response {
    if !authenticated {
        return common::ui_login_redirect();
    }
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/ui/rivets/catalog"))
}

pub fn register_routes() {
    let _ = root as fn(_) -> _;
    let _ = root_slash as fn(_) -> _;
    let _ = docker_root as fn(_) -> _;
    let _ = docker_root_slash as fn(_) -> _;
    let _ = docker_tags as fn(_, _) -> _;
    let _ = crates_root as fn(_) -> _;
    let _ = crates_root_slash as fn(_) -> _;
    let _ = files_root as fn(_) -> _;
    let _ = files_root_slash as fn(_) -> _;
    let _ = artifacts_root as fn(_) -> _;
    let _ = artifacts_root_slash as fn(_) -> _;
    let _ = rivets_root as fn(_) -> _;
    let _ = rivets_root_slash as fn(_) -> _;
    let _ = apk_root as fn(_) -> _;
    let _ = apk_root_slash as fn(_) -> _;
    api::register_routes();
    pages::register_routes();
}
