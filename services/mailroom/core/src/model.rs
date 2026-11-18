//! The JSON the API speaks. Times are Unix seconds.

use quench_serde::{Deserialize, Serialize};

/// What the API says about a person's mailbox. Never carries the password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    /// The name the person gave it ("Work").
    pub label: String,
    pub address: String,
    pub username: String,
    pub imap_host: String,
    pub imap_port: u16,
    /// `tls` or `starttls`.
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    /// `ok`, `syncing`, `error` or `disabled`.
    pub state: String,
    /// What went wrong last time, in words for a person.
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_sync: Option<i64>,
    pub unread: u32,
    #[serde(default)]
    pub signature: String,
    /// Whether the account trusts a certificate of its own (a bridge, a private CA).
    #[serde(default)]
    pub custom_certificate: bool,
    /// Whether a sent message is copied to the Sent folder by Mailroom (not needed where the provider does it).
    #[serde(default)]
    pub save_sent: bool,
}

/// A mailbox folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub account_id: String,
    /// The server's name for it (`INBOX`, `[Gmail]/Sent Mail`).
    pub path: String,
    /// The last part of the path, for showing.
    pub name: String,
    /// `inbox`, `sent`, `drafts`, `trash`, `junk`, `archive`, or empty.
    pub role: String,
    pub unread: u32,
    pub total: u32,
}

/// A person or a mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Contact {
    #[serde(default)]
    pub name: String,
    pub address: String,
}

impl Contact {
    /// The name when there is one, else the address.
    #[must_use]
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            &self.address
        } else {
            &self.name
        }
    }

    /// `Name <address>`, or just the address.
    #[must_use]
    pub fn full(&self) -> String {
        if self.name.is_empty() {
            self.address.clone()
        } else {
            format!("{} <{}>", self.name, self.address)
        }
    }
}

/// One row of a message list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: String,
    pub account_id: String,
    pub folder_id: String,
    pub subject: String,
    pub from: Contact,
    #[serde(default)]
    pub to: Vec<Contact>,
    pub date: i64,
    /// The first words of the body, when known.
    #[serde(default)]
    pub preview: String,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub attachments: bool,
    pub size: u32,
}

/// A page of messages.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MessageList {
    pub items: Vec<MessageSummary>,
    /// How many match in all (not just on this page).
    pub total: u32,
    pub offset: u32,
}

/// A file attached to a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentInfo {
    pub index: u32,
    pub filename: String,
    pub media_type: String,
    pub size: u32,
    /// Shown inside the message already.
    pub inline: bool,
}

/// An opened message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub summary: MessageSummary,
    #[serde(default)]
    pub cc: Vec<Contact>,
    #[serde(default)]
    pub reply_to: Vec<Contact>,
    pub message_id: String,
    /// The text of the message (made from the HTML when there is no text part).
    pub text: String,
    /// Whether there is an HTML version to show (fetched safely from `html_url`).
    pub has_html: bool,
    /// Remote images the HTML would load, held back until the person allows them.
    pub remote_images: u32,
    pub attachments: Vec<AttachmentInfo>,
    #[serde(default)]
    pub list_unsubscribe: String,
}

/// What a person sends from a mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Compose {
    pub account_id: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub text: String,
    /// The message being answered (its id here), so the reply is threaded and marked answered.
    #[serde(default)]
    pub in_reply_to: Option<String>,
    /// Uploaded files to attach (ids from the upload endpoint).
    #[serde(default)]
    pub attachments: Vec<String>,
}

/// The answer to a send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queued {
    pub id: String,
    /// `queued`, or `duplicate` when the idempotency key was seen before.
    pub status: String,
}

/// A file uploaded to attach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upload {
    pub id: String,
    pub filename: String,
    pub size: u32,
}

/// A change to one or more messages.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Change {
    pub ids: Vec<String>,
    #[serde(default)]
    pub seen: Option<bool>,
    #[serde(default)]
    pub flagged: Option<bool>,
    /// Move to the folder with this role (`trash`, `archive`, `junk`, `inbox`), or to a folder id.
    #[serde(default)]
    pub move_to: Option<String>,
    /// Delete for good (only from the trash).
    #[serde(default)]
    pub delete: bool,
}

/// A suggestion for the add-account form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    pub note: String,
    /// The provider's default for the Sent copy.
    pub save_sent: bool,
}

/// What the person typed to add or change a mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AccountInput {
    pub label: String,
    pub address: String,
    pub username: String,
    /// Empty on a change means "keep the one stored".
    #[serde(default)]
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    #[serde(default)]
    pub signature: String,
    /// PEM of a certificate to trust for this server; empty means the public roots only.
    #[serde(default)]
    pub tls_roots_pem: String,
    #[serde(default)]
    pub save_sent: bool,
}

/// The result of trying a mailbox's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    pub imap: Step,
    pub smtp: Step,
}

/// One half of a connection test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub ok: bool,
    /// What failed, for a person; empty when `ok`.
    #[serde(default)]
    pub error: String,
}

/// What the machine-to-machine send endpoint takes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SendRequest {
    /// One of the administrator's sender identities (its address); the default identity when empty.
    #[serde(default)]
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
    #[serde(default)]
    pub html: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Extra headers: `List-Unsubscribe`, `List-Unsubscribe-Post`, `Auto-Submitted`, `X-*`.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// A retry with the same key does not send twice (kept for a day).
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// Who the signed-in person is to Mailroom, for the client to decide what to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    pub username: String,
    pub can_write: bool,
    pub admin: bool,
    pub max_accounts: u32,
}

/// An error the API sends: `{"error": "..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}
