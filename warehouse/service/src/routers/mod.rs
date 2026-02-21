use actix_web::{HttpResponse, get};
use std::sync::LazyLock;
use utoipa::OpenApi;

pub mod admin;
pub mod crates;
pub mod docker;
pub mod health;
pub mod ui;

static DOCKER_STORAGE_ROOT: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("STORAGE_PATH", "./storage"));

#[derive(OpenApi)]
#[openapi(
    nest(
        (path = "/admin", api = admin::AdminApiDoc),
        (path = "/token", api = docker::DockerAuthApiDoc),
        (path = "/v1/api/crates", api = crates::CratesApiDoc),
        (path = "/v2", api = docker::DockerApiDoc),
        (path = "/health", api = health::HealthApiDoc),
    )
)]
pub struct OpenApiDoc;

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
