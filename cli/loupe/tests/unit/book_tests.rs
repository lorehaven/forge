use crate::support::{Scratch, press, rows, settings, typed};
use docs_book::Book;
use loupe::pager::{Doc, Key, Mode, Pager};

/// A small book: Home, Guide (with Setup and Usage under it), Reference.
fn book() -> Scratch {
    let dir = Scratch::new("book");
    dir.write("_config.yml", "title: Manual\n");
    dir.write(
        "index.md",
        "---\ntitle: Home\nnav_order: 1\n---\n# Home\n\nStart with the [guide](guide/index.md).\n",
    );
    dir.write(
        "guide/index.md",
        "---\ntitle: Guide\nnav_order: 2\nhas_children: true\n---\n# Guide\n\nRead [setup](setup.md#install).\n",
    );
    let intro = "intro\n\n".repeat(15);
    dir.write(
        "guide/setup.md",
        &format!("---\ntitle: Setup\nparent: Guide\nnav_order: 1\n---\n# Setup\n\n{intro}## Install\n\nsteps\n\n[back home](../index.md) [nothing](nothing.md) [a file](data.csv)\n"),
    );
    dir.write(
        "guide/usage.md",
        "---\ntitle: Usage\nparent: Guide\nnav_order: 2\n---\n# Usage\n",
    );
    dir.write(
        "reference.md",
        "---\ntitle: Reference\nnav_order: 3\n---\n# Reference\n",
    );
    dir.write("guide/data.csv", "a,b\n");
    dir
}

fn open(dir: &Scratch, route: &str) -> Pager {
    let book = Book::open(dir.path()).unwrap();
    let page = book.page(route).unwrap();
    Pager::book(book, page, settings(), None, (50, 12))
}

fn page_of(pager: &Pager) -> usize {
    match pager.doc() {
        Doc::Book(page) => *page,
        Doc::File { .. } => panic!("not a book page"),
    }
}

#[quench_test::test]
fn the_status_line_shows_the_breadcrumb() {
    let dir = book();
    let p = open(&dir, "guide/setup");
    assert!(
        p.screen().status.starts_with("Manual › Guide › Setup  1-"),
        "{}",
        p.screen().status
    );
    assert_eq!(rows(&p)[0], "Setup");
}

#[quench_test::test]
fn the_navigation_lists_the_tree_and_filters() {
    let dir = book();
    let mut p = open(&dir, "");
    press(&mut p, &[Key::Char('o')]);
    let text = rows(&p);
    assert_eq!(text[0], "Navigation (type to filter)");
    assert_eq!(
        text[1..6],
        [
            "  Home ◂",
            "  Guide",
            "    Setup",
            "    Usage",
            "  Reference"
        ]
    );
    press(&mut p, &typed("us"));
    assert_eq!(
        p.mode(),
        &Mode::Nav {
            filter: "us".to_string(),
            selected: 0
        }
    );
    assert_eq!(rows(&p)[..2], ["Navigation: us", "    Usage"]);
    press(&mut p, &[Key::Enter]);
    assert_eq!(rows(&p)[0], "Usage");
    assert!(p.screen().status.starts_with("Manual › Guide › Usage"));
    press(&mut p, &[Key::Backspace]);
    assert_eq!(rows(&p)[0], "Home");
    press(&mut p, &[Key::Char('o')]);
    press(&mut p, &typed("zzz"));
    assert_eq!(rows(&p)[1], "  no page matches");
    press(
        &mut p,
        &[
            Key::Backspace,
            Key::Backspace,
            Key::Backspace,
            Key::Down,
            Key::Down,
            Key::Enter,
        ],
    );
    assert_eq!(rows(&p)[0], "Setup");
    press(&mut p, &[Key::Char('o'), Key::Esc]);
    assert_eq!(p.mode(), &Mode::Normal);
}

#[quench_test::test]
fn brackets_go_to_the_previous_and_next_page() {
    let dir = book();
    let mut p = open(&dir, "");
    press(&mut p, &[Key::Char('[')]);
    assert!(p.screen().status.contains("this is the first page"));
    let order = ["Guide", "Setup", "Usage", "Reference"];
    for title in order {
        press(&mut p, &[Key::Char(']')]);
        assert_eq!(rows(&p)[0], title);
    }
    press(&mut p, &[Key::Char(']')]);
    assert!(p.screen().status.contains("this is the last page"));
    press(&mut p, &[Key::Char('[')]);
    assert_eq!(rows(&p)[0], "Usage");
    press(&mut p, &[Key::Backspace, Key::Backspace]);
    assert_eq!(rows(&p)[0], "Usage");
}

#[quench_test::test]
fn links_open_pages_of_the_book_at_their_anchor() {
    let dir = book();
    let mut p = open(&dir, "");
    let home = page_of(&p);
    press(&mut p, &[Key::Tab, Key::Enter]);
    assert_eq!(rows(&p)[0], "Guide");
    press(&mut p, &[Key::Tab, Key::Enter]);
    assert!(
        p.top() > 0 && rows(&p).iter().any(|r| r == "Install"),
        "{:?}",
        rows(&p)
    );
    assert!(p.screen().status.starts_with("Manual › Guide › Setup"));
    press(&mut p, &[Key::Tab, Key::Enter]);
    assert_eq!(page_of(&p), home);
    assert_eq!(p.history_len(), 3);
    press(&mut p, &[Key::Backspace]);
    assert!(
        p.top() > 0 && rows(&p).iter().any(|r| r == "Install"),
        "{:?}",
        rows(&p)
    );
    press(&mut p, &[Key::Backspace]);
    assert_eq!(rows(&p)[0], "Guide");
    press(&mut p, &[Key::Backspace]);
    assert_eq!(page_of(&p), home);
}

#[quench_test::test]
fn missing_pages_and_outside_files() {
    let dir = book();
    let mut p = open(&dir, "guide/setup");
    press(&mut p, &[Key::Tab, Key::Tab, Key::Enter]);
    assert!(
        p.screen()
            .status
            .contains("no page in the book for `nothing.md`"),
        "{}",
        p.screen().status
    );
    press(&mut p, &[Key::Tab, Key::Enter]);
    match p.mode() {
        Mode::Confirm(target) => assert!(target.ends_with("guide/data.csv"), "{target}"),
        other => panic!("expected a confirmation, got {other:?}"),
    }
}
