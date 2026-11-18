use mail_client::header::{Headers, Params};

#[quench_test::test]
fn headers_are_unfolded_and_found_case_insensitively() {
    let raw = b"Subject: one\r\n two\r\n\tthree\r\nFrom: a@b.c\r\nX-Dup: 1\r\nx-dup: 2\r\n\r\nbody";
    let (headers, offset) = Headers::parse(raw);
    assert_eq!(headers.get("SUBJECT"), Some("one two three"));
    assert_eq!(headers.get("from"), Some("a@b.c"));
    assert_eq!(headers.get_all("x-dup").collect::<Vec<_>>(), ["1", "2"]);
    assert_eq!(&raw[offset..], b"body");
    assert_eq!(headers.len(), 4);
    assert!(!headers.is_empty());
    assert_eq!(headers.iter().count(), 4);
    assert!(headers.get("missing").is_none());
}

#[quench_test::test]
fn lf_only_input_and_missing_blank_line_are_tolerated() {
    let (headers, offset) = Headers::parse(b"A: 1\nB: 2\n\nrest");
    assert_eq!(headers.get("b"), Some("2"));
    assert_eq!(offset, 11);
    let (headers, offset) = Headers::parse(b"A: 1\nB: 2");
    assert_eq!(headers.len(), 2);
    assert_eq!(offset, 9);
}

#[quench_test::test]
fn garbage_lines_and_bad_names_are_skipped() {
    let (headers, _) = Headers::parse(
        b"From a@b Mon Jan 1\r\nnot a header\r\nBad Name: x\r\n: empty\r\nGood: yes\r\n\r\n",
    );
    assert_eq!(headers.len(), 1);
    assert_eq!(headers.get("good"), Some("yes"));
    assert!(Headers::parse(b"").0.is_empty());
}

#[quench_test::test]
fn a_leading_continuation_line_is_ignored() {
    let (headers, _) = Headers::parse(b" orphan\r\nA: 1\r\n\r\n");
    assert_eq!(headers.len(), 1);
}

#[quench_test::test]
fn field_count_and_value_length_are_bounded() {
    let mut raw = String::new();
    for i in 0..2000 {
        raw.push_str(&format!("X-{i}: v\r\n"));
    }
    raw.push_str("\r\n");
    assert_eq!(Headers::parse(raw.as_bytes()).0.len(), 512);

    let mut raw = String::from("Long: x\r\n");
    for _ in 0..2000 {
        raw.push_str(&format!(" {}\r\n", "y".repeat(100)));
    }
    let (headers, _) = Headers::parse(raw.as_bytes());
    assert!(headers.get("long").unwrap().len() < 70_000);
}

#[quench_test::test]
fn decoded_reads_encoded_words() {
    let (headers, _) = Headers::parse(b"Subject: =?UTF-8?B?emHFvA==?=\r\n\r\n");
    assert_eq!(headers.decoded("subject").as_deref(), Some("zaż"));
    assert_eq!(headers.decoded("none"), None);
}

#[quench_test::test]
fn from_pairs_builds_a_block() {
    let headers = Headers::from_pairs(vec![("A".into(), "1".into())]);
    assert_eq!(headers.get("a"), Some("1"));
}

#[quench_test::test]
fn params_parse_value_and_quoted_parameters() {
    let p = Params::parse("Text/Plain; charset=\"UTF-8\"; format=flowed", true);
    assert_eq!(p.value, "text/plain");
    assert_eq!(p.get("charset"), Some("UTF-8"));
    assert_eq!(p.get("format"), Some("flowed"));
    assert_eq!(p.get("missing"), None);
    let p = Params::parse("Attachment", false);
    assert_eq!(p.value, "Attachment");
    assert!(p.params.is_empty());
}

#[quench_test::test]
fn semicolons_inside_quotes_do_not_split() {
    let p = Params::parse(r#"attachment; filename="a;b.txt"; x="q\"uote""#, true);
    assert_eq!(p.get("filename"), Some("a;b.txt"));
    assert_eq!(p.get("x"), Some("q\"uote"));
}

#[quench_test::test]
fn rfc2231_continuations_and_encoding_are_joined() {
    let p = Params::parse(
        "attachment; filename*0*=utf-8''za%C5%BC; filename*1*=%C3%B3%C5%82.txt",
        true,
    );
    assert_eq!(p.get("filename"), Some("zażół.txt"));
    let p = Params::parse("attachment; filename*=iso-8859-2''%B1.txt", true);
    assert_eq!(p.get("filename"), Some("ą.txt"));
    let p = Params::parse(
        "attachment; filename*0=\"part1\"; filename*1=\"part2\"",
        true,
    );
    assert_eq!(p.get("filename"), Some("part1part2"));
    // Out-of-order pieces are put in order.
    let p = Params::parse("a; n*1=B; n*0=A", true);
    assert_eq!(p.get("n"), Some("AB"));
}

#[quench_test::test]
fn encoded_words_in_plain_parameters_are_decoded() {
    let p = Params::parse("attachment; filename=\"=?UTF-8?B?emHFvC50eHQ=?=\"", true);
    assert_eq!(p.get("filename"), Some("zaż.txt"));
}

#[quench_test::test]
fn parameter_without_value_or_name_is_skipped() {
    let p = Params::parse("a; ; =x; novalue; k=v", true);
    assert_eq!(p.params, vec![("k".to_string(), "v".to_string())]);
}
