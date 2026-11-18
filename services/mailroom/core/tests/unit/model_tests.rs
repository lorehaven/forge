use mailroom_core::admin::{Page, Settings, SettingsInput};
use mailroom_core::{ApiError, Change, Compose, Contact, FOLDERS, MessageList, SendRequest};
use quench_serde::json;

#[quench_test::test]
fn contacts_show_a_name_or_the_address() {
    let named = Contact {
        name: "Ann".into(),
        address: "ann@home.example".into(),
    };
    assert_eq!(
        (named.label(), named.full().as_str()),
        ("Ann", "Ann <ann@home.example>")
    );
    let bare = Contact {
        name: String::new(),
        address: "ann@home.example".into(),
    };
    assert_eq!(
        (bare.label(), bare.full().as_str()),
        ("ann@home.example", "ann@home.example")
    );
}

#[quench_test::test]
fn optional_fields_may_be_missing_from_the_json() {
    let c: Compose = json::from_str(r#"{"to":["ann"],"subject":"s","text":"t"}"#).unwrap();
    assert!(
        c.cc.is_empty() && c.bcc.is_empty() && c.in_reply_to.is_none() && c.attachments.is_empty()
    );
    let r: SendRequest = json::from_str(r#"{"to":["ann"],"subject":"s","text":"t"}"#).unwrap();
    assert!(
        r.html.is_none()
            && r.unsubscribe.is_none()
            && r.kind.is_none()
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
        domain: "home.example".into(),
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
fn a_mailbox_has_three_folders() {
    assert_eq!(FOLDERS, ["inbox", "sent", "trash"]);
}
