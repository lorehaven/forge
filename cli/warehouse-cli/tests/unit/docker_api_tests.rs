use reqwest::header::{HeaderMap, HeaderValue, WWW_AUTHENTICATE};
use warehouse_cli::api::docker_api::{DockerApi, parse_bearer_challenge, to_https_url};
use warehouse_cli::domain::{RegistryConfig, RegistryDockerConfig};
use wiremock::matchers::{basic_auth, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn registry(url: &str) -> RegistryConfig {
    RegistryConfig {
        base_path: String::new(),
        docker: RegistryDockerConfig {
            url: url.to_string(),
            path: "/v2".to_string(),
            ..RegistryDockerConfig::default()
        },
        ..RegistryConfig::default()
    }
}

#[test]
fn should_try_https_fallback_only_for_http_urls_with_the_tls_mismatch_error() {
    assert_eq!(
        to_https_url("http://example.net/x"),
        Some("https://example.net/x".to_string())
    );
    assert_eq!(to_https_url("https://example.net/x"), None);
}

#[test]
fn parse_bearer_challenge_reads_realm_and_service() {
    let mut headers = HeaderMap::new();
    headers.insert(
        WWW_AUTHENTICATE,
        HeaderValue::from_static(
            r#"Bearer realm="https://auth.example.net/token",service="registry.example.net""#,
        ),
    );
    let challenge = parse_bearer_challenge(&headers).expect("challenge");
    assert_eq!(challenge.realm, "https://auth.example.net/token");
    assert_eq!(challenge.service.as_deref(), Some("registry.example.net"));
}

#[test]
fn parse_bearer_challenge_is_none_without_a_bearer_prefix() {
    let mut headers = HeaderMap::new();
    headers.insert(
        WWW_AUTHENTICATE,
        HeaderValue::from_static(r#"Basic realm="x""#),
    );
    assert!(parse_bearer_challenge(&headers).is_none());
}

#[test]
fn parse_bearer_challenge_is_none_without_a_realm() {
    let mut headers = HeaderMap::new();
    headers.insert(
        WWW_AUTHENTICATE,
        HeaderValue::from_static(r#"Bearer service="x""#),
    );
    assert!(parse_bearer_challenge(&headers).is_none());
}

#[tokio::test]
async fn catalog_returns_repositories_when_unauthenticated_access_is_allowed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/_catalog"))
        .and(query_param("n", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "repositories": ["a/b", "c/d"]
        })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = DockerApi::new(&reg).expect("build client");
    let repos = api.catalog(&reg, 50).await.expect("catalog");
    assert_eq!(repos, vec!["a/b".to_string(), "c/d".to_string()]);
}

#[tokio::test]
async fn catalog_surfaces_a_non_401_failure_directly() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/_catalog"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = DockerApi::new(&reg).expect("build client");
    let error = api.catalog(&reg, 50).await.unwrap_err();
    assert!(error.to_string().contains("boom"));
}

#[tokio::test]
async fn catalog_errors_on_a_401_without_a_bearer_challenge() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/_catalog"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = DockerApi::new(&reg).expect("build client");
    let error = api.catalog(&reg, 50).await.unwrap_err();
    assert!(error.to_string().contains("without a Bearer challenge"));
}

#[tokio::test]
async fn catalog_errors_on_a_401_challenge_without_credentials_configured() {
    let server = MockServer::start().await;
    let challenge = format!(r#"Bearer realm="{}/token",service="reg""#, server.uri());
    Mock::given(method("GET"))
        .and(path("/v2/_catalog"))
        .respond_with(
            ResponseTemplate::new(401).insert_header("WWW-Authenticate", challenge.as_str()),
        )
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = DockerApi::new(&reg).expect("build client");
    let error = api.catalog(&reg, 50).await.unwrap_err();
    assert!(error.to_string().contains("missing username"));
}

#[tokio::test]
async fn catalog_fetches_a_token_and_retries_on_401() {
    let server = MockServer::start().await;
    let challenge = format!(r#"Bearer realm="{}/token",service="reg""#, server.uri());
    Mock::given(method("GET"))
        .and(path("/v2/_catalog"))
        .and(header("authorization", "Bearer the-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "repositories": ["a/b"]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/_catalog"))
        .respond_with(
            ResponseTemplate::new(401).insert_header("WWW-Authenticate", challenge.as_str()),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/token"))
        .and(query_param("service", "reg"))
        .and(basic_auth("user", "pass"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "token": "the-token" })),
        )
        .mount(&server)
        .await;

    let mut reg = registry(&server.uri());
    reg.docker.username = Some("user".to_string());
    reg.docker.password = Some("pass".to_string());
    let api = DockerApi::new(&reg).expect("build client");
    let repos = api.catalog(&reg, 50).await.expect("catalog after auth");
    assert_eq!(repos, vec!["a/b".to_string()]);
}

#[tokio::test]
async fn tags_scopes_the_token_request_to_the_repository() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/my/repo/tags/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "my/repo",
            "tags": ["latest", "v1"]
        })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = DockerApi::new(&reg).expect("build client");
    let (name, tags) = api.tags(&reg, "my/repo", 100).await.expect("tags");
    assert_eq!(name, "my/repo");
    assert_eq!(tags, vec!["latest".to_string(), "v1".to_string()]);
}
