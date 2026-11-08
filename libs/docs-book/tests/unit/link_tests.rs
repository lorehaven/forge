use crate::support::{book, page};
use docs_book::{Book, Target, links};

fn site() -> (quench_core::tmp::TempDir, Book) {
    let dir = book(&[
        ("_config.yml", "title: Demo\nbaseurl: /docs\n"),
        ("index.md", &page("title: Home", "")),
        (
            "services/index.md",
            &page("title: Services\nhas_children: true", ""),
        ),
        (
            "services/gauge/index.md",
            &page("title: Gauge\nparent: Services\nhas_children: true", ""),
        ),
        (
            "services/gauge/metrics.md",
            &page("title: Metrics\nparent: Gauge", ""),
        ),
        (
            "services/gauge/configuration.md",
            &page("title: Configuration\nparent: Gauge", ""),
        ),
        (
            "crates/tex.md",
            &page("title: Tex\npermalink: /crates/quench-tex/", ""),
        ),
    ]);
    let book = Book::open(dir.path()).unwrap();
    (dir, book)
}

fn to(book: &Book, from: &str, target: &str) -> String {
    let from = book.page(from).unwrap();
    match book.resolve(from, target) {
        Target::Page { page, anchor } => {
            format!(
                "{}{}",
                book.pages()[page].path,
                anchor.map(|a| format!("#{a}")).unwrap_or_default()
            )
        }
        other => format!("{other:?}"),
    }
}

#[quench_test::test]
fn links_resolve_the_way_the_built_site_serves_them() {
    let (_dir, book) = site();
    let from = "services/gauge/metrics";
    for (target, page) in [
        ("configuration.md", "services/gauge/configuration.md"),
        ("configuration.html", "services/gauge/configuration.md"),
        ("configuration", "services/gauge/configuration.md"),
        ("Configuration.MD", "Missing(\"Configuration.MD\")"),
        (
            "configuration.md#targets",
            "services/gauge/configuration.md#targets",
        ),
        ("./index.md", "services/gauge/index.md"),
        ("../gauge/", "services/gauge/index.md"),
        ("..", "services/index.md"),
        ("../../index.html", "index.md"),
        ("/services/gauge/metrics.html", "services/gauge/metrics.md"),
        (
            "/docs/services/gauge/metrics.html",
            "services/gauge/metrics.md",
        ),
        ("/crates/quench-tex/", "crates/tex.md"),
        ("/crates/quench-tex", "crates/tex.md"),
        (
            "../../crates/quench-tex/index.html",
            "Missing(\"../../crates/quench-tex/index.html\")",
        ),
    ] {
        assert_eq!(to(&book, from, target), page, "{target}");
    }
}

#[quench_test::test]
fn anchors_outside_links_files_and_missing_pages_are_told_apart() {
    let (_dir, book) = site();
    let from = "services/gauge/metrics";
    assert_eq!(to(&book, from, "#alerts"), "Anchor(\"alerts\")");
    assert_eq!(
        to(&book, from, "https://example.com/x.md"),
        "Outside(\"https://example.com/x.md\")"
    );
    assert_eq!(to(&book, from, "mailto:a@b"), "Outside(\"mailto:a@b\")");
    assert_eq!(to(&book, from, "diagram.png"), "Outside(\"diagram.png\")");
    assert_eq!(to(&book, from, "nowhere.md"), "Missing(\"nowhere.md\")");
    assert_eq!(book.baseurl(), "/docs");
    assert_eq!(book.title(), Some("Demo"));
}

#[quench_test::test]
fn pages_are_found_by_route_path_or_permalink() {
    let (_dir, book) = site();
    let path = |route: &str| book.page(route).map(|i| book.pages()[i].path.clone());
    assert_eq!(
        path("services/gauge/metrics").as_deref(),
        Some("services/gauge/metrics.md")
    );
    assert_eq!(
        path("/services/gauge/").as_deref(),
        Some("services/gauge/index.md")
    );
    assert_eq!(
        path("services/gauge/metrics.md").as_deref(),
        Some("services/gauge/metrics.md")
    );
    assert_eq!(path("crates/quench-tex").as_deref(), Some("crates/tex.md"));
    assert_eq!(path("").as_deref(), Some("index.md"));
    assert_eq!(path("nope"), None);
}

#[quench_test::test]
fn links_in_code_are_not_links() {
    let body = "[a](one.md) `[b](two.md)`\n```\n[c](three.md)\n```\n[d](four.md \"title\")\n";
    assert_eq!(links(body), ["one.md", "four.md"]);
}
