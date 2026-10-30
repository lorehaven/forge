use gatehouse_core::notice::{self, ACCOUNT_ERRORS, ADMIN_ERRORS, LOGIN_ERRORS, REGISTER_ERRORS};
use gatehouse_core::{Notice, Query, Tone};

fn query(pairs: &[(&str, &str)]) -> Query {
    Query::new(
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
            .collect(),
    )
}

#[quench_test::test]
fn wrong_credentials_are_err_one() {
    let n = notice::login(&query(&[("err", "1")])).unwrap();
    assert_eq!(n.key, "ui_login_invalid_credentials");
    assert_eq!(n.tone, Tone::Error);
}

#[quench_test::test]
fn only_known_errors_are_shown() {
    for key in LOGIN_ERRORS {
        assert_eq!(notice::login(&query(&[("err", key)])).unwrap().key, key);
    }
    assert_eq!(notice::login(&query(&[("err", "made up")])), None);
    assert_eq!(notice::login(&query(&[("err", "<b>x</b>")])), None);
    assert_eq!(notice::register(&query(&[("err", "nope")])), None);
    assert_eq!(notice::account(&query(&[("err", "nope")])), None);
    assert_eq!(notice::admin(&query(&[("err", "nope")])), None);
    assert_eq!(notice::password_page(&query(&[("err", "nope")])), None);
    for key in REGISTER_ERRORS {
        assert_eq!(notice::register(&query(&[("err", key)])).unwrap().key, key);
    }
    for key in ACCOUNT_ERRORS {
        assert_eq!(notice::account(&query(&[("err", key)])).unwrap().key, key);
    }
    for key in ADMIN_ERRORS {
        assert_eq!(notice::admin(&query(&[("err", key)])).unwrap().key, key);
    }
}

#[quench_test::test]
fn a_reset_is_more_specific_than_a_link_sent() {
    let n = notice::login(&query(&[("reset", "1"), ("reset_requested", "1")])).unwrap();
    assert_eq!(
        n,
        Notice {
            key: "ui_login_reset_ok",
            tone: Tone::Ok
        }
    );
    for (name, key) in [
        ("reset_requested", "ui_login_reset_requested_ok"),
        ("email_changed", "ui_login_email_changed_ok"),
        ("invited", "ui_login_invite_accepted_ok"),
        ("resend_requested", "ui_login_resend_requested_ok"),
        ("verified", "ui_login_verified_ok"),
        ("registered", "ui_login_registered_ok"),
    ] {
        assert_eq!(notice::login(&query(&[(name, "1")])).unwrap().key, key);
    }
    assert_eq!(notice::login(&query(&[])), None);
}

#[quench_test::test]
fn an_error_wins_over_good_news() {
    let n = notice::login(&query(&[
        ("registered", "1"),
        ("err", "ui_login_rate_limited"),
    ]));
    assert_eq!(n.unwrap().key, "ui_login_rate_limited");
}

#[quench_test::test]
fn the_resend_link_is_offered_where_a_lost_mail_is_likely() {
    assert!(notice::login_offers_resend(&query(&[(
        "err",
        "ui_login_email_unverified"
    )])));
    assert!(notice::login_offers_resend(&query(&[("registered", "1")])));
    assert!(notice::login_offers_resend(&query(&[(
        "resend_requested",
        "1"
    )])));
    assert!(!notice::login_offers_resend(&query(&[("err", "1")])));
}

#[quench_test::test]
fn the_account_and_admin_pages_read_ok() {
    assert_eq!(
        notice::account(&query(&[("ok", "saved")])).unwrap().key,
        "ui_account_ok_saved"
    );
    assert_eq!(
        notice::account(&query(&[("ok", "mfa_enabled")]))
            .unwrap()
            .tone,
        Tone::Ok
    );
    assert_eq!(notice::account(&query(&[("ok", "x")])), None);
    assert_eq!(
        notice::admin(&query(&[("ok", "deleted")])).unwrap().key,
        "ui_admin_ok_deleted"
    );
    let failed = notice::admin(&query(&[("ok", "invite_failed")])).unwrap();
    assert_eq!(
        (failed.key, failed.tone),
        ("ui_admin_warn_invite_failed", Tone::Error)
    );
    assert_eq!(
        notice::login_mfa(&query(&[("err", "1")])).unwrap().key,
        "ui_login_mfa_invalid"
    );
    assert_eq!(notice::login_mfa(&query(&[])), None);
}
