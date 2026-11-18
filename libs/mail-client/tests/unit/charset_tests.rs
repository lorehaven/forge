use mail_client::charset::{decode, is_known};

#[quench_test::test]
fn utf8_and_ascii_pass_through() {
    assert_eq!(decode("utf-8", "zażółć".as_bytes()), "zażółć");
    assert_eq!(decode("UTF8", b"plain"), "plain");
    assert_eq!(decode("us-ascii", b"plain"), "plain");
    assert_eq!(decode("", b"plain"), "plain");
}

#[quench_test::test]
fn invalid_utf8_is_replaced_not_refused() {
    assert_eq!(decode("utf-8", &[b'a', 0xFF, b'b']), "a\u{FFFD}b");
}

#[quench_test::test]
fn polish_legacy_sets_decode() {
    // "zażółć" in ISO-8859-2 and Windows-1250.
    assert_eq!(
        decode("iso-8859-2", &[0x7A, 0x61, 0xBF, 0xF3, 0xB3, 0xE6]),
        "zażółć"
    );
    assert_eq!(
        decode("windows-1250", &[0x7A, 0x61, 0xBF, 0xF3, 0xB3, 0xE6]),
        "zażółć"
    );
    assert_eq!(decode("CP1250", &[0xB9]), "ą");
    assert_eq!(decode("latin2", &[0xB1]), "ą");
}

#[quench_test::test]
fn western_sets_decode() {
    assert_eq!(decode("iso-8859-1", &[0xE9]), "é");
    assert_eq!(decode("windows-1252", &[0x80, 0xE9]), "€é");
    assert_eq!(decode("iso-8859-15", &[0xA4]), "€");
}

#[quench_test::test]
fn cyrillic_and_greek_and_turkish_sets_decode() {
    assert_eq!(decode("windows-1251", &[0xCF, 0xF0, 0xE8]), "При");
    assert_eq!(decode("koi8-r", &[0xD0, 0xD2, 0xC9]), "при");
    assert_eq!(decode("iso-8859-5", &[0xBF]), "П");
    assert_eq!(decode("iso-8859-7", &[0xE1]), "α");
    assert_eq!(decode("windows-1253", &[0xE1]), "α");
    assert_eq!(decode("iso-8859-9", &[0xFE]), "ş");
    assert_eq!(decode("windows-1254", &[0xFE]), "ş");
}

#[quench_test::test]
fn unknown_labels_fall_back_to_lossy_utf8() {
    assert_eq!(decode("x-unknown", "ok".as_bytes()), "ok");
    assert!(!is_known("x-unknown"));
    assert!(is_known("UTF-8"));
    assert!(is_known("\"windows-1250\""));
}

#[quench_test::test]
fn ascii_bytes_below_0x80_are_unchanged_in_every_set() {
    for label in ["iso-8859-2", "windows-1251", "koi8-r", "windows-1252"] {
        assert_eq!(decode(label, b"Hello, 123"), "Hello, 123");
    }
}
