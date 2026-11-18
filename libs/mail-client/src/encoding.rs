//! Content transfer encodings (base64, quoted-printable) and RFC 2047 encoded words in headers.

use crate::charset;

const fn b64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

/// Base64 as mail carries it: line breaks and stray characters are skipped, padding is optional and decoding stops at the
/// first `=`. Never fails; what cannot be decoded is dropped.
#[must_use]
pub fn base64_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &byte in input {
        if byte == b'=' {
            break;
        }
        let Some(value) = b64_value(byte) else {
            continue;
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    out
}

/// Standard base64 with padding, no line breaks.
#[must_use]
pub fn base64_encode(input: &[u8]) -> String {
    quench_core::base64::STANDARD.encode(input)
}

/// Standard base64 broken into CRLF-terminated lines of 76 characters, as a MIME body needs it.
#[must_use]
pub fn base64_lines(input: &[u8]) -> String {
    let encoded = base64_encode(input);
    let mut out = String::with_capacity(encoded.len() + encoded.len() / 38 + 2);
    for chunk in encoded.as_bytes().chunks(76) {
        // The alphabet is ASCII, so any slice of it is valid UTF-8.
        out.push_str(&String::from_utf8_lossy(chunk));
        out.push_str("\r\n");
    }
    out
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Quoted-printable. In `header` mode (RFC 2047 "Q") an underscore is a space and there are no soft line breaks.
#[must_use]
pub fn quoted_printable_decode(input: &[u8], header: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let byte = input[i];
        if byte == b'_' && header {
            out.push(b' ');
            i += 1;
        } else if byte == b'=' {
            // A soft line break: "=" before the end of a line.
            let rest = &input[i + 1..];
            if rest.starts_with(b"\r\n") {
                i += 3;
            } else if rest.starts_with(b"\n") {
                i += 2;
            } else if let (Some(&a), Some(&b)) = (rest.first(), rest.get(1)) {
                if let (Some(a), Some(b)) = (hex(a), hex(b)) {
                    out.push(a << 4 | b);
                    i += 3;
                } else {
                    out.push(b'=');
                    i += 1;
                }
            } else if rest.is_empty() {
                // A trailing "=" with nothing after it.
                i += 1;
            } else {
                out.push(b'=');
                i += 1;
            }
        } else {
            out.push(byte);
            i += 1;
        }
    }
    out
}

/// Decodes the encoded words (`=?charset?B|Q?text?=`) in a header value; text outside them is kept as it is. Whitespace
/// between two encoded words is dropped, as RFC 2047 says.
#[must_use]
pub fn decode_words(value: &str) -> String {
    if !value.contains("=?") {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    let mut previous_was_word = false;
    while let Some(start) = rest.find("=?") {
        let (before, candidate) = rest.split_at(start);
        if let Some((decoded, length)) = encoded_word(candidate) {
            if !(previous_was_word && before.trim().is_empty()) {
                out.push_str(before);
            }
            out.push_str(&decoded);
            rest = &candidate[length..];
            previous_was_word = true;
        } else {
            out.push_str(before);
            out.push_str("=?");
            rest = &candidate[2..];
            previous_was_word = false;
        }
    }
    out.push_str(rest);
    out
}

/// The word at the start of `text` and how many bytes it took, if it is one.
fn encoded_word(text: &str) -> Option<(String, usize)> {
    let body = text.strip_prefix("=?")?;
    let (charset, body) = body.split_once('?')?;
    let (encoding, body) = body.split_once('?')?;
    let (payload, _) = body.split_once("?=")?;
    // The language of RFC 2231 may trail the charset (`utf-8*pl`).
    let full_charset_length = charset.len();
    let charset = charset.split('*').next().unwrap_or(charset);
    if charset.is_empty() || payload.contains(char::is_whitespace) {
        return None;
    }
    let bytes = match encoding {
        "B" | "b" => base64_decode(payload.as_bytes()),
        "Q" | "q" => quoted_printable_decode(payload.as_bytes(), true),
        _ => return None,
    };
    let length = 2 + full_charset_length + 1 + encoding.len() + 1 + payload.len() + 2;
    Some((charset::decode(charset, &bytes), length))
}

/// Percent-decoding of an RFC 2231 parameter value (`%C5%82` is two bytes).
#[must_use]
pub fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(&a), Some(&b)) = (bytes.get(i + 1), bytes.get(i + 2))
            && let (Some(a), Some(b)) = (hex(a), hex(b))
        {
            out.push(a << 4 | b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}
