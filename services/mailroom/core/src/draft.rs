//! A form's text, checked into the body the API wants.

use crate::model::Compose;

/// Why a form is not ready to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    NoRecipients,
    RecipientInvalid,
    TooManyRecipients,
    SubjectLine,
    EmptyMessage,
}

impl DraftError {
    /// The words for a person, in English.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoRecipients => "add at least one recipient",
            Self::RecipientInvalid => "write a recipient as a user name or name@domain",
            Self::TooManyRecipients => "too many recipients",
            Self::SubjectLine => "the subject must be one line",
            Self::EmptyMessage => "write something first",
        }
    }
}

/// Whether `text` can be the part of an address before the `@`: letters, digits, `.`, `_`, `-`, 1 to 64 long, not starting or
/// ending with a dot.
#[must_use]
pub fn local_part_ok(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && !text.starts_with('.')
        && !text.ends_with('.')
        && !text.contains("..")
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A recipient as typed (`ann`, `ann@home.example`, `Ann <ann@home.example>`) reduced to the lowercase user name, or `None`
/// when it is not one. Whether the domain is this estate's is for the server to say.
#[must_use]
pub fn recipient_name(entry: &str) -> Option<String> {
    let entry = entry.trim();
    let inner = entry
        .rsplit_once('<')
        .and_then(|(_, rest)| rest.strip_suffix('>'))
        .unwrap_or(entry)
        .trim();
    let local = inner.split_once('@').map_or(inner, |(local, _)| local);
    local_part_ok(local).then(|| local.to_ascii_lowercase())
}

/// The text of an address field (commas, semicolons or lines) as a list.
#[must_use]
pub fn split_recipients(text: &str) -> Vec<String> {
    text.split([',', ';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The compose form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposeDraft {
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub text: String,
    pub in_reply_to: Option<String>,
    pub attachments: Vec<String>,
    /// The saved draft this form came from, removed when the message is sent.
    pub draft: Option<String>,
}

impl ComposeDraft {
    /// Checks the form.
    ///
    /// # Errors
    ///
    /// The first thing wrong with the form.
    pub fn validate(&self) -> Result<Compose, DraftError> {
        let to = split_recipients(&self.to);
        let cc = split_recipients(&self.cc);
        let bcc = split_recipients(&self.bcc);
        let total = to.len() + cc.len() + bcc.len();
        if total == 0 {
            return Err(DraftError::NoRecipients);
        }
        if total > 50 {
            return Err(DraftError::TooManyRecipients);
        }
        if to
            .iter()
            .chain(&cc)
            .chain(&bcc)
            .any(|r| recipient_name(r).is_none())
        {
            return Err(DraftError::RecipientInvalid);
        }
        if self.subject.contains(['\r', '\n']) {
            return Err(DraftError::SubjectLine);
        }
        if self.text.trim().is_empty() && self.attachments.is_empty() {
            return Err(DraftError::EmptyMessage);
        }
        Ok(Compose {
            to,
            cc,
            bcc,
            subject: self.subject.trim().to_string(),
            text: self.text.clone(),
            in_reply_to: self.in_reply_to.clone(),
            attachments: self.attachments.clone(),
            draft: self.draft.clone(),
        })
    }
}
