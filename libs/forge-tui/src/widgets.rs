//! The components of the web apps, drawn with terminal cells.
//!
//! Each returns a [`Line`] (or lines) that already carries its colours; none knows the terminal size except where a
//! `width` is passed.

use crate::line::{Line, Style};
use crate::theme::{Theme, Tone};

/// A soft status badge: ` ● Up to date ` on a tinted background (`.fui-badge`).
#[must_use]
pub fn badge(theme: &Theme, tone: Tone, text: &str) -> Line {
    let soft = theme.soft(tone);
    let style = Style::new().fg(soft.text).bg(soft.bg);
    Line::styled(format!(" ● {text} "), style)
}

/// A small label without the dot, e.g. a version (` v0.1.22 `).
#[must_use]
pub fn tag(theme: &Theme, tone: Tone, text: &str) -> Line {
    let soft = theme.soft(tone);
    Line::styled(format!(" {text} "), Style::new().fg(soft.text).bg(soft.bg))
}

/// A keyboard key as a chip (` ↵ `).
#[must_use]
pub fn kbd(theme: &Theme, key: &str) -> Line {
    Line::styled(
        format!(" {key} "),
        Style::new()
            .fg(theme.text_secondary)
            .bg(theme.surface_2)
            .bold(),
    )
}

/// A key chip followed by what it does, for the footer of a screen.
#[must_use]
pub fn hint(theme: &Theme, key: &str, label: &str) -> Line {
    kbd(theme, key).push(format!(" {label}"), Style::new().fg(theme.text_tertiary))
}

/// How a [`button`] looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// Solid brand fill: the main action.
    Primary,
    /// Raised neutral fill.
    Secondary,
    /// Nothing to do: plain disabled text.
    Disabled,
}

/// A button label with its fill (`.fui-btn`).
#[must_use]
pub fn button(theme: &Theme, kind: ButtonKind, text: &str) -> Line {
    let style = match kind {
        ButtonKind::Primary => Style::new().fg(theme.on_brand).bg(theme.brand_solid).bold(),
        ButtonKind::Secondary => Style::new().fg(theme.text_secondary).bg(theme.surface_2),
        ButtonKind::Disabled => Style::new().fg(theme.text_disabled),
    };
    Line::styled(format!(" {text} "), style)
}

/// The glyph an alert of this tone leads with.
#[must_use]
pub const fn alert_icon(tone: Tone) -> &'static str {
    match tone {
        Tone::Success => "✓",
        Tone::Warning => "!",
        Tone::Error => "✕",
        Tone::Brand | Tone::Info | Tone::Gray => "i",
    }
}

/// A full-width alert: a tone-coloured bar, an icon and the message on the soft background (`.fui-alert`).
#[must_use]
pub fn alert(theme: &Theme, tone: Tone, text: &str, width: usize) -> Line {
    alert_with_icon(theme, tone, alert_icon(tone), text, width)
}

/// An [`alert`] with its own leading glyph (a spinner frame while something runs).
#[must_use]
pub fn alert_with_icon(theme: &Theme, tone: Tone, icon: &str, text: &str, width: usize) -> Line {
    let soft = theme.soft(tone);
    let base = Style::new().bg(soft.bg);
    Line::styled("▌", Style::new().fg(soft.text).bg(soft.bg))
        .push(format!(" {icon} "), base.fg(soft.text).bold())
        .push(text, base.fg(soft.text))
        .pad_to(width, base)
}

/// A card: rounded border on the page background, an optional header row, a divider, then the body rows.
///
/// `width` is the whole card including the border; body rows get `width - 2` columns (see [`card_inner`]), cut or
/// padded on the surface colour. A row that sets its own background (a selected row) should pad itself.
#[must_use]
pub fn card(theme: &Theme, header: Option<Line>, body: Vec<Line>, width: usize) -> Vec<Line> {
    let inner = card_inner(width);
    let edge = Style::new().fg(theme.border).bg(theme.bg);
    let side = Style::new().fg(theme.border).bg(theme.surface);
    let fill = Style::new().bg(theme.surface);
    let rule = "─".repeat(inner);
    let row = |line: Line| {
        Line::styled("│", side)
            .append(line.on(theme.surface).pad_to(inner, fill))
            .push("│", side)
    };

    let mut lines = vec![Line::styled(format!("╭{rule}╮"), edge)];
    if let Some(header) = header {
        lines.push(row(header));
        lines.push(Line::styled(format!("├{rule}┤"), side));
    }
    lines.extend(body.into_iter().map(row));
    lines.push(Line::styled(format!("╰{rule}╯"), edge));
    lines
}

/// Columns available to a card's rows.
#[must_use]
pub const fn card_inner(width: usize) -> usize {
    width.saturating_sub(2)
}
