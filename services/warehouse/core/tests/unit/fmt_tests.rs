use warehouse_core::fmt::{PreviewKind, content_type, encode, format_bytes, percent, preview_kind};

#[quench_test::test]
fn sizes_read_like_the_old_pages() {
    assert_eq!(format_bytes(3), "3 B");
    assert_eq!(format_bytes(1536), "1.50 KiB");
    assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MiB");
    assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
}

#[quench_test::test]
fn the_quota_bar_is_clamped() {
    assert!((percent(50, 100) - 50.0).abs() < f64::EPSILON);
    assert!((percent(500, 100) - 100.0).abs() < f64::EPSILON);
    assert!(percent(5, 0).abs() < f64::EPSILON);
}

#[quench_test::test]
fn urls_are_encoded_per_component() {
    assert_eq!(encode("a b/c"), "a%20b%2Fc");
    assert_eq!(encode("v1.0-rc_1~"), "v1.0-rc_1~");
    assert_eq!(encode("ü"), "%C3%BC");
}

#[quench_test::test]
fn a_file_is_previewed_by_its_extension() {
    assert_eq!(preview_kind("a/B.PNG"), PreviewKind::Image);
    assert_eq!(preview_kind("clip.mp4"), PreviewKind::Video);
    assert_eq!(preview_kind("song.flac"), PreviewKind::Audio);
    assert_eq!(preview_kind("doc.pdf"), PreviewKind::Pdf);
    assert_eq!(preview_kind("notes.md"), PreviewKind::Text);
    assert_eq!(preview_kind("data.json"), PreviewKind::Text);
    assert_eq!(preview_kind("blob.bin"), PreviewKind::None);
    assert_eq!(preview_kind("dir.d/noext"), PreviewKind::None);
    assert_eq!(content_type("x.svg"), "image/svg+xml");
}
