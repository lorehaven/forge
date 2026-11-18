//! The JSON the API speaks. Times are Unix seconds.
//!
//! Mailroom is the estate's own mail: every Gatehouse user has one mailbox (`username@<domain>`), services write to people
//! through it, and nothing leaves the cluster. A mailbox has three folders, `inbox`, `sent` and `trash`.

use quench_serde::{Deserialize, Serialize};

/// The folders of a mailbox, in the order they are shown.
pub const FOLDERS: [&str; 3] = ["inbox", "sent", "trash"];

/// Who the signed-in person is to Mailroom, for the client to decide what to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    pub username: String,
    /// Their address here (`ann@<domain>`).
    pub address: String,
    pub can_write: bool,
    pub admin: bool,
}

/// A mailbox folder with its counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    /// `inbox`, `sent` or `trash`.
    pub id: String,
    pub unread: u32,
    pub total: u32,
}

/// A person or a service that writes mail.
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
    /// The folder it is in.
    pub folder: String,
    pub subject: String,
    pub from: Contact,
    #[serde(default)]
    pub to: Vec<Contact>,
    pub date: i64,
    /// The first words of the body.
    #[serde(default)]
    pub preview: String,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub attachments: bool,
    pub size: u32,
    /// Written by a service (Gatehouse, ...) rather than a person.
    #[serde(default)]
    pub automated: bool,
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
}

/// An opened message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub summary: MessageSummary,
    #[serde(default)]
    pub cc: Vec<Contact>,
    pub message_id: String,
    /// The text of the message (made from the HTML when there is no text part).
    pub text: String,
    /// Whether there is an HTML version to show (fetched safely from the message's `/html` endpoint).
    pub has_html: bool,
    /// Remote images the HTML would load, held back until the person allows them.
    pub remote_images: u32,
    pub attachments: Vec<AttachmentInfo>,
    /// The link that stops this kind of automated mail, when the sender gave one.
    #[serde(default)]
    pub unsubscribe: String,
}

/// What a person sends.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Compose {
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
pub struct Sent {
    /// The message's id in the sender's `sent` folder (a service has none: the delivery's id).
    pub id: String,
    /// `delivered`, or `duplicate` when the idempotency key was seen before.
    pub status: String,
    /// How many mailboxes it reached.
    pub delivered: u32,
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
    /// Move to `inbox` or `trash`.
    #[serde(default)]
    pub move_to: Option<String>,
    /// Delete for good (only from the trash).
    #[serde(default)]
    pub delete: bool,
}

/// What the machine-to-machine send endpoint takes. The sender is the calling service (`gatehouse@<domain>`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SendRequest {
    /// Usernames or `username@<domain>`; nothing outside this estate.
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
    #[serde(default)]
    pub html: Option<String>,
    /// The link that stops this kind of mail (shown by the reader).
    #[serde(default)]
    pub unsubscribe: Option<String>,
    /// What sort of message it is (`verification`, `notification`, ...), for the people reading the delivery log.
    #[serde(default)]
    pub kind: Option<String>,
    /// A retry with the same key does not deliver twice (kept for a day).
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// An error the API sends: `{"error": "..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}
