use crate::support::{Scratch, long_page, pager, press, rows, settings, typed};
use loupe::pager::{Content, Doc, Effect, Item, Key, Mark, Mode, Pager, Signal};

fn heading_line(pager: &Pager, slug: &str) -> usize {
    pager.rendered().heading_line(slug).unwrap()
}

#[quench_test::test]
fn scrolling_stays_within_the_page() {
    let mut p = pager(&long_page(), 60, 20);
    let total = p.rendered().lines.len();
    let max_top = total - 19;
    press(&mut p, &[Key::Char('k'), Key::Up]);
    assert_eq!(p.top(), 0);
    press(&mut p, &[Key::Char('j'), Key::Down]);
    assert_eq!(p.top(), 2);
    press(&mut p, &[Key::Char(' ')]);
    assert_eq!(p.top(), 2 + 19);
    press(&mut p, &[Key::Char('b')]);
    assert_eq!(p.top(), 2);
    press(&mut p, &[Key::Char('G')]);
    assert_eq!(p.top(), max_top);
    press(&mut p, &[Key::Char('j'), Key::PageDown, Key::End]);
    assert_eq!(p.top(), max_top);
    assert_eq!(rows(&p).len(), 19);
    assert_eq!(
        rows(&p).last().unwrap(),
        &p.rendered().lines[total - 1].text().trim_end().to_string()
    );
    press(&mut p, &[Key::Char('g')]);
    assert_eq!(p.top(), 0);
    assert!(
        p.screen().status.starts_with("page.md  1-19/"),
        "{}",
        p.screen().status
    );
}

#[quench_test::test]
fn a_short_page_does_not_scroll() {
    let mut p = pager("# One\n\nline\n", 40, 10);
    press(&mut p, &[Key::Char('G'), Key::Char(' ')]);
    assert_eq!(p.top(), 0);
    let screen = p.screen();
    assert_eq!(screen.rows.len(), 9);
    assert_eq!(screen.rows[8].text(), "", "past the end: empty rows");
    assert!(screen.status.ends_with("1-4/4 100%"), "{}", screen.status);
}

#[quench_test::test]
fn search_finds_highlights_and_cycles() {
    let mut p = pager(&long_page(), 60, 20);
    press(&mut p, &[Key::Char('/')]);
    press(&mut p, &typed("Needle"));
    assert_eq!(p.screen().status, "/Needle");
    // Upper case: case-sensitive, so only "Another needle" does not match... none match `Needle`.
    press(&mut p, &[Key::Enter]);
    assert!(p.screen().status.contains("pattern not found: Needle"));
    press(&mut p, &[Key::Char('/')]);
    press(&mut p, &typed("needle"));
    press(&mut p, &[Key::Enter]);
    let first = p.top();
    let screen = p.screen();
    let current: Vec<String> = screen
        .rows
        .iter()
        .flat_map(|r| r.pieces.iter())
        .filter(|piece| piece.mark == Mark::Current)
        .map(|piece| piece.text.clone())
        .collect();
    assert_eq!(current, ["needle"]);
    assert!(rows(&p).iter().any(|r| r.contains("The needle is here")));
    press(&mut p, &[Key::Char('n')]);
    assert!(p.top() > first);
    assert!(rows(&p).iter().any(|r| r.contains("Another needle")));
    press(&mut p, &[Key::Char('n')]);
    assert!(p.screen().status.contains("search wrapped to the top"));
    assert!(rows(&p).iter().any(|r| r.contains("The needle is here")));
    press(&mut p, &[Key::Char('N')]);
    assert!(p.screen().status.contains("search wrapped to the bottom"));
    assert!(rows(&p).iter().any(|r| r.contains("Another needle")));
}

#[quench_test::test]
fn search_input_can_be_cancelled_and_edited() {
    let mut p = pager(&long_page(), 60, 20);
    press(
        &mut p,
        &[
            Key::Char('/'),
            Key::Char('x'),
            Key::Backspace,
            Key::Char('y'),
        ],
    );
    assert_eq!(p.mode(), &Mode::Search("y".to_string()));
    press(&mut p, &[Key::Esc]);
    assert_eq!(p.mode(), &Mode::Normal);
    press(&mut p, &[Key::Char('n')]);
    assert!(p.screen().status.contains("no search yet"));
}

#[quench_test::test]
fn tab_focuses_links_and_enter_follows_an_anchor_with_history() {
    let mut p = pager(&long_page(), 60, 20);
    press(&mut p, &[Key::Tab]);
    let focused = p.focused().unwrap();
    assert_eq!(p.rendered().links[focused.link].destination, "#second");
    let marked: String = p.screen().rows[focused.line]
        .pieces
        .iter()
        .filter(|piece| piece.mark == Mark::Focus)
        .map(|piece| piece.text.as_str())
        .collect();
    assert_eq!(marked, "the second section");
    press(&mut p, &[Key::Enter]);
    assert_eq!(p.top(), heading_line(&p, "second"));
    assert_eq!(rows(&p)[0], "Second");
    assert_eq!(p.history_len(), 1);
    press(&mut p, &[Key::Backspace]);
    assert_eq!(p.top(), 0);
    assert_eq!(p.history_len(), 0);
    press(&mut p, &[Key::Char('h')]);
    assert!(p.screen().status.contains("nothing to go back to"));
}

#[quench_test::test]
fn shift_tab_goes_back_and_focus_wraps() {
    let mut p = pager(&long_page(), 60, 20);
    press(&mut p, &[Key::Tab, Key::Tab]);
    let second = p.focused().unwrap();
    assert_eq!(
        p.rendered().links[second.link].destination,
        "https://example.com"
    );
    press(&mut p, &[Key::BackTab]);
    assert_eq!(
        p.rendered().links[p.focused().unwrap().link].destination,
        "#second"
    );
}

#[quench_test::test]
fn an_outside_url_asks_before_opening() {
    let mut p = pager(&long_page(), 60, 20);
    press(&mut p, &[Key::Tab, Key::Tab, Key::Enter]);
    assert_eq!(p.mode(), &Mode::Confirm("https://example.com".to_string()));
    assert_eq!(p.screen().status, "open https://example.com ? (y/n)");
    assert_eq!(
        p.key(Key::Char('y')),
        Some(Effect::Open("https://example.com".to_string()))
    );
    assert_eq!(p.mode(), &Mode::Normal);
    press(&mut p, &[Key::Enter]);
    assert_eq!(p.key(Key::Char('n')), None);
    assert!(p.screen().status.contains("not opened"));
}

#[quench_test::test]
fn a_footnote_reference_jumps_to_its_definition_and_back() {
    let mut p = pager(&long_page(), 60, 20);
    // The third link of the page is the footnote's reference.
    press(&mut p, &[Key::Tab, Key::Tab, Key::Tab]);
    let spot = p.focused().unwrap();
    assert_eq!(p.rendered().links[spot.link].destination, "#fn-n");
    press(&mut p, &[Key::Enter]);
    let definition = p.rendered().footnotes[0].definition_line;
    assert_eq!(p.top(), definition.min(p.rendered().lines.len() - 19));
    assert!(rows(&p).iter().any(|r| r.contains("The footnote's text.")));
    press(&mut p, &[Key::Backspace]);
    assert_eq!(p.top(), 0);
}

#[quench_test::test]
fn links_between_files_open_them_and_back_returns() {
    let dir = Scratch::new("files");
    let other = dir.write(
        "other.md",
        &format!(
            "---\ntitle: Other\n---\n# Other page\n\n{}## Part\n\ntext\n",
            "filler\n\n".repeat(10)
        ),
    );
    let first = dir.write(
        "first.md",
        "# First\n\n[other](other.md) [part](other.md#part) [gone](gone.md) [file](data.txt)\n",
    );
    dir.write("data.txt", "data");
    let markdown = std::fs::read_to_string(&first).unwrap();
    let mut p = Pager::file(
        "first.md",
        Some(first.clone()),
        markdown,
        settings(),
        None,
        (60, 10),
    );
    press(&mut p, &[Key::Tab, Key::Enter]);
    assert!(matches!(p.doc(), Doc::File { path: Some(path), .. } if *path == other));
    assert_eq!(rows(&p)[0], "Other page");
    assert!(!rows(&p).iter().any(|r| r.contains("title:")));
    assert!(p.screen().status.contains("other.md"));
    press(&mut p, &[Key::Backspace]);
    assert!(matches!(p.doc(), Doc::File { path: Some(path), .. } if *path == first));
    press(&mut p, &[Key::Tab, Key::Tab, Key::Enter]);
    assert!(
        p.top() > 0 && rows(&p).iter().any(|r| r == "Part"),
        "{:?}",
        rows(&p)
    );
    press(
        &mut p,
        &[Key::Backspace, Key::Tab, Key::Tab, Key::Tab, Key::Enter],
    );
    assert!(
        p.screen().status.contains("missing: `gone.md`"),
        "{}",
        p.screen().status
    );
    press(&mut p, &[Key::Tab, Key::Enter]);
    assert!(matches!(p.mode(), Mode::Confirm(t) if t.ends_with("data.txt")));
}

#[quench_test::test]
fn a_missing_anchor_is_a_message() {
    let mut p = pager("[nowhere](#none)\n", 40, 10);
    press(&mut p, &[Key::Tab, Key::Enter]);
    assert!(p.screen().status.contains("no heading #none"));
}

#[quench_test::test]
fn the_table_of_contents_jumps_to_a_heading() {
    let mut p = pager(&long_page(), 60, 20);
    press(&mut p, &[Key::Char('t')]);
    assert_eq!(p.mode(), &Mode::Toc(0));
    let text = rows(&p);
    assert_eq!(text[0], "Contents");
    assert_eq!(
        &text[1..5],
        ["  Title", "    First", "    Second", "    Third"]
    );
    assert_eq!(p.screen().rows[1].pieces[0].mark, Mark::Selected);
    press(&mut p, &[Key::Char('j'), Key::Char('j'), Key::Enter]);
    assert_eq!(p.mode(), &Mode::Normal);
    assert_eq!(p.top(), heading_line(&p, "second"));
    press(&mut p, &[Key::Backspace]);
    assert_eq!(p.top(), 0);
    press(&mut p, &[Key::Char('t'), Key::Esc]);
    assert_eq!(p.mode(), &Mode::Normal);
}

#[quench_test::test]
fn the_footnotes_list_jumps_to_one() {
    let mut p = pager(&long_page(), 60, 20);
    press(&mut p, &[Key::Char('f')]);
    assert_eq!(rows(&p)[..2], ["Footnotes", "  1. The footnote's text."]);
    press(&mut p, &[Key::Enter]);
    assert!(rows(&p).iter().any(|r| r.contains("The footnote's text.")));
    press(&mut p, &[Key::Backspace]);
    assert_eq!(p.top(), 0);
    let mut none = pager("# x\n", 40, 10);
    press(&mut none, &[Key::Char('f')]);
    assert!(none.screen().status.contains("no footnotes"));
}

#[quench_test::test]
fn resizing_keeps_the_place() {
    let mut p = pager(&long_page(), 60, 20);
    press(
        &mut p,
        &[
            Key::Char('t'),
            Key::Char('j'),
            Key::Char('j'),
            Key::Enter,
            Key::Char('j'),
        ],
    );
    assert_eq!(rows(&p)[0], "─".repeat(60));
    let generation = p.generation();
    p.resize(30, 12);
    assert!(p.generation() > generation);
    assert_eq!(p.width(), 30);
    assert_eq!(p.top(), heading_line(&p, "second") + 1);
    assert_eq!(rows(&p)[0], "─".repeat(30));
    assert_eq!(rows(&p).len(), 11);
    // Only the height: no new layout.
    let generation = p.generation();
    p.resize(30, 30);
    assert_eq!(p.generation(), generation);
    // A fixed width (--width) is kept whatever the terminal.
    let mut fixed = Pager::file("x", None, long_page(), settings(), Some(50), (100, 20));
    assert_eq!(fixed.width(), 50);
    fixed.resize(120, 20);
    assert_eq!(fixed.width(), 50);
}

#[quench_test::test]
fn m_switches_a_formula_to_its_source_and_back() {
    let mut p = pager("Some $x^2$ here.\n", 40, 10);
    assert_eq!(rows(&p)[0], "Some x² here.");
    press(&mut p, &[Key::Char('m')]);
    assert_eq!(rows(&p)[0], "Some $x^2$ here.");
    assert!(p.screen().status.contains("formula shown as TeX source"));
    assert!(
        p.candidates().is_empty(),
        "a formula shown as source is not drawn as an image"
    );
    press(&mut p, &[Key::Char('m')]);
    assert_eq!(rows(&p)[0], "Some x² here.");
    assert_eq!(p.candidates().len(), 1);
    let mut none = pager("no math\n", 40, 10);
    press(&mut none, &[Key::Char('m')]);
    assert!(none.screen().status.contains("no formula on screen"));
}

#[quench_test::test]
fn y_copies_the_code_block_on_screen() {
    let mut p = pager(&long_page(), 60, 20);
    press(&mut p, &[Key::Char('y')]);
    assert!(p.screen().status.contains("no code block on screen"));
    press(
        &mut p,
        &[Key::Char('t'), Key::Char('j'), Key::Char('j'), Key::Enter],
    );
    assert_eq!(
        p.key(Key::Char('y')),
        Some(Effect::Copy("echo one\necho two".to_string()))
    );
    assert!(p.screen().status.contains("copied 2 lines"));
}

#[quench_test::test]
fn long_code_lines_are_copied_whole() {
    let long = format!("echo {}", "x".repeat(70));
    let mut p = pager(&format!("```\n{long}\n```\n"), 30, 10);
    assert_eq!(p.key(Key::Char('y')), Some(Effect::Copy(long)));
}

#[quench_test::test]
fn quit_suspend_and_not_a_book() {
    let mut p = pager("x\n", 40, 10);
    assert_eq!(p.key(Key::Char('q')), Some(Effect::Quit));
    assert_eq!(p.key(Key::Ctrl('c')), Some(Effect::Quit));
    assert_eq!(p.key(Key::Ctrl('z')), Some(Effect::Suspend));
    press(&mut p, &[Key::Char('o')]);
    assert!(p.screen().status.contains("not a book"));
    press(&mut p, &[Key::Char(']')]);
    assert!(p.screen().status.contains("not in a book"));
}

#[quench_test::test]
fn formula_candidates_and_blanking() {
    let mut p = pager("Some $x^2$ here.\n\n$$\n\\frac{a}{b}\n$$\n\nend\n", 40, 10);
    let candidates = p.candidates();
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        (candidates[0].row, candidates[0].column, candidates[0].width),
        (0, 5, 2)
    );
    assert!(matches!(
        &candidates[1].content,
        Content::Tex { display: true, .. }
    ));
    assert_eq!(
        (
            candidates[1].column,
            candidates[1].width,
            candidates[1].height
        ),
        (0, 40, 3)
    );
    let all = candidates.iter().map(|c| c.item).collect();
    let blanked = p.screen_with(&all);
    let text: Vec<String> = blanked
        .rows
        .iter()
        .map(|r| r.text().trim_end().to_string())
        .collect();
    assert_eq!(text[..7], ["Some    here.", "", "", "", "", "", "end"]);
    // Scrolled so the display formula is cut by the top: still drawn, its first row cropped.
    p.resize(40, 4);
    press(&mut p, &[Key::Char('j'), Key::Char('j'), Key::Char('j')]);
    assert_eq!(p.top(), 3);
    let cut = p.candidates();
    assert_eq!(cut.len(), 1);
    assert_eq!(
        (
            cut[0].item,
            cut[0].row,
            cut[0].skip,
            cut[0].shown,
            cut[0].height
        ),
        (Item::Formula(1), 0, 1, 2, 3)
    );
    // Cut by the bottom.
    press(&mut p, &[Key::Char('g')]);
    let cut = p.candidates();
    assert_eq!((cut[1].row, cut[1].skip, cut[1].shown), (2, 0, 1));
    let blanked = p.screen_with(&cut.iter().map(|c| c.item).collect());
    assert_eq!(blanked.rows[2].text().trim_end(), "");
    // An overlay hides the page: no candidates.
    let mut q = pager("# T\n\nSome $x^2$ here.\n", 40, 10);
    assert_eq!(q.candidates().len(), 1);
    press(&mut q, &[Key::Char('t')]);
    assert_eq!(q.candidates(), []);
}

#[quench_test::test]
fn signals_arrive_like_keys() {
    let mut p = pager(&long_page(), 60, 20);
    assert_eq!(p.signal(Signal::Interrupt), Some(Effect::Exit(130)));
    assert_eq!(p.signal(Signal::Terminate), Some(Effect::Exit(143)));
    assert_eq!(p.signal(Signal::Stop), Some(Effect::Suspend));
    assert_eq!(p.signal(Signal::Continue), Some(Effect::Redraw));
    press(
        &mut p,
        &[Key::Char('t'), Key::Char('j'), Key::Char('j'), Key::Enter],
    );
    let generation = p.generation();
    assert_eq!(p.signal(Signal::Resize(30, 12)), None);
    assert!(p.generation() > generation);
    assert_eq!((p.width(), rows(&p).len()), (30, 11));
    assert_eq!(rows(&p)[0], "Second", "the place is kept");
}

#[quench_test::test]
fn an_anchor_near_the_end_becomes_the_top_line() {
    let mut p = pager(&long_page(), 60, 20);
    let third = p.rendered().heading_line("third").unwrap();
    let max_top = p.rendered().lines.len() - 19;
    assert!(third > max_top, "the heading is within the last screen");
    press(&mut p, &[Key::Char('t'), Key::End, Key::Enter]);
    assert_eq!(p.top(), third);
    assert_eq!(rows(&p)[0], "Third");
    assert_eq!(rows(&p).last().unwrap(), "", "padded below the page");
    // Scrolling down stays; up goes back into the page; G goes to the real bottom.
    press(&mut p, &[Key::Char('j')]);
    assert_eq!(p.top(), third);
    press(&mut p, &[Key::Char('k')]);
    assert_eq!(p.top(), third - 1);
    press(&mut p, &[Key::Backspace]);
    assert_eq!(p.top(), 0);
    let mut q = pager(&long_page(), 60, 20);
    press(&mut q, &[Key::Char('G')]);
    assert_eq!(q.top(), max_top);
}

#[quench_test::test]
fn y_finds_code_blocks_in_quotes_and_lists_but_not_inline_code() {
    let page = "> quote\n>\n> ```sh\n> echo one\n>\n> echo two\n> ```\n\n- item `inline`\n\n  ```\n  in list\n  ```\n";
    let mut p = pager(page, 30, 5);
    assert_eq!(
        p.key(Key::Char('y')),
        Some(Effect::Copy("echo one\n\necho two".to_string()))
    );
    press(&mut p, &[Key::Char('j'); 9]);
    assert_eq!(p.top(), 9);
    assert_eq!(
        p.key(Key::Char('y')),
        Some(Effect::Copy("in list".to_string()))
    );
    let mut table = pager("| a |\n|---|\n| `x` |\n\nand `y`\n", 30, 20);
    assert_eq!(table.key(Key::Char('y')), None);
    assert!(table.screen().status.contains("no code block on screen"));
}
