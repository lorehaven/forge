use actix_web::{HttpResponse, get};
use std::sync::LazyLock;
use utoipa::OpenApi;

pub mod admin;
pub mod crates;
pub mod docker;
pub mod health;
pub mod ui;

static DOCKER_STORAGE_ROOT: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("STORAGE_PATH", "./storage/docker"));

struct FeatureFlags {
    docker: bool,
    crates: bool,
}

static FEATURE_FLAGS: LazyLock<FeatureFlags> = LazyLock::new(|| FeatureFlags {
    docker: feature_enabled("FEATURE_DOCKER_ENABLED", false),
    crates: feature_enabled("FEATURE_CRATES_ENABLED", false),
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

#[derive(OpenApi)]
#[openapi(
    nest((path = "/health", api = health::HealthApiDoc),)
)]
struct BaseOpenApiDoc;

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
        (path = "/v1/api/crates", api = crates::CratesApiDoc),
    )
)]
struct CratesOpenApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = BaseOpenApiDoc::openapi();
    if docker_enabled() {
        doc.merge(DockerOpenApiDoc::openapi());
    }
    if crates_enabled() {
        doc.merge(CratesOpenApiDoc::openapi());
    }
    doc
}

#[get("/swagger-ui")]
async fn swagger_redirect() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .append_header(("Location", "/swagger-ui/"))
        .finish()
}

#[get("/swagger-ui/")]
async fn swagger_index_redirect() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .append_header(("Location", "/swagger-ui/index.html"))
        .finish()
}
