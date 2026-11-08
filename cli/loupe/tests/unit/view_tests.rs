use crate::support::pager;
use loupe::view::frame;

#[quench_test::test]
fn the_frame_draws_rows_hyperlinks_and_the_status_line() {
    let p = pager("# T\n\nGo [there](https://example.com).\n", 30, 6);
    let out = frame(&p.screen(), &p.rendered().links, 30);
    assert!(
        out.starts_with("\x1b[1;1H\x1b[2K\x1b[1;35mT\x1b[0m"),
        "{out:?}"
    );
    assert!(
        out.contains("\x1b]8;;https://example.com\x1b\\\x1b[4;34mthere\x1b[0m\x1b]8;;\x1b\\"),
        "{out:?}"
    );
    assert!(
        out.contains("\x1b[6;1H\x1b[2K\x1b[7mpage.md  1-4/4 100%"),
        "{out:?}"
    );
    assert!(out.ends_with(&format!(
        "{}\x1b[0m",
        " ".repeat(30 - "page.md  1-4/4 100%".len())
    )));
}
