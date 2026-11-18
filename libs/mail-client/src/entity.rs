//! The structure of a message: headers, and a body that is data, or parts.

use crate::charset;
use crate::encoding;
use crate::header::{Headers, Params};

/// Deepest multipart nesting followed; deeper parts are kept as opaque data.
const MAX_DEPTH: usize = 12;
/// Most entities read from one message.
const MAX_PARTS: usize = 1000;

/// One part of a message (the whole message is the first).
#[derive(Debug, Clone)]
pub struct Entity {
    pub headers: Headers,
    /// `type/subtype`, lower-case; `text/plain` when the part does not say.
    pub media_type: String,
    /// The `Content-Type` parameters (`charset`, `boundary`, `name`, ...).
    pub params: Vec<(String, String)>,
    /// `inline` or `attachment`, if given.
    pub disposition: Option<String>,
    /// The file name from `Content-Disposition` or the `name` parameter.
    pub filename: Option<String>,
    /// `Content-ID` without the angle brackets.
    pub content_id: Option<String>,
    pub body: Body,
}

/// What an entity holds.
#[derive(Debug, Clone)]
pub enum Body {
    /// A leaf: the bytes after the transfer encoding is undone.
    Data(Vec<u8>),
    /// A multipart: the parts between the boundaries.
    Parts(Vec<Entity>),
}

impl Entity {
    /// The `charset` parameter.
    #[must_use]
    pub fn charset(&self) -> Option<&str> {
        self.param("charset")
    }

    /// A `Content-Type` parameter by lower-case name.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The leaf's bytes decoded as text in its charset; empty for a multipart.
    #[must_use]
    pub fn text(&self) -> String {
        match &self.body {
            Body::Data(bytes) => charset::decode(self.charset().unwrap_or("utf-8"), bytes),
            Body::Parts(_) => String::new(),
        }
    }

    /// Whether the part is meant to be saved rather than shown: an attachment disposition, or a file name without `inline`.
    #[must_use]
    pub fn is_attachment(&self) -> bool {
        match self.disposition.as_deref() {
            Some("attachment") => true,
            Some(_) => false,
            None => self.filename.is_some() && !self.media_type.starts_with("text/"),
        }
    }
}

/// Reads a whole message (or one part's bytes) into an entity tree. Never fails: a malformed message gives the best reading
/// that can be had, at worst a single text part.
#[must_use]
pub fn parse(raw: &[u8]) -> Entity {
    let mut budget = MAX_PARTS;
    parse_entity(raw, 0, &mut budget, "text/plain")
}

fn parse_entity(raw: &[u8], depth: usize, budget: &mut usize, default_type: &str) -> Entity {
    *budget = budget.saturating_sub(1);
    let (headers, offset) = Headers::parse(raw);
    let body = &raw[offset.min(raw.len())..];

    let content_type = headers
        .get("content-type")
        .map(|v| Params::parse(v, true))
        .filter(|p| p.value.contains('/'))
        .unwrap_or_else(|| Params {
            value: default_type.to_string(),
            params: Vec::new(),
        });
    let disposition = headers
        .get("content-disposition")
        .map(|v| Params::parse(v, true));
    let filename = disposition
        .as_ref()
        .and_then(|d| d.get("filename"))
        .or_else(|| {
            content_type
                .params
                .iter()
                .find(|(n, _)| n == "name")
                .map(|(_, v)| v.as_str())
        })
        .map(clean_filename);
    let content_id = headers
        .get("content-id")
        .map(|v| {
            v.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string()
        })
        .filter(|v| !v.is_empty());
    let media_type = content_type.value.clone();
    let boundary = content_type
        .params
        .iter()
        .find(|(n, _)| n == "boundary")
        .map(|(_, v)| v.clone());

    let multipart = media_type.starts_with("multipart/") && depth < MAX_DEPTH && *budget > 0;
    let entity_body = match boundary {
        Some(boundary) if multipart && !boundary.is_empty() => {
            let default = if media_type == "multipart/digest" {
                "message/rfc822"
            } else {
                "text/plain"
            };
            let mut parts = Vec::new();
            for part in split_multipart(body, &boundary) {
                if *budget == 0 {
                    break;
                }
                parts.push(parse_entity(part, depth + 1, budget, default));
            }
            Body::Parts(parts)
        }
        _ => Body::Data(decode_transfer(
            body,
            headers.get("content-transfer-encoding").unwrap_or(""),
        )),
    };

    Entity {
        headers,
        media_type,
        params: content_type.params,
        disposition: disposition.map(|d| d.value),
        filename,
        content_id,
        body: entity_body,
    }
}

/// A file name that is safe to show and to put in a `Content-Disposition` header: no path, no control characters.
#[must_use]
pub fn clean_filename(name: &str) -> String {
    let name = encoding::decode_words(name);
    let base = name.rsplit(['/', '\\']).next().unwrap_or(&name);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>' | '|' | ':' | '*' | '?'))
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() {
        "attachment".to_string()
    } else {
        cleaned.chars().take(180).collect()
    }
}

fn decode_transfer(body: &[u8], encoding_name: &str) -> Vec<u8> {
    match encoding_name.trim().to_ascii_lowercase().as_str() {
        "base64" => encoding::base64_decode(body),
        "quoted-printable" => encoding::quoted_printable_decode(body, false),
        _ => body.to_vec(),
    }
}

/// The parts between the delimiter lines of a multipart body. The preamble and epilogue are dropped; a missing closing
/// delimiter ends the last part at the end of the input.
fn split_multipart<'a>(body: &'a [u8], boundary: &str) -> Vec<&'a [u8]> {
    let delimiter = format!("--{boundary}");
    let delimiter = delimiter.as_bytes();
    let mut parts = Vec::new();
    let mut part_start: Option<usize> = None;
    let mut line_start = 0;
    while line_start <= body.len() {
        let line_end = body[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(body.len(), |i| line_start + i);
        let line = &body[line_start..line_end];
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let rest = line.strip_prefix(delimiter);
        let trimmed = rest.map(|r| {
            let end = r
                .iter()
                .rposition(|b| !matches!(b, b' ' | b'\t'))
                .map_or(0, |i| i + 1);
            &r[..end]
        });
        let is_close = trimmed == Some(b"--".as_slice());
        let is_delimiter = is_close || trimmed == Some(b"".as_slice());
        if is_delimiter {
            if let Some(start) = part_start.take() {
                // The line break before a delimiter belongs to the delimiter.
                let mut end = line_start;
                if end > start && body[end - 1] == b'\n' {
                    end -= 1;
                }
                if end > start && body[end - 1] == b'\r' {
                    end -= 1;
                }
                parts.push(&body[start..end.max(start)]);
            }
            if is_close {
                return parts;
            }
            part_start = Some((line_end + 1).min(body.len()));
        }
        if line_end >= body.len() {
            break;
        }
        line_start = line_end + 1;
    }
    if let Some(start) = part_start
        && start <= body.len()
    {
        parts.push(&body[start..]);
    }
    parts
}
