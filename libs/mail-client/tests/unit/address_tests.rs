use mail_client::address::{Contact, parse_list, valid_address};

fn one(text: &str) -> Contact {
    let list = parse_list(text);
    assert_eq!(list.len(), 1, "{text}: {list:?}");
    list.into_iter().next().unwrap()
}

#[quench_test::test]
fn the_usual_shapes_are_read() {
    assert_eq!(
        one("alice@example.com"),
        Contact {
            name: String::new(),
            address: "alice@example.com".into()
        }
    );
    let c = one("Alice <alice@example.com>");
    assert_eq!(
        (c.name.as_str(), c.address.as_str()),
        ("Alice", "alice@example.com")
    );
    let c = one("\"Smith, John\" <j@example.com>");
    assert_eq!(c.name, "Smith, John");
    let c = one("<bare@example.com>");
    assert_eq!(
        (c.name.as_str(), c.address.as_str()),
        ("", "bare@example.com")
    );
}

#[quench_test::test]
fn lists_split_on_commas_outside_quotes() {
    let list = parse_list("\"Doe, Jane\" <jane@x.org>, bob@y.org, Carol <carol@z.org>");
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].name, "Doe, Jane");
    assert_eq!(list[1].address, "bob@y.org");
    assert_eq!(list[2].name, "Carol");
}

#[quench_test::test]
fn encoded_word_names_are_decoded() {
    let c = one("=?UTF-8?B?SmFuIFpha8WCYWQ=?= <jan@example.com>");
    assert_eq!(c.name, "Jan Zakład");
}

#[quench_test::test]
fn comments_provide_a_name_or_are_ignored() {
    let c = one("alice@example.com (Alice A)");
    assert_eq!(
        (c.name.as_str(), c.address.as_str()),
        ("Alice A", "alice@example.com")
    );
    let c = one("Alice (work) <alice@example.com>");
    assert_eq!(c.name, "Alice");
}

#[quench_test::test]
fn groups_contribute_their_members_and_empty_groups_nothing() {
    let list = parse_list("Team: a@x.org, b@x.org;");
    assert_eq!(
        list.iter().map(|c| c.address.as_str()).collect::<Vec<_>>(),
        ["a@x.org", "b@x.org"]
    );
    assert!(parse_list("undisclosed-recipients:;").is_empty());
}

#[quench_test::test]
fn junk_is_skipped_not_fatal() {
    assert!(parse_list("").is_empty());
    assert!(parse_list("not an address").is_empty());
    assert!(parse_list("<broken").is_empty());
    assert!(parse_list("a@b <").is_empty());
    let list = parse_list("junk, ok@x.org, <>");
    assert_eq!(list.len(), 1);
}

#[quench_test::test]
fn quoted_names_unescape_and_lose_control_characters() {
    let c = one("\"a \\\"b\\\" c\" <x@y.org>");
    assert_eq!(c.name, "a \"b\" c");
    let c = one("\"line\u{7}bell\" <x@y.org>");
    assert_eq!(c.name, "linebell");
}

#[quench_test::test]
fn display_formats_name_and_address() {
    assert_eq!(
        Contact {
            name: "A".into(),
            address: "a@b.c".into()
        }
        .display(),
        "A <a@b.c>"
    );
    assert_eq!(
        Contact {
            name: String::new(),
            address: "a@b.c".into()
        }
        .display(),
        "a@b.c"
    );
}

#[quench_test::test]
fn valid_address_is_permissive_but_not_unsafe() {
    for ok in ["a@b.c", "a+b@c.d", "a@localhost"] {
        assert!(valid_address(ok), "{ok}");
    }
    for bad in [
        "",
        "a",
        "@b",
        "a@",
        "a b@c.d",
        "a@b c",
        "a@b\n.c",
        "a<@b.c",
        "a@b.c,d@e.f",
        "a\"@b.c",
    ] {
        assert!(!valid_address(bad), "{bad:?}");
    }
    assert!(!valid_address(&format!("{}@x.org", "a".repeat(400))));
}

#[quench_test::test]
fn a_very_long_list_is_capped() {
    let text = (0..800)
        .map(|i| format!("u{i}@x.org"))
        .collect::<Vec<_>>()
        .join(", ");
    assert_eq!(parse_list(&text).len(), 500);
}

#[quench_test::test]
fn source_routes_keep_the_final_address() {
    assert_eq!(one("<@a.org,@b.org:user@c.org>").address, "user@c.org");
}
