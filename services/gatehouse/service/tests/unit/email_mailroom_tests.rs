use gatehouse_service::email::{Mail, MailroomSender, Recipient, Sender};
use mailroom_client::Config;
use quench_test::mock::matchers::{header, method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};

const fn alice() -> Recipient<'static> {
    Recipient {
        address: "alice@example.test",
        username: "alice",
        locale: Some("pl-PL"),
    }
}

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
        .respond_with(ResponseTemplate::new(200).set_body_json(
            quench_serde::json!({"access_token": "tok", "token_type": "Bearer", "expires_in": 900}),
        ))
        .mount(server)
        .await;
}

#[quench_test::test]
fn an_account_message_becomes_a_send_request_with_the_rendered_text() {
    let mail = Mail::Verification {
        link: "https://ennor.example/gatehouse/ui/verify?token=abc",
    };
    let request = MailroomSender::request(&mail, &alice()).unwrap();
    assert_eq!(request.to, ["alice@example.test"]);
    assert!(
        request.from.is_empty(),
        "Mailroom picks the sender identity"
    );
    assert_ne!(request.subject, "");
    assert!(
        request
            .text
            .contains("https://ennor.example/gatehouse/ui/verify?token=abc")
    );
    assert!(
        request
            .html
            .as_deref()
            .unwrap()
            .contains("https://ennor.example/gatehouse/ui/verify?token=abc")
    );
    assert_eq!(
        request.headers,
        [("X-Forge-Mail".to_string(), "verification".to_string())]
    );
    assert!(request.idempotency_key.is_none());
}

#[quench_test::test]
fn a_notification_carries_the_one_click_unsubscribe_headers() {
    let vars = [("repo", "forge"), ("run", "42")];
    let mail = Mail::Notification {
        template: "conveyor.run.failed",
        vars: &vars,
        unsubscribe: "https://ennor.example/gatehouse/ui/unsubscribe?t=1",
    };
    let request = MailroomSender::request(&mail, &alice()).unwrap();
    assert!(request.headers.contains(&(
        "List-Unsubscribe".to_string(),
        "<https://ennor.example/gatehouse/ui/unsubscribe?t=1>".to_string()
    )));
    assert!(request.headers.contains(&(
        "List-Unsubscribe-Post".to_string(),
        "List-Unsubscribe=One-Click".to_string()
    )));
}

#[quench_test::test]
fn an_unknown_notification_template_is_a_permanent_error() {
    let mail = Mail::Notification {
        template: "no.such.template",
        vars: &[],
        unsubscribe: "https://x",
    };
    let error = MailroomSender::request(&mail, &alice()).unwrap_err();
    assert!(!error.is_transient());
    assert!(error.to_string().contains("no.such.template"));
}

#[quench_test::test]
async fn a_message_is_handed_to_mailroom_with_a_machine_token() {
    let server = MockServer::start().await;
    token(&server).await;
    Mock::given(method("POST"))
        .and(path("/mailroom/api/v1/send"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(quench_serde::json!({"id": "m1", "status": "queued"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let sender = MailroomSender::new(&config(&server)).unwrap();
    sender
        .send(
            &alice(),
            &Mail::PasswordReset {
                link: "https://x/reset?t=1",
            },
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let body = requests
        .iter()
        .find(|r| r.path.ends_with("/send"))
        .unwrap()
        .body_string();
    assert!(body.contains("alice@example.test") && body.contains("https://x/reset?t=1"));
    server.verify().await;
}

#[quench_test::test]
async fn mailroom_being_busy_is_transient_and_a_refusal_is_not() {
    for (status, transient) in [(503, true), (429, true), (400, false), (403, false)] {
        let server = MockServer::start().await;
        token(&server).await;
        Mock::given(method("POST"))
            .and(path("/mailroom/api/v1/send"))
            .respond_with(ResponseTemplate::new(status).set_body_string("{\"error\":\"x\"}"))
            .mount(&server)
            .await;
        let sender = MailroomSender::new(&config(&server)).unwrap();
        let error = sender
            .send(&alice(), &Mail::PasswordChanged)
            .await
            .unwrap_err();
        assert_eq!(error.is_transient(), transient, "{status}");
    }
}

#[quench_test::test]
fn an_incomplete_configuration_is_refused_at_startup() {
    let config = Config {
        base_url: "http://m".into(),
        gatehouse_url: "http://g".into(),
        client_id: "c".into(),
        client_secret: String::new(),
        tls_verify: true,
    };
    assert!(
        MailroomSender::new(&config)
            .unwrap_err()
            .contains("MAILROOM_CLIENT_SECRET")
    );
}
