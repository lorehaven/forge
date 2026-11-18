use mail_client::encoding::*;

#[quench_test::test]
fn base64_round_trips_and_tolerates_noise() {
    assert_eq!(base64_decode(b"aGVsbG8="), b"hello");
    assert_eq!(base64_decode(b"aGVs\r\nbG8"), b"hello");
    assert_eq!(base64_decode(b"aGVsbG8"), b"hello");
    assert_eq!(base64_decode(b"aGV$sbG8="), b"hello");
    assert_eq!(base64_decode(b""), b"");
    assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    assert_eq!(
        base64_decode(base64_encode(&[0, 255, 1, 2, 3, 250]).as_bytes()),
        [0, 255, 1, 2, 3, 250]
    );
}

#[quench_test::test]
fn base64_lines_wrap_at_76_with_crlf() {
    let data = vec![7u8; 100];
    let text = base64_lines(&data);
    assert!(text.ends_with("\r\n"));
    for line in text.trim_end().split("\r\n") {
        assert!(line.len() <= 76);
    }
    assert_eq!(base64_decode(text.as_bytes()), data);
    assert_eq!(base64_lines(b""), "");
}

#[quench_test::test]
fn quoted_printable_decodes_escapes_and_soft_breaks() {
    assert_eq!(
        quoted_printable_decode(b"caf=C3=A9", false),
        "café".as_bytes()
    );
    assert_eq!(quoted_printable_decode(b"a=\r\nb", false), b"ab");
    assert_eq!(quoted_printable_decode(b"a=\nb", false), b"ab");
    assert_eq!(quoted_printable_decode(b"a_b", false), b"a_b");
    assert_eq!(quoted_printable_decode(b"a_b", true), b"a b");
    assert_eq!(quoted_printable_decode(b"100=%", false), b"100=%");
    assert_eq!(quoted_printable_decode(b"end=", false), b"end");
    assert_eq!(quoted_printable_decode(b"=ZZ", false), b"=ZZ");
    assert_eq!(quoted_printable_decode(b"x=4", false), b"x=4");
}

#[quench_test::test]
fn encoded_words_decode_in_both_encodings() {
    assert_eq!(decode_words("=?UTF-8?B?emHFvMOzxYLEhw==?="), "zażółć");
    assert_eq!(
        decode_words("=?utf-8?Q?za=C5=BC=C3=B3=C5=82=C4=87?="),
        "zażółć"
    );
    assert_eq!(
        decode_words("=?iso-8859-2?Q?Za=BF=F3=B3=E6_g=EA=B6l=B1?="),
        "Zażółć gęślą"
    );
    assert_eq!(decode_words("=?UTF-8*pl?B?emHFvA==?="), "zaż");
    assert_eq!(decode_words("plain text"), "plain text");
    assert_eq!(decode_words("a =?UTF-8?B?Yg==?= c"), "a b c");
}

#[quench_test::test]
fn whitespace_between_adjacent_encoded_words_is_dropped() {
    assert_eq!(decode_words("=?UTF-8?B?YQ==?= =?UTF-8?B?Yg==?="), "ab");
    assert_eq!(
        decode_words("=?UTF-8?B?YQ==?=\t\r\n =?UTF-8?B?Yg==?="),
        "ab"
    );
}

#[quench_test::test]
fn malformed_encoded_words_are_left_alone() {
    for text in [
        "=?UTF-8?X?abc?=",
        "=?UTF-8?B?abc",
        "=?broken",
        "=??B??=",
        "=?utf-8?B?a b?=",
    ] {
        assert_eq!(decode_words(text), text, "{text}");
    }
}

#[quench_test::test]
fn percent_decoding_handles_bad_escapes() {
    assert_eq!(percent_decode("a%20b"), b"a b");
    assert_eq!(percent_decode("%C5%82"), "ł".as_bytes());
    assert_eq!(percent_decode("100%"), b"100%");
    assert_eq!(percent_decode("%zz"), b"%zz");
}
