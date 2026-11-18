use mail_client::imap::{Folder, Value, parse_internal_date, utf7_decode, utf7_encode};

#[quench_test::test]
fn modified_utf7_round_trips() {
    for name in [
        "INBOX",
        "Wysłane",
        "Kosz/Zażółć",
        "A & B",
        "日本語",
        "plain",
        "&",
        "x&y&z",
        "Ünïcödé",
        "a😀b",
    ] {
        let encoded = utf7_encode(name);
        assert!(encoded.is_ascii(), "{encoded}");
        assert_eq!(utf7_decode(&encoded), name, "{name} -> {encoded}");
    }
    assert_eq!(utf7_encode("Wysłane"), "Wys&AUI-ane");
    assert_eq!(utf7_encode("A & B"), "A &- B");
    assert_eq!(utf7_decode("&AUI-"), "ł");
}

#[quench_test::test]
fn broken_utf7_does_not_panic() {
    for bad in ["&", "&&&", "&AU", "&-&-", "&!!!-", "x&", ""] {
        let _ = utf7_decode(bad);
    }
    assert_eq!(utf7_decode("a&"), "a&");
}

#[quench_test::test]
fn internal_dates_parse_with_zones() {
    assert_eq!(
        parse_internal_date("17-Jul-1996 02:44:25 -0700"),
        Some(837_596_665)
    );
    assert_eq!(parse_internal_date("01-Jan-1970 00:00:00 +0000"), Some(0));
    assert_eq!(parse_internal_date("01-Jan-1970 01:00:00 +0100"), Some(0));
    assert_eq!(parse_internal_date("garbage"), None);
    assert_eq!(parse_internal_date(""), None);
}

fn folder(path: &str, delimiter: Option<char>, attributes: &[&str]) -> Folder {
    Folder {
        path: path.into(),
        delimiter,
        attributes: attributes.iter().map(ToString::to_string).collect(),
    }
}

#[quench_test::test]
fn folder_roles_come_from_special_use_then_from_names() {
    assert_eq!(folder("INBOX", Some('/'), &[]).role(), "inbox");
    assert_eq!(folder("inbox", Some('/'), &[]).role(), "inbox");
    assert_eq!(folder("Whatever", Some('/'), &["sent"]).role(), "sent");
    assert_eq!(
        folder("[Gmail]/Wersje robocze", Some('/'), &["drafts"]).role(),
        "drafts"
    );
    assert_eq!(folder("X", None, &["trash"]).role(), "trash");
    assert_eq!(folder("X", None, &["junk"]).role(), "junk");
    assert_eq!(folder("X", None, &["archive"]).role(), "archive");
    assert_eq!(folder("X", None, &["all"]).role(), "all");
    assert_eq!(folder("X", None, &["flagged"]).role(), "flagged");
    assert_eq!(folder("Sent Items", Some('/'), &[]).role(), "sent");
    assert_eq!(folder("INBOX.Wysłane", Some('.'), &[]).role(), "sent");
    assert_eq!(folder("Kosz", Some('/'), &[]).role(), "trash");
    assert_eq!(folder("Spam", Some('/'), &[]).role(), "junk");
    assert_eq!(folder("Archiwum", Some('/'), &[]).role(), "archive");
    assert_eq!(folder("Szkice", Some('/'), &[]).role(), "drafts");
    assert_eq!(folder("Projects", Some('/'), &[]).role(), "");
}

#[quench_test::test]
fn noselect_folders_are_not_selectable() {
    assert!(folder("a", None, &["hasnochildren"]).selectable());
    assert!(!folder("a", None, &["noselect"]).selectable());
    assert!(!folder("a", None, &["nonexistent"]).selectable());
}

#[quench_test::test]
fn values_expose_their_content() {
    assert_eq!(Value::Atom("12".into()).number(), Some(12));
    assert_eq!(Value::Atom("x".into()).number(), None);
    assert_eq!(Value::Nil.number(), None);
    assert_eq!(Value::Str(b"hi".to_vec()).text().as_deref(), Some("hi"));
    assert_eq!(Value::Atom("a".into()).text().as_deref(), Some("a"));
    assert_eq!(Value::Nil.text(), None);
    assert_eq!(Value::Str(b"b".to_vec()).bytes(), Some(&b"b"[..]));
    assert_eq!(Value::Atom("c".into()).bytes(), Some(&b"c"[..]));
    assert_eq!(Value::List(vec![]).bytes(), None);
    assert_eq!(
        Value::List(vec![Value::Nil]).list().map(<[Value]>::len),
        Some(1)
    );
    assert!(Value::Nil.list().is_none());
}
