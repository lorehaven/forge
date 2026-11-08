use crate::support::{book, page};
use docs_book::Book;

fn titles(book: &Book) -> Vec<String> {
    book.nav()
        .walk()
        .iter()
        .map(|&(i, depth)| format!("{}{}", "  ".repeat(depth), book.pages()[i].label()))
        .collect()
}

#[quench_test::test]
fn the_tree_follows_parents_and_orders_siblings_like_just_the_docs() {
    let dir = book(&[
        ("index.md", &page("title: Home\nnav_order: 1", "")),
        (
            "guides/index.md",
            &page("title: Guides\nnav_order: 2\nhas_children: true", ""),
        ),
        ("guides/b.md", &page("title: beta\nparent: Guides", "")),
        ("guides/a.md", &page("title: Alpha\nparent: Guides", "")),
        (
            "guides/first.md",
            &page("title: Zeta first\nparent: Guides\nnav_order: 1", ""),
        ),
        ("hidden.md", &page("title: Hidden\nnav_exclude: true", "")),
        ("untitled.md", &page("nav_order: 9", "")),
        ("orphan.md", &page("title: Orphan\nparent: Nobody", "")),
    ]);
    let book = Book::open(dir.path()).unwrap();
    assert_eq!(
        titles(&book),
        ["Home", "Guides", "  Zeta first", "  Alpha", "  beta"],
        "nav_order first, then titles case aside; hidden, untitled and orphaned pages left out"
    );
}

#[quench_test::test]
fn grand_parent_tells_apart_two_parents_with_the_same_title() {
    let dir = book(&[
        ("a/index.md", &page("title: A\nhas_children: true", "")),
        ("b/index.md", &page("title: B\nhas_children: true", "")),
        (
            "a/api.md",
            &page("title: API\nparent: A\nhas_children: true", ""),
        ),
        (
            "b/api.md",
            &page("title: API\nparent: B\nhas_children: true", ""),
        ),
        (
            "a/api/x.md",
            &page("title: X\nparent: API\ngrand_parent: A", ""),
        ),
        (
            "b/api/y.md",
            &page("title: Y\nparent: API\ngrand_parent: B", ""),
        ),
    ]);
    let book = Book::open(dir.path()).unwrap();
    assert_eq!(
        titles(&book),
        ["A", "  API", "    X", "B", "  API", "    Y"]
    );
    let y = book.page("b/api/y").unwrap();
    let trail: Vec<&str> = book
        .nav()
        .trail(y)
        .iter()
        .map(|&i| book.pages()[i].label())
        .collect();
    assert_eq!(trail, ["B", "API", "Y"]);
}

#[quench_test::test]
fn neighbours_are_the_pages_before_and_after_in_reading_order() {
    let dir = book(&[
        ("index.md", &page("title: Home\nnav_order: 1", "")),
        (
            "one.md",
            &page("title: One\nnav_order: 2\nhas_children: true", ""),
        ),
        ("one/a.md", &page("title: A\nparent: One", "")),
        ("two.md", &page("title: Two\nnav_order: 3", "")),
    ]);
    let book = Book::open(dir.path()).unwrap();
    let label = |i: Option<usize>| i.map(|i| book.pages()[i].label().to_string());
    let a = book.page("one/a").unwrap();
    let (before, after) = book.nav().neighbours(a);
    assert_eq!(
        (label(before), label(after)),
        (Some("One".into()), Some("Two".into()))
    );
    let home = book.page("").unwrap();
    assert_eq!(book.nav().neighbours(home).0, None);
}

#[quench_test::test]
fn problems_name_every_navigation_mistake() {
    let dir = book(&[
        ("a.md", &page("title: A\nnav_order: 1", "")),
        ("b.md", &page("title: B\nnav_order: 1", "")),
        ("c.md", &page("title: C\nparent: A", "")),
        ("d.md", &page("title: D\nparent: Nowhere", "")),
        ("e.md", &page("nav_order: 3", "[x](missing.md)")),
        ("plain.md", "no front matter\n"),
        (
            "f.md",
            &page("title: F\nnav_order: 4", "[r](_reports/x.md)"),
        ),
        ("_reports/x.md", "internal\n"),
    ]);
    let problems = Book::open(dir.path()).unwrap().problems().join("\n");
    for expected in [
        "plain.md: no front matter",
        "e.md: no `title`",
        "c.md: parent `A` exists but has no `has_children: true`",
        "d.md: parent `Nowhere` is not the title of any page",
        "nav_order 1 is used by 2 pages under the top level: a.md, b.md",
        "e.md: broken link `missing.md`",
        "f.md: link `_reports/x.md` leads to a file Jekyll leaves out of the site",
    ] {
        assert!(problems.contains(expected), "{expected}\n---\n{problems}");
    }
}
