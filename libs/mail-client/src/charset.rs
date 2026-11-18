//! Text in the character sets mail still carries, decoded to Unicode.
//!
//! UTF-8, US-ASCII and the ISO-8859 / Windows single-byte sets the estate's mail actually uses (Western, Central European,
//! Cyrillic, Greek, Turkish, KOI8-R). Anything else is decoded as UTF-8 with replacement characters rather than refused:
//! a message with an odd label is still worth showing.

use crate::charset_tables as tables;

/// The text `bytes` hold in character set `label` (case-insensitive, `windows-1250` and `cp1250` alike).
#[must_use]
pub fn decode(label: &str, bytes: &[u8]) -> String {
    let label = label.trim().trim_matches('"').to_ascii_lowercase();
    match label.as_str() {
        "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" | "l1" | "iso_8859-1" => {
            bytes.iter().map(|&b| char::from(b)).collect()
        }
        "windows-1252" | "cp1252" | "x-ansi" => high(bytes, &tables::WINDOWS_1252),
        "windows-1250" | "cp1250" | "x-cp1250" => high(bytes, &tables::WINDOWS_1250),
        "iso-8859-2" | "iso8859-2" | "latin2" | "latin-2" | "l2" | "iso_8859-2" => {
            high(bytes, &tables::ISO_8859_2)
        }
        "windows-1251" | "cp1251" | "x-cp1251" => high(bytes, &tables::WINDOWS_1251),
        "iso-8859-5" | "iso8859-5" => high(bytes, &tables::ISO_8859_5),
        "iso-8859-7" | "iso8859-7" => high(bytes, &tables::ISO_8859_7),
        "windows-1253" | "cp1253" => high(bytes, &tables::WINDOWS_1253),
        "iso-8859-9" | "iso8859-9" | "latin5" => high(bytes, &tables::ISO_8859_9),
        "windows-1254" | "cp1254" => high(bytes, &tables::WINDOWS_1254),
        "iso-8859-15" | "iso8859-15" | "latin9" | "latin-9" => high(bytes, &tables::ISO_8859_15),
        "koi8-r" | "koi8r" => high(bytes, &tables::KOI8_R),
        // UTF-8, US-ASCII (often mislabelled 8-bit) and everything unknown.
        _ => lossy(bytes),
    }
}

/// Whether [`decode`] knows this label (the rest fall back to UTF-8 with replacements).
#[must_use]
pub fn is_known(label: &str) -> bool {
    let label = label.trim().trim_matches('"').to_ascii_lowercase();
    matches!(
        label.as_str(),
        "utf-8"
            | "utf8"
            | "us-ascii"
            | "ascii"
            | "iso-8859-1"
            | "latin1"
            | "windows-1252"
            | "cp1252"
            | "windows-1250"
            | "cp1250"
            | "iso-8859-2"
            | "latin2"
            | "windows-1251"
            | "cp1251"
            | "iso-8859-5"
            | "iso-8859-7"
            | "windows-1253"
            | "iso-8859-9"
            | "windows-1254"
            | "iso-8859-15"
            | "koi8-r"
    )
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn high(bytes: &[u8], table: &[char; 128]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                char::from(b)
            } else {
                table[usize::from(b) - 0x80]
            }
        })
        .collect()
}
