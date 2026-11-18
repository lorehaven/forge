use mail_client::entity::{self, Body, clean_filename};
use mail_client::message::{attachment_data, display_header, parse, summarize};

fn crlf(text: &str) -> Vec<u8> {
    text.replace('\n', "\r\n").into_bytes()
}

#[quench_test::test]
fn a_simple_text_message() {
    let raw = crlf(
        "From: Alice <alice@example.com>\nTo: bob@example.com\nSubject: Hello\nDate: Tue, 29 Sep 2026 12:00:00 +0000\nMessage-ID: <1@x>\n\nHi Bob\n",
    );
    let m = parse(&raw);
    assert_eq!(m.summary.subject, "Hello");
    assert_eq!(m.summary.from[0].name, "Alice");
    assert_eq!(m.summary.to[0].address, "bob@example.com");
    assert_eq!(m.summary.date, Some(1_790_683_200));
    assert_eq!(m.summary.message_id, "<1@x>");
    assert_eq!(m.text.trim(), "Hi Bob");
    assert!(m.html.is_none());
    assert!(m.attachments.is_empty());
}

#[quench_test::test]
fn summaries_read_threading_and_list_headers() {
    let s = summarize(&crlf(
        "In-Reply-To: <a@x>\nReferences: <r1@x> <r2@x>\nList-Unsubscribe: <mailto:u@x>\nCc: c@x.org\nReply-To: r@x.org\nSubject: =?UTF-8?B?emHFvA==?=\n\n",
    ));
    assert_eq!(s.in_reply_to, "<a@x>");
    assert_eq!(s.references, ["<r1@x>", "<r2@x>"]);
    assert_eq!(s.list_unsubscribe, "<mailto:u@x>");
    assert_eq!(s.cc[0].address, "c@x.org");
    assert_eq!(s.reply_to[0].address, "r@x.org");
    assert_eq!(s.subject, "zaż");
    assert_eq!(summarize(b"").subject, "");
}

#[quench_test::test]
fn subjects_lose_control_characters() {
    let s = summarize(&crlf("Subject: =?UTF-8?Q?a=0Ab=09c?=\n\n"));
    assert_eq!(s.subject, "a b c");
    assert_eq!(display_header("x\u{7}y"), "x y");
}

#[quench_test::test]
fn alternative_parts_give_text_and_html_once() {
    let raw = crlf(
        "Content-Type: multipart/alternative; boundary=\"b\"\n\n--b\nContent-Type: text/plain; charset=utf-8\n\nplain body\n--b\nContent-Type: text/html; charset=utf-8\n\n<p>html body</p>\n--b--\n",
    );
    let m = parse(&raw);
    assert_eq!(m.text.trim(), "plain body");
    assert_eq!(m.html.as_deref().map(str::trim), Some("<p>html body</p>"));
}

#[quench_test::test]
fn html_only_gets_a_text_version() {
    let raw = crlf(
        "Content-Type: text/html\n\n<html><body><p>Hello <b>world</b></p><script>x()</script></body></html>",
    );
    let m = parse(&raw);
    assert_eq!(m.text, "Hello world");
    assert!(m.html.is_some());
}

#[quench_test::test]
fn mixed_parts_collect_attachments_in_order() {
    let raw = crlf(
        "Content-Type: multipart/mixed; boundary=\"m\"\n\n--m\nContent-Type: text/plain\n\nsee attached\n--m\nContent-Type: application/pdf; name=\"a.pdf\"\nContent-Disposition: attachment; filename=\"a.pdf\"\nContent-Transfer-Encoding: base64\n\nJVBERg==\n--m\nContent-Type: image/png\nContent-Disposition: inline; filename=\"i.png\"\nContent-ID: <img1>\nContent-Transfer-Encoding: base64\n\niVBORw==\n--m--\n",
    );
    let m = parse(&raw);
    assert_eq!(m.text.trim(), "see attached");
    assert_eq!(m.attachments.len(), 2);
    assert_eq!(m.attachments[0].filename, "a.pdf");
    assert_eq!(m.attachments[0].size, 4);
    assert_eq!(m.attachments[0].media_type, "application/pdf");
    assert!(!m.attachments[0].inline);
    assert!(m.attachments[1].inline);
    assert_eq!(m.attachments[1].content_id.as_deref(), Some("img1"));
    let (name, kind, data) = attachment_data(&raw, 0).unwrap();
    assert_eq!((name.as_str(), kind.as_str()), ("a.pdf", "application/pdf"));
    assert_eq!(data, b"%PDF");
    assert_eq!(attachment_data(&raw, 1).unwrap().2.len(), 4);
    assert!(attachment_data(&raw, 2).is_none());
}

#[quench_test::test]
fn nested_related_and_alternative_structures() {
    let raw = crlf(
        "Content-Type: multipart/mixed; boundary=\"o\"\n\n--o\nContent-Type: multipart/alternative; boundary=\"a\"\n\n--a\nContent-Type: text/plain\n\nt\n--a\nContent-Type: multipart/related; boundary=\"r\"\n\n--r\nContent-Type: text/html\n\n<img src=\"cid:pic\">\n--r\nContent-Type: image/gif\nContent-ID: <pic>\nContent-Transfer-Encoding: base64\n\nR0lGOA==\n--r--\n--a--\n--o\nContent-Type: text/plain; name=n.txt\nContent-Disposition: attachment; filename=n.txt\n\nfile text\n--o--\n",
    );
    let m = parse(&raw);
    assert_eq!(m.text.trim(), "t");
    assert!(m.html.unwrap().contains("cid:pic"));
    assert_eq!(m.attachments.len(), 2);
    assert!(m.attachments[0].inline);
    assert_eq!(m.attachments[1].filename, "n.txt");
    assert_eq!(attachment_data(&raw, 1).unwrap().2, b"file text");
}

#[quench_test::test]
fn signed_and_encrypted_messages_expose_the_signature_and_payload() {
    let signed = crlf(
        "Content-Type: multipart/signed; boundary=\"s\"; protocol=\"application/pgp-signature\"\n\n--s\nContent-Type: text/plain\n\nsigned text\n--s\nContent-Type: application/pgp-signature; name=\"sig.asc\"\n\n-----BEGIN PGP SIGNATURE-----\n--s--\n",
    );
    let m = parse(&signed);
    assert_eq!(m.text.trim(), "signed text");
    assert_eq!(m.attachments.len(), 1);
    assert_eq!(m.attachments[0].filename, "sig.asc");
    assert!(attachment_data(&signed, 0).is_some());

    let encrypted = crlf(
        "Content-Type: multipart/encrypted; boundary=\"e\"\n\n--e\nContent-Type: application/pgp-encrypted\n\nVersion: 1\n--e\nContent-Type: application/octet-stream\n\nciphertext\n--e--\n",
    );
    let m = parse(&encrypted);
    assert_eq!(m.attachments.len(), 2);
    assert_eq!(m.text, "");
}

#[quench_test::test]
fn transfer_encodings_and_charsets_apply() {
    let raw = crlf(
        "Content-Type: text/plain; charset=iso-8859-2\nContent-Transfer-Encoding: quoted-printable\n\nZa=BF=F3=B3=E6\n",
    );
    assert_eq!(parse(&raw).text.trim(), "Zażółć");
    let raw = crlf(
        "Content-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: base64\n\nemHFvMOzxYLEhw==\n",
    );
    assert_eq!(parse(&raw).text.trim(), "zażółć");
}

#[quench_test::test]
fn missing_content_type_and_unknown_encoding_default_to_text() {
    let m = parse(&crlf("Subject: x\n\nplain\n"));
    assert_eq!(m.text.trim(), "plain");
    let m = parse(&crlf(
        "Content-Type: nonsense\nContent-Transfer-Encoding: weird\n\nplain\n",
    ));
    assert_eq!(m.text.trim(), "plain");
}

#[quench_test::test]
fn attachments_without_names_get_a_safe_one() {
    let raw = crlf(
        "Content-Type: multipart/mixed; boundary=\"m\"\n\n--m\nContent-Type: application/zip\nContent-Disposition: attachment\n\nPK\n--m\nContent-Type: message/rfc822\n\nSubject: inner\n--m\nContent-Type: application/x-we!rd\n\nz\n--m--\n",
    );
    let m = parse(&raw);
    assert_eq!(m.attachments[0].filename, "attachment.zip");
    assert_eq!(m.attachments[1].filename, "message.eml");
    assert_eq!(m.attachments[2].filename, "attachment.xwerd");
}

#[quench_test::test]
fn filenames_cannot_carry_paths_or_control_characters() {
    assert_eq!(clean_filename("../../etc/passwd"), "passwd");
    assert_eq!(clean_filename("C:\\Windows\\evil.exe"), "evil.exe");
    assert_eq!(clean_filename("a\r\nb.txt"), "ab.txt");
    assert_eq!(clean_filename(".htaccess"), "htaccess");
    assert_eq!(clean_filename(""), "attachment");
    assert_eq!(clean_filename("\"<>|:*?"), "attachment");
    assert_eq!(clean_filename(&"x".repeat(500)).len(), 180);
    assert_eq!(clean_filename("=?UTF-8?B?emHFvC50eHQ=?="), "zaż.txt");
}

#[quench_test::test]
fn a_missing_closing_boundary_still_yields_the_last_part() {
    let raw = crlf(
        "Content-Type: multipart/mixed; boundary=\"b\"\n\n--b\nContent-Type: text/plain\n\nfirst\n--b\nContent-Type: text/plain\n\nsecond",
    );
    let m = parse(&raw);
    assert!(
        m.text.contains("first") && m.text.contains("second"),
        "{}",
        m.text
    );
}

#[quench_test::test]
fn boundary_lookalikes_inside_a_part_do_not_split_it() {
    let raw = crlf(
        "Content-Type: multipart/mixed; boundary=\"abc\"\n\n--abc\nContent-Type: text/plain\n\nline\n--abcdef not a delimiter\nmore\n--abc--\n",
    );
    let tree = entity::parse(&raw);
    let Body::Parts(parts) = &tree.body else {
        panic!("multipart")
    };
    assert_eq!(parts.len(), 1);
    assert!(parts[0].text().contains("not a delimiter"));
}

#[quench_test::test]
fn deep_nesting_and_part_floods_are_bounded() {
    let mut raw = String::new();
    for i in 0..40 {
        raw.push_str(&format!(
            "Content-Type: multipart/mixed; boundary=\"b{i}\"\n\n--b{i}\n"
        ));
    }
    raw.push_str("Content-Type: text/plain\n\ndeep\n");
    let m = parse(&crlf(&raw));
    let _ = m; // must not overflow the stack or take forever

    let mut raw = String::from("Content-Type: multipart/mixed; boundary=\"b\"\n\n");
    for _ in 0..5000 {
        raw.push_str("--b\nContent-Type: text/plain\n\nx\n");
    }
    raw.push_str("--b--\n");
    let tree = entity::parse(&crlf(&raw));
    let Body::Parts(parts) = tree.body else {
        panic!()
    };
    assert!(parts.len() <= 1000);
}

#[quench_test::test]
fn huge_bodies_are_cut() {
    let body = "a".repeat(5 * 1024 * 1024);
    let m = parse(&crlf(&format!("Content-Type: text/plain\n\n{body}")));
    assert!(m.text.len() <= 4 * 1024 * 1024);
}

#[quench_test::test]
fn entity_helpers_expose_what_was_parsed() {
    let tree = entity::parse(&crlf(
        "Content-Type: text/plain; charset=\"iso-8859-2\"; format=flowed\nContent-Disposition: attachment; filename=\"a.txt\"\nContent-ID: <id1>\n\nx",
    ));
    assert_eq!(tree.charset(), Some("iso-8859-2"));
    assert_eq!(tree.param("format"), Some("flowed"));
    assert!(tree.is_attachment());
    assert_eq!(tree.content_id.as_deref(), Some("id1"));
    let multipart = entity::parse(&crlf(
        "Content-Type: multipart/mixed; boundary=\"b\"\n\n--b\n\nx\n--b--\n",
    ));
    assert_eq!(multipart.text(), "");
    // A named non-text part without a disposition is an attachment; a named text part is shown.
    assert!(entity::parse(&crlf("Content-Type: image/png; name=\"p.png\"\n\nx")).is_attachment());
    assert!(!entity::parse(&crlf("Content-Type: text/plain; name=\"p.txt\"\n\nx")).is_attachment());
    assert!(!entity::parse(&crlf("Content-Disposition: inline\n\nx")).is_attachment());
}

#[quench_test::test]
fn empty_and_binary_input_do_not_panic() {
    let _ = parse(b"");
    let _ = parse(&[0xFF; 1000]);
    let _ = parse(b"\r\n\r\n\r\n");
    let _ = parse(b"Content-Type: multipart/mixed\n\nx");
    let _ = parse(b"Content-Type: multipart/mixed; boundary=\n\nx");
}
