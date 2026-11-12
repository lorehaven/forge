//! Small helpers for showing people.

use genevia_core::wire::Card;

/// `1899 – 1955`, `* 1899`, `† 1955` or empty.
#[must_use]
pub fn lifespan(c: &Card) -> String {
    match (c.birth_year, c.death_year) {
        (Some(b), Some(d)) => format!("{b} – {d}"),
        (Some(b), None) => format!("* {b}"),
        (None, Some(d)) => format!("† {d}"),
        (None, None) => String::new(),
    }
}

/// The class that colours a card by sex.
#[must_use]
pub fn sex_class(c: &Card) -> &'static str {
    match c.sex.as_str() {
        "M" => "m",
        "F" => "f",
        _ => "u",
    }
}

/// What a generation is called, counting back from the root.
#[must_use]
pub fn generation_name(g: u32) -> String {
    match g {
        0 => "Osoba główna".to_string(),
        1 => "Rodzice".to_string(),
        2 => "Dziadkowie".to_string(),
        3 => "Pradziadkowie".to_string(),
        4 => "Prapradziadkowie".to_string(),
        n => format!("Pokolenie {n}"),
    }
}

/// `ojcowska` or `macierzysta` for ancestors above the root: which parent the line goes through.
#[must_use]
pub const fn line_of(ahn: u32) -> &'static str {
    if ahn < 2 {
        ""
    } else {
        let g = genevia_core::fan::generation_of(ahn);
        if (ahn >> (g - 1)) == 2 {
            "ojcowska"
        } else {
            "macierzysta"
        }
    }
}

/// The address of the list of trees.
#[must_use]
pub fn home_url() -> String {
    format!("{}/ui", crate::api::base())
}

/// The address of a view of a tree, rooted at `xref` (blank for the tree's own root).
#[must_use]
pub fn url(tree: &str, view: &str, xref: &str) -> String {
    let base = crate::api::base();
    if xref.is_empty() {
        format!("{base}/ui/t/{tree}/{view}")
    } else {
        format!("{base}/ui/t/{tree}/{view}/{xref}")
    }
}
