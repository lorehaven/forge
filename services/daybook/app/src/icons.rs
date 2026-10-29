//! The icons, drawn as strokes so they are as heavy as the text next to them (the symbol fonts' glyphs are thin).

use quench_ui::prelude::*;

const COLOUR: &str = "#d4d4d4";

/// A 24x24 icon of stroked `d` paths as an image.
fn icon(paths: &str, class: &str) -> Element {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='{COLOUR}' \
         stroke-width='2.2' stroke-linecap='round' stroke-linejoin='round'><path d='{paths}'/></svg>"
    );
    let mut url = String::from("data:image/svg+xml,");
    for b in svg.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/:".contains(&b) {
            url.push(char::from(b));
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    img()
        .attr("src", url)
        .attr("alt", "")
        .attr("aria-hidden", "true")
        .class(class)
}

#[must_use]
pub fn menu(class: &str) -> Element {
    icon("M4 6h16M4 12h16M4 18h16", class)
}

#[must_use]
pub fn plus(class: &str) -> Element {
    icon("M12 5v14M5 12h14", class)
}

#[must_use]
pub fn calendar(class: &str) -> Element {
    icon(
        "M5 5h14a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM4 10h16M8 3v4M16 3v4",
        class,
    )
}

#[must_use]
pub fn repeat(class: &str) -> Element {
    icon(
        "M17 2l4 4-4 4M3 11V9a3 3 0 0 1 3-3h15M7 22l-4-4 4-4M21 13v2a3 3 0 0 1-3 3H3",
        class,
    )
}

#[must_use]
pub fn bell(class: &str) -> Element {
    icon(
        "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10 21a2 2 0 0 0 4 0",
        class,
    )
}

#[must_use]
pub fn trash(class: &str) -> Element {
    icon("M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3", class)
}

#[must_use]
pub fn tag(class: &str) -> Element {
    icon("M3 12V4h8l10 10-8 8L3 12zM7.5 8.5h.01", class)
}

#[must_use]
pub fn inbox(class: &str) -> Element {
    icon("M3 13l3-8h12l3 8v6H3v-6zM3 13h5a4 4 0 0 0 8 0h5", class)
}

#[must_use]
pub fn list(class: &str) -> Element {
    icon("M9 6h12M9 12h12M9 18h12M4 6h.01M4 12h.01M4 18h.01", class)
}

#[must_use]
pub fn chevron_left(class: &str) -> Element {
    icon("M15 5l-7 7 7 7", class)
}

#[must_use]
pub fn chevron_right(class: &str) -> Element {
    icon("M9 5l7 7-7 7", class)
}

#[must_use]
pub fn close(class: &str) -> Element {
    icon("M6 6l12 12M18 6L6 18", class)
}

#[must_use]
pub fn edit(class: &str) -> Element {
    icon("M4 20h4L19 9l-4-4L4 16v4zM13 7l4 4", class)
}

#[must_use]
pub fn logout(class: &str) -> Element {
    icon(
        "M9 4H5a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1h4M16 8l4 4-4 4M20 12H9",
        class,
    )
}

#[must_use]
pub fn eye(class: &str) -> Element {
    icon(
        "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0",
        class,
    )
}

#[must_use]
pub fn eye_off(class: &str) -> Element {
    icon(
        "M3 3l18 18M10.6 6.1A10 10 0 0 1 12 6c6 0 10 6 10 6a17 17 0 0 1-3.2 3.8M6.4 7.7A17 17 0 0 0 2 12s4 7 10 7a9.700 9.700 0 0 0 4-.9M9.9 9.9a3 3 0 0 0 4.200 4.200",
        class,
    )
}

#[must_use]
pub fn refresh(class: &str) -> Element {
    icon(
        "M20 11a8 8 0 0 0-14.500-3M4 4v4h4M4 13a8 8 0 0 0 14.500 3M20 20v-4h-4",
        class,
    )
}

#[must_use]
pub fn check(class: &str) -> Element {
    icon("M5 12l5 5 9-10", class)
}
