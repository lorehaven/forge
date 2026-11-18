//! Tests that need no database and no network.

use mailroom_service::config::Config;
use mailroom_service::error::Error;
use mailroom_service::limits::Limits;
use mailroom_service::mailbox::{html_policy, search_terms};
use mailroom_service::routers::api::gate::{Need, SameOrigin, meets, need};
use mailroom_service::routers::api::messages::content_disposition;
use mailroom_service::routers::api::protected_prefix;
use mailroom_service::settings::Stored;
use mailroom_service::sync::{flag_state, uid_set};
use mailroom_service::vault::{TRANSPORT_CONTEXT, Vault, VaultError, account_context};
use mailroom_service::worker::{MAX_ATTEMPTS, backoff};
use quench_auth::domain::jwt::Claims;
use quench_http::prelude::http::{Method, StatusCode};

const KEY: &str = "0123456789abcdef0123456789abcdef-key";

#[quench_test::test]
fn a_sealed_secret_opens_only_for_its_context() {
    let vault = Vault::new(KEY, None).unwrap();
    let sealed = vault.seal(&account_context("a1", "ann"), "hunter2");
    assert!(sealed.starts_with("v1."));
    assert!(!sealed.contains("hunter2"));
    assert_eq!(
        vault.open(&account_context("a1", "ann"), &sealed).unwrap(),
        ("hunter2".to_string(), false)
    );
    // Moved to another account or another owner: it does not open.
    assert_eq!(
        vault.open(&account_context("a2", "ann"), &sealed),
        Err(VaultError::Unreadable)
    );
    assert_eq!(
        vault.open(&account_context("a1", "bob"), &sealed),
        Err(VaultError::Unreadable)
    );
    assert_eq!(
        vault.open(TRANSPORT_CONTEXT, &sealed),
        Err(VaultError::Unreadable)
    );
}

#[quench_test::test]
fn two_seals_of_the_same_secret_differ() {
    let vault = Vault::new(KEY, None).unwrap();
    assert_ne!(vault.seal("c", "x"), vault.seal("c", "x"));
}

#[quench_test::test]
fn a_tampered_or_foreign_value_is_unreadable() {
    let vault = Vault::new(KEY, None).unwrap();
    let sealed = vault.seal("c", "secret");
    let mut bytes = sealed.clone().into_bytes();
    let last = bytes.len() - 3;
    bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
    assert_eq!(
        vault.open("c", &String::from_utf8(bytes).unwrap()),
        Err(VaultError::Unreadable)
    );
    for junk in ["", "v1.", "v1.!!!", "v2.abcd", "plain text", "v1.AAAA"] {
        assert_eq!(vault.open("c", junk), Err(VaultError::Unreadable), "{junk}");
    }
    let other = Vault::new("another-key-0123456789abcdef0123456789", None).unwrap();
    assert_eq!(other.open("c", &sealed), Err(VaultError::Unreadable));
}

#[quench_test::test]
fn a_rotated_key_opens_old_values_and_says_so() {
    let old = Vault::new(KEY, None).unwrap();
    let sealed = old.seal("c", "secret");
    let rotated = Vault::new("a-brand-new-key-0123456789abcdef012345", Some(KEY)).unwrap();
    assert_eq!(
        rotated.open("c", &sealed).unwrap(),
        ("secret".to_string(), true)
    );
    let resealed = rotated.seal("c", "secret");
    assert_eq!(
        rotated.open("c", &resealed).unwrap(),
        ("secret".to_string(), false)
    );
}

#[quench_test::test]
fn short_keys_are_refused_and_ephemeral_ones_work() {
    assert_eq!(Vault::new("short", None).unwrap_err(), VaultError::WeakKey);
    assert!(Vault::new(KEY, Some("short")).is_err());
    let vault = Vault::ephemeral();
    assert_eq!(vault.open("c", &vault.seal("c", "x")).unwrap().0, "x");
    assert!(!format!("{vault:?}").contains("key"));
    assert!(
        !VaultError::WeakKey.to_string().is_empty()
            && !VaultError::Unreadable.to_string().is_empty()
    );
}

#[quench_test::test]
fn startup_refuses_a_missing_key_unless_told_otherwise() {
    let mut config = Config::for_tests("s", "postgres://x");
    config.vault_key = String::new();
    assert!(mailroom_service::boot::vault(&config).is_err());
    config.allow_ephemeral_key = true;
    assert!(mailroom_service::boot::vault(&config).is_ok());
    config.allow_ephemeral_key = false;
    config.vault_key = "short".into();
    assert!(mailroom_service::boot::vault(&config).is_err());
}

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
fn search_text_becomes_a_few_lowercase_words() {
    assert_eq!(search_terms("  Hello   WORLD "), ["hello", "world"]);
    assert!(search_terms("").is_empty());
    assert_eq!(search_terms("a b c d e f g h i j").len(), 8);
    assert_eq!(search_terms(&"x".repeat(200))[0].len(), 64);
}

#[quench_test::test]
fn uid_sets_are_compact_and_ordered() {
    assert_eq!(uid_set(&[]), "");
    assert_eq!(uid_set(&[5]), "5");
    assert_eq!(uid_set(&[3, 1, 2, 7, 9, 8, 20]), "1:3,7:9,20");
    assert_eq!(uid_set(&[4, 4, 4]), "4");
}

#[quench_test::test]
fn flags_are_read_from_the_servers_words() {
    let f = |list: &[&str]| flag_state(&list.iter().map(ToString::to_string).collect::<Vec<_>>());
    assert_eq!(f(&[]), (false, false, false));
    assert_eq!(f(&["\\Seen", "\\flagged"]), (true, true, false));
    assert_eq!(f(&["\\ANSWERED", "$Forwarded"]), (false, false, true));
}

#[quench_test::test]
fn retries_back_off_and_stop() {
    let waits: Vec<i64> = (1..=MAX_ATTEMPTS).map(backoff).collect();
    assert!(waits.windows(2).all(|w| w[0] <= w[1]), "{waits:?}");
    assert_eq!(waits[0], 60);
    assert_eq!(backoff(0), 60);
    assert!(backoff(99) >= 6 * 3600);
}

#[quench_test::test]
fn settings_pick_the_default_identity_and_match_addresses_case_insensitively() {
    let mut stored = Stored::default();
    assert!(stored.identity("").is_none());
    stored.identities = vec![
        mailroom_core::admin::Identity {
            address: "a@x.example".into(),
            name: String::new(),
            default: false,
        },
        mailroom_core::admin::Identity {
            address: "b@x.example".into(),
            name: "B".into(),
            default: true,
        },
    ];
    assert_eq!(stored.identity("").unwrap().address, "b@x.example");
    assert_eq!(
        stored.identity(" A@X.EXAMPLE ").unwrap().address,
        "a@x.example"
    );
    assert!(stored.identity("c@x.example").is_none());
    stored.identities[1].default = false;
    assert_eq!(stored.identity("").unwrap().address, "a@x.example");
    let public = stored.public(true);
    assert!(public.transport.is_none() && public.identities.len() == 2);
}

#[quench_test::test]
fn errors_tell_the_caller_little_and_the_right_status() {
    let cases = [
        (Error::NotFound, StatusCode::NOT_FOUND),
        (Error::Forbidden, StatusCode::FORBIDDEN),
        (Error::Invalid("bad".into()), StatusCode::BAD_REQUEST),
        (Error::Conflict("clash".into()), StatusCode::CONFLICT),
        (Error::Limited(30), StatusCode::TOO_MANY_REQUESTS),
        (Error::Upstream("down".into()), StatusCode::BAD_GATEWAY),
        (
            Error::Internal("db password is hunter2".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            Error::Vault(VaultError::Unreadable),
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
fn the_environment_makes_a_config() {
    let config = Config::for_tests("sch", "postgres://x");
    assert_eq!(config.schema, "sch");
    assert!(!config.background);
    assert!(config.seed_transport.is_none());
    let from_env = Config::from_env();
    assert!(from_env.initial_messages > 0);
    assert!(from_env.background);
}
