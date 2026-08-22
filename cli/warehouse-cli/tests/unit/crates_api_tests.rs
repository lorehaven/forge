use warehouse_cli::api::crates_api::{CratesApi, urlencoding_simple};
use warehouse_cli::domain::{RegistryConfig, RegistryCratesConfig, RegistryDockerConfig};
use wiremock::matchers::{header, method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn registry(url: &str, token: Option<&str>) -> RegistryConfig {
    RegistryConfig {
        base_path: String::new(),
        docker: RegistryDockerConfig::default(),
        crates: RegistryCratesConfig {
            url: url.to_string(),
            token: token.map(str::to_string),
            insecure_tls: false,
        },
        ..RegistryConfig::default()
    }
}

#[test]
fn urlencoding_simple_escapes_reserved_bytes_and_spaces() {
    assert_eq!(urlencoding_simple("hello world"), "hello+world");
    assert_eq!(urlencoding_simple("a/b"), "a%2Fb");
    assert_eq!(urlencoding_simple("safe-._~09AZaz"), "safe-._~09AZaz");
}

#[tokio::test]
async fn search_parses_crates_and_total_and_sends_the_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/crates"))
        .and(query_param("q", "foo bar"))
        .and(query_param("per_page", "5"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "crates": [{ "name": "foo", "max_version": "1.0.0", "description": null }],
            "meta": { "total": 1 }
        })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), Some("tok"));
    let api = CratesApi::new(&reg.crates).expect("build client");
    let (crates, total) = api.search(&reg, "foo bar", 5).await.expect("search");
    assert_eq!(total, 1);
    assert_eq!(crates[0].name, "foo");
}

#[tokio::test]
async fn search_surfaces_a_non_success_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/crates"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), None);
    let api = CratesApi::new(&reg.crates).expect("build client");
    let error = api.search(&reg, "foo", 10).await.unwrap_err();
    assert!(error.to_string().contains("request failed"));
}

#[tokio::test]
async fn versions_parses_every_index_line() {
    let server = MockServer::start().await;
    let body = format!(
        "{}\n{}\n",
        serde_json::json!({
            "name": "foo", "vers": "1.0.0", "cksum": "abc", "yanked": false,
            "deps": [], "features": {}
        }),
        serde_json::json!({
            "name": "foo", "vers": "1.1.0", "cksum": "def", "yanked": true,
            "deps": [], "features": {}
        }),
    );
    Mock::given(method("GET"))
        .and(path_regex(r"^/index/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), None);
    let api = CratesApi::new(&reg.crates).expect("build client");
    let records = api.versions(&reg, "foo").await.expect("versions");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].vers, "1.0.0");
    assert!(records[1].yanked);
}

#[tokio::test]
async fn versions_reports_not_found_as_a_named_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/index/"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), None);
    let api = CratesApi::new(&reg.crates).expect("build client");
    let error = api.versions(&reg, "missing").await.unwrap_err();
    assert!(error.to_string().contains("'missing' not found"));
}

#[tokio::test]
async fn versions_errors_when_every_line_fails_to_parse() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/index/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json\n"))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), None);
    let api = CratesApi::new(&reg.crates).expect("build client");
    let error = api.versions(&reg, "foo").await.unwrap_err();
    assert!(error.to_string().contains("no version records found"));
}

#[tokio::test]
async fn yank_succeeds_when_the_server_reports_ok() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/crates/foo/1.0.0/yank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), None);
    let api = CratesApi::new(&reg.crates).expect("build client");
    api.yank(&reg, "foo", "1.0.0").await.expect("yank");
}

#[tokio::test]
async fn yank_errors_when_the_server_reports_ok_false() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/crates/foo/1.0.0/yank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": false })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), None);
    let api = CratesApi::new(&reg.crates).expect("build client");
    let error = api.yank(&reg, "foo", "1.0.0").await.unwrap_err();
    assert!(error.to_string().contains("ok=false"));
}

#[tokio::test]
async fn unyank_succeeds_when_the_server_reports_ok() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/crates/foo/1.0.0/unyank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri(), None);
    let api = CratesApi::new(&reg.crates).expect("build client");
    api.unyank(&reg, "foo", "1.0.0").await.expect("unyank");
}
