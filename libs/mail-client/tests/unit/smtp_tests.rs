use mail_client::compose::{Draft, build};
use mail_client::imap::Auth;
use mail_client::netguard::Policy;
use mail_client::presets::Security;
use mail_client::smtp::{Config, send, test};
use mail_testkit::{CERT_PEM, SmtpOptions, SmtpServer};

fn built(to: &str) -> mail_client::compose::Built {
    build(
        &Draft {
            from: "Forge <noreply@example.com>".into(),
            to: vec![to.into()],
            subject: "Hello".into(),
            text: Some("Body".into()),
            ..Draft::default()
        },
        0,
    )
    .unwrap()
}

async fn server(options: SmtpOptions) -> SmtpServer {
    let server = SmtpServer::start(options).await;
    server.add_user("noreply@example.com", "pw");
    server
}

fn config(server: &SmtpServer, security: Security) -> Config {
    let mut config = Config::new(
        "127.0.0.1",
        server.port(),
        security,
        "noreply@example.com",
        Auth::Password("pw".into()),
    );
    config.policy = Policy {
        private_hosts: vec!["127.0.0.1".into()],
        extra_ports: vec![server.port()],
        ..Policy::default()
    };
    config.tls_roots_pem = vec![CERT_PEM.to_string()];
    config.tls_server_name = Some("localhost".into());
    config.timeout = std::time::Duration::from_secs(5);
    config
}

#[quench_test::test]
async fn sends_over_implicit_tls_and_the_server_keeps_it() {
    let s = server(SmtpOptions::default()).await;
    send(&config(&s, Security::Tls), &built("alice@example.org"))
        .await
        .unwrap();
    let got = s.received();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].from, "noreply@example.com");
    assert_eq!(got[0].to, ["alice@example.org"]);
    assert_eq!(got[0].auth_user.as_deref(), Some("noreply@example.com"));
    assert_eq!(got[0].header("subject").as_deref(), Some("Hello"));
    assert!(got[0].text().contains("Body"));
}

#[quench_test::test]
async fn sends_over_starttls() {
    let s = server(SmtpOptions {
        implicit_tls: false,
        starttls: true,
        ..SmtpOptions::default()
    })
    .await;
    send(&config(&s, Security::StartTls), &built("alice@example.org"))
        .await
        .unwrap();
    assert_eq!(s.received().len(), 1);
    let log = s.log();
    assert!(
        log.iter().position(|l| l == "STARTTLS").unwrap()
            < log.iter().position(|l| l == "AUTH ...").unwrap()
    );
}

#[quench_test::test]
async fn a_wrong_password_is_permanent() {
    let s = server(SmtpOptions::default()).await;
    let mut cfg = config(&s, Security::Tls);
    cfg.auth = Auth::Password("nope".into());
    let error = send(&cfg, &built("a@example.org")).await.unwrap_err();
    assert!(!error.transient, "{error}");
    assert!(!error.to_string().contains("nope"));
    assert!(s.received().is_empty());
}

#[quench_test::test]
async fn a_refused_recipient_is_permanent_and_a_busy_server_is_transient() {
    let s = server(SmtpOptions {
        reject_recipients: vec!["gone@example.org".into()],
        ..SmtpOptions::default()
    })
    .await;
    let error = send(&config(&s, Security::Tls), &built("gone@example.org"))
        .await
        .unwrap_err();
    assert!(!error.transient, "{error}");
    s.fail_next_data(451, "4.3.0 try again later");
    let error = send(&config(&s, Security::Tls), &built("a@example.org"))
        .await
        .unwrap_err();
    assert!(error.transient, "{error}");
    send(&config(&s, Security::Tls), &built("a@example.org"))
        .await
        .unwrap();
    assert_eq!(s.received().len(), 1);
}

#[quench_test::test]
async fn oversized_messages_are_refused_by_the_server() {
    let s = server(SmtpOptions {
        max_size: 1000,
        ..SmtpOptions::default()
    })
    .await;
    let mut d = Draft {
        from: "a@example.com".into(),
        to: vec!["b@example.org".into()],
        subject: "big".into(),
        text: Some("x".repeat(5000)),
        ..Draft::default()
    };
    let big = build(&d, 0).unwrap();
    assert!(send(&config(&s, Security::Tls), &big).await.is_err());
    d.text = Some("small".into());
    send(&config(&s, Security::Tls), &build(&d, 0).unwrap())
        .await
        .unwrap();
}

#[quench_test::test]
async fn the_policy_blocks_internal_targets_and_unknown_certificates_fail() {
    let s = server(SmtpOptions::default()).await;
    let mut cfg = config(&s, Security::Tls);
    cfg.policy = Policy {
        extra_ports: vec![s.port()],
        ..Policy::default()
    };
    let error = send(&cfg, &built("a@example.org")).await.unwrap_err();
    assert!(!error.transient);
    assert!(error.message.contains("public internet"), "{error}");
    let mut cfg = config(&s, Security::Tls);
    cfg.tls_roots_pem.clear();
    assert!(send(&cfg, &built("a@example.org")).await.is_err());
    let mut cfg = config(&s, Security::Tls);
    cfg.tls_roots_pem = vec!["not a pem".into()];
    assert!(send(&cfg, &built("a@example.org")).await.is_err());
    assert!(s.received().is_empty());
}

#[quench_test::test]
async fn oauth_is_not_supported_for_sending_yet() {
    let s = server(SmtpOptions::default()).await;
    let mut cfg = config(&s, Security::Tls);
    cfg.auth = Auth::XOAuth2("t".into());
    let error = send(&cfg, &built("a@example.org")).await.unwrap_err();
    assert!(error.to_string().contains("OAuth"));
}

#[quench_test::test]
async fn test_connects_and_logs_in_without_sending() {
    let s = server(SmtpOptions::default()).await;
    test(&config(&s, Security::Tls)).await.unwrap();
    assert!(s.received().is_empty());
    let mut cfg = config(&s, Security::Tls);
    cfg.auth = Auth::Password("bad".into());
    assert!(test(&cfg).await.is_err());
    assert_eq!(s.failed_logins(), 1);
}

#[quench_test::test]
async fn unresolvable_hosts_are_transient() {
    let mut cfg = Config::new(
        "no-such-host.invalid",
        587,
        Security::StartTls,
        "u",
        Auth::Password("p".into()),
    );
    cfg.timeout = std::time::Duration::from_secs(2);
    let error = send(&cfg, &built("a@example.org")).await.unwrap_err();
    assert!(error.transient);
    assert_eq!(error.to_string(), error.message);
}
