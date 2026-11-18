//! A form's text, checked into the body the API wants.

use crate::admin::TransportInput;
use crate::model::{AccountInput, Compose};

/// Why a form is not ready to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    AddressInvalid,
    LabelRequired,
    HostRequired,
    PortInvalid,
    SecurityInvalid,
    PasswordRequired,
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
            Self::AddressInvalid => "that is not an email address",
            Self::LabelRequired => "give the mailbox a name",
            Self::HostRequired => "a server name is required",
            Self::PortInvalid => "the port must be a number from 1 to 65535",
            Self::SecurityInvalid => "choose TLS or STARTTLS",
            Self::PasswordRequired => "a password is required",
            Self::NoRecipients => "add at least one recipient",
            Self::RecipientInvalid => "one of the recipients is not an email address",
            Self::TooManyRecipients => "too many recipients",
            Self::SubjectLine => "the subject must be one line",
            Self::EmptyMessage => "write something first",
        }
    }
}

/// A loose but sound check: something, one `@`, a dotted or `localhost` domain, no spaces or brackets.
#[must_use]
pub fn plausible_address(text: &str) -> bool {
    let text = text.trim();
    let Some((local, domain)) = text.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && text.len() <= 254
        && !text.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | ',' | ';')
        })
        && (domain.contains('.') || domain == "localhost")
        && !domain.starts_with('.')
        && !domain.ends_with('.')
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

/// The add/change mailbox form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountDraft {
    pub label: String,
    pub address: String,
    pub username: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: String,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: String,
    pub smtp_security: String,
    pub signature: String,
    pub tls_roots_pem: String,
    pub save_sent: bool,
}

fn port(text: &str) -> Result<u16, DraftError> {
    text.trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or(DraftError::PortInvalid)
}

fn security(text: &str) -> Result<String, DraftError> {
    match text.trim() {
        s @ ("tls" | "starttls") => Ok(s.to_string()),
        _ => Err(DraftError::SecurityInvalid),
    }
}

impl AccountDraft {
    /// Checks the form. `existing` is true when changing a mailbox, where a blank password keeps the stored one.
    ///
    /// # Errors
    ///
    /// The first thing wrong with the form.
    pub fn validate(&self, existing: bool) -> Result<AccountInput, DraftError> {
        let address = self.address.trim();
        if !plausible_address(address) {
            return Err(DraftError::AddressInvalid);
        }
        if self.label.trim().is_empty() {
            return Err(DraftError::LabelRequired);
        }
        if self.imap_host.trim().is_empty() || self.smtp_host.trim().is_empty() {
            return Err(DraftError::HostRequired);
        }
        if self.password.is_empty() && !existing {
            return Err(DraftError::PasswordRequired);
        }
        let username = if self.username.trim().is_empty() {
            address
        } else {
            self.username.trim()
        };
        Ok(AccountInput {
            label: self.label.trim().to_string(),
            address: address.to_string(),
            username: username.to_string(),
            password: self.password.clone(),
            imap_host: self.imap_host.trim().to_string(),
            imap_port: port(&self.imap_port)?,
            imap_security: security(&self.imap_security)?,
            smtp_host: self.smtp_host.trim().to_string(),
            smtp_port: port(&self.smtp_port)?,
            smtp_security: security(&self.smtp_security)?,
            signature: self.signature.clone(),
            tls_roots_pem: self.tls_roots_pem.trim().to_string(),
            save_sent: self.save_sent,
        })
    }
}

/// The compose form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposeDraft {
    pub account_id: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub text: String,
    pub in_reply_to: Option<String>,
    pub attachments: Vec<String>,
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
        if total > 100 {
            return Err(DraftError::TooManyRecipients);
        }
        // `Name <a@b>` is accepted as typed; only the address inside is checked.
        let inner = |entry: &str| -> String {
            entry
                .rsplit_once('<')
                .and_then(|(_, rest)| rest.strip_suffix('>'))
                .unwrap_or(entry)
                .trim()
                .to_string()
        };
        if to
            .iter()
            .chain(&cc)
            .chain(&bcc)
            .any(|r| !plausible_address(&inner(r)))
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
            account_id: self.account_id.clone(),
            to,
            cc,
            bcc,
            subject: self.subject.trim().to_string(),
            text: self.text.clone(),
            in_reply_to: self.in_reply_to.clone(),
            attachments: self.attachments.clone(),
        })
    }
}

/// The administrator's transport form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TransportDraft {
    pub host: String,
    pub port: String,
    pub security: String,
    pub username: String,
    pub password: String,
    pub tls_roots_pem: String,
}

impl TransportDraft {
    /// Checks the form; a blank password keeps the stored one.
    ///
    /// # Errors
    ///
    /// The first thing wrong with the form.
    pub fn validate(&self) -> Result<TransportInput, DraftError> {
        if self.host.trim().is_empty() {
            return Err(DraftError::HostRequired);
        }
        Ok(TransportInput {
            host: self.host.trim().to_string(),
            port: port(&self.port)?,
            security: security(&self.security)?,
            username: self.username.trim().to_string(),
            password: self.password.clone(),
            tls_roots_pem: self.tls_roots_pem.trim().to_string(),
        })
    }
}
