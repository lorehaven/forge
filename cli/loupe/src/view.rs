//! A [`Screen`] as the escape sequences that draw it.
//!
//! Each row from the top left, styles by role through the
//! default theme, marks in reverse video, OSC 8 hyperlinks for URLs, and the status line in reverse.

use crate::graphics::move_to;
use crate::pager::{Mark, Piece, Screen};
use quench_markdown_term::{LinkTarget, Theme, display_width};
use std::fmt::Write as _;

const CLEAR_LINE: &str = "\x1b[2K";

fn sgr(piece: &Piece, theme: &Theme) -> String {
    let style = &piece.style;
    let mut codes: Vec<String> = Vec::new();
    if style.bold || matches!(piece.mark, Mark::Current | Mark::Selected) {
        codes.push("1".into());
    }
    if style.dim {
        codes.push("2".into());
    }
    if style.italic {
        codes.push("3".into());
    }
    if style.underline || piece.mark == Mark::Focus {
        codes.push("4".into());
    }
    if piece.mark != Mark::None {
        codes.push("7".into());
    }
    if style.strikethrough {
        codes.push("9".into());
    }
    if let Some(colour) = theme.colour(style.role) {
        codes.push(colour.foreground().to_string());
    }
    codes.join(";")
}

fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// The whole frame for a terminal `columns` wide.
#[must_use]
pub fn frame(screen: &Screen, links: &[LinkTarget], columns: usize) -> String {
    let theme = Theme::default();
    let mut out = String::new();
    for (row_no, row) in screen.rows.iter().enumerate() {
        out.push_str(&move_to(0, row_no));
        out.push_str(CLEAR_LINE);
        for piece in &row.pieces {
            let url = piece
                .style
                .link
                .and_then(|l| links.get(l))
                .filter(|l| l.is_url())
                .map(|l| clean(&l.destination));
            if let Some(url) = &url {
                let _ = write!(out, "\x1b]8;;{url}\x1b\\");
            }
            let codes = sgr(piece, &theme);
            if codes.is_empty() {
                out.push_str(&piece.text);
            } else {
                let _ = write!(out, "\x1b[{codes}m{}\x1b[0m", piece.text);
            }
            if url.is_some() {
                out.push_str("\x1b]8;;\x1b\\");
            }
        }
    }
    out.push_str(&move_to(0, screen.rows.len()));
    out.push_str(CLEAR_LINE);
    let mut status = clean(&screen.status);
    let mut width = display_width(&status);
    while width > columns {
        status.pop();
        width = display_width(&status);
    }
    let _ = write!(out, "\x1b[7m{status}{}\x1b[0m", " ".repeat(columns - width));
    out
}
