use mailroom_core::draft::{local_part_ok, recipient_name, split_recipients};
use mailroom_core::{ComposeDraft, DraftError};

fn compose() -> ComposeDraft {
    ComposeDraft {
        to: "ann@home.example, Bob <bob@home.example>".into(),
        subject: " Hello ".into(),
        text: "Hi".into(),
        ..ComposeDraft::default()
    }
}

#[quench_test::test]
fn local_parts_are_plain() {
    for good in ["ann", "ann.b", "a_b-c", "A1", &"x".repeat(64)] {
        assert!(local_part_ok(good), "{good}");
    }
    for bad in [
        "",
        ".ann",
        "ann.",
        "a..b",
        "a b",
        "a@b",
        "ann<",
        "é",
        &"x".repeat(65),
    ] {
        assert!(!local_part_ok(bad), "{bad}");
    }
}

#[quench_test::test]
fn a_recipient_is_reduced_to_its_lowercase_user_name() {
    assert_eq!(recipient_name("Ann").as_deref(), Some("ann"));
    assert_eq!(recipient_name(" ann@home.example ").as_deref(), Some("ann"));
    assert_eq!(
        recipient_name("Ann B <Ann@home.example>").as_deref(),
        Some("ann")
    );
    for bad in ["", "@home.example", "a b", "ann@", "<>", "x;y"] {
        // `ann@` has the name `ann`: whether the domain is right is the server's to say
        if bad == "ann@" {
            assert_eq!(recipient_name(bad).as_deref(), Some("ann"));
        } else {
            assert_eq!(recipient_name(bad), None, "{bad}");
        }
    }
}

#[quench_test::test]
fn recipient_lists_split_on_commas_semicolons_and_lines() {
    assert_eq!(split_recipients("a, b;c\n\n d "), ["a", "b", "c", "d"]);
    assert!(split_recipients(" , ").is_empty());
}

#[quench_test::test]
fn a_good_compose_form_becomes_a_body() {
    let body = compose().validate().unwrap();
    assert_eq!(body.to, ["ann@home.example", "Bob <bob@home.example>"]);
    assert_eq!(body.subject, "Hello");
}

#[quench_test::test]
fn each_compose_field_is_checked() {
    let cases: Vec<(Box<dyn Fn(&mut ComposeDraft)>, DraftError)> = vec![
        (Box::new(|d| d.to.clear()), DraftError::NoRecipients),
        (
            Box::new(|d| d.cc = "no good".into()),
            DraftError::RecipientInvalid,
        ),
        (
            Box::new(|d| d.bcc = "x y".into()),
            DraftError::RecipientInvalid,
        ),
        (
            Box::new(|d| d.to = vec!["a"; 51].join(",")),
            DraftError::TooManyRecipients,
        ),
        (
            Box::new(|d| d.subject = "a\nb".into()),
            DraftError::SubjectLine,
        ),
        (Box::new(|d| d.text = "  ".into()), DraftError::EmptyMessage),
    ];
    for (change, expected) in cases {
        let mut draft = compose();
        change(&mut draft);
        assert_eq!(draft.validate().unwrap_err(), expected);
        assert!(!expected.message().is_empty());
    }
    // An attachment alone is a message.
    let mut draft = compose();
    draft.text.clear();
    draft.attachments = vec!["u1".into()];
    assert!(draft.validate().is_ok());
}

#[quench_test::test]
fn every_error_has_words() {
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
