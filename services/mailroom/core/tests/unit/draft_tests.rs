use mailroom_core::draft::{plausible_address, split_recipients};
use mailroom_core::{AccountDraft, ComposeDraft, DraftError, TransportDraft};

fn account() -> AccountDraft {
    AccountDraft {
        label: "Work".into(),
        address: "me@example.com".into(),
        username: String::new(),
        password: "pw".into(),
        imap_host: "imap.example.com".into(),
        imap_port: "993".into(),
        imap_security: "tls".into(),
        smtp_host: "smtp.example.com".into(),
        smtp_port: "465".into(),
        smtp_security: "tls".into(),
        ..AccountDraft::default()
    }
}

#[quench_test::test]
fn a_good_account_form_becomes_a_body_and_the_username_defaults_to_the_address() {
    let input = account().validate(false).unwrap();
    assert_eq!(input.username, "me@example.com");
    assert_eq!(input.imap_port, 993);
    assert_eq!(input.smtp_security, "tls");
}

#[quench_test::test]
fn each_account_field_is_checked() {
    let cases: Vec<(Box<dyn Fn(&mut AccountDraft)>, DraftError)> = vec![
        (
            Box::new(|d| d.address = "nope".into()),
            DraftError::AddressInvalid,
        ),
        (
            Box::new(|d| d.label = "  ".into()),
            DraftError::LabelRequired,
        ),
        (
            Box::new(|d| d.imap_host = String::new()),
            DraftError::HostRequired,
        ),
        (
            Box::new(|d| d.smtp_host = String::new()),
            DraftError::HostRequired,
        ),
        (
            Box::new(|d| d.password = String::new()),
            DraftError::PasswordRequired,
        ),
        (
            Box::new(|d| d.imap_port = "0".into()),
            DraftError::PortInvalid,
        ),
        (
            Box::new(|d| d.smtp_port = "70000".into()),
            DraftError::PortInvalid,
        ),
        (
            Box::new(|d| d.smtp_port = "x".into()),
            DraftError::PortInvalid,
        ),
        (
            Box::new(|d| d.imap_security = "none".into()),
            DraftError::SecurityInvalid,
        ),
        (
            Box::new(|d| d.smtp_security = String::new()),
            DraftError::SecurityInvalid,
        ),
    ];
    for (mutate, expected) in cases {
        let mut d = account();
        mutate(&mut d);
        assert_eq!(d.validate(false).unwrap_err(), expected);
        assert!(!expected.message().is_empty());
    }
}

#[quench_test::test]
fn a_blank_password_is_fine_when_changing_a_mailbox() {
    let mut d = account();
    d.password = String::new();
    assert_eq!(d.validate(true).unwrap().password, "");
}

#[quench_test::test]
fn addresses_are_plausible_or_not() {
    for ok in ["a@b.co", "a.b+c@d.e.f", "x@localhost"] {
        assert!(plausible_address(ok), "{ok}");
    }
    for bad in [
        "",
        "a",
        "@b.c",
        "a@",
        "a@b",
        "a b@c.d",
        "a@.c.d",
        "a@c.d.",
        "a<b@c.d",
        "a@b.c,d@e.f",
        "a\"@b.c",
        "a@b.c;",
    ] {
        assert!(!plausible_address(bad), "{bad}");
    }
}

#[quench_test::test]
fn recipients_split_on_commas_semicolons_and_lines() {
    assert_eq!(
        split_recipients("a@b.c, d@e.f;g@h.i\n j@k.l ,,"),
        ["a@b.c", "d@e.f", "g@h.i", "j@k.l"]
    );
    assert!(split_recipients("  ").is_empty());
}

fn compose() -> ComposeDraft {
    ComposeDraft {
        account_id: "a1".into(),
        to: "x@y.zz".into(),
        subject: " Hi ".into(),
        text: "body".into(),
        ..ComposeDraft::default()
    }
}

#[quench_test::test]
fn a_compose_form_is_read_and_trimmed() {
    let mut d = compose();
    d.to = "A <a@b.cc>, c@d.ee".into();
    d.cc = "e@f.gg".into();
    let body = d.validate().unwrap();
    assert_eq!(body.to, ["A <a@b.cc>", "c@d.ee"]);
    assert_eq!(body.cc, ["e@f.gg"]);
    assert_eq!(body.subject, "Hi");
}

#[quench_test::test]
fn compose_errors() {
    let mut d = compose();
    d.to = String::new();
    assert_eq!(d.validate().unwrap_err(), DraftError::NoRecipients);
    d.to = "bad".into();
    assert_eq!(d.validate().unwrap_err(), DraftError::RecipientInvalid);
    d.to = "A <bad>".into();
    assert_eq!(d.validate().unwrap_err(), DraftError::RecipientInvalid);
    d.to = (0..101)
        .map(|i| format!("u{i}@x.yy"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(d.validate().unwrap_err(), DraftError::TooManyRecipients);
    let mut d = compose();
    d.subject = "a\nb".into();
    assert_eq!(d.validate().unwrap_err(), DraftError::SubjectLine);
    let mut d = compose();
    d.text = "  ".into();
    assert_eq!(d.validate().unwrap_err(), DraftError::EmptyMessage);
    d.attachments = vec!["u1".into()];
    assert!(d.validate().is_ok());
    for e in [
        DraftError::NoRecipients,
        DraftError::RecipientInvalid,
        DraftError::TooManyRecipients,
        DraftError::SubjectLine,
        DraftError::EmptyMessage,
    ] {
        assert!(!e.message().is_empty());
    }
}

#[quench_test::test]
fn the_transport_form_is_checked() {
    let ok = TransportDraft {
        host: "smtp.example.com".into(),
        port: "587".into(),
        security: "starttls".into(),
        username: "u".into(),
        password: String::new(),
        tls_roots_pem: " ".into(),
    };
    let input = ok.validate().unwrap();
    assert_eq!(input.port, 587);
    assert_eq!(input.tls_roots_pem, "");
    let mut bad = ok.clone();
    bad.host = String::new();
    assert_eq!(bad.validate().unwrap_err(), DraftError::HostRequired);
    let mut bad = ok.clone();
    bad.port = "x".into();
    assert_eq!(bad.validate().unwrap_err(), DraftError::PortInvalid);
    let mut bad = ok;
    bad.security = "plain".into();
    assert_eq!(bad.validate().unwrap_err(), DraftError::SecurityInvalid);
}
