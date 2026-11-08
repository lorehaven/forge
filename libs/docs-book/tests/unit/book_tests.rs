use docs_book::Book;
use std::path::Path;

/// The forge book itself, when this runs inside the forge repository.
fn forge_docs() -> Option<Book> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    dir.is_dir().then(|| Book::open(&dir).unwrap())
}

#[quench_test::test]
fn the_forge_book_has_no_problems_and_every_page_is_reachable() {
    let Some(book) = forge_docs() else {
        return;
    };
    assert_eq!(book.problems(), Vec::<String>::new());
    let shown = book.nav().walk().len();
    let excluded = book.pages().iter().filter(|p| p.nav_exclude).count();
    assert_eq!(
        shown + excluded,
        book.pages().len(),
        "every page is in the navigation or excluded"
    );
}

#[quench_test::test]
fn every_link_in_the_forge_book_resolves() {
    let Some(book) = forge_docs() else {
        return;
    };
    let mut missing = Vec::new();
    for (i, page) in book.pages().iter().enumerate() {
        for target in docs_book::links(&page.body) {
            if let docs_book::Target::Missing(t) = book.resolve(i, &target) {
                missing.push(format!("{}: {t}", page.path));
            }
        }
    }
    assert_eq!(missing, Vec::<String>::new());
}

#[quench_test::test]
fn a_missing_directory_is_an_error() {
    let err = Book::open(Path::new("/nonexistent/book"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("does not exist"), "{err}");
}
