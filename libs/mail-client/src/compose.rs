//! Writing a message: headers, a text and/or HTML body, attachments, and the SMTP envelope that goes with it.
//!
//! `quench-mail` builds attachment-less messages; this builds the multipart structure a mail client needs and reuses its
//! header encoders. Every address goes through `quench_mail`'s strict parser and every header value is checked for line
//! breaks, so nothing a user types can add a header or a recipient.

use crate::{date, encoding};
use quench_mail::{Address, Envelope, Mailbox, mime};
use std::fmt;
use std::fmt::Write as _;

/// Most recipients (to, cc and bcc together) of one message.
pub const MAX_RECIPIENTS: usize = 100;
/// Most bytes of attachments in one message.
pub const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;
/// Most attachments in one message.
pub const MAX_ATTACHMENTS: usize = 25;

/// Why a draft cannot be turned into a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeError {
    /// This address does not parse (the text is the offending input, trimmed).
    InvalidAddress(String),
    NoRecipients,
    TooManyRecipients,
    /// A value that must be one line has a line break in it (the field's name).
    LineBreak(&'static str),
    /// A header the caller may not set.
    ForbiddenHeader(String),
    TooManyAttachments,
    AttachmentsTooLarge,
    /// The message has neither text nor HTML nor an attachment.
    Empty,
}

impl fmt::Display for ComposeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress(a) => write!(f, "not a valid address: {a}"),
            Self::NoRecipients => f.write_str("add at least one recipient"),
            Self::TooManyRecipients => write!(f, "more than {MAX_RECIPIENTS} recipients"),
            Self::LineBreak(field) => write!(f, "{field} must be a single line"),
            Self::ForbiddenHeader(name) => write!(f, "the header {name} cannot be set"),
            Self::TooManyAttachments => write!(f, "more than {MAX_ATTACHMENTS} attachments"),
            Self::AttachmentsTooLarge => f.write_str("the attachments are too large"),
            Self::Empty => f.write_str("the message is empty"),
        }
    }
}

impl std::error::Error for ComposeError {}

/// A file to attach.
#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub media_type: String,
    pub data: Vec<u8>,
}

/// What the writer typed (or a service asked for).
#[derive(Debug, Clone, Default)]
pub struct Draft {
    /// `Name <a@b>` or `a@b`.
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Option<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    /// Message-ID being answered, with angle brackets.
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub attachments: Vec<Attachment>,
    /// Extra headers; only [`ALLOWED_HEADERS`] and `X-` names are accepted.
    pub headers: Vec<(String, String)>,
}

/// Headers a caller may add besides `X-*`.
pub const ALLOWED_HEADERS: &[&str] = &[
    "list-unsubscribe",
    "list-unsubscribe-post",
    "list-id",
    "auto-submitted",
    "precedence",
    "importance",
    "priority",
    "keywords",
];

/// A message ready to send.
#[derive(Debug, Clone)]
pub struct Built {
    /// The message with CRLF line endings, as it goes over SMTP and into a Sent folder.
    pub raw: Vec<u8>,
    pub envelope: Envelope,
    /// `<id@domain>`.
    pub message_id: String,
}

/// Builds the message. `now` is Unix seconds (the `Date` header).
///
/// # Errors
///
/// A [`ComposeError`] naming what is wrong with the draft.
pub fn build(draft: &Draft, now: i64) -> Result<Built, ComposeError> {
    let from: Mailbox = parse_mailbox(&draft.from)?;
    let to = parse_all(&draft.to)?;
    let cc = parse_all(&draft.cc)?;
    let bcc = parse_all(&draft.bcc)?;
    let reply_to = draft.reply_to.as_deref().map(parse_mailbox).transpose()?;

    let recipients = to.len() + cc.len() + bcc.len();
    if recipients == 0 {
        return Err(ComposeError::NoRecipients);
    }
    if recipients > MAX_RECIPIENTS {
        return Err(ComposeError::TooManyRecipients);
    }
    if draft.subject.contains(['\r', '\n']) {
        return Err(ComposeError::LineBreak("the subject"));
    }
    if draft.attachments.len() > MAX_ATTACHMENTS {
        return Err(ComposeError::TooManyAttachments);
    }
    if draft
        .attachments
        .iter()
        .map(|a| a.data.len())
        .sum::<usize>()
        > MAX_ATTACHMENT_BYTES
    {
        return Err(ComposeError::AttachmentsTooLarge);
    }
    let has_body = draft.text.as_deref().is_some_and(|t| !t.is_empty())
        || draft.html.as_deref().is_some_and(|t| !t.is_empty());
    if !has_body && draft.attachments.is_empty() {
        return Err(ComposeError::Empty);
    }

    let id = format!(
        "<{}@{}>",
        quench_core::uuid::Uuid::new_v4().simple(),
        from.address.domain()
    );
    let mut out = String::new();
    let mut header = |name: &str, value: &str| {
        out.push_str(&mime::fold_header(name, value));
        out.push_str("\r\n");
    };
    header("Date", &date::format(now));
    header("From", &mailbox_list(std::slice::from_ref(&from)));
    if let Some(reply_to) = &reply_to {
        header("Reply-To", &mailbox_list(std::slice::from_ref(reply_to)));
    }
    if !to.is_empty() {
        header("To", &mailbox_list(&to));
    }
    if !cc.is_empty() {
        header("Cc", &mailbox_list(&cc));
    }
    if !draft.subject.is_empty() {
        header("Subject", &mime::unstructured(&draft.subject));
    }
    header("Message-ID", &id);
    if let Some(reply) = draft.in_reply_to.as_deref() {
        header("In-Reply-To", &message_id_field(reply, "In-Reply-To")?);
    }
    if !draft.references.is_empty() {
        let refs: Result<Vec<String>, _> = draft
            .references
            .iter()
            .map(|r| message_id_field(r, "References"))
            .collect();
        header("References", &refs?.join(" "));
    }
    header("MIME-Version", "1.0");
    for (name, value) in &draft.headers {
        let lower = name.to_ascii_lowercase();
        if !(lower.starts_with("x-") || ALLOWED_HEADERS.contains(&lower.as_str())) {
            return Err(ComposeError::ForbiddenHeader(name.clone()));
        }
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.contains(['\r', '\n', '\0'])
        {
            return Err(ComposeError::ForbiddenHeader(name.clone()));
        }
        header(name, &mime::unstructured(value));
    }

    let text = draft.text.as_deref().filter(|t| !t.is_empty());
    let html = draft.html.as_deref().filter(|t| !t.is_empty());
    if draft.attachments.is_empty() {
        write_body(&mut out, text, html);
    } else {
        let boundary = boundary();
        let _ = write!(
            out,
            "Content-Type: multipart/mixed;\r\n boundary=\"{boundary}\"\r\n\r\nThis is a multi-part message in MIME format.\r\n--{boundary}\r\n"
        );
        if text.is_none() && html.is_none() {
            out.push_str("Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 7bit\r\n\r\n\r\n");
        } else {
            write_body(&mut out, text, html);
        }
        for attachment in &draft.attachments {
            let _ = write!(out, "--{boundary}\r\n");
            write_attachment(&mut out, attachment);
        }
        let _ = write!(out, "--{boundary}--\r\n");
    }

    let envelope = Envelope {
        from: from.address,
        to: to
            .iter()
            .chain(&cc)
            .chain(&bcc)
            .map(|m| m.address.clone())
            .collect(),
    };
    Ok(Built {
        raw: out.into_bytes(),
        envelope,
        message_id: id,
    })
}

fn boundary() -> String {
    format!("mailroom-{}", quench_core::uuid::Uuid::new_v4().simple())
}

fn parse_mailbox(text: &str) -> Result<Mailbox, ComposeError> {
    if text.contains(['\r', '\n', '\0']) {
        return Err(ComposeError::LineBreak("an address"));
    }
    text.trim()
        .parse::<Mailbox>()
        .map_err(|_| ComposeError::InvalidAddress(text.trim().chars().take(80).collect()))
}

fn parse_all(list: &[String]) -> Result<Vec<Mailbox>, ComposeError> {
    list.iter()
        .filter(|a| !a.trim().is_empty())
        .map(|a| parse_mailbox(a))
        .collect()
}

fn mailbox_list(mailboxes: &[Mailbox]) -> String {
    mailboxes
        .iter()
        .map(|m| {
            m.name.as_ref().map_or_else(
                || m.address.to_string(),
                |name| format!("{} <{}>", mime::phrase(name), m.address),
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A `<msg-id>` for In-Reply-To / References: angle brackets, no spaces or line breaks.
fn message_id_field(id: &str, field: &'static str) -> Result<String, ComposeError> {
    let id = id.trim();
    if id.contains(['\r', '\n', '\0', ' ']) {
        return Err(ComposeError::LineBreak(field));
    }
    if id.starts_with('<') && id.ends_with('>') && id.len() > 2 {
        Ok(id.to_string())
    } else {
        Ok(format!("<{}>", id.trim_matches(['<', '>'])))
    }
}

fn write_body(out: &mut String, text: Option<&str>, html: Option<&str>) {
    match (text, html) {
        (Some(text), None) => {
            out.push_str(&part_headers("text/plain", text));
            out.push_str("\r\n");
            out.push_str(&part_body(text));
        }
        (None, Some(html)) => {
            out.push_str(&part_headers("text/html", html));
            out.push_str("\r\n");
            out.push_str(&part_body(html));
        }
        (Some(text), Some(html)) => {
            let boundary = boundary();
            let _ = write!(
                out,
                "Content-Type: multipart/alternative;\r\n boundary=\"{boundary}\"\r\n\r\n"
            );
            for (kind, body) in [("text/plain", text), ("text/html", html)] {
                let _ = write!(out, "--{boundary}\r\n");
                out.push_str(&part_headers(kind, body));
                out.push_str("\r\n");
                out.push_str(&part_body(body));
            }
            let _ = write!(out, "--{boundary}--\r\n");
        }
        (None, None) => out.push_str("\r\n"),
    }
}

fn part_headers(content_type: &str, body: &str) -> String {
    let encoding = if mime::is_7bit(&mime::normalize_crlf(body)) {
        "7bit"
    } else {
        "quoted-printable"
    };
    format!(
        "Content-Type: {content_type}; charset=utf-8\r\nContent-Transfer-Encoding: {encoding}\r\n"
    )
}

fn part_body(body: &str) -> String {
    let normalized = mime::normalize_crlf(body);
    let mut encoded = if mime::is_7bit(&normalized) {
        normalized
    } else {
        mime::quoted_printable(&normalized)
    };
    if !encoded.ends_with("\r\n") {
        encoded.push_str("\r\n");
    }
    encoded
}

fn write_attachment(out: &mut String, attachment: &Attachment) {
    let name = crate::entity::clean_filename(&attachment.filename);
    let media_type = safe_media_type(&attachment.media_type);
    // The file name as an RFC 2231 parameter: always correct for non-ASCII, and no quoting to get wrong.
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let _ = write!(
        out,
        "Content-Type: {media_type}\r\nContent-Disposition: attachment; filename*=utf-8''{encoded}\r\nContent-Transfer-Encoding: base64\r\n\r\n"
    );
    out.push_str(&encoding::base64_lines(&attachment.data));
}

/// `type/subtype` made of token characters, or `application/octet-stream`.
fn safe_media_type(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    let ok = value.split_once('/').is_some_and(|(a, b)| {
        let token = |s: &str| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-+.".contains(&c))
        };
        token(a) && token(b)
    });
    if ok {
        value
    } else {
        "application/octet-stream".to_string()
    }
}

/// `Re: subject`, without stacking `Re:`s.
#[must_use]
pub fn reply_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.len() >= 3 && trimmed[..3].eq_ignore_ascii_case("re:") {
        trimmed.to_string()
    } else {
        format!("Re: {trimmed}")
    }
}

/// `Fwd: subject`, without stacking.
#[must_use]
pub fn forward_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.len() >= 4 && trimmed[..4].eq_ignore_ascii_case("fwd:") {
        trimmed.to_string()
    } else {
        format!("Fwd: {trimmed}")
    }
}

/// The original text quoted for a reply: `> ` before every line, with a lead-in line.
#[must_use]
pub fn quote(original: &str, who: &str, when: Option<i64>) -> String {
    let lead = when.map_or_else(
        || format!("{who} wrote:"),
        |t| format!("On {}, {who} wrote:", date::format(t)),
    );
    let mut out = String::with_capacity(original.len() + lead.len() + 16);
    out.push_str(&lead);
    out.push('\n');
    for line in original.lines() {
        out.push_str("> ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Parses an address, for callers that need the strict form (the service validating an account's address).
///
/// # Errors
///
/// [`ComposeError::InvalidAddress`].
pub fn parse_address(text: &str) -> Result<Address, ComposeError> {
    text.trim()
        .parse()
        .map_err(|_| ComposeError::InvalidAddress(text.trim().chars().take(80).collect()))
}
