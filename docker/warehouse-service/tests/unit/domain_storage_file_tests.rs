//! `prefix_upper_bound` - the successor key that turns a literal-prefix match
//! into the half-open range `[prefix, prefix++)` the `C`-collation listing
//! index resolves as a bound.

use warehouse_service::domain::storage_file::prefix_upper_bound;

#[test]
fn bumps_the_last_byte_of_an_ascii_prefix() {
    assert_eq!(prefix_upper_bound("photos").as_deref(), Some("photot"));
    assert_eq!(prefix_upper_bound("videos").as_deref(), Some("videot"));
    assert_eq!(
        prefix_upper_bound("custom:foo").as_deref(),
        Some("custom:fop")
    );
}

#[test]
fn a_trailing_slash_bumps_to_the_next_byte() {
    // '/' is 0x2F; the successor is 0x30, '0'.
    assert_eq!(prefix_upper_bound("photos/").as_deref(), Some("photos0"));
}

#[test]
fn an_empty_prefix_has_no_successor() {
    assert_eq!(prefix_upper_bound(""), None);
}

#[test]
fn a_non_ascii_prefix_still_brackets_correctly_when_the_bump_stays_valid() {
    // 'é' is U+00E9 (`C3 A9`); bumping the trailing byte gives `C3 AA`, U+00EA,
    // still well-formed - so the range is `[café, cafê)`.
    let upper = prefix_upper_bound("café").unwrap();
    assert!("café/2026/01/1-a.jpg".as_bytes() >= "café".as_bytes());
    assert!("café/2026/01/1-a.jpg".as_bytes() < upper.as_bytes());
}

#[test]
fn a_bump_that_would_break_utf8_falls_back_to_none() {
    // U+007F's one byte is 0x7F; +1 is 0x80, a lone continuation byte and not
    // valid UTF-8 - `prefix_upper_bound` reports "no bound" and the query
    // keeps only `starts_with` for such a prefix. No category prefix this
    // service lists ends this way.
    assert_eq!(prefix_upper_bound("x\u{7f}"), None);
}

#[test]
fn the_bound_brackets_exactly_the_paths_that_start_with_the_prefix() {
    let prefix = "photos";
    let upper = prefix_upper_bound(prefix).unwrap();

    for inside in [
        "photos",
        "photos/2026/01/1-a.jpg",
        "photos/2019/12/9-z.png",
        "photoszzz",
    ] {
        assert!(inside >= prefix, "{inside:?} should be >= {prefix:?}");
        assert!(
            inside.as_bytes() < upper.as_bytes(),
            "{inside:?} should sort before {upper:?}"
        );
    }

    for outside in ["photor/2026/01/1-a.jpg", "photot", "videos/2026/01/1-a.jpg"] {
        let below = outside < prefix;
        let at_or_above_upper = outside.as_bytes() >= upper.as_bytes();
        assert!(
            below || at_or_above_upper,
            "{outside:?} should fall outside [{prefix:?}, {upper:?})"
        );
    }
}
