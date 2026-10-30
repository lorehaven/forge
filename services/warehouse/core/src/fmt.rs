//! How the pages write sizes and file types, and how a value goes into a URL.

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

/// `1.50 MiB`, `3 B`: the old pages' sizes.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_bytes(bytes: i64) -> String {
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= 1024.0 {
        format!("{:.2} KiB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// A byte count as GiB with two decimals, for the quota field.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn gib_text(bytes: i64) -> String {
    format!("{:.2}", bytes as f64 / GIB)
}

/// A byte count as MiB with two decimals, for the max-file field.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn mib_text(bytes: i64) -> String {
    format!("{:.2}", bytes as f64 / MIB)
}

/// The fill of a quota bar, 0 to 100.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn percent(used: i64, quota: i64) -> f64 {
    if quota > 0 {
        (used as f64 / quota as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
}

/// One URL component, percent-encoded.
#[must_use]
pub fn encode(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// What a file is shown as in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Image,
    Video,
    Audio,
    Pdf,
    Text,
    None,
}

/// The `Content-Type` the service serves a file inline with (its extension decides): the same table as the service's.
#[must_use]
pub fn content_type(path: &str) -> &'static str {
    let ext = path
        .rsplit('.')
        .next()
        .filter(|ext| !ext.contains('/'))
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "oga" | "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "xml" => "text/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "txt" | "md" | "markdown" | "log" | "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg"
        | "rs" | "py" | "js" | "ts" | "sh" | "c" | "h" | "cpp" | "go" | "java" | "rb" | "sql"
        | "env" => "text/plain",
        _ => "application/octet-stream",
    }
}

#[must_use]
pub fn preview_kind(path: &str) -> PreviewKind {
    let content_type = content_type(path);
    if content_type.starts_with("image/") {
        PreviewKind::Image
    } else if content_type.starts_with("video/") {
        PreviewKind::Video
    } else if content_type.starts_with("audio/") {
        PreviewKind::Audio
    } else if content_type == "application/pdf" {
        PreviewKind::Pdf
    } else if content_type.starts_with("text/") || content_type == "application/json" {
        PreviewKind::Text
    } else {
        PreviewKind::None
    }
}
