// Colours are written as in the stylesheet (`0xecfdf3`), so the tables can be compared with it by eye.
#![allow(clippy::unreadable_literal)]

//! The design tokens: the semantic colours of `forge-shell/assets/tokens.css` for a mode (dark or light) and a brand
//! palette. A test keeps the values equal to the stylesheet.

use crate::color::Rgb;

/// Dark is the default, as on the web.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

impl Mode {
    /// `light` or `dark`, anything else `None`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    /// From `COLORFGBG` (`foreground;background` colour indexes, set by rxvt, Konsole and others): a background of 7
    /// or 15 is light.
    #[must_use]
    pub fn from_colorfgbg(value: &str) -> Option<Self> {
        let background: u8 = value.rsplit(';').next()?.trim().parse().ok()?;
        Some(if matches!(background, 7 | 15) {
            Self::Light
        } else {
            Self::Dark
        })
    }
}

/// The brand scale; the web picks it with the `fui_palette` cookie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    #[default]
    Green,
    Purple,
    Blue,
    Teal,
    Orange,
    Rose,
}

/// The steps of a brand scale the terminal uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brand {
    pub c50: Rgb,
    pub c200: Rgb,
    pub c300: Rgb,
    pub c500: Rgb,
    pub c600: Rgb,
    pub c700: Rgb,
}

impl Palette {
    pub const ALL: [Self; 6] = [
        Self::Green,
        Self::Purple,
        Self::Blue,
        Self::Teal,
        Self::Orange,
        Self::Rose,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Purple => "purple",
            Self::Blue => "blue",
            Self::Teal => "teal",
            Self::Orange => "orange",
            Self::Rose => "rose",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    #[must_use]
    pub const fn brand(self) -> Brand {
        const fn b(c50: u32, c200: u32, c300: u32, c500: u32, c600: u32, c700: u32) -> Brand {
            Brand {
                c50: Rgb::hex(c50),
                c200: Rgb::hex(c200),
                c300: Rgb::hex(c300),
                c500: Rgb::hex(c500),
                c600: Rgb::hex(c600),
                c700: Rgb::hex(c700),
            }
        }
        match self {
            Self::Green => b(0xecfdf3, 0xabefc6, 0x75e0a7, 0x17b26a, 0x079455, 0x067647),
            Self::Purple => b(0xf9f5ff, 0xe9d7fe, 0xd6bbfb, 0x9e77ed, 0x7f56d9, 0x6941c6),
            Self::Blue => b(0xeff8ff, 0xb2ddff, 0x84caff, 0x2e90fa, 0x1570ef, 0x175cd3),
            Self::Teal => b(0xf0fdf9, 0x99f6e0, 0x5fe9d0, 0x15b79e, 0x0e9384, 0x107569),
            Self::Orange => b(0xfff6ed, 0xfddcab, 0xfeb273, 0xfb6514, 0xec4a0a, 0xc4320a),
            Self::Rose => b(0xfff1f3, 0xfecdd6, 0xfea3b4, 0xf63d68, 0xe31b54, 0xc01048),
        }
    }
}

/// The meaning of a badge, alert or button, as in the web components.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Brand,
    Success,
    Warning,
    Error,
    /// The blue scale (`--fui-blue-*`): neutral information.
    Info,
    Gray,
}

/// A soft tone: tinted background, readable text and a border in the same hue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Soft {
    pub bg: Rgb,
    pub text: Rgb,
    pub border: Rgb,
}

/// The semantic colours of one mode and palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub mode: Mode,
    pub palette: Palette,
    pub bg: Rgb,
    pub bg_sunken: Rgb,
    pub surface: Rgb,
    pub surface_2: Rgb,
    pub surface_hover: Rgb,
    pub border: Rgb,
    pub border_strong: Rgb,
    pub text: Rgb,
    pub text_secondary: Rgb,
    pub text_tertiary: Rgb,
    pub text_disabled: Rgb,
    pub link: Rgb,
    pub brand_solid: Rgb,
    pub brand_solid_hover: Rgb,
    /// Text on a solid brand fill.
    pub on_brand: Rgb,
}

const WHITE: Rgb = Rgb::hex(0xffffff);

impl Theme {
    #[must_use]
    pub const fn new(mode: Mode, palette: Palette) -> Self {
        let brand = palette.brand();
        match mode {
            Mode::Dark => Self {
                mode,
                palette,
                bg: Rgb::hex(0x0d0e10),
                bg_sunken: Rgb::hex(0x08090a),
                surface: Rgb::hex(0x141517),
                surface_2: Rgb::hex(0x1b1c1f),
                surface_hover: Rgb::hex(0x212326),
                border: Rgb::hex(0x232528),
                border_strong: Rgb::hex(0x36383d),
                text: Rgb::hex(0xf4f4f5),
                text_secondary: Rgb::hex(0xcecfd2),
                text_tertiary: Rgb::hex(0x94969c),
                text_disabled: Rgb::hex(0x61646c),
                link: brand.c300,
                brand_solid: brand.c600,
                brand_solid_hover: brand.c500,
                on_brand: WHITE,
            },
            Mode::Light => Self {
                mode,
                palette,
                bg: Rgb::hex(0xf9fafb),
                bg_sunken: Rgb::hex(0xf2f4f7),
                surface: WHITE,
                surface_2: Rgb::hex(0xf9fafb),
                surface_hover: Rgb::hex(0xf9fafb),
                border: Rgb::hex(0xeaecf0),
                border_strong: Rgb::hex(0xd0d5dd),
                text: Rgb::hex(0x101828),
                text_secondary: Rgb::hex(0x344054),
                text_tertiary: Rgb::hex(0x667085),
                text_disabled: Rgb::hex(0x98a2b3),
                link: brand.c700,
                brand_solid: brand.c600,
                brand_solid_hover: brand.c700,
                on_brand: WHITE,
            },
        }
    }

    /// The default theme: dark, green.
    #[must_use]
    pub const fn dark() -> Self {
        Self::new(Mode::Dark, Palette::Green)
    }

    /// From `FORGE_THEME` (`dark`/`light`), else `COLORFGBG`, else dark; the palette from `FORGE_PALETTE`, else green.
    #[must_use]
    pub fn from_env() -> Self {
        let get = |key: &str| quench_core::env::get_or(key, "");
        Self::from_vars(
            &get("FORGE_THEME"),
            &get("COLORFGBG"),
            &get("FORGE_PALETTE"),
        )
    }

    /// The decision of [`from_env`](Self::from_env) from the three variables' values (empty when unset).
    #[must_use]
    pub fn from_vars(theme: &str, colorfgbg: &str, palette: &str) -> Self {
        let mode = Mode::parse(theme)
            .or_else(|| Mode::from_colorfgbg(colorfgbg))
            .unwrap_or_default();
        Self::new(mode, Palette::parse(palette).unwrap_or_default())
    }

    /// The brand scale of this theme's palette.
    #[must_use]
    pub const fn brand(&self) -> Brand {
        self.palette.brand()
    }

    /// The soft colours of a tone, as `--fui-<tone>-soft-*`: dark mode tints the hue at 14% (brand 16%) over the
    /// surface, light mode uses the 50 / 700 / 200 steps.
    #[must_use]
    pub fn soft(&self, tone: Tone) -> Soft {
        let brand = self.brand();
        // (step 50, step 200, step 500, step 700, dark-mode text)
        let (c50, c200, c500, c700, dark_text) = match tone {
            Tone::Brand => (brand.c50, brand.c200, brand.c500, brand.c700, brand.c300),
            Tone::Success => (
                Rgb::hex(0xecfdf3),
                Rgb::hex(0xabefc6),
                Rgb::hex(0x17b26a),
                Rgb::hex(0x067647),
                Rgb::hex(0x75e0a7),
            ),
            Tone::Warning => (
                Rgb::hex(0xfffaeb),
                Rgb::hex(0xfedf89),
                Rgb::hex(0xf79009),
                Rgb::hex(0xb54708),
                Rgb::hex(0xfec84b),
            ),
            Tone::Error => (
                Rgb::hex(0xfef3f2),
                Rgb::hex(0xfecdca),
                Rgb::hex(0xf04438),
                Rgb::hex(0xb42318),
                Rgb::hex(0xfda29b),
            ),
            Tone::Info => (
                Rgb::hex(0xeff8ff),
                Rgb::hex(0xb2ddff),
                Rgb::hex(0x2e90fa),
                Rgb::hex(0x175cd3),
                Rgb::hex(0x84caff),
            ),
            Tone::Gray => (
                Rgb::hex(0xf9fafb),
                Rgb::hex(0xeaecf0),
                WHITE,
                Rgb::hex(0x344054),
                Rgb::hex(0xcecfd2),
            ),
        };
        match self.mode {
            Mode::Light => Soft {
                bg: c50,
                text: c700,
                border: c200,
            },
            Mode::Dark => {
                let (bg_alpha, border_alpha) = match tone {
                    Tone::Brand => (16, 35),
                    Tone::Gray => (6, 12),
                    _ => (14, 35),
                };
                Soft {
                    bg: c500.over(self.surface, bg_alpha),
                    text: dark_text,
                    border: c500.over(self.surface, border_alpha),
                }
            }
        }
    }
}
