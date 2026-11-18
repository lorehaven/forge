//! Header blocks: unfolding, lookup, and parameters (`Content-Type: text/plain; charset=utf-8`, RFC 2231 included).

use crate::{charset, encoding};

/// The header fields of one message or part, in order, values unfolded but not decoded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    items: Vec<(String, String)>,
}

/// Most header fields kept from one block; a message with more is hostile.
const MAX_FIELDS: usize = 512;
/// Longest value kept (bytes); the rest is cut.
const MAX_VALUE: usize = 64 * 1024;

impl Headers {
    /// Parses the header block at the start of `raw` and returns it with the offset where the body starts (after the blank
    /// line, or the end of the input when there is none).
    #[must_use]
    pub fn parse(raw: &[u8]) -> (Self, usize) {
        let mut items: Vec<(String, String)> = Vec::new();
        let mut pos = 0;
        while pos < raw.len() {
            let end = raw[pos..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(raw.len(), |i| pos + i);
            let mut line = &raw[pos..end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            let next = (end + 1).min(raw.len());
            if line.is_empty() {
                return (Self { items }, next);
            }
            if matches!(line[0], b' ' | b'\t') {
                // A continuation: part of the previous field.
                if let Some((_, value)) = items.last_mut()
                    && value.len() < MAX_VALUE
                {
                    value.push(' ');
                    value.push_str(String::from_utf8_lossy(line).trim());
                }
            } else if let Some(colon) = line.iter().position(|&b| b == b':') {
                let name = String::from_utf8_lossy(&line[..colon]).trim().to_string();
                let value = String::from_utf8_lossy(&line[colon + 1..])
                    .trim()
                    .to_string();
                if !name.is_empty()
                    && !name.contains(char::is_whitespace)
                    && items.len() < MAX_FIELDS
                {
                    items.push((name, value));
                }
            }
            // A line that is neither (mbox "From " lines, garbage) is skipped.
            pos = next;
        }
        (Self { items }, raw.len())
    }

    /// Builds a block from fields already in hand.
    #[must_use]
    pub fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        Self { items: pairs }
    }

    /// The first field named `name` (case-insensitive), raw.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Every field named `name`, in order.
    pub fn get_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.items
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The first field named `name`, with its encoded words decoded: for subjects and names.
    #[must_use]
    pub fn decoded(&self, name: &str) -> Option<String> {
        self.get(name).map(encoding::decode_words)
    }

    /// All fields as (name, raw value).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items.iter().map(|(n, v)| (n.as_str(), v.as_str()))
    }

    /// How many fields.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A structured field value: the main value and its `; name=value` parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params {
    /// The value before the first `;`, trimmed (lower-cased by [`Params::parse`] only for media types, see there).
    pub value: String,
    /// Parameters with RFC 2231 continuations joined and decoded; names lower-cased.
    pub params: Vec<(String, String)>,
}

impl Params {
    /// Parses `value; a=b; c="d e"`. The value is lower-cased when `lower` is set (media types, dispositions).
    #[must_use]
    pub fn parse(field: &str, lower: bool) -> Self {
        let mut pieces = split_params(field).into_iter();
        let mut value = pieces.next().unwrap_or_default().trim().to_string();
        if lower {
            value = value.to_ascii_lowercase();
        }
        let mut raw: Vec<(String, String)> = Vec::new();
        for piece in pieces {
            if let Some((name, val)) = piece.split_once('=') {
                let name = name.trim().to_ascii_lowercase();
                if !name.is_empty() {
                    raw.push((name, unquote(val.trim())));
                }
            }
        }
        Self {
            value,
            params: join_rfc2231(&raw),
        }
    }

    /// The parameter `name` (lower-case).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Splits on `;` outside quotes.
fn split_params(field: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in field.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if quoted && c == '\\' {
            current.push(c);
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
            current.push(c);
        } else if c == ';' && !quoted {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    out.push(current);
    out
}

fn unquote(value: &str) -> String {
    let Some(inner) = value.strip_prefix('"') else {
        return value.to_string();
    };
    let mut out = String::with_capacity(inner.len());
    let mut escaped = false;
    for c in inner.chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

/// Joins `name*0=..; name*1=..` pieces and decodes `name*=charset'lang'%XX` values; plain parameters pass through (their
/// encoded words are decoded too: senders put them there although RFC 2047 forbids it).
fn join_rfc2231(raw: &[(String, String)]) -> Vec<(String, String)> {
    let mut names: Vec<String> = Vec::new();
    for (name, _) in raw {
        let base = name.split('*').next().unwrap_or(name).to_string();
        if !names.contains(&base) {
            names.push(base);
        }
    }
    let mut out = Vec::new();
    for base in names {
        // (index, encoded?, text)
        let mut pieces: Vec<(usize, bool, &str)> = Vec::new();
        let mut plain: Option<&str> = None;
        for (name, value) in raw {
            let Some(rest) = name.strip_prefix(base.as_str()) else {
                continue;
            };
            match rest {
                "" => plain = plain.or(Some(value)),
                "*" => pieces.push((0, true, value)),
                _ => {
                    let Some(rest) = rest.strip_prefix('*') else {
                        continue;
                    };
                    let (digits, encoded) = rest
                        .strip_suffix('*')
                        .map_or((rest, false), |digits| (digits, true));
                    if let Ok(index) = digits.parse::<usize>() {
                        pieces.push((index, encoded, value));
                    }
                }
            }
        }
        if pieces.is_empty() {
            if let Some(value) = plain {
                out.push((base, encoding::decode_words(value)));
            }
            continue;
        }
        pieces.sort_by_key(|(index, _, _)| *index);
        pieces.truncate(64);
        let mut charset_label = String::from("utf-8");
        let mut bytes: Vec<u8> = Vec::new();
        for (position, (_, encoded, text)) in pieces.iter().enumerate() {
            if *encoded {
                let mut text = *text;
                if position == 0
                    && let Some((label, rest)) = text.split_once('\'')
                    && let Some((_, rest)) = rest.split_once('\'')
                {
                    charset_label = label.to_string();
                    text = rest;
                }
                bytes.extend(encoding::percent_decode(text));
            } else {
                bytes.extend_from_slice(text.as_bytes());
            }
        }
        out.push((base, charset::decode(&charset_label, &bytes)));
    }
    out
}
