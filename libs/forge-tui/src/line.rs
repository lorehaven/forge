//! Styled text: spans of text with a colour and emphasis, joined into lines.
//!
//! Widths are counted in `char`s. Every glyph the widgets draw (box corners, `●`, `↑`, `↵`, braille spinners) is one
//! column wide, and tool text is ASCII, so no wide-character table is needed.

use crate::color::{ColorLevel, Rgb};

/// Colour and emphasis of a span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
}

impl Style {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            underline: false,
        }
    }

    #[must_use]
    pub const fn fg(mut self, color: Rgb) -> Self {
        self.fg = Some(color);
        self
    }

    #[must_use]
    pub const fn bg(mut self, color: Rgb) -> Self {
        self.bg = Some(color);
        self
    }

    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    #[must_use]
    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    const fn is_plain(self) -> bool {
        self.fg.is_none() && self.bg.is_none() && !self.bold && !self.underline
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Span {
    text: String,
    style: Style,
}

/// One row of a screen: spans of text, each with its own [`Style`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    spans: Vec<Span>,
}

impl Line {
    #[must_use]
    pub const fn new() -> Self {
        Self { spans: Vec::new() }
    }

    /// A line of one span.
    #[must_use]
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self::new().push(text, style)
    }

    /// Adds a span (empty text is dropped).
    #[must_use]
    pub fn push(mut self, text: impl Into<String>, style: Style) -> Self {
        let text = text.into();
        if !text.is_empty() {
            self.spans.push(Span { text, style });
        }
        self
    }

    /// Adds the spans of `other` after this line's.
    #[must_use]
    pub fn append(mut self, other: Self) -> Self {
        self.spans.extend(other.spans);
        self
    }

    /// Adds `count` spaces in `style`.
    #[must_use]
    pub fn space(self, count: usize, style: Style) -> Self {
        self.push(" ".repeat(count), style)
    }

    /// Width in columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.text.chars().count()).sum()
    }

    /// The text without any styling.
    #[must_use]
    pub fn plain(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Gives every span that has no background `bg`.
    #[must_use]
    pub fn on(mut self, bg: Rgb) -> Self {
        for span in &mut self.spans {
            span.style.bg.get_or_insert(bg);
        }
        self
    }

    /// Cuts the line to `width` columns, ending in `…` when something was cut.
    #[must_use]
    pub fn truncate(mut self, width: usize) -> Self {
        let total = self.width();
        if total <= width {
            return self;
        }
        let keep = width.saturating_sub(1);
        let mut used = 0;
        let mut spans = Vec::new();
        for mut span in self.spans.drain(..) {
            let len = span.text.chars().count();
            if used + len <= keep {
                used += len;
                spans.push(span);
                continue;
            }
            let take = keep - used;
            span.text = span.text.chars().take(take).collect();
            if !span.text.is_empty() {
                spans.push(span);
            }
            break;
        }
        let style = spans.last().map_or_else(Style::new, |s| s.style);
        let mut line = Self { spans };
        if width > 0 {
            line = line.push("…", style);
        }
        line
    }

    /// Exactly `width` columns: cut ([`truncate`](Self::truncate)) or padded with spaces in `fill`.
    #[must_use]
    pub fn pad_to(self, width: usize, fill: Style) -> Self {
        let current = self.width();
        if current > width {
            return self.truncate(width);
        }
        self.space(width - current, fill)
    }

    /// The line as terminal text: SGR codes around each styled span, reset after it.
    #[must_use]
    pub fn render(&self, level: ColorLevel) -> String {
        let mut out = String::new();
        for span in &self.spans {
            let style = span.style;
            if level == ColorLevel::None || style.is_plain() {
                out.push_str(&span.text);
                continue;
            }
            let mut codes: Vec<String> = Vec::new();
            if style.bold {
                codes.push("1".to_string());
            }
            if style.underline {
                codes.push("4".to_string());
            }
            codes.extend(style.fg.and_then(|c| level.sgr(c, false)));
            codes.extend(style.bg.and_then(|c| level.sgr(c, true)));
            out.push_str("\x1b[");
            out.push_str(&codes.join(";"));
            out.push('m');
            out.push_str(&span.text);
            out.push_str("\x1b[0m");
        }
        out
    }
}

/// Paints a whole screen: every line `width` columns wide on `bg`, and blank `bg` rows down to `height` (0 leaves the
/// number of rows alone; a taller list is cut to `height`).
#[must_use]
pub fn fill_screen(lines: Vec<Line>, width: usize, height: usize, bg: Rgb) -> Vec<Line> {
    let fill = Style::new().bg(bg);
    let mut out: Vec<Line> = lines
        .into_iter()
        .map(|line| line.on(bg).pad_to(width, fill))
        .collect();
    if height > 0 {
        out.truncate(height);
        while out.len() < height {
            out.push(Line::new().space(width, fill));
        }
    }
    out
}
