//! Styled output for tools that print lines instead of owning the screen.
//!
//! Drop-in for the `quench-cli` helpers the forge tools used (`Tone`, `print_status`, `print_box_banner`), restyled
//! like the web apps: a status line leads with a soft badge, a banner is a rounded card, a section has a brand
//! accent bar. Unlike [`crate::widgets`] nothing here sets a page background or a text colour on plain text, because the
//! terminal's own background is unknown: only badges (which carry their own background) and accents are coloured, and
//! ordinary text keeps the terminal's default colour, so it reads in light and dark terminals alike.
//!
//! Styling is off when standard output is not a terminal or `NO_COLOR` is set. The `*_with` functions take the theme and
//! colour level explicitly (for tests); the others use [`Theme::from_env`] and [`ColorLevel::for_stdout`].

use crate::color::ColorLevel;
use crate::line::{Line, Style};
use crate::theme::{Theme, Tone as Soft};
use std::sync::OnceLock;

/// What a status line says, with the names `quench_cli::prelude::Tone` has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Warn,
    Error,
}

impl Tone {
    const fn soft(self) -> Soft {
        match self {
            Self::Info => Soft::Info,
            Self::Success => Soft::Success,
            Self::Warn => Soft::Warning,
            Self::Error => Soft::Error,
        }
    }
}

fn environment() -> &'static (Theme, ColorLevel) {
    static ENV: OnceLock<(Theme, ColorLevel)> = OnceLock::new();
    ENV.get_or_init(|| (Theme::from_env(), ColorLevel::for_stdout()))
}

/// A soft badge around `label` (` build `), padded inside to at least `width` columns.
#[must_use]
pub fn chip_with(
    theme: &Theme,
    level: ColorLevel,
    tone: Tone,
    label: &str,
    width: usize,
) -> String {
    let soft = theme.soft(tone.soft());
    Line::styled(
        format!(" {label:<width$} "),
        Style::new().fg(soft.text).bg(soft.bg).bold(),
    )
    .render(level)
}

/// `label` as a badge, then `message` in the terminal's own text colour.
#[must_use]
pub fn status_with(
    theme: &Theme,
    level: ColorLevel,
    tone: Tone,
    label: &str,
    message: &str,
) -> String {
    if label.is_empty() {
        return message.to_string();
    }
    format!("{} {message}", chip_with(theme, level, tone, label, 0))
}

/// A rounded card of two lines: the title in bold and the subtitle muted.
#[must_use]
pub fn banner_with(theme: &Theme, level: ColorLevel, title: &str, subtitle: &str) -> Vec<String> {
    const INNER: usize = 45;
    let edge = Style::new().fg(theme.border_strong);
    let row = |text: Line| {
        Line::styled("│ ", edge)
            .append(text.pad_to(INNER, Style::new()))
            .push(" │", edge)
            .render(level)
    };
    let rule = "─".repeat(INNER + 2);
    vec![
        Line::styled(format!("╭{rule}╮"), edge).render(level),
        row(Line::styled(title, Style::new().bold())),
        row(Line::styled(subtitle, Style::new().fg(theme.text_tertiary))),
        Line::styled(format!("╰{rule}╯"), edge).render(level),
    ]
}

/// A section heading: a brand accent bar, the title in bold, and `detail` muted.
#[must_use]
pub fn section_with(theme: &Theme, level: ColorLevel, title: &str, detail: &str) -> String {
    let mut line = Line::styled("▌", Style::new().fg(theme.brand_solid))
        .push(format!(" {title}"), Style::new().bold());
    if !detail.is_empty() {
        line = line.push(format!("  {detail}"), Style::new().fg(theme.text_tertiary));
    }
    line.render(level)
}

/// A horizontal rule `width` columns wide.
#[must_use]
pub fn rule_with(theme: &Theme, level: ColorLevel, width: usize) -> String {
    Line::styled("─".repeat(width), Style::new().fg(theme.border_strong)).render(level)
}

/// `text` in the muted text colour (secondary information).
#[must_use]
pub fn muted_with(theme: &Theme, level: ColorLevel, text: &str) -> String {
    Line::styled(text, Style::new().fg(theme.text_tertiary)).render(level)
}

/// `text` in bold.
#[must_use]
pub fn strong_with(level: ColorLevel, text: &str) -> String {
    Line::styled(text, Style::new().bold()).render(level)
}

/// `text` in the brand colour.
#[must_use]
pub fn accent_with(theme: &Theme, level: ColorLevel, text: &str) -> String {
    Line::styled(text, Style::new().fg(theme.brand_solid).bold()).render(level)
}

/// [`chip_with`] for the environment's theme.
#[must_use]
pub fn chip(tone: Tone, label: &str, width: usize) -> String {
    let (theme, level) = environment();
    chip_with(theme, *level, tone, label, width)
}

/// Prints a status line: the label as a badge, then the message.
pub fn print_status(tone: Tone, label: &str, message: &str) {
    let (theme, level) = environment();
    println!("{}", status_with(theme, *level, tone, label, message));
}

/// Prints a two-line banner in a rounded card.
pub fn print_box_banner(title: &str, subtitle: &str) {
    let (theme, level) = environment();
    for row in banner_with(theme, *level, title, subtitle) {
        println!("{row}");
    }
}

/// A section heading (see [`section_with`]).
#[must_use]
pub fn section(title: &str, detail: &str) -> String {
    let (theme, level) = environment();
    section_with(theme, *level, title, detail)
}

/// A horizontal rule (see [`rule_with`]).
#[must_use]
pub fn rule(width: usize) -> String {
    let (theme, level) = environment();
    rule_with(theme, *level, width)
}

/// Muted text (see [`muted_with`]).
#[must_use]
pub fn muted(text: &str) -> String {
    let (theme, level) = environment();
    muted_with(theme, *level, text)
}

/// Bold text (see [`strong_with`]).
#[must_use]
pub fn strong(text: &str) -> String {
    strong_with(environment().1, text)
}

/// Brand-coloured text (see [`accent_with`]).
#[must_use]
pub fn accent(text: &str) -> String {
    let (theme, level) = environment();
    accent_with(theme, *level, text)
}
