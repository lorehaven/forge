use quench_srv::prelude::{routers::BaseOpenApiDoc, with_base_path};
use std::sync::LazyLock;
use utoipa::OpenApi;

pub mod admin;
pub mod crates;
pub mod docker;
pub mod files;
pub mod ui;

static CRATES_STORAGE_ROOT: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("CRATES_STORAGE_PATH", "./storage/crates"));

static DOCKER_STORAGE_ROOT: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("STORAGE_PATH", "./storage/docker"));

struct FeatureFlags {
    docker: bool,
    crates: bool,
    files: bool,
}

static FEATURE_FLAGS: LazyLock<FeatureFlags> = LazyLock::new(|| FeatureFlags {
    docker: feature_enabled("FEATURE_DOCKER_ENABLED", false),
    crates: feature_enabled("FEATURE_CRATES_ENABLED", false),
    files: feature_enabled("FEATURE_FILES_ENABLED", false),
});

fn feature_enabled(name: &str, default: bool) -> bool {
    match envmnt::get_or(name, if default { "true" } else { "false" })
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

pub fn docker_enabled() -> bool {
    FEATURE_FLAGS.docker
}

pub fn crates_enabled() -> bool {
    FEATURE_FLAGS.crates
}

pub fn files_enabled() -> bool {
    FEATURE_FLAGS.files
}

#[derive(OpenApi)]
#[openapi(
    nest(
        (path = "/admin", api = admin::AdminApiDoc),
        (path = "/token", api = docker::DockerAuthApiDoc),
        (path = "/v2", api = docker::DockerApiDoc),
    )
)]
struct DockerOpenApiDoc;

#[derive(OpenApi)]
#[openapi(
    nest(
        (path = "/api/v1/crates", api = crates::CratesApiDoc),
        (path = "/index", api = crates::CratesIndexApiDoc),
    )
)]
struct CratesOpenApiDoc;

#[derive(OpenApi)]
#[openapi(nest((path = "/api/v1/files", api = files::FilesApiDoc),))]
struct FilesOpenApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = BaseOpenApiDoc::openapi();
    if docker_enabled() {
        doc.merge(DockerOpenApiDoc::openapi());
    }
    if crates_enabled() {
        doc.merge(CratesOpenApiDoc::openapi());
    }
    if files_enabled() {
        doc.merge(FilesOpenApiDoc::openapi());
    }
    doc
}
