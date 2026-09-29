//! Profile pictures uploaded from the account page. Stored inline in `avatar_url`
//! as a `data:` URI, so no file store is needed - hence the small size cap.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub const MAX_AVATAR_BYTES: usize = 256 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum AvatarError {
    TooLarge,
    UnsupportedType,
}

/// Sniffs the bytes rather than trusting the client's `Content-Type`. SVG is
/// left out on purpose: it can carry script.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub fn to_data_uri(bytes: &[u8]) -> Result<String, AvatarError> {
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err(AvatarError::TooLarge);
    }
    let mime = sniff(bytes).ok_or(AvatarError::UnsupportedType)?;
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

/// Inverse of [`to_data_uri`], re-checking the type so a stored value can never
/// be served as anything but an image.
pub fn from_data_uri(uri: &str) -> Option<(&'static str, Vec<u8>)> {
    let (_, payload) = uri.strip_prefix("data:")?.split_once(";base64,")?;
    let bytes = STANDARD.decode(payload).ok()?;
    Some((sniff(&bytes)?, bytes))
}
