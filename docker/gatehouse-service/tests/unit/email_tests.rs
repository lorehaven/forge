use gatehouse_service::RateLimiter;
use gatehouse_service::email::{
    BudgetedSender, Kind, LoggingSender, Mail, Recipient, SendError, Sender, SmtpConfig,
    SmtpSender, TlsMode, Vars, daily_mail_limit, language, render, render_with,
};
use gatehouse_service::test_support::RecordingSender;
use gatehouse_service::ui::pages::register::VERIFICATION_TTL_SECS;
use gatehouse_service::ui::pages::reset::RESET_TTL_SECS;
use quench_mail::{Address, Mailbox};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

fn alice() -> Recipient<'static> {
    Recipient {
        address: "alice@example.test",
        username: "alice",
        locale: None,
    }
}

// -- LoggingSender --------------------------------------------------

#[tokio::test]
async fn logging_sender_accepts_both_kinds() {
    LoggingSender
        .send_verification(&alice(), "https://example.test/verify/tok")
        .await
        .unwrap();
    LoggingSender
        .send_password_reset(&alice(), "https://example.test/reset/tok")
        .await
        .unwrap();
}

#[tokio::test]
async fn logging_sender_is_usable_through_the_sender_trait_object() {
    let sender: Box<dyn Sender> = Box::new(LoggingSender);
    sender
        .send_verification(&alice(), "https://example.test/verify/tok2")
        .await
        .unwrap();
}

// -- SendError ------------------------------------------------------

#[test]
fn send_error_reports_whether_a_retry_could_help() {
    assert!(SendError::transient("busy").is_transient());
    assert!(!SendError::permanent("no such user").is_transient());
    assert_eq!(SendError::new("x", true).to_string(), "x");
}

// -- templates ------------------------------------------------------

#[test]
fn locale_tags_resolve_to_a_supported_language_or_english() {
    for (tag, expected) in [
        (Some("pl-PL"), "pl"),
        (Some("pl_PL"), "pl"),
        (Some("PL"), "pl"),
        (Some("de-DE"), "de"),
        (Some("fr"), "fr"),
        (Some("es-ES"), "es"),
        (Some("en-US"), "en"),
        (Some("en-GB"), "en"),
        (Some("ja-JP"), "en"),
        (Some(""), "en"),
        (Some("  "), "en"),
        (Some("../etc"), "en"),
        (None, "en"),
    ] {
        assert_eq!(language(tag), expected, "{tag:?}");
    }
}

#[test]
fn every_kind_in_every_language_carries_the_link_and_the_username() {
    let link = "https://ennor.ddns.net/gatehouse/ui/verify?token=abc123";
    for kind in [Kind::Verification, Kind::PasswordReset] {
        for locale in ["en-US", "pl-PL", "de-DE", "fr-FR", "es-ES"] {
            let mail = render(kind, Some(locale), "alice", link);
            assert!(!mail.subject.is_empty(), "{kind:?} {locale}");
            assert!(mail.text.contains(link), "{kind:?} {locale} text");
            assert!(mail.text.contains("alice"), "{kind:?} {locale} text");
            assert!(
                mail.html
                    .contains(&format!("<a href=\"{link}\">{link}</a>")),
                "{kind:?} {locale} html"
            );
            assert!(
                mail.html
                    .contains(&format!("lang=\"{}\"", language(Some(locale))))
            );
            assert!(
                !mail.text.contains('{'),
                "unfilled placeholder in {kind:?} {locale}"
            );
            // The link sits alone in a paragraph, so it survives being wrapped.
            assert!(mail.text.contains(&format!("\n\n{link}\n\n")));
        }
    }
}

#[test]
fn languages_differ_and_unknown_falls_back_to_english() {
    let en = render(Kind::Verification, Some("en-US"), "a", "https://x.test/v");
    let pl = render(Kind::Verification, Some("pl-PL"), "a", "https://x.test/v");
    let unknown = render(Kind::Verification, Some("xx-XX"), "a", "https://x.test/v");
    let none = render(Kind::Verification, None, "a", "https://x.test/v");
    assert_ne!(en.subject, pl.subject);
    assert_eq!(en, unknown);
    assert_eq!(en, none);
    assert_ne!(
        render(Kind::PasswordReset, None, "a", "l").subject,
        en.subject
    );
}

#[test]
fn values_are_escaped_in_html_and_not_reinterpreted() {
    let link = "https://x.test/v?a=1&b=\"2\"";
    let mail = render(
        Kind::Verification,
        None,
        "<script>alert('x')</script>",
        link,
    );
    assert!(!mail.html.contains("<script>"));
    assert!(
        mail.html
            .contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;")
    );
    assert!(mail.html.contains("a=1&amp;b=&quot;2&quot;"));
    // Plain text is verbatim.
    assert!(mail.text.contains("<script>alert('x')</script>"));
}

#[test]
fn a_username_that_looks_like_a_placeholder_stays_a_username() {
    let mail = render(Kind::Verification, None, "{link}", "https://x.test/real");
    assert!(mail.text.starts_with("Hello {link},"), "{}", mail.text);
    assert_eq!(mail.text.matches("https://x.test/real").count(), 1);
}

#[test]
fn the_validity_stated_in_the_email_matches_the_real_token_lifetimes() {
    assert_eq!(VERIFICATION_TTL_SECS, 24 * 60 * 60);
    assert_eq!(RESET_TTL_SECS, 60 * 60);
    let verification = render(Kind::Verification, Some("en"), "a", "l");
    assert!(verification.text.contains("valid for 24 hours"));
    let reset = render(Kind::PasswordReset, Some("en"), "a", "l");
    assert!(reset.text.contains("valid for 1 hour"));
    assert!(
        render(Kind::Verification, Some("pl"), "a", "l")
            .text
            .contains("24 godziny")
    );
    assert!(
        render(Kind::PasswordReset, Some("pl"), "a", "l")
            .text
            .contains("1 godzinę")
    );
}

// -- SmtpConfig -----------------------------------------------------

fn config(vars: &[(&str, &str)]) -> Result<Option<SmtpConfig>, String> {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    SmtpConfig::from_lookup(&|key| map.get(key).cloned())
}

#[test]
fn no_smtp_host_means_mail_is_not_configured() {
    assert!(config(&[]).unwrap().is_none());
    assert!(config(&[("SMTP_HOST", "  ")]).unwrap().is_none());
    // Other settings alone do not turn it on (or error).
    assert!(
        config(&[("MAIL_FROM", "not even valid")])
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_full_configuration_parses() {
    let cfg = config(&[
        ("SMTP_HOST", "stalwart-smtp.stalwart.svc.cluster.local"),
        ("SMTP_PORT", "465"),
        ("SMTP_TLS", "implicit"),
        ("SMTP_TLS_SERVER_NAME", "ennor.ddns.net"),
        ("SMTP_USERNAME", "noreply@ennor.ddns.net"),
        ("SMTP_PASSWORD", "s3cret"),
        ("MAIL_FROM", "noreply@ennor.ddns.net"),
        ("MAIL_FROM_NAME", "Forge"),
        ("SMTP_TIMEOUT_SECS", "20"),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(cfg.host, "stalwart-smtp.stalwart.svc.cluster.local");
    assert_eq!(cfg.port, Some(465));
    assert_eq!(cfg.tls, TlsMode::Implicit);
    assert_eq!(cfg.tls_server_name.as_deref(), Some("ennor.ddns.net"));
    assert_eq!(
        cfg.credentials,
        Some(("noreply@ennor.ddns.net".into(), "s3cret".into()))
    );
    assert_eq!(cfg.from.to_string(), "Forge <noreply@ennor.ddns.net>");
    assert_eq!(cfg.timeout, Duration::from_secs(20));
}

#[test]
fn defaults_are_implicit_tls_forge_and_fifteen_seconds() {
    let cfg = config(&[
        ("SMTP_HOST", "mail.test"),
        ("MAIL_FROM", "noreply@mail.test"),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(cfg.tls, TlsMode::Implicit);
    assert_eq!(cfg.port, None);
    assert_eq!(cfg.credentials, None);
    assert_eq!(cfg.from.name.as_deref(), Some("Forge"));
    assert_eq!(cfg.timeout, Duration::from_secs(15));
}

#[test]
fn a_blank_sender_name_means_no_display_name() {
    let cfg = config(&[
        ("SMTP_HOST", "mail.test"),
        ("MAIL_FROM", "noreply@mail.test"),
        ("MAIL_FROM_NAME", "  "),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(cfg.from.name, None);
}

#[test]
fn tls_modes_parse_case_insensitively() {
    for (value, expected) in [
        ("IMPLICIT", TlsMode::Implicit),
        ("StartTLS", TlsMode::StartTls),
        ("none", TlsMode::None),
    ] {
        let cfg = config(&[
            ("SMTP_HOST", "mail.test"),
            ("MAIL_FROM", "a@mail.test"),
            ("SMTP_TLS", value),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(cfg.tls, expected, "{value}");
    }
}

#[test]
fn bad_configuration_is_reported_not_ignored() {
    let base = [("SMTP_HOST", "mail.test"), ("MAIL_FROM", "a@mail.test")];
    let with = |extra: &[(&str, &str)]| {
        let mut all = base.to_vec();
        all.extend_from_slice(extra);
        config(&all)
    };
    assert!(
        with(&[("SMTP_TLS", "ssl")])
            .unwrap_err()
            .contains("SMTP_TLS")
    );
    assert!(
        with(&[("SMTP_PORT", "http")])
            .unwrap_err()
            .contains("SMTP_PORT")
    );
    assert!(
        with(&[("SMTP_PORT", "70000")])
            .unwrap_err()
            .contains("SMTP_PORT")
    );
    assert!(
        with(&[("SMTP_TIMEOUT_SECS", "0")])
            .unwrap_err()
            .contains("SMTP_TIMEOUT_SECS")
    );
    assert!(
        with(&[("SMTP_USERNAME", "u")])
            .unwrap_err()
            .contains("together")
    );
    assert!(
        with(&[("SMTP_PASSWORD", "p")])
            .unwrap_err()
            .contains("together")
    );
    assert!(
        config(&[("SMTP_HOST", "mail.test")])
            .unwrap_err()
            .contains("MAIL_FROM")
    );
    assert!(
        config(&[("SMTP_HOST", "mail.test"), ("MAIL_FROM", "nope")])
            .unwrap_err()
            .contains("MAIL_FROM")
    );
}

#[test]
fn the_password_never_appears_in_debug_output() {
    let cfg = config(&[
        ("SMTP_HOST", "mail.test"),
        ("MAIL_FROM", "a@mail.test"),
        ("SMTP_USERNAME", "u"),
        ("SMTP_PASSWORD", "hunter2-secret"),
    ])
    .unwrap()
    .unwrap();
    let shown = format!("{cfg:?}");
    assert!(!shown.contains("hunter2-secret"), "{shown}");
    assert!(shown.contains("mail.test"));
}

#[test]
fn credentials_without_tls_fail_when_building_the_sender() {
    let cfg = config(&[
        ("SMTP_HOST", "mail.test"),
        ("MAIL_FROM", "a@mail.test"),
        ("SMTP_TLS", "none"),
        ("SMTP_USERNAME", "u"),
        ("SMTP_PASSWORD", "p"),
    ])
    .unwrap()
    .unwrap();
    assert!(SmtpSender::new(cfg).is_err());
}

// -- SmtpSender against a fake server -------------------------------

/// A minimal plain-text SMTP server that records each DATA payload; recipients
/// containing `bad` are refused with `550`.
async fn fake_server() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let received = Arc::new(Mutex::new(Vec::new()));
    let log = received.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut tcp, _)) = listener.accept().await else {
                return;
            };
            let log = log.clone();
            tokio::spawn(async move {
                let (read, mut write) = tcp.split();
                let mut read = BufReader::new(read);
                let _ = write.write_all(b"220 fake ESMTP\r\n").await;
                let mut line = String::new();
                loop {
                    line.clear();
                    if read.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let upper = line.to_ascii_uppercase();
                    let reply: &[u8] = if upper.starts_with("EHLO") {
                        b"250-fake\r\n250 8BITMIME\r\n"
                    } else if upper.starts_with("RCPT") && line.contains("bad") {
                        b"550 5.1.1 no such mailbox\r\n"
                    } else if upper.starts_with("DATA") {
                        let _ = write.write_all(b"354 go\r\n").await;
                        let mut data = String::new();
                        loop {
                            let mut l = String::new();
                            if read.read_line(&mut l).await.unwrap_or(0) == 0 {
                                return;
                            }
                            if l == ".\r\n" {
                                break;
                            }
                            data.push_str(&l);
                        }
                        log.lock().unwrap().push(data);
                        b"250 2.0.0 queued\r\n"
                    } else if upper.starts_with("QUIT") {
                        let _ = write.write_all(b"221 bye\r\n").await;
                        return;
                    } else {
                        b"250 ok\r\n"
                    };
                    let _ = write.write_all(reply).await;
                }
            });
        }
    });
    (port, received)
}

fn sender_for(port: u16) -> SmtpSender {
    SmtpSender::new(SmtpConfig {
        host: "127.0.0.1".into(),
        port: Some(port),
        tls: TlsMode::None,
        tls_server_name: None,
        credentials: None,
        from: Mailbox::new(
            Some("Forge".into()),
            Address::new("noreply", "ennor.ddns.net").unwrap(),
        ),
        timeout: Duration::from_secs(3),
    })
    .unwrap()
}

#[tokio::test]
async fn smtp_sender_delivers_a_verification_email() {
    let (port, received) = fake_server().await;
    let link = "https://ennor.ddns.net/gatehouse/ui/verify?token=abc";
    sender_for(port)
        .send_verification(&alice(), link)
        .await
        .expect("delivered");

    let mails = received.lock().unwrap();
    assert_eq!(mails.len(), 1);
    let mail = &mails[0];
    assert!(
        mail.contains("From: Forge <noreply@ennor.ddns.net>\r\n"),
        "{mail}"
    );
    assert!(mail.contains("To: alice@example.test\r\n"));
    assert!(mail.contains("Subject: Confirm your email address\r\n"));
    assert!(mail.contains("Auto-Submitted: auto-generated\r\n"));
    assert!(mail.contains("multipart/alternative"));
    assert!(mail.contains(link));
    assert!(mail.contains("<a href=\"https://ennor.ddns.net/gatehouse/ui/verify?token=abc\">"));
}

#[tokio::test]
async fn smtp_sender_writes_the_email_in_the_recipients_language() {
    let (port, received) = fake_server().await;
    let polish = Recipient {
        locale: Some("pl-PL"),
        ..alice()
    };
    sender_for(port)
        .send_password_reset(&polish, "https://ennor.ddns.net/r?token=x")
        .await
        .expect("delivered");
    let mails = received.lock().unwrap();
    // Non-ASCII subject travels as an encoded word, and the body is decodable.
    assert!(mails[0].contains("Subject: =?UTF-8?B?"), "{}", mails[0]);
    assert!(mails[0].contains("quoted-printable"));
}

#[tokio::test]
async fn a_refused_address_is_a_permanent_failure() {
    let (port, _) = fake_server().await;
    let bad = Recipient {
        address: "bad@example.test",
        ..alice()
    };
    let err = sender_for(port)
        .send_verification(&bad, "https://x.test/v")
        .await
        .unwrap_err();
    assert!(!err.is_transient(), "{err}");
    assert!(err.to_string().contains("RCPT TO"), "{err}");
}

#[tokio::test]
async fn an_unreachable_server_is_a_transient_failure() {
    let err = sender_for(1)
        .send_verification(&alice(), "https://x.test/v")
        .await
        .unwrap_err();
    assert!(err.is_transient(), "{err}");
}

#[tokio::test]
async fn an_address_the_mailer_cannot_use_is_a_permanent_failure() {
    let (port, received) = fake_server().await;
    let broken = Recipient {
        address: "not an address",
        ..alice()
    };
    let err = sender_for(port)
        .send_verification(&broken, "https://x.test/v")
        .await
        .unwrap_err();
    assert!(!err.is_transient());
    assert!(received.lock().unwrap().is_empty());
}

// -- daily budget ---------------------------------------------------

fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |key| map.get(key).cloned()
}

#[test]
fn the_daily_limit_defaults_below_the_relays_allowance() {
    assert_eq!(daily_mail_limit(&lookup(&[])).unwrap(), 250);
    assert_eq!(
        daily_mail_limit(&lookup(&[("MAIL_DAILY_LIMIT", "  ")])).unwrap(),
        250
    );
    assert_eq!(
        daily_mail_limit(&lookup(&[("MAIL_DAILY_LIMIT", "40")])).unwrap(),
        40
    );
}

#[test]
fn a_nonsense_daily_limit_is_an_error_not_a_silent_default() {
    for bad in ["0", "-3", "many", "1.5", "1e3"] {
        let err = daily_mail_limit(&lookup(&[("MAIL_DAILY_LIMIT", bad)])).unwrap_err();
        assert!(err.contains("MAIL_DAILY_LIMIT"), "{bad}: {err}");
    }
}

#[tokio::test]
async fn the_budget_stops_sending_when_the_day_is_spent() {
    let recorder = Arc::new(RecordingSender::default());
    let sender = BudgetedSender::new(recorder.clone(), RateLimiter::in_memory(), 2);
    sender
        .send_verification(&alice(), "https://x.test/1")
        .await
        .unwrap();
    sender
        .send_password_reset(&alice(), "https://x.test/2")
        .await
        .unwrap();

    let err = sender
        .send_verification(&alice(), "https://x.test/3")
        .await
        .unwrap_err();
    assert!(err.is_transient(), "{err}");
    assert!(err.to_string().contains("budget"), "{err}");
    let err = sender
        .send_password_reset(&alice(), "https://x.test/4")
        .await
        .unwrap_err();
    assert!(err.is_transient());

    let sent = recorder.sent();
    assert_eq!(
        sent.len(),
        2,
        "nothing past the budget reaches the mail server"
    );
    assert_eq!(sent[0].link, "https://x.test/1");
}

#[tokio::test]
async fn the_budget_is_shared_between_both_kinds_of_email() {
    let recorder = Arc::new(RecordingSender::default());
    let sender = BudgetedSender::new(recorder.clone(), RateLimiter::in_memory(), 1);
    sender.send_verification(&alice(), "l").await.unwrap();
    assert!(sender.send_password_reset(&alice(), "l").await.is_err());
}

// -- the other kinds ------------------------------------------------

const ALL_KINDS: [Kind; 8] = [
    Kind::Verification,
    Kind::PasswordReset,
    Kind::Invite,
    Kind::EmailChange,
    Kind::EmailChanged,
    Kind::PasswordChanged,
    Kind::MfaEnabled,
    Kind::MfaDisabled,
];

fn has_link(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Verification | Kind::PasswordReset | Kind::Invite | Kind::EmailChange
    )
}

#[test]
fn every_kind_renders_in_every_language() {
    let link = "https://ennor.ddns.net/gatehouse/ui/x?token=abc";
    for kind in ALL_KINDS {
        for locale in ["en-US", "pl-PL", "de-DE", "fr-FR", "es-ES"] {
            let mail = render_with(
                kind,
                Some(locale),
                &Vars {
                    username: "alice",
                    link: if has_link(kind) { link } else { "" },
                    new_email: "new@example.org",
                },
            );
            let ctx = format!("{kind:?} {locale}");
            assert!(!mail.subject.is_empty(), "{ctx}");
            assert!(mail.text.contains("alice"), "{ctx}");
            assert!(!mail.text.contains('{'), "unfilled placeholder in {ctx}");
            assert!(
                mail.html
                    .contains(&format!("lang=\"{}\"", language(Some(locale))))
            );
            if has_link(kind) {
                assert!(mail.text.contains(&format!("\n\n{link}\n\n")), "{ctx}");
                assert!(mail.html.contains(&format!("<a href=\"{link}\">")), "{ctx}");
            } else {
                assert!(
                    !mail.text.contains("http"),
                    "a notice carries no link: {ctx}"
                );
                assert!(!mail.html.contains("<a "), "a notice has no anchor: {ctx}");
            }
        }
    }
}

#[test]
fn subjects_are_distinct_between_kinds_and_between_languages() {
    for locale in ["en", "pl", "de", "fr", "es"] {
        let mut subjects: Vec<String> = ALL_KINDS
            .iter()
            .map(|k| render(*k, Some(locale), "a", "l").subject)
            .collect();
        subjects.sort();
        subjects.dedup();
        assert_eq!(subjects.len(), ALL_KINDS.len(), "{locale}");
    }
    for kind in ALL_KINDS {
        let en = render(kind, Some("en"), "a", "l").subject;
        for locale in ["pl", "de", "fr", "es"] {
            assert_ne!(
                render(kind, Some(locale), "a", "l").subject,
                en,
                "{kind:?} {locale}"
            );
        }
    }
}

#[test]
fn the_new_address_appears_in_the_change_notice_and_is_escaped_in_html() {
    let mail = render_with(
        Kind::EmailChanged,
        Some("en"),
        &Vars {
            username: "alice",
            link: "",
            new_email: "a&b<x>@example.org",
        },
    );
    assert!(
        mail.text.contains("changed to a&b<x>@example.org"),
        "{}",
        mail.text
    );
    assert!(
        mail.html.contains("a&amp;b&lt;x&gt;@example.org"),
        "{}",
        mail.html
    );
    assert!(!mail.html.contains("<x>"));
}

#[test]
fn validity_windows_stated_in_the_emails() {
    assert!(
        render(Kind::Invite, Some("en"), "a", "l")
            .text
            .contains("valid for 7 days")
    );
    assert!(
        render(Kind::Invite, Some("pl"), "a", "l")
            .text
            .contains("7 dni")
    );
    assert!(
        render(Kind::EmailChange, Some("en"), "a", "l")
            .text
            .contains("valid for 24 hours")
    );
    assert!(
        render(Kind::EmailChange, Some("pl"), "a", "l")
            .text
            .contains("24 godziny")
    );
}

#[test]
fn mail_variants_map_to_kinds_links_and_addresses() {
    let cases = [
        (
            Mail::Verification { link: "l1" },
            Kind::Verification,
            "l1",
            "",
        ),
        (
            Mail::PasswordReset { link: "l2" },
            Kind::PasswordReset,
            "l2",
            "",
        ),
        (Mail::Invite { link: "l3" }, Kind::Invite, "l3", ""),
        (
            Mail::EmailChange { link: "l4" },
            Kind::EmailChange,
            "l4",
            "",
        ),
        (
            Mail::EmailChanged { new_email: "n@x.y" },
            Kind::EmailChanged,
            "",
            "n@x.y",
        ),
        (Mail::PasswordChanged, Kind::PasswordChanged, "", ""),
        (Mail::MfaEnabled, Kind::MfaEnabled, "", ""),
        (Mail::MfaDisabled, Kind::MfaDisabled, "", ""),
    ];
    for (mail, kind, link, new_email) in cases {
        assert_eq!(mail.kind(), kind);
        assert_eq!(mail.link(), link);
        assert_eq!(mail.new_email(), new_email);
        let vars = mail.vars("alice");
        assert_eq!(
            (vars.username, vars.link, vars.new_email),
            ("alice", link, new_email)
        );
    }
    let labels: Vec<&str> = ALL_KINDS.iter().map(|k| k.label()).collect();
    let mut unique = labels.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        labels.len(),
        "labels are distinct: {labels:?}"
    );
}

#[tokio::test]
async fn the_logging_sender_accepts_every_kind() {
    for mail in [
        Mail::Invite {
            link: "https://x.test/i",
        },
        Mail::EmailChange {
            link: "https://x.test/c",
        },
        Mail::EmailChanged { new_email: "n@x.y" },
        Mail::PasswordChanged,
        Mail::MfaEnabled,
        Mail::MfaDisabled,
    ] {
        LoggingSender.send(&alice(), &mail).await.unwrap();
    }
}

#[tokio::test]
async fn smtp_sender_delivers_a_notice_that_carries_no_link() {
    let (port, received) = fake_server().await;
    sender_for(port)
        .send(&alice(), &Mail::PasswordChanged)
        .await
        .expect("delivered");
    let mails = received.lock().unwrap();
    let mail = &mails[0];
    assert!(
        mail.contains("Subject: Your password was changed\r\n"),
        "{mail}"
    );
    assert!(mail.contains("To: alice@example.test\r\n"));
    assert!(mail.contains("Auto-Submitted: auto-generated"));
    assert!(!mail.contains("http"), "no link in a notice: {mail}");
}

#[tokio::test]
async fn smtp_sender_names_the_new_address_in_the_change_notice() {
    let (port, received) = fake_server().await;
    sender_for(port)
        .send(
            &alice(),
            &Mail::EmailChanged {
                new_email: "fresh@example.org",
            },
        )
        .await
        .expect("delivered");
    let mails = received.lock().unwrap();
    assert!(mails[0].contains("fresh@example.org"), "{}", mails[0]);
}

#[tokio::test]
async fn the_recording_sender_notes_kind_link_and_detail() {
    let recorder = RecordingSender::default();
    recorder
        .send(
            &alice(),
            &Mail::Invite {
                link: "https://x.test/i",
            },
        )
        .await
        .unwrap();
    recorder
        .send(&alice(), &Mail::EmailChanged { new_email: "n@x.y" })
        .await
        .unwrap();
    recorder.send(&alice(), &Mail::MfaEnabled).await.unwrap();
    let sent = recorder.sent();
    assert_eq!(sent.len(), 3);
    assert_eq!(
        (sent[0].kind, sent[0].link.as_str()),
        ("invite", "https://x.test/i")
    );
    assert_eq!(
        (sent[1].kind, sent[1].detail.as_str()),
        ("email-changed", "n@x.y")
    );
    assert_eq!((sent[2].kind, sent[2].link.as_str()), ("mfa-enabled", ""));
    assert_eq!(recorder.sent_of("invite").len(), 1);
}

#[tokio::test]
async fn the_budget_covers_the_new_kinds_too() {
    let recorder = Arc::new(RecordingSender::default());
    let sender = BudgetedSender::new(recorder.clone(), RateLimiter::in_memory(), 2);
    sender.send(&alice(), &Mail::PasswordChanged).await.unwrap();
    sender.send(&alice(), &Mail::MfaEnabled).await.unwrap();
    assert!(sender.send(&alice(), &Mail::MfaDisabled).await.is_err());
    assert_eq!(recorder.sent().len(), 2);
}
