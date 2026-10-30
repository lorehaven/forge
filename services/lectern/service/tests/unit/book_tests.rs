use crate::support::pack;
use lectern_service::book::{Book, Found, clean, content_type, rewrite_root_urls};

fn rewritten(path: &str, text: &str) -> String {
    String::from_utf8(rewrite_root_urls(text.as_bytes(), path, "/lectern/forge").to_vec()).unwrap()
}

#[quench_test::test]
fn html_links_from_the_root_move_below_the_prefix() {
    assert_eq!(
        rewritten(
            "index.html",
            r#"<a href="/cli/x.html">a</a><img src='/a.png'><a href="/">h</a>"#
        ),
        r#"<a href="/lectern/forge/cli/x.html">a</a><img src='/lectern/forge/a.png'><a href="/lectern/forge/">h</a>"#
    );
}

#[quench_test::test]
fn other_links_stay_as_they_are() {
    let html = r##"<a href="#top">t</a><a href="x.html">r</a><a href="//cdn.example/x.js">c</a><a href="https://e.org/">e</a>"##;
    assert_eq!(rewritten("index.html", html), html);
}

#[quench_test::test]
fn prose_that_quotes_a_link_is_not_a_link() {
    assert_eq!(
        rewritten(
            "a.html",
            r#"<p>write <code>href="/ui/home"</code> or <code>&lt;a href="/x"&gt;</code></p><a href="/y">y</a>"#
        ),
        r#"<p>write <code>href="/ui/home"</code> or <code>&lt;a href="/x"&gt;</code></p><a href="/lectern/forge/y">y</a>"#
    );
}

#[quench_test::test]
fn css_scripts_and_the_search_index_are_rewritten_too() {
    assert_eq!(
        rewritten(
            "a.css",
            "a{background:url(/img/x.png)} b{background:url('/y.png')}"
        ),
        "a{background:url(/lectern/forge/img/x.png)} b{background:url('/lectern/forge/y.png')}"
    );
    assert_eq!(
        rewritten(
            "assets/js/just-the-docs.js",
            "open('GET', '/assets/js/search-data.json')"
        ),
        "open('GET', '/lectern/forge/assets/js/search-data.json')"
    );
    assert_eq!(
        rewritten(
            "assets/js/search-data.json",
            r#"{"0": {"url": "/cli/a.html#b"}}"#
        ),
        r#"{"0": {"url": "/lectern/forge/cli/a.html#b"}}"#
    );
    assert_eq!(rewritten("logo.png", "href=\"/x\""), "href=\"/x\"");
}

#[quench_test::test]
fn paths_resolve_to_files_directories_or_nothing() {
    let book = Book::from_files(
        "b",
        "1",
        &[
            ("index.html", "root"),
            ("cli/index.html", "cli"),
            ("cli/a.html", "a"),
        ],
    );
    assert!(matches!(book.find(""), Found::File(f) if f.body.as_ref() == b"root"));
    assert!(matches!(book.find("cli/"), Found::File(f) if f.body.as_ref() == b"cli"));
    assert!(matches!(book.find("cli"), Found::Slash));
    assert!(matches!(book.find("/cli/a.html"), Found::File(_)));
    assert!(matches!(book.find("nope.html"), Found::Missing));
    assert!(matches!(book.find("cli/nope/"), Found::Missing));
}

#[quench_test::test]
fn archive_paths_cannot_climb_out() {
    assert_eq!(clean("./a//b.html").as_deref(), Some("a/b.html"));
    assert_eq!(clean("../etc/passwd"), None);
    assert_eq!(clean("a/../../b"), None);
    assert_eq!(clean("./"), None);
}

#[quench_test::test]
fn content_types_follow_the_extension() {
    assert_eq!(content_type("a/b.HTML"), "text/html; charset=utf-8");
    assert_eq!(content_type("x.svg"), "image/svg+xml");
    assert_eq!(content_type("x.woff2"), "font/woff2");
    assert_eq!(content_type("x.unknown"), "application/octet-stream");
    assert_eq!(content_type("noextension"), "application/octet-stream");
}

#[quench_test::test]
fn a_release_is_unpacked_and_its_links_rewritten() {
    let (_, archive, manifest) = pack(
        "forge",
        "1",
        &[
            ("index.html", r#"<a href="/guides/">g</a>"#),
            ("guides/index.html", "g"),
        ],
        None,
    );
    let latest: quench_serde::Value = quench_serde::json::from_str(&manifest).unwrap();
    let book = Book::from_release("forge", &latest, &archive, "/lectern/forge").unwrap();
    assert_eq!(
        (
            book.title.as_str(),
            book.version.as_str(),
            book.file_count()
        ),
        ("FORGE", "1", 2)
    );
    let Found::File(index) = book.find("") else {
        panic!("no index")
    };
    assert_eq!(
        index.body.as_ref(),
        br#"<a href="/lectern/forge/guides/">g</a>"#
    );
}

#[quench_test::test]
fn a_book_built_with_a_baseurl_is_served_as_it_is() {
    let (_, archive, manifest) = pack(
        "forge",
        "1",
        &[("index.html", r#"<a href="/forge/x">x</a>"#)],
        Some("/forge"),
    );
    let latest: quench_serde::Value = quench_serde::json::from_str(&manifest).unwrap();
    let book = Book::from_release("forge", &latest, &archive, "/elsewhere/forge").unwrap();
    let Found::File(index) = book.find("") else {
        panic!("no index")
    };
    assert_eq!(index.body.as_ref(), br#"<a href="/forge/x">x</a>"#);
}

#[quench_test::test]
fn a_damaged_or_empty_release_is_refused() {
    let (_, mut archive, manifest) = pack("forge", "1", &[("index.html", "x")], None);
    let latest: quench_serde::Value = quench_serde::json::from_str(&manifest).unwrap();
    archive[20] ^= 0xff;
    let error = Book::from_release("forge", &latest, &archive, "/forge").unwrap_err();
    assert!(error.contains("checksum"), "{error}");

    let (_, empty, manifest) = pack("forge", "1", &[], None);
    let latest: quench_serde::Value = quench_serde::json::from_str(&manifest).unwrap();
    assert!(Book::from_release("forge", &latest, &empty, "/forge").is_err());
}
