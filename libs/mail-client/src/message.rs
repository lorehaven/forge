//! A message as a reader needs it: who, when, what, the text, the HTML, the attachments.

use crate::address::{self, Contact};
use crate::date;
use crate::encoding::decode_words;
use crate::entity::{self, Body, Entity};
use crate::sanitize;

/// Longest text or HTML body kept (bytes of the decoded text); the rest is cut.
pub const MAX_BODY: usize = 4 * 1024 * 1024;

/// An attachment or an inline resource of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Position among the message's attachments, stable for the same raw message: [`attachment_data`] takes it.
    pub index: usize,
    pub filename: String,
    pub media_type: String,
    pub size: usize,
    /// For a resource an HTML body points at as `cid:...`.
    pub content_id: Option<String>,
    /// Shown inside the body (an image the HTML refers to), not a download of its own.
    pub inline: bool,
}

/// What a header block says about a message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub subject: String,
    pub from: Vec<Contact>,
    pub to: Vec<Contact>,
    pub cc: Vec<Contact>,
    pub reply_to: Vec<Contact>,
    /// Unix seconds from the `Date` header, if it parses.
    pub date: Option<i64>,
    pub message_id: String,
    pub in_reply_to: String,
    pub references: Vec<String>,
    /// The `List-Unsubscribe` header, raw.
    pub list_unsubscribe: String,
}

/// A parsed message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {
    pub summary: Summary,
    /// The plain-text body; made from the HTML when the message has none.
    pub text: String,
    /// The HTML body as sent (not safe to show: see [`crate::sanitize`]).
    pub html: Option<String>,
    pub attachments: Vec<Attachment>,
}

/// The fields of a header block, for lists (no body needed).
#[must_use]
pub fn summarize(header_block: &[u8]) -> Summary {
    let (headers, _) = crate::header::Headers::parse(header_block);
    let contacts = |name: &str| -> Vec<Contact> {
        headers
            .get_all(name)
            .flat_map(address::parse_list)
            .collect()
    };
    Summary {
        subject: headers
            .decoded("subject")
            .map(|s| clean_line(&s))
            .unwrap_or_default(),
        from: contacts("from"),
        to: contacts("to"),
        cc: contacts("cc"),
        reply_to: contacts("reply-to"),
        date: headers.get("date").and_then(date::parse),
        message_id: headers
            .get("message-id")
            .map(|v| v.trim().to_string())
            .unwrap_or_default(),
        in_reply_to: headers
            .get("in-reply-to")
            .map(|v| v.trim().to_string())
            .unwrap_or_default(),
        references: headers
            .get("references")
            .map(|v| v.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default(),
        list_unsubscribe: headers
            .get("list-unsubscribe")
            .map(|v| v.trim().to_string())
            .unwrap_or_default(),
    }
}

/// A header-derived string with control characters (CR, LF, tabs and the rest) turned into spaces.
fn clean_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Parses a whole message.
#[must_use]
pub fn parse(raw: &[u8]) -> Message {
    let root = entity::parse(raw);
    let mut texts = Vec::new();
    let mut htmls = Vec::new();
    let mut leaves = Vec::new();
    collect(&root, &mut texts, &mut htmls, &mut leaves);
    let attachments = leaves
        .iter()
        .enumerate()
        .map(|(index, (entity, inline))| to_attachment(index, entity, *inline))
        .collect();

    let text = cap(texts.join("\n\n"));
    let html = if htmls.is_empty() {
        None
    } else {
        Some(cap(htmls.join("\n")))
    };
    let text = if text.trim().is_empty() {
        html.as_deref().map(sanitize::to_text).unwrap_or_default()
    } else {
        text
    };
    let header_end = crate::header::Headers::parse(raw).1.min(raw.len());
    Message {
        summary: summarize(&raw[..header_end]),
        text,
        html,
        attachments,
    }
}

fn cap(mut text: String) -> String {
    if text.len() > MAX_BODY {
        let mut end = MAX_BODY;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// Walks the tree in order, sorting leaves into body text, body HTML and attachments.
fn collect<'a>(
    entity: &'a Entity,
    texts: &mut Vec<String>,
    htmls: &mut Vec<String>,
    attachments: &mut Vec<(&'a Entity, bool)>,
) {
    match &entity.body {
        Body::Parts(parts) => match entity.media_type.as_str() {
            "multipart/alternative" => {
                // The alternatives say the same thing: take the text and the HTML each once, from the richest
                // alternative that has them.
                let mut alt_text: Option<Vec<String>> = None;
                let mut alt_html: Option<Vec<String>> = None;
                for part in parts {
                    let (mut t, mut h) = (Vec::new(), Vec::new());
                    collect(part, &mut t, &mut h, attachments);
                    if !t.is_empty() {
                        alt_text = Some(t);
                    }
                    if !h.is_empty() {
                        alt_html = Some(h);
                    }
                }
                texts.extend(alt_text.unwrap_or_default());
                htmls.extend(alt_html.unwrap_or_default());
            }
            "multipart/signed" => {
                // The first part is the message; the second is the signature, offered as a download.
                for (position, part) in parts.iter().enumerate() {
                    if position == 0 {
                        collect(part, texts, htmls, attachments);
                    } else {
                        attachments.push((part, false));
                    }
                }
            }
            "multipart/encrypted" => {
                for part in parts {
                    attachments.push((part, false));
                }
            }
            _ => {
                for part in parts {
                    collect(part, texts, htmls, attachments);
                }
            }
        },
        Body::Data(_) => match entity.media_type.as_str() {
            "text/plain" if !entity.is_attachment() => texts.push(entity.text()),
            "text/html" if !entity.is_attachment() => htmls.push(entity.text()),
            _ => {
                let inline = entity.content_id.is_some()
                    && entity.disposition.as_deref() != Some("attachment")
                    && entity.media_type.starts_with("image/");
                attachments.push((entity, inline));
            }
        },
    }
}

fn to_attachment(index: usize, entity: &Entity, inline: bool) -> Attachment {
    let size = match &entity.body {
        Body::Data(bytes) => bytes.len(),
        Body::Parts(_) => 0,
    };
    let filename = entity.filename.clone().unwrap_or_else(|| {
        if entity.media_type.starts_with("message/") {
            "message.eml".to_string()
        } else {
            let extension = entity.media_type.rsplit('/').next().unwrap_or("bin");
            format!("attachment.{}", clean_extension(extension))
        }
    });
    Attachment {
        index,
        filename,
        media_type: entity.media_type.clone(),
        size,
        content_id: entity.content_id.clone(),
        inline,
    }
}

fn clean_extension(extension: &str) -> String {
    let cleaned: String = extension
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    if cleaned.is_empty() {
        "bin".to_string()
    } else {
        cleaned
    }
}

/// The bytes of attachment `index` of a raw message (the numbering [`parse`] gives), with its file name and media type.
#[must_use]
pub fn attachment_data(raw: &[u8], index: usize) -> Option<(String, String, Vec<u8>)> {
    let root = entity::parse(raw);
    let (mut texts, mut htmls, mut leaves) = (Vec::new(), Vec::new(), Vec::new());
    collect(&root, &mut texts, &mut htmls, &mut leaves);
    let (entity, inline) = *leaves.get(index)?;
    let Body::Data(bytes) = &entity.body else {
        return None;
    };
    let attachment = to_attachment(index, entity, inline);
    Some((attachment.filename, attachment.media_type, bytes.clone()))
}

/// Decodes a header value for display (encoded words, control characters removed).
#[must_use]
pub fn display_header(value: &str) -> String {
    clean_line(&decode_words(value))
}
