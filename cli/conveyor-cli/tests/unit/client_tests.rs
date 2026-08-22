use conveyor_cli::client::{Client, explain};
use conveyor_cli::test_support::{ENV_LOCK, EnvGuard};

#[test]
fn explain_prefers_the_error_field_from_a_json_body() {
    let status = reqwest::StatusCode::BAD_REQUEST;
    let body = r#"{"error": "missing field `owner`"}"#;
    assert_eq!(explain(status, body), "missing field `owner`");
}

#[test]
fn explain_falls_back_to_the_raw_body_when_not_json() {
    let status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
    assert_eq!(explain(status, "  boom  \n"), "boom");
}

#[test]
fn explain_falls_back_to_the_status_when_the_body_is_empty() {
    let status = reqwest::StatusCode::NOT_FOUND;
    assert_eq!(explain(status, ""), "conveyor answered 404 Not Found");
}

#[test]
fn explain_adds_a_credentials_hint_on_401() {
    let status = reqwest::StatusCode::UNAUTHORIZED;
    let body = r#"{"error": "invalid token"}"#;
    assert_eq!(
        explain(status, body),
        "invalid token (set CONVEYOR_USERNAME and CONVEYOR_PASSWORD)"
    );
}

#[test]
fn explain_ignores_json_without_an_error_field() {
    let status = reqwest::StatusCode::BAD_REQUEST;
    let body = r#"{"detail": "nope"}"#;
    assert_eq!(explain(status, body), r#"{"detail": "nope"}"#);
}

// Below: `Client::for_tests_with_token` bypasses the async `new()`/login
// flow (and its `FileConfig::load()` env-var reads) so a `Client` can be
// pointed at a `wiremock` server directly, to exercise `get`/`post`/etc.

fn client_for(server: &wiremock::MockServer, token: Option<&str>) -> Client {
    Client::for_tests_with_token(server.uri(), token.map(str::to_string))
}

#[test]
fn url_joins_the_base_and_api_prefix() {
    let client = Client::for_tests("https://conveyor.example".to_string());
    assert_eq!(client.url("/runs"), "https://conveyor.example/api/v1/runs");
}

#[tokio::test]
async fn get_decodes_a_successful_json_response() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": 1})))
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let value: serde_json::Value = client.get("/runs/1").await.expect("get");
    assert_eq!(value["id"], 1);
}

#[tokio::test]
async fn get_sends_the_bearer_token_when_present() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs"))
        .and(header("authorization", "Bearer secret-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let client = client_for(&server, Some("secret-token"));
    let value: serde_json::Value = client.get("/runs").await.expect("get");
    assert!(value.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_surfaces_the_servers_error_message_on_failure() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/missing"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"error": "no such run"})),
        )
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let err = client
        .get::<serde_json::Value>("/runs/missing")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no such run"), "{err}");
}

#[tokio::test]
async fn get_errors_when_the_body_does_not_match_the_expected_shape() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(Debug, serde::Deserialize)]
    struct Expected {
        #[allow(dead_code)]
        required_field: String,
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"nope": true})))
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let err = client.get::<Expected>("/runs/1").await.unwrap_err();
    assert!(
        err.to_string()
            .contains("answered with something unexpected"),
        "{err}"
    );
}

#[tokio::test]
async fn post_sends_the_json_body_and_decodes_the_response() {
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/runs"))
        .and(body_json(serde_json::json!({"repo": "forge"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": 7})))
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let value: serde_json::Value = client
        .post("/runs", &serde_json::json!({"repo": "forge"}))
        .await
        .expect("post");
    assert_eq!(value["id"], 7);
}

#[tokio::test]
async fn put_and_patch_decode_successful_responses() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/runs/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/runs/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let put: serde_json::Value = client
        .put("/runs/1", &serde_json::json!({}))
        .await
        .expect("put");
    let patch: serde_json::Value = client
        .patch("/runs/1", &serde_json::json!({}))
        .await
        .expect("patch");
    assert_eq!(put["ok"], true);
    assert_eq!(patch["ok"], true);
}

#[tokio::test]
async fn send_empty_succeeds_on_a_bodyless_success_response() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/runs/1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    client
        .send_empty(reqwest::Method::DELETE, "/runs/1")
        .await
        .expect("send_empty");
}

#[tokio::test]
async fn send_empty_errors_with_the_servers_message_on_failure() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/runs/1"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(serde_json::json!({"error": "forbidden"})),
        )
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let err = client
        .send_empty(reqwest::Method::DELETE, "/runs/1")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("forbidden"), "{err}");
}

#[tokio::test]
async fn stream_returns_the_raw_response_on_success() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/1/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_string("log line\n"))
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let response = client.stream("/runs/1/logs").await.expect("stream");
    let text = response.text().await.expect("text");
    assert_eq!(text, "log line\n");
}

#[tokio::test]
async fn stream_errors_with_the_servers_message_on_failure() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/1/logs"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&server)
        .await;

    let client = client_for(&server, None);
    let err = client.stream("/runs/1/logs").await.unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}

/// `Client::new` unconditionally calls `FileConfig::load()`, which reads
/// `XDG_CONFIG_HOME`/`HOME` - and some machines have a REAL
/// `~/.config/conveyor/config.toml` with live admin credentials for a
/// real server. Every `new_*` test below must run inside this, pointed
/// at a fresh empty directory, or it would pick up those real
/// credentials and log in for real. Shares `test_support`'s lock with
/// `config_tests.rs` (same names, same `tests/unit.rs` binary).
async fn isolated_from_the_real_config_file<F, Fut, T>(body: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let _lock = ENV_LOCK.lock().await;
    let empty_dir = tempfile::tempdir().expect("tempdir");
    let _xdg = EnvGuard::set("XDG_CONFIG_HOME", empty_dir.path().to_str().unwrap());
    let _url = EnvGuard::unset("CONVEYOR_URL");
    let _user = EnvGuard::unset("CONVEYOR_USERNAME");
    let _pass = EnvGuard::unset("CONVEYOR_PASSWORD");
    let _gatehouse = EnvGuard::unset("GATEHOUSE_URL");
    // The lock and env guards above are held across this `.await` -
    // that's the whole point: `body()`'s `FileConfig::load()` call must
    // see the isolated env for its entire (synchronous, but still
    // scheduled) execution, not just at construction time.
    body().await
}

#[tokio::test]
async fn new_requires_a_url_when_none_is_given_by_flag_env_or_file() {
    let err = isolated_from_the_real_config_file(|| async {
        Client::new(None, None, None, None, true).await
    })
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("conveyor's address is not set"),
        "{err}"
    );
}

#[tokio::test]
async fn new_succeeds_with_an_explicit_url_and_no_username() {
    let client = isolated_from_the_real_config_file(|| async {
        Client::new(
            Some("https://conveyor.example/".to_string()),
            None,
            None,
            None,
            true,
        )
        .await
    })
    .await
    .expect("new");
    assert_eq!(client.base_url_for_tests(), "https://conveyor.example");
    assert!(client.token_for_tests().is_none());
}

#[tokio::test]
async fn new_requires_a_gatehouse_url_when_a_username_is_given() {
    let err = isolated_from_the_real_config_file(|| async {
        Client::new(
            Some("https://conveyor.example".to_string()),
            Some("alice".to_string()),
            Some("pw".to_string()),
            None,
            true,
        )
        .await
    })
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("gatehouse's address is not"),
        "{err}"
    );
}

#[tokio::test]
async fn new_logs_in_and_carries_the_token_from_gatehouse() {
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gatehouse = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/login"))
        .and(body_json(
            serde_json::json!({"username": "alice", "password": "pw"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "minted-token"})),
        )
        .mount(&gatehouse)
        .await;

    let client = isolated_from_the_real_config_file(|| async {
        Client::new(
            Some("https://conveyor.example".to_string()),
            Some("alice".to_string()),
            Some("pw".to_string()),
            Some(gatehouse.uri()),
            true,
        )
        .await
    })
    .await
    .expect("new");
    assert_eq!(client.token_for_tests(), Some("minted-token"));
}

#[tokio::test]
async fn new_reports_the_login_rejection_without_leaking_the_password() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gatehouse = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/login"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&gatehouse)
        .await;

    let err = isolated_from_the_real_config_file(|| async {
        Client::new(
            Some("https://conveyor.example".to_string()),
            Some("alice".to_string()),
            Some("wrong".to_string()),
            Some(gatehouse.uri()),
            true,
        )
        .await
    })
    .await
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("alice"), "{message}");
    assert!(!message.contains("wrong"), "{message}");
}
