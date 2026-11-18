#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::wildcard_imports
)]

use mailroom_client::{Client, Config, SendRequest, classify};
use quench_test::mock::matchers::{header, method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};

fn config(server: &MockServer) -> Config {
    Config {
        base_url: format!("{}/mailroom", server.uri()),
        gatehouse_url: format!("{}/gatehouse", server.uri()),
        client_id: "gatehouse-mailroom".into(),
        client_secret: "s3cret".into(),
        tls_verify: true,
    }
}

async fn token(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/gatehouse/api/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(quench_serde::json!({"access_token": "tok-1", "token_type": "Bearer", "expires_in": 900})))
        .mount(server)
        .await;
}

fn request() -> SendRequest {
    SendRequest {
        to: vec!["a@example.org".into()],
        subject: "Hi".into(),
        text: "Body".into(),
        ..SendRequest::default()
    }
}

#[quench_test::test]
fn the_environment_decides_whether_mailroom_is_used() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        }
    };
    assert!(Config::from_lookup(&env(&[])).is_none());
    assert!(Config::from_lookup(&env(&[("MAILROOM_URL", "  ")])).is_none());
    let config = Config::from_lookup(&env(&[
        ("MAILROOM_URL", "http://mailroom/mailroom"),
        ("GATEHOUSE_URL", "http://gatehouse/gatehouse"),
        ("SERVICE_NAME", "conveyor"),
        ("MAILROOM_CLIENT_SECRET", "x"),
    ]))
    .unwrap();
    assert_eq!(config.client_id, "conveyor-mailroom");
    assert!(config.tls_verify);
    let off = Config::from_lookup(&env(&[
        ("MAILROOM_URL", "u"),
        ("MAILROOM_TLS_VERIFY", "false"),
    ]))
    .unwrap();
    assert!(!off.tls_verify);
    assert_eq!(off.client_id, "forge-mailroom");
    let named = Config::from_lookup(&env(&[
        ("MAILROOM_URL", "u"),
        ("MAILROOM_CLIENT_ID", "mine"),
    ]))
    .unwrap();
    assert_eq!(named.client_id, "mine");
    assert!(!format!("{config:?}").contains('x') || format!("{config:?}").contains("<redacted>"));
}

#[quench_test::test]
fn an_incomplete_configuration_is_refused_without_naming_a_secret() {
    for (blank, name) in [
        ("base", "MAILROOM_URL"),
        ("gate", "GATEHOUSE_URL"),
        ("secret", "MAILROOM_CLIENT_SECRET"),
    ] {
        let mut config = Config {
            base_url: "u".into(),
            gatehouse_url: "g".into(),
            client_id: "c".into(),
            client_secret: "s".into(),
            tls_verify: true,
        };
        match blank {
            "base" => config.base_url.clear(),
            "gate" => config.gatehouse_url.clear(),
            _ => config.client_secret.clear(),
        }
        let failure = Client::new(&config).unwrap_err();
        assert!(failure.message.contains(name) && !failure.transient);
    }
}

#[quench_test::test]
async fn a_send_gets_a_token_and_posts_the_message() {
    let server = MockServer::start().await;
    token(&server).await;
    Mock::given(method("POST"))
        .and(path("/mailroom/api/v1/send"))
        .and(header("authorization", "Bearer tok-1"))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(quench_serde::json!({"id": "m1", "status": "queued"})),
        )
        .expect(2)
        .mount(&server)
        .await;
    let client = Client::new(&config(&server)).unwrap();
    let queued = client.send(&request()).await.unwrap();
    assert_eq!(
        (queued.id.as_str(), queued.status.as_str()),
        ("m1", "queued")
    );
    // The token is kept for the next call.
    client.send(&request()).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.path.ends_with("/token"))
            .count(),
        1
    );
    let sent = requests.iter().find(|r| r.path.ends_with("/send")).unwrap();
    let body = sent.body_string();
    assert!(body.contains("\"subject\":\"Hi\"") && body.contains("a@example.org"));
    server.verify().await;
}

#[quench_test::test]
async fn refusals_are_permanent_and_overload_is_transient() {
    for (status, transient) in [
        (400, false),
        (403, false),
        (404, false),
        (429, true),
        (503, true),
        (500, true),
    ] {
        let server = MockServer::start().await;
        token(&server).await;
        Mock::given(method("POST"))
            .and(path("/mailroom/api/v1/send"))
            .respond_with(ResponseTemplate::new(status).set_body_string("{\"error\":\"nope\"}"))
            .mount(&server)
            .await;
        let failure = Client::new(&config(&server))
            .unwrap()
            .send(&request())
            .await
            .unwrap_err();
        assert_eq!(failure.transient, transient, "{status}: {failure}");
        assert!(failure.message.contains(&status.to_string()), "{failure}");
    }
}

#[quench_test::test]
async fn a_refused_login_is_permanent_and_an_absent_gatehouse_is_transient() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gatehouse/api/v1/token"))
        .respond_with(ResponseTemplate::new(401).set_body_string("invalid_client"))
        .mount(&server)
        .await;
    let failure = Client::new(&config(&server))
        .unwrap()
        .send(&request())
        .await
        .unwrap_err();
    assert!(!failure.transient, "{failure}");

    let mut unreachable = config(&server);
    unreachable.gatehouse_url = "http://127.0.0.1:1/gatehouse".into();
    let failure = Client::new(&unreachable)
        .unwrap()
        .send(&request())
        .await
        .unwrap_err();
    assert!(failure.transient, "{failure}");
}

#[quench_test::test]
fn messages_are_classified_by_their_status() {
    assert!(!classify("HTTP 400 Bad Request: x").transient);
    assert!(!classify("HTTP 422 Unprocessable: x").transient);
    assert!(classify("HTTP 429 Too Many Requests: x").transient);
    assert!(classify("HTTP 408 Request Timeout: x").transient);
    assert!(classify("HTTP 502 Bad Gateway: x").transient);
    assert!(classify("connection refused").transient);
    assert!(!classify("client_credentials grant was refused: x").transient);
    let long = classify(&format!("HTTP 500 x\n{}", "y".repeat(1000)));
    assert!(long.message.len() <= 300 && !long.message.contains('\n'));
}

quench_test::main!();
