//! Tests that need no database and no network.

use mailroom_service::config::{Config, domain_ok};
use mailroom_service::error::Error;
use mailroom_service::limits::Limits;
use mailroom_service::mailbox::html_policy;
use mailroom_service::routers::api::gate::{Need, SameOrigin, meets, need};
use mailroom_service::routers::api::messages::content_disposition;
use mailroom_service::routers::api::protected_prefix;
use mailroom_service::settings::Stored;
use mailroom_service::store::mailboxes::like_prefix;
use quench_auth::domain::jwt::Claims;
use quench_http::prelude::http::{Method, StatusCode};

#[quench_test::test]
fn limits_count_per_key_and_reset_with_the_window() {
    let limits = Limits::default();
    for _ in 0..3 {
        assert!(limits.hit("k", 3, 60, 1000).is_ok());
    }
    assert_eq!(limits.hit("k", 3, 60, 1010), Err(50));
    assert!(limits.hit("other", 3, 60, 1010).is_ok());
    assert!(limits.hit("k", 3, 60, 1060).is_ok());
    limits.clear("k");
    for _ in 0..3 {
        assert!(limits.hit("k", 3, 60, 1061).is_ok());
    }
    assert_eq!(limits.hit("zero", 0, 60, 1).unwrap_err(), 60);
}

#[quench_test::test]
fn limits_do_not_grow_without_bound() {
    let limits = Limits::default();
    for i in 0..12_000 {
        let _ = limits.hit(&format!("k{i}"), 1, 60, 0);
    }
    assert!(limits.hit("fresh", 1, 60, 10_000).is_ok());
}

fn claims(scope: &str) -> Claims {
    Claims::for_audiences(
        "ann".into(),
        vec!["mailroom".into()],
        scope.into(),
        None,
        900,
    )
}

#[quench_test::test]
fn the_gate_decides_by_path_and_method() {
    let api = "/mailroom/api/v1";
    assert_eq!(
        need(&Method::GET, "/mailroom/api/v1/messages", api),
        Need::Read
    );
    assert_eq!(
        need(&Method::HEAD, "/mailroom/api/v1/messages", api),
        Need::Read
    );
    assert_eq!(
        need(&Method::POST, "/mailroom/api/v1/outgoing", api),
        Need::Write
    );
    assert_eq!(
        need(&Method::DELETE, "/mailroom/api/v1/accounts/1", api),
        Need::Write
    );
    assert_eq!(
        need(&Method::GET, "/mailroom/api/v1/admin/stats", api),
        Need::Admin
    );
    assert_eq!(
        need(&Method::GET, "/mailroom/api/v1/admin", api),
        Need::Admin
    );
    assert_eq!(
        need(&Method::POST, "/mailroom/api/v1/send", api),
        Need::Send
    );
    // A path that merely starts like a special one is an ordinary one.
    assert_eq!(
        need(&Method::GET, "/mailroom/api/v1/administrators", api),
        Need::Read
    );
    assert_eq!(
        need(&Method::POST, "/mailroom/api/v1/sendmail", api),
        Need::Write
    );
    assert_eq!(protected_prefix("/"), "/api/v1");
    assert_eq!(protected_prefix("/mailroom"), "/mailroom/api/v1");
}

#[quench_test::test]
fn grants_map_to_what_a_person_may_do() {
    let reader = claims("mailroom:read");
    assert!(meets(&reader, "mailroom", Need::Read));
    assert!(!meets(&reader, "mailroom", Need::Write));
    assert!(!meets(&reader, "mailroom", Need::Admin));
    assert!(!meets(&reader, "mailroom", Need::Send));
    let writer = claims("mailroom:write");
    assert!(meets(&writer, "mailroom", Need::Read) && meets(&writer, "mailroom", Need::Write));
    assert!(!meets(&writer, "mailroom", Need::Admin));
    let admin = claims("mailroom:admin");
    assert!(
        meets(&admin, "mailroom", Need::Admin)
            && meets(&admin, "mailroom", Need::Write)
            && meets(&admin, "mailroom", Need::Read)
    );
    assert!(!meets(&admin, "mailroom", Need::Send));
    let sender = claims("mailroom:send");
    assert!(meets(&sender, "mailroom", Need::Send));
    assert!(!meets(&sender, "mailroom", Need::Read) && !meets(&sender, "mailroom", Need::Write));
    // A machine identity (the wildcard `service` role) sends mail and does nothing else.
    let machine = claims("service");
    assert!(meets(&machine, "mailroom", Need::Send));
    for n in [Need::Read, Need::Write, Need::Admin] {
        assert!(!meets(&machine, "mailroom", n), "{n:?}");
    }
    // A person's wildcard role does not make them a sender.
    assert!(!meets(&claims("admin"), "mailroom", Need::Send));
    let nobody = claims("sage:write");
    for n in [Need::Read, Need::Write, Need::Admin, Need::Send] {
        assert!(!meets(&nobody, "mailroom", n));
    }
    assert!(meets(&claims("admin"), "mailroom", Need::Admin));
    assert!(meets(&claims("admin"), "mailroom", Need::Read));
}

#[quench_test::test]
fn cross_site_state_changes_are_refused_but_reads_and_tokens_pass() {
    let guard = SameOrigin::new(vec!["https://mail.example.org".into()]);
    let host = Some("ennor.ddns.net");
    assert!(guard.allows(&Method::GET, false, Some("cross-site"), None, host));
    assert!(guard.allows(&Method::POST, true, Some("cross-site"), None, host));
    assert!(guard.allows(&Method::POST, false, Some("same-origin"), None, host));
    assert!(guard.allows(&Method::POST, false, Some("none"), None, host));
    assert!(!guard.allows(&Method::POST, false, Some("cross-site"), None, host));
    assert!(!guard.allows(&Method::DELETE, false, Some("same-site"), None, host));
    assert!(guard.allows(
        &Method::POST,
        false,
        None,
        Some("https://ennor.ddns.net"),
        host
    ));
    assert!(!guard.allows(
        &Method::POST,
        false,
        None,
        Some("https://evil.example"),
        host
    ));
    assert!(!guard.allows(
        &Method::PUT,
        false,
        None,
        Some("https://ennor.ddns.net.evil.example"),
        host
    ));
    assert!(guard.allows(
        &Method::POST,
        false,
        None,
        Some("https://mail.example.org"),
        host
    ));
    assert!(guard.allows(&Method::POST, false, None, None, host));
    assert!(!guard.allows(&Method::POST, false, None, Some("https://x"), None));
}

#[quench_test::test]
fn attachment_names_are_safe_in_the_download_header() {
    assert_eq!(
        content_disposition("report.pdf"),
        "attachment; filename=\"report.pdf\"; filename*=UTF-8''report.pdf"
    );
    let tricky = content_disposition("a\"b\r\nX-Evil: 1;.txt");
    assert!(!tricky.contains('\r') && !tricky.contains('\n'));
    assert_eq!(tricky.matches('"').count(), 2, "{tricky}");
    let polish = content_disposition("zażółć.txt");
    assert!(polish.contains("filename=\"za____.txt\"") || polish.contains("filename=\"za"));
    assert!(polish.contains("filename*=UTF-8''za%C5%BC%C3%B3%C5%82%C4%87.txt"));
}

#[quench_test::test]
fn the_html_policy_blocks_script_and_networks_unless_images_were_allowed() {
    let strict = html_policy(false);
    for needle in [
        "default-src 'none'",
        "sandbox",
        "form-action 'none'",
        "base-uri 'none'",
        "frame-ancestors 'self'",
        "img-src data:;",
    ] {
        assert!(strict.contains(needle), "{strict}");
    }
    assert!(
        !strict.contains("script-src")
            && !strict.contains("allow-scripts")
            && !strict.contains("allow-same-origin")
    );
    let loose = html_policy(true);
    assert!(loose.contains("img-src data: https: http:"));
    assert!(!loose.contains("allow-scripts") && !loose.contains("allow-same-origin"));
}

#[quench_test::test]
fn like_patterns_are_escaped() {
    assert_eq!(like_prefix("ann"), "ann%");
    assert_eq!(like_prefix("a_b%c\\d"), "a\\_b\\%c\\\\d%");
    assert_eq!(like_prefix(""), "%");
}

#[quench_test::test]
fn settings_have_sane_defaults_and_show_the_domain() {
    let stored = Stored::default();
    assert_eq!(stored.quota_bytes(), 500 * 1024 * 1024);
    let shown = stored.public("home.example");
    assert_eq!(shown.domain, "home.example");
    assert_eq!(
        (
            shown.daily_budget,
            shown.hourly_per_source,
            shown.hourly_per_person
        ),
        (2000, 600, 120)
    );
    let json = quench_serde::json::to_string(&stored);
    assert_eq!(
        quench_serde::json::from_str::<Stored>(&json).unwrap(),
        stored
    );
}

#[quench_test::test]
fn errors_tell_the_caller_little_and_the_right_status() {
    let cases = [
        (Error::NotFound, StatusCode::NOT_FOUND),
        (Error::Forbidden, StatusCode::FORBIDDEN),
        (Error::Invalid("bad".into()), StatusCode::BAD_REQUEST),
        (Error::Conflict("clash".into()), StatusCode::CONFLICT),
        (Error::Limited(30), StatusCode::TOO_MANY_REQUESTS),
        (
            Error::Internal("db password is hunter2".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ];
    for (error, status) in cases {
        let (got, text) = error.parts();
        assert_eq!(got, status);
        assert!(
            !text.contains("hunter2") && !text.to_lowercase().contains("sql"),
            "{text}"
        );
        assert_eq!(error.response().into_inner().status, status);
    }
    let limited = Error::Limited(30).response().into_inner();
    assert_eq!(limited.headers.get("retry-after").unwrap(), "30");
}

#[quench_test::test]
fn domains_are_checked_and_the_environment_makes_a_config() {
    for good in ["ennor.ddns.net", "home.example", "localhost", "a-b.c"] {
        assert!(domain_ok(good), "{good}");
    }
    for bad in [
        "",
        "Home.example",
        "nodots",
        ".x.y",
        "x.y.",
        "a..b",
        "-a.b",
        "a.b-",
        "a b.c",
        "a@b.c",
        "é.example",
    ] {
        assert!(!domain_ok(bad), "{bad}");
    }
    let config = Config::for_tests("sch", "postgres://x");
    assert_eq!(
        (config.schema.as_str(), config.domain.as_str()),
        ("sch", "home.example")
    );
    assert!(!config.background);
    let from_env = Config::from_env();
    assert!(from_env.background && domain_ok(&from_env.domain));
    assert!(from_env.maintenance_interval.as_secs() > 0);
}
