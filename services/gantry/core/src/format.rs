//! Text for the screens.

/// One URL part, percent-encoded.
#[must_use]
pub fn encode(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// `2026-10-29T10:00:05.123Z` as `2026-10-29 10:00:05`: how the history shows a time.
#[must_use]
pub fn when(rfc3339: &str) -> String {
    let text = rfc3339.replace('T', " ");
    let text = text.split(['.', 'Z', '+']).next().unwrap_or_default();
    text.to_string()
}
