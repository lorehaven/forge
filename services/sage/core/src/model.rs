//! The JSON the API speaks.

use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub updated_at: String,
}

/// A conversation as the sidebar lists it. A blank `title` is a conversation made by attaching a file before the first message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationItem {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub updated_at: String,
}

/// A file as the composer, a message and the project's list show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
    pub message_id: Option<String>,
    /// `uploaded` (queued), `processing`, `ready`, `failed`.
    pub status: String,
    pub error_message: Option<String>,
}

impl FileInfo {
    /// Still being worked on, so the chip is read again.
    #[must_use]
    pub fn in_progress(&self) -> bool {
        self.status == "uploaded" || self.status == "processing"
    }

    #[must_use]
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }
}

/// Where an answer's material came from: an excerpt of a file, or a web page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub file_name: String,
    pub chunk_index: Option<i32>,
    pub detail: Option<String>,
    pub similarity: Option<f64>,
}

/// One message of the active branch, with what hangs off it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageNode {
    pub id: String,
    pub parent_id: Option<String>,
    /// `user` or `assistant`.
    pub role: String,
    pub content: String,
    pub created_at: String,
    /// The ids of the alternatives at this point of the tree (this message included), oldest first.
    pub siblings: Vec<String>,
    pub sources: Vec<Source>,
    pub attachments: Vec<FileInfo>,
}

impl MessageNode {
    #[must_use]
    pub fn is_user(&self) -> bool {
        self.role == "user"
    }

    /// Where this message stands among its alternatives, counting from 1, and how many there are.
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        let at = self
            .siblings
            .iter()
            .position(|s| *s == self.id)
            .unwrap_or(0);
        (at + 1, self.siblings.len().max(1))
    }

    /// The alternative before and after this one, wrapping around.
    #[must_use]
    pub fn neighbours(&self) -> Option<(&str, &str)> {
        let total = self.siblings.len();
        if total < 2 {
            return None;
        }
        let at = self.siblings.iter().position(|s| *s == self.id)?;
        let prev = &self.siblings[(at + total - 1) % total];
        let next = &self.siblings[(at + 1) % total];
        Some((prev, next))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationView {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub messages: Vec<MessageNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOption {
    pub id: String,
    pub model: String,
}

/// A configured default model and how far its launch is: `running`, `starting`, `pending`, `failed` or `unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultModelState {
    pub name: String,
    pub embedding: bool,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchProviderOption {
    pub id: String,
    pub label: String,
}

/// What the composer and the initializing screen need to know about the models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelsView {
    /// Whether switchboard answered.
    pub switchboard_ok: bool,
    /// Every default model runs: the chat can be used.
    pub ready: bool,
    /// Chat-capable instances only.
    pub instances: Vec<ModelOption>,
    pub defaults: Vec<DefaultModelState>,
    pub search_providers: Vec<SearchProviderOption>,
    pub default_search_provider: String,
    /// The `accept` value of the file picker.
    pub accept: String,
}

/// The body of `POST /api/v1/turns`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendBody {
    pub instance_id: String,
    pub message: String,
    pub conversation_id: String,
    pub project_id: Option<String>,
    pub search_provider: Option<String>,
    /// The staged files, comma separated.
    pub file_ids: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnStarted {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegenerateBody {
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditBody {
    pub new_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchBody {
    pub target_message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewProject {
    pub name: String,
}

/// An id the service made: `GET /api/v1/conversation-id` (a conversation that is not stored yet), the new message of an edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Created {
    pub id: String,
}

/// The answer to opening a conversation whose last message is the person's: the turn that answers it, if one could start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resumed {
    pub id: Option<String>,
}
