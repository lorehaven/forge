//! The icons, drawn as strokes so they are as heavy as the text next to them.

use std::fmt::Write as _;

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
            let _ = write!(url, "%{b:02X}");
        }
    }
    img()
        .attr("src", url)
        .attr("alt", "")
        .attr("aria-hidden", "true")
        .class(class)
}

/// A plus.
#[must_use]
pub fn plus(class: &str) -> Element {
    icon("M12 5v14M5 12h14", class)
}

/// An arrow pointing left: back.
#[must_use]
pub fn back(class: &str) -> Element {
    icon("M19 12H5M11 6l-6 6 6 6", class)
}

/// A bin: delete.
#[must_use]
pub fn bin(class: &str) -> Element {
    icon("M4 7h16M10 7V4h4v3M6 7l1 13h10l1-13M10 11v6M14 11v6", class)
}

/// A bell: the next alarm.
#[must_use]
pub fn bell(class: &str) -> Element {
    icon("M6 17V11a6 6 0 0 1 12 0v6l2 2H4zM10 21h4", class)
}
