use mailroom_core::admin::{Page, Settings, SettingsInput};
use mailroom_core::{Account, ApiError, Change, Compose, Contact, MessageList, SendRequest};
use quench_serde::json;

#[quench_test::test]
fn contacts_show_a_name_or_the_address() {
    let named = Contact {
        name: "Ann".into(),
        address: "a@b.cc".into(),
    };
    assert_eq!(
        (named.label(), named.full().as_str()),
        ("Ann", "Ann <a@b.cc>")
    );
    let bare = Contact {
        name: String::new(),
        address: "a@b.cc".into(),
    };
    assert_eq!((bare.label(), bare.full().as_str()), ("a@b.cc", "a@b.cc"));
}

#[quench_test::test]
fn optional_fields_may_be_missing_from_the_json() {
    let c: Compose =
        json::from_str(r#"{"account_id":"a","to":["x@y.zz"],"subject":"s","text":"t"}"#).unwrap();
    assert!(c.cc.is_empty() && c.in_reply_to.is_none() && c.attachments.is_empty());
    let r: SendRequest = json::from_str(r#"{"to":["x@y.zz"],"subject":"s","text":"t"}"#).unwrap();
    assert!(
        r.from.is_empty()
            && r.html.is_none()
            && r.headers.is_empty()
            && r.idempotency_key.is_none()
    );
    let change: Change = json::from_str(r#"{"ids":["1"],"seen":true}"#).unwrap();
    assert_eq!(change.seen, Some(true));
    assert!(!change.delete && change.move_to.is_none());
    let s: SettingsInput = json::from_str("{}").unwrap();
    assert_eq!(s, SettingsInput::default());
}

#[quench_test::test]
fn models_round_trip_through_json() {
    let list = MessageList::default();
    assert_eq!(
        json::from_str::<MessageList>(&json::to_string(&list)).unwrap(),
        list
    );
    let err = ApiError { error: "no".into() };
    assert_eq!(json::to_string(&err), r#"{"error":"no"}"#);
    let settings = Settings {
        daily_budget: 5,
        ..Settings::default()
    };
    assert_eq!(
        json::from_str::<Settings>(&json::to_string(&settings)).unwrap(),
        settings
    );
    let page: Page<u32> = Page {
        items: vec![1, 2],
        total: 9,
    };
    assert_eq!(
        json::from_str::<Page<u32>>(&json::to_string(&page)).unwrap(),
        page
    );
}

#[quench_test::test]
fn an_account_never_carries_a_password_field() {
    let json = json::to_string(&Account {
        id: "1".into(),
        label: "l".into(),
        address: "a@b.cc".into(),
        username: "u".into(),
        imap_host: "i".into(),
        imap_port: 993,
        imap_security: "tls".into(),
        smtp_host: "s".into(),
        smtp_port: 465,
        smtp_security: "tls".into(),
        state: "ok".into(),
        last_error: None,
        last_sync: None,
        unread: 0,
        signature: String::new(),
        custom_certificate: false,
        save_sent: true,
    });
    assert!(!json.contains("password") && !json.contains("secret"));
}
