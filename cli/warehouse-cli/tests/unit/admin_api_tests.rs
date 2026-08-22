use base64::{Engine as _, engine::general_purpose::STANDARD};
use warehouse_cli::api::admin_api::{AdminApi, admin_base_url};
use warehouse_cli::domain::{
    RegistryConfig, RegistryCratesConfig, RegistryDockerConfig, service_url,
};

const CRATES_GC: &str = "/admin/crates/gc";
const DOCKER_GC: &str = "/admin/docker/gc";

fn registry(docker_url: &str, crates_url: &str, base_path: &str) -> RegistryConfig {
    RegistryConfig {
        base_path: base_path.to_string(),
        docker: RegistryDockerConfig {
            url: docker_url.to_string(),
            path: "/v2".to_string(),
            ..RegistryDockerConfig::default()
        },
        crates: RegistryCratesConfig {
            url: crates_url.to_string(),
            ..RegistryCratesConfig::default()
        },
        ..RegistryConfig::default()
    }
}

/// Both admin endpoints sit in the same server-side scope, so they have to
/// resolve the same way. Resolving them differently is what made
/// `admin gc --docker` 404 while `--crates` worked.
#[test]
fn both_gc_endpoints_resolve_against_the_same_base() {
    let reg = registry("https://example.net", "https://example.net/warehouse", "");

    let crates = service_url(admin_base_url(&reg), &reg.base_path, CRATES_GC).unwrap();
    let docker = service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap();

    assert_eq!(crates, "https://example.net/warehouse/admin/crates/gc");
    assert_eq!(docker, "https://example.net/warehouse/admin/docker/gc");
}

/// A registry may carry the service prefix in the crates URL instead of in
/// `base_path` — the docker URL stays bare, because `/v2` is served outside
/// the base-path scope.
#[test]
fn the_base_path_may_live_in_the_crates_url() {
    let reg = registry("https://example.net", "https://example.net/warehouse", "");

    assert_eq!(
        service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
        "https://example.net/warehouse/admin/docker/gc"
    );
}

/// ... or in `base_path`, with both URLs bare.
#[test]
fn the_base_path_may_live_in_the_base_path_field() {
    let reg = registry("https://example.net", "https://example.net", "/warehouse");

    assert_eq!(
        service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
        "https://example.net/warehouse/admin/docker/gc"
    );
}

#[test]
fn a_service_at_the_root_needs_no_prefix() {
    let reg = registry(
        "https://registry.local:8443",
        "https://registry.local:8443",
        "",
    );

    assert_eq!(
        service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
        "https://registry.local:8443/admin/docker/gc"
    );
}

#[test]
fn a_docker_only_registry_falls_back_to_the_docker_host() {
    let reg = registry("https://example.net", "", "/warehouse");

    assert_eq!(admin_base_url(&reg), "https://example.net");
    assert_eq!(
        service_url(admin_base_url(&reg), &reg.base_path, DOCKER_GC).unwrap(),
        "https://example.net/warehouse/admin/docker/gc"
    );
}

/// The docker registry itself is mounted at the server root, outside the
/// base path — so admin resolution must not be reused for `/v2`.
#[test]
fn the_docker_registry_root_stays_unprefixed() {
    let reg = registry("https://example.net", "https://example.net/warehouse", "");

    assert_eq!(
        warehouse_cli::domain::api_url(&reg, "/_catalog").unwrap(),
        "https://example.net/v2/_catalog"
    );
}

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn run_crates_gc_parses_the_report_and_sends_a_bearer_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/crates/gc"))
        .and(header("authorization", "Bearer secret-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted_crates": 3,
            "kept_crates": 10,
            "removed_index_entries": 1,
            "deleted_owner_files": 0,
            "removed_empty_dirs": 2
        })))
        .mount(&server)
        .await;

    let mut reg = registry(&server.uri(), &server.uri(), "");
    reg.crates.token = Some("secret-token".to_string());
    let api = AdminApi::new(&reg).expect("build client");

    let report = api.run_crates_gc(&reg, CRATES_GC).await.expect("gc report");
    assert_eq!(report.deleted_crates, 3);
    assert_eq!(report.kept_crates, 10);
    assert_eq!(report.removed_empty_dirs, 2);
}

#[tokio::test]
async fn run_crates_gc_omits_the_auth_header_without_a_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/crates/gc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted_crates": 0,
            "kept_crates": 0,
            "removed_index_entries": 0,
            "deleted_owner_files": 0,
            "removed_empty_dirs": 0
        })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), &server.uri(), "");
    let api = AdminApi::new(&reg).expect("build client");
    api.run_crates_gc(&reg, CRATES_GC).await.expect("gc report");
}

#[tokio::test]
async fn run_crates_gc_reports_a_non_success_status_with_the_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/crates/gc"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), &server.uri(), "");
    let api = AdminApi::new(&reg).expect("build client");
    let error = api.run_crates_gc(&reg, CRATES_GC).await.unwrap_err();
    assert!(error.to_string().contains("boom"));
}

#[tokio::test]
async fn run_docker_gc_parses_the_report_and_sends_basic_auth() {
    let server = MockServer::start().await;
    let expected = format!(
        "Basic {}",
        STANDARD.encode(format!("{}:{}", "user", "pass"))
    );
    Mock::given(method("POST"))
        .and(path("/admin/docker/gc"))
        .and(header("authorization", expected.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted": 5,
            "kept": 7
        })))
        .mount(&server)
        .await;

    let mut reg = registry(&server.uri(), "", "");
    reg.docker.username = Some("user".to_string());
    reg.docker.password = Some("pass".to_string());
    let api = AdminApi::new(&reg).expect("build client");

    let report = api.run_docker_gc(&reg, DOCKER_GC).await.expect("gc report");
    assert_eq!(report.deleted, 5);
    assert_eq!(report.kept, 7);
}

#[tokio::test]
async fn run_docker_gc_errors_when_username_is_set_without_a_password() {
    let server = MockServer::start().await;
    let mut reg = registry(&server.uri(), "", "");
    reg.docker.username = Some("user".to_string());
    let api = AdminApi::new(&reg).expect("build client");

    let error = api.run_docker_gc(&reg, DOCKER_GC).await.unwrap_err();
    assert!(error.to_string().contains("missing password"));
}

#[tokio::test]
async fn run_docker_gc_reports_a_non_success_status_with_the_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/docker/gc"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), "", "");
    let api = AdminApi::new(&reg).expect("build client");
    let error = api.run_docker_gc(&reg, DOCKER_GC).await.unwrap_err();
    assert!(error.to_string().contains("forbidden"));
}
