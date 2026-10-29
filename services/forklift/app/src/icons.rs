//! The icons, drawn as strokes so they are as heavy as the text next to them (the symbol fonts' glyphs are thin).

use quench_ui::prelude::*;

const COLOUR: &str = "#d4d4d4";

/// A 24x24 icon of stroked `d` paths as an image, `size` pixels wide.
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

/// An eye: show what is typed.
#[must_use]
pub fn eye(class: &str) -> Element {
    icon(
        "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0",
        class,
    )
}

/// An eye struck through: hide what is typed.
#[must_use]
pub fn eye_off(class: &str) -> Element {
    icon(
        "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0M4 4l16 16",
        class,
    )
}

#[must_use]
pub fn refresh(class: &str) -> Element {
    icon("M20 12a8 8 0 1 1-2.6-5.9M20 4v5h-5", class)
}

#[must_use]
pub fn sign_out(class: &str) -> Element {
    icon("M10 4H5v16h5M15 8l4 4-4 4M19 12H9", class)
}

#[must_use]
pub fn back(class: &str) -> Element {
    icon("M19 12H5M11 6l-6 6 6 6", class)
}

/// An arrow up out of a tray: the backup.
#[must_use]
pub fn backup(class: &str) -> Element {
    icon("M12 15V4M7 9l5-5 5 5M5 15v5h14v-5", class)
}

/// A folder.
#[must_use]
pub fn folder(class: &str) -> Element {
    icon("M3 6h6l2 2h10v11H3z", class)
}

/// A plus.
#[must_use]
pub fn plus(class: &str) -> Element {
    icon("M12 5v14M5 12h14", class)
}

/// A cross: remove.
#[must_use]
pub fn close(class: &str) -> Element {
    icon("M6 6l12 12M18 6L6 18", class)
}

/// A gear with eight teeth and a hub.
#[must_use]
pub fn gear(class: &str) -> Element {
    icon(
        "M10.0 4.7L10.4 1.9L13.6 1.9L14.0 4.7L15.8 5.4L18.0 3.7L20.3 6.0L18.6 8.2L19.3 10.0L22.1 10.4L22.1 13.6L19.3 14.0L18.6 15.8L20.3 18.0L18.0 20.3L15.8 18.6L14.0 19.3L13.6 22.1L10.4 22.1L10.0 19.3L8.2 18.6L6.0 20.3L3.7 18.0L5.4 15.8L4.7 14.0L1.9 13.6L1.9 10.4L4.7 10.0L5.4 8.2L3.7 6.0L6.0 3.7L8.2 5.4ZM8.8 12a3.2 3.2 0 1 0 6.4 0a3.2 3.2 0 1 0-6.4 0",
        class,
    )
}
