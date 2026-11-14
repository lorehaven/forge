use forge_tui::{ColorLevel, Line, Rgb, Style, fill_screen};

const RED: Rgb = Rgb(240, 68, 56);
const INK: Rgb = Rgb(1, 2, 3);

#[quench_test::test]
fn width_and_plain_ignore_styling() {
    let line = Line::styled("ab", Style::new().fg(RED).bold()).push("cde", Style::new());
    assert_eq!(line.width(), 5);
    assert_eq!(line.plain(), "abcde");
}

#[quench_test::test]
fn truecolor_render_wraps_each_styled_span() {
    let line = Line::styled("hi", Style::new().fg(RED).bg(INK).bold()).push("!", Style::new());
    assert_eq!(
        line.render(ColorLevel::TrueColor),
        "\x1b[1;38;2;240;68;56;48;2;1;2;3mhi\x1b[0m!"
    );
}

#[quench_test::test]
fn ansi256_render_uses_palette_indexes() {
    let out = Line::styled("x", Style::new().fg(Rgb(255, 0, 0))).render(ColorLevel::Ansi256);
    assert_eq!(out, "\x1b[38;5;196mx\x1b[0m");
}

#[quench_test::test]
fn level_none_renders_plain_text() {
    let line = Line::styled("x", Style::new().fg(RED).bold());
    assert_eq!(line.render(ColorLevel::None), "x");
}

#[quench_test::test]
fn truncate_ends_in_an_ellipsis_and_pad_fills() {
    let line = Line::styled("abcdefgh", Style::new());
    assert_eq!(line.clone().truncate(5).plain(), "abcd…");
    assert_eq!(line.clone().truncate(8).plain(), "abcdefgh");
    assert_eq!(line.clone().pad_to(10, Style::new()).plain(), "abcdefgh  ");
    assert_eq!(line.pad_to(3, Style::new()).plain(), "ab…");
}

#[quench_test::test]
fn truncate_cuts_across_spans() {
    let line = Line::styled("abc", Style::new().bold()).push("defg", Style::new());
    assert_eq!(line.truncate(5).plain(), "abcd…");
}

#[quench_test::test]
fn on_only_fills_missing_backgrounds() {
    let line = Line::styled("a", Style::new())
        .push("b", Style::new().bg(INK))
        .on(RED);
    let out = line.render(ColorLevel::TrueColor);
    assert!(out.contains("48;2;240;68;56ma"), "{out}");
    assert!(out.contains("48;2;1;2;3mb"), "{out}");
}

#[quench_test::test]
fn fill_screen_paints_every_row_to_the_full_size() {
    let lines = fill_screen(vec![Line::styled("hi", Style::new())], 6, 3, INK);
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|l| l.width() == 6));
    assert_eq!(lines[0].plain(), "hi    ");
    // Height 0 keeps the row count; a short height cuts.
    assert_eq!(
        fill_screen(vec![Line::new(), Line::new()], 4, 0, INK).len(),
        2
    );
    assert_eq!(
        fill_screen(vec![Line::new(), Line::new()], 4, 1, INK).len(),
        1
    );
}
