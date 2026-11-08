//! The math font formulas are typeset with: the bundled Noto Sans Math, or a TrueType font chosen with
//! `--math-font` or `math_font` in the config file, validated before it is used.

use quench_tex::typeset::Fonts;
use quench_text::{Font, FontError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The characters a math font must have glyphs for.
pub const BASICS: [char; 8] = ['+', '−', '=', '(', ')', '∑', '∫', '√'];

/// Where the math font comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Noto Sans Math, embedded in quench-text.
    Bundled,
    /// A file, chosen on the command line or in the config file (`from` says which).
    File { path: PathBuf, from: &'static str },
}

impl Source {
    /// How `doctor` and the warnings name it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Bundled => "Noto Sans Math, bundled (quench-text)".to_string(),
            Self::File { path, from } => format!("{} ({from})", path.display()),
        }
    }
}

/// The fonts to typeset with, or why the chosen math font cannot be used.
#[derive(Debug, Clone)]
pub struct MathFont {
    pub source: Source,
    pub fonts: Result<Fonts, String>,
}

impl MathFont {
    /// The bundled font.
    #[must_use]
    pub fn bundled() -> Self {
        Self {
            source: Source::Bundled,
            fonts: Ok(Fonts::bundled()),
        }
    }

    /// The font at `path` (read and validated), or the bundled one without a path.
    #[must_use]
    pub fn choose(path: Option<&Path>, from: &'static str) -> Self {
        path.map_or_else(Self::bundled, |path| Self {
            source: Source::File {
                path: path.to_path_buf(),
                from,
            },
            fonts: load(path),
        })
    }

    /// The one warning shown when the chosen font cannot be used.
    #[must_use]
    pub fn warning(&self) -> Option<String> {
        self.fonts
            .as_ref()
            .err()
            .map(|why| format!("no math font: {why}; formulas are shown as text"))
    }
}

/// Reads and validates the math font at `path`.
///
/// # Errors
/// Why it cannot be used, naming the file: it cannot be read, is a CFF font, is not a font, has no `MATH` table
/// (or one without its constants, glyph info or variants), or lacks a glyph for one of [`BASICS`].
pub fn load(path: &Path) -> Result<Fonts, String> {
    let shown = path.display();
    let bytes = std::fs::read(path).map_err(|e| format!("{shown} cannot be read: {e}"))?;
    validate(&bytes).map_err(|why| format!("{shown} {why}"))
}

/// Validates the bytes of a font file; the error reads after its name ("is a CFF font, …").
///
/// # Errors
/// As [`load`].
pub fn validate(bytes: &[u8]) -> Result<Fonts, String> {
    let subtables = math_subtables(bytes);
    let font = Font::parse(bytes.to_vec()).map_err(|e| match e {
        FontError::Cff => "is a CFF font: CFF outlines are not read yet".to_string(),
        FontError::NotTrueType => "is not a TrueType font".to_string(),
        other => format!("is not a font quench-text reads ({other})"),
    })?;
    if font.math().is_none() {
        return Err("has no MATH table: it is not a math font".to_string());
    }
    match subtables {
        Some([constants, glyph_info, variants]) => {
            let missing: Vec<&str> = [
                (constants, "constants"),
                (glyph_info, "glyph info"),
                (variants, "variants"),
            ]
            .iter()
            .filter(|(present, _)| !present)
            .map(|&(_, name)| name)
            .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "has a MATH table without its {}",
                    missing.join(", ")
                ));
            }
        }
        None => return Err("has no MATH table: it is not a math font".to_string()),
    }
    let missing: String = BASICS
        .iter()
        .filter(|&&c| font.glyph(c).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "has no glyphs for {missing}: it is not a math font"
        ));
    }
    Fonts::with_math(Arc::new(font))
        .map_err(|_| "has a MATH table without its constants".to_string())
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
        *bytes.get(at + 2)?,
        *bytes.get(at + 3)?,
    ]))
}

/// Whether the `MATH` table's three subtables (constants, glyph info, variants) are there: their offsets in
/// the table's header are not zero. `None` without a `MATH` table.
fn math_subtables(bytes: &[u8]) -> Option<[bool; 3]> {
    let count = usize::from(u16_at(bytes, 4)?);
    let offset = (0..count).find_map(|i| {
        let record = 12 + i * 16;
        (bytes.get(record..record + 4)? == b"MATH").then(|| u32_at(bytes, record + 8))?
    })?;
    let at = usize::try_from(offset).ok()?;
    Some([4, 6, 8].map(|field| u16_at(bytes, at + field).is_some_and(|o| o != 0)))
}
