//! Colours and how many of them the terminal can show.

use std::fmt::Write as _;

/// An sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// The escape that makes `self` the background of what is drawn or erased next (empty at [`ColorLevel::None`]).
    #[must_use]
    pub fn background_escape(self, level: ColorLevel) -> String {
        level
            .sgr(self, true)
            .map_or_else(String::new, |codes| format!("\x1b[{codes}m"))
    }

    /// `0xRRGGBB`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // every channel is masked to 8 bits first
    pub const fn hex(value: u32) -> Self {
        Self(
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        )
    }

    /// `self` drawn at `percent` opacity (0 to 100) over `under`, the way CSS composites `rgba()`.
    #[must_use]
    pub fn over(self, under: Self, percent: u8) -> Self {
        let alpha = u16::from(percent.min(100));
        let mix = |top: u8, bottom: u8| {
            let value = (u16::from(top) * alpha + u16::from(bottom) * (100 - alpha) + 50) / 100;
            u8::try_from(value).unwrap_or(u8::MAX)
        };
        Self(
            mix(self.0, under.0),
            mix(self.1, under.1),
            mix(self.2, under.2),
        )
    }

    /// The nearest entry of the xterm 256-colour palette (colour cube or grey ramp).
    #[must_use]
    pub fn to_256(self) -> u8 {
        // The cube's channel levels are 0, 95, 135, 175, 215, 255.
        let step = |value: u8| -> u8 {
            match value {
                0..=47 => 0,
                48..=114 => 1,
                _ => (value - 35) / 40,
            }
        };
        let level = |step: u8| -> i32 {
            if step == 0 {
                0
            } else {
                55 + 40 * i32::from(step)
            }
        };
        let distance = |r: i32, g: i32, b: i32| {
            let (dr, dg, db) = (
                r - i32::from(self.0),
                g - i32::from(self.1),
                b - i32::from(self.2),
            );
            dr * dr + dg * dg + db * db
        };
        let (r, g, b) = (step(self.0), step(self.1), step(self.2));
        let cube = distance(level(r), level(g), level(b));

        let average = (i32::from(self.0) + i32::from(self.1) + i32::from(self.2)) / 3;
        let grey_step = ((average - 8) / 10).clamp(0, 23);
        let grey_level = 8 + 10 * grey_step;
        let grey = distance(grey_level, grey_level, grey_level);
        if grey < cube {
            232 + u8::try_from(grey_step).unwrap_or(0)
        } else {
            16 + 36 * r + 6 * g + b
        }
    }
}

/// How many colours the terminal can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorLevel {
    /// No styling at all (`NO_COLOR`, a dumb terminal, or output that is not a screen).
    None,
    /// The 256-colour palette.
    Ansi256,
    /// 24-bit colour.
    TrueColor,
}

impl ColorLevel {
    /// From the environment: `NO_COLOR` (non-empty) or `TERM=dumb` is [`None`](Self::None), `COLORTERM` of
    /// `truecolor` or `24bit` is [`TrueColor`](Self::TrueColor), anything else [`Ansi256`](Self::Ansi256).
    #[must_use]
    pub fn detect() -> Self {
        let get = |key: &str| quench_core::env::get_or(key, "");
        Self::from_vars(&get("NO_COLOR"), &get("TERM"), &get("COLORTERM"))
    }

    /// [`detect`](Self::detect), but unstyled when standard output is not a terminal (a pipe or a file).
    #[must_use]
    pub fn for_stdout() -> Self {
        use std::io::IsTerminal as _;
        if std::io::stdout().is_terminal() {
            Self::detect()
        } else {
            Self::None
        }
    }

    /// The decision of [`detect`](Self::detect) from the three variables' values (empty when unset).
    #[must_use]
    pub fn from_vars(no_color: &str, term: &str, colorterm: &str) -> Self {
        if !no_color.is_empty() || term == "dumb" {
            Self::None
        } else if matches!(colorterm, "truecolor" | "24bit") {
            Self::TrueColor
        } else {
            Self::Ansi256
        }
    }

    /// The SGR parameter for a foreground (`background` false) or background colour, `None` when unstyled.
    pub(crate) fn sgr(self, color: Rgb, background: bool) -> Option<String> {
        let base = if background { 48 } else { 38 };
        let mut out = String::new();
        match self {
            Self::None => return None,
            Self::Ansi256 => {
                let _ = write!(out, "{base};5;{}", color.to_256());
            }
            Self::TrueColor => {
                let _ = write!(out, "{base};2;{};{};{}", color.0, color.1, color.2);
            }
        }
        Some(out)
    }
}
