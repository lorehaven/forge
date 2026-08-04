//! Unit tests for `routers/files.rs`.

use sage_service::routers::files::*;

#[test]
fn extension_lookup_is_case_insensitive_and_uses_the_last_dot() {
    assert_eq!(allowed_mime_type("photo.PNG"), Some("image/png"));
    assert_eq!(allowed_mime_type("archive.tar.gz"), None);
    assert_eq!(allowed_mime_type("notes.backup.md"), Some("text/markdown"));
    assert_eq!(allowed_mime_type("README"), None);
    assert_eq!(allowed_mime_type("evil.exe"), None);
}

#[test]
fn accept_attribute_covers_every_supported_extension() {
    let accept = upload_accept_attribute();
    let listed: Vec<&str> = accept.split(',').collect();
    assert_eq!(listed.len(), ALLOWED_UPLOAD_TYPES.len());
    for (ext, _) in ALLOWED_UPLOAD_TYPES {
        assert!(
            listed.contains(&format!(".{ext}").as_str()),
            "accept filter is missing .{ext}"
        );
    }
    // The formats the picker used to be limited to, plus images.
    for expected in [".pdf", ".txt", ".csv", ".md", ".png", ".jpg", ".webp"] {
        assert!(accept.contains(expected), "accept filter lost {expected}");
    }
}

#[test]
fn no_duplicate_extensions() {
    let mut seen: Vec<&str> = ALLOWED_UPLOAD_TYPES.iter().map(|(ext, _)| *ext).collect();
    let total = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), total, "duplicate extension in the accept table");
}

/// Everything the upload endpoint accepts must have somewhere to go: images
/// bypass extraction, every other MIME type must be one the extractor
/// knows, or the file would be stored only to fail processing.
#[test]
fn every_accepted_mime_is_handled_downstream() {
    for (ext, mime) in ALLOWED_UPLOAD_TYPES {
        if sage_service::files::is_image_mime(mime) {
            continue;
        }
        // Empty/dummy input still fails (no content), but not with the "unsupported type" error.
        if let Err(err) = sage_service::files::extractor::extract_text(mime, b"probe") {
            assert!(
                !err.starts_with("Unsupported MIME type"),
                ".{ext} maps to {mime}, which the extractor rejects: {err}"
            );
        }
    }
}
