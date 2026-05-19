use actix_web::{HttpResponse, get};
use std::sync::LazyLock;
use utoipa::OpenApi;

pub mod admin;
pub mod crates;
pub mod docker;
pub mod files;
pub mod health;
pub mod ui;

static CRATES_STORAGE_ROOT: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("CRATES_STORAGE_PATH", "./storage/crates"));

static DOCKER_STORAGE_ROOT: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("STORAGE_PATH", "./storage/docker"));

static BASE_PATH: LazyLock<String> =
    LazyLock::new(|| normalize_base_path(&envmnt::get_or("BASE_PATH", "/")));

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

pub fn with_base_path(path: &str) -> String {
    if BASE_PATH.as_str() == "/" {
        return path.to_string();
    }

    match path {
        "" => BASE_PATH.clone(),
        "/" => format!("{}/", BASE_PATH.as_str()),
        _ => format!("{}{}", BASE_PATH.as_str(), path),
    }
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

#[get("/swagger-ui")]
async fn swagger_redirect() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/swagger-ui/")))
        .finish()
}

#[get("/swagger-ui/")]
async fn swagger_index_redirect() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/swagger-ui/index.html")))
        .finish()
}

fn normalize_base_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }

    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.is_empty() {
        "/".to_string()
    } else if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{without_trailing}")
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_base_path, with_base_path};

    #[test]
    fn normalizes_base_path_values() {
        assert_eq!(normalize_base_path(""), "/");
        assert_eq!(normalize_base_path("/"), "/");
        assert_eq!(normalize_base_path("warehouse"), "/warehouse");
        assert_eq!(normalize_base_path("/warehouse"), "/warehouse");
        assert_eq!(normalize_base_path("/warehouse/"), "/warehouse");
    }

    #[test]
    fn prefixes_redirect_paths() {
        assert!(with_base_path("/").starts_with('/'));
        assert!(with_base_path("/swagger-ui").starts_with('/'));
    }
}
