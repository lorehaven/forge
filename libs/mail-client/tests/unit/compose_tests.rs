use mail_client::compose::{
    Attachment, ComposeError, Draft, build, forward_subject, parse_address, quote, reply_subject,
};
use mail_client::message::{attachment_data, parse};

fn draft() -> Draft {
    Draft {
        from: "Forge <noreply@example.com>".into(),
        to: vec!["alice@example.org".into()],
        subject: "Hello".into(),
        text: Some("Body text".into()),
        ..Draft::default()
    }
}

fn text(built: &mail_client::compose::Built) -> String {
    String::from_utf8_lossy(&built.raw).into_owned()
}

#[quench_test::test]
fn a_plain_message_has_the_headers_a_reader_needs() {
    let built = build(&draft(), 1_790_683_200).unwrap();
    let raw = text(&built);
    assert!(raw.contains("Date: Tue, 29 Sep 2026 12:00:00 +0000\r\n"));
    assert!(raw.contains("From: Forge <noreply@example.com>\r\n"));
    assert!(raw.contains("To: alice@example.org\r\n"));
    assert!(raw.contains("Subject: Hello\r\n"));
    assert!(raw.contains("MIME-Version: 1.0\r\n"));
    assert!(raw.contains(&format!("Message-ID: {}\r\n", built.message_id)));
    assert!(built.message_id.ends_with("@example.com>"));
    assert!(raw.contains("Content-Type: text/plain; charset=utf-8"));
    assert!(raw.ends_with("Body text\r\n"));
    assert_eq!(built.envelope.from.as_str(), "noreply@example.com");
    assert_eq!(built.envelope.to.len(), 1);
}

#[quench_test::test]
fn what_is_built_reads_back_the_same() {
    let mut d = draft();
    d.cc = vec!["Bob <bob@example.org>".into()];
    d.subject = "Zażółć gęślą jaźń".into();
    d.text = Some("Zażółć\r\ngęślą".into());
    d.html = Some("<p>Zażółć</p>".into());
    let built = build(&d, 0).unwrap();
    let back = parse(&built.raw);
    assert_eq!(back.summary.subject, "Zażółć gęślą jaźń");
    assert_eq!(back.summary.cc[0].name, "Bob");
    assert_eq!(
        back.text.trim(),
        "Zażółć\ngęślą".replace('\n', "\r\n").trim()
    );
    assert!(back.html.unwrap().contains("Zażółć"));
}

#[quench_test::test]
fn bcc_is_in_the_envelope_and_in_no_header() {
    let mut d = draft();
    d.bcc = vec!["hidden@example.net".into()];
    let built = build(&d, 0).unwrap();
    assert_eq!(built.envelope.to.len(), 2);
    assert!(!text(&built).contains("hidden@example.net"));
    assert!(!text(&built).to_ascii_lowercase().contains("bcc:"));
}

#[quench_test::test]
fn attachments_make_a_mixed_message_that_reads_back() {
    let mut d = draft();
    d.attachments = vec![
        Attachment {
            filename: "zażółć.txt".into(),
            media_type: "text/plain".into(),
            data: "dane".as_bytes().to_vec(),
        },
        Attachment {
            filename: "../../evil.sh".into(),
            media_type: "bad type".into(),
            data: vec![0, 1, 2, 255],
        },
    ];
    let built = build(&d, 0).unwrap();
    let raw = text(&built);
    assert!(raw.contains("multipart/mixed"));
    assert!(raw.contains("filename*=utf-8''za%C5%BC%C3%B3%C5%82%C4%87.txt"));
    assert!(raw.contains("filename*=utf-8''evil.sh"));
    let back = parse(&built.raw);
    assert_eq!(back.attachments.len(), 2);
    assert_eq!(back.attachments[0].filename, "zażółć.txt");
    assert_eq!(back.attachments[1].media_type, "application/octet-stream");
    assert_eq!(
        attachment_data(&built.raw, 1).unwrap().2,
        vec![0, 1, 2, 255]
    );
}

#[quench_test::test]
fn an_attachment_only_message_is_allowed() {
    let mut d = draft();
    d.text = None;
    d.attachments = vec![Attachment {
        filename: "a.bin".into(),
        media_type: "application/octet-stream".into(),
        data: vec![1],
    }];
    let built = build(&d, 0).unwrap();
    assert_eq!(parse(&built.raw).attachments.len(), 1);
}

#[quench_test::test]
fn threading_headers_are_written_as_message_ids() {
    let mut d = draft();
    d.in_reply_to = Some("abc@x".into());
    d.references = vec!["<r1@x>".into(), "r2@x".into()];
    let raw = text(&build(&d, 0).unwrap());
    assert!(raw.contains("In-Reply-To: <abc@x>\r\n"));
    assert!(raw.contains("References: <r1@x> <r2@x>\r\n"));
}

#[quench_test::test]
fn extra_headers_are_limited_to_a_safe_set() {
    let mut d = draft();
    d.headers = vec![
        ("List-Unsubscribe".into(), "<https://x.example/u>".into()),
        ("X-Source".into(), "gatehouse".into()),
        ("Auto-Submitted".into(), "auto-generated".into()),
    ];
    let raw = text(&build(&d, 0).unwrap());
    assert!(raw.contains("List-Unsubscribe: <https://x.example/u>\r\n"));
    assert!(raw.contains("X-Source: gatehouse\r\n"));
    for (name, value) in [
        ("Bcc", "x@y.z"),
        ("From", "evil@y.z"),
        ("To", "x@y.z"),
        ("Content-Type", "x"),
        ("Subject", "x"),
        ("X-Bad", "line\r\nBcc: evil@y.z"),
        ("X Bad", "x"),
        ("", "x"),
        ("X-Nul", "a\0b"),
    ] {
        let mut d = draft();
        d.headers = vec![(name.into(), value.into())];
        assert!(
            matches!(build(&d, 0), Err(ComposeError::ForbiddenHeader(_))),
            "{name}: {value:?}"
        );
    }
}

#[quench_test::test]
fn header_injection_is_refused_everywhere() {
    for mutate in [
        |d: &mut Draft| d.subject = "Hi\r\nBcc: evil@x.org".into(),
        |d: &mut Draft| d.subject = "Hi\nBcc: evil@x.org".into(),
        |d: &mut Draft| d.from = "a@b.c\r\nBcc: evil@x.org".into(),
        |d: &mut Draft| d.to = vec!["a@b.c\r\nBcc: evil@x.org".into()],
        |d: &mut Draft| d.cc = vec!["Name\r\n <a@b.c>".into()],
        |d: &mut Draft| d.reply_to = Some("a@b.c\nBcc: x@y.z".into()),
        |d: &mut Draft| d.in_reply_to = Some("<a@b>\r\nBcc: x@y.z".into()),
        |d: &mut Draft| d.references = vec!["<a@b> <c@d>\nX: y".into()],
    ] {
        let mut d = draft();
        mutate(&mut d);
        assert!(build(&d, 0).is_err());
    }
}

#[quench_test::test]
fn names_with_specials_are_quoted_or_encoded() {
    let mut d = draft();
    d.from = "\"Doe, John\" <john@example.com>".into();
    d.to = vec!["Zażółć <z@example.org>".into()];
    let raw = text(&build(&d, 0).unwrap());
    assert!(raw.contains("From: \"Doe, John\" <john@example.com>"));
    assert!(raw.contains("To: =?UTF-8?B?"));
}

#[quench_test::test]
fn addresses_and_recipients_are_checked() {
    let mut d = draft();
    d.to = vec![];
    assert_eq!(build(&d, 0).unwrap_err(), ComposeError::NoRecipients);
    d.to = vec!["   ".into()];
    assert_eq!(build(&d, 0).unwrap_err(), ComposeError::NoRecipients);
    d.to = vec!["not an address".into()];
    assert!(matches!(build(&d, 0), Err(ComposeError::InvalidAddress(_))));
    d.to = (0..101).map(|i| format!("u{i}@example.org")).collect();
    assert_eq!(build(&d, 0).unwrap_err(), ComposeError::TooManyRecipients);
    let mut d = draft();
    d.from = "nope".into();
    assert!(matches!(build(&d, 0), Err(ComposeError::InvalidAddress(_))));
    let long = "x".repeat(200);
    let mut d = draft();
    d.to = vec![format!("{long}@example.org")];
    let Err(ComposeError::InvalidAddress(shown)) = build(&d, 0) else {
        panic!()
    };
    assert!(shown.len() <= 80);
}

#[quench_test::test]
fn empty_messages_and_oversized_attachments_are_refused() {
    let mut d = draft();
    d.text = None;
    assert_eq!(build(&d, 0).unwrap_err(), ComposeError::Empty);
    d.text = Some(String::new());
    assert_eq!(build(&d, 0).unwrap_err(), ComposeError::Empty);

    let mut d = draft();
    d.attachments = vec![Attachment {
        filename: "big".into(),
        media_type: "x/y".into(),
        data: vec![0; 26 * 1024 * 1024],
    }];
    assert_eq!(build(&d, 0).unwrap_err(), ComposeError::AttachmentsTooLarge);
    d.attachments = (0..26)
        .map(|i| Attachment {
            filename: format!("f{i}"),
            media_type: "x/y".into(),
            data: vec![1],
        })
        .collect();
    assert_eq!(build(&d, 0).unwrap_err(), ComposeError::TooManyAttachments);
}

#[quench_test::test]
fn error_messages_are_readable() {
    for e in [
        ComposeError::InvalidAddress("x".into()),
        ComposeError::NoRecipients,
        ComposeError::TooManyRecipients,
        ComposeError::LineBreak("the subject"),
        ComposeError::ForbiddenHeader("Bcc".into()),
        ComposeError::TooManyAttachments,
        ComposeError::AttachmentsTooLarge,
        ComposeError::Empty,
    ] {
        assert!(!e.to_string().is_empty());
    }
}

#[quench_test::test]
fn reply_and_forward_helpers() {
    assert_eq!(reply_subject("Hello"), "Re: Hello");
    assert_eq!(reply_subject("re: Hello"), "re: Hello");
    assert_eq!(reply_subject("RE: Hello"), "RE: Hello");
    assert_eq!(forward_subject("Hello"), "Fwd: Hello");
    assert_eq!(forward_subject("FWD: Hello"), "FWD: Hello");
    assert_eq!(forward_subject("  Hello "), "Fwd: Hello");
    let q = quote("a\nb", "Alice", Some(0));
    assert_eq!(
        q,
        "On Thu, 1 Jan 1970 00:00:00 +0000, Alice wrote:\n> a\n> b\n"
    );
    assert_eq!(quote("x", "Bob", None), "Bob wrote:\n> x\n");
}

#[quench_test::test]
fn parse_address_is_strict() {
    assert_eq!(parse_address(" a@b.co ").unwrap().as_str(), "a@b.co");
    assert!(parse_address("a b@c.d").is_err());
}

#[quench_test::test]
fn long_lines_in_bodies_are_encoded_not_raw() {
    let mut d = draft();
    d.text = Some(format!("{}\r\n", "ł".repeat(500)));
    let built = build(&d, 0).unwrap();
    for line in text(&built).split("\r\n") {
        assert!(line.len() <= 998, "line of {}", line.len());
    }
    assert!(parse(&built.raw).text.contains(&"ł".repeat(500)));
}
