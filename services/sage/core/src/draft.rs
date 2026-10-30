//! What the person types, checked into what the API takes.

use crate::model::{NewProject, SendBody};

/// Why a form was not sent: an i18n key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftError(pub &'static str);

/// The longest a project name may be.
pub const PROJECT_NAME_MAX: usize = 120;

/// The first 30 characters of a text, with an ellipsis when it is longer: a conversation's title, a navigator tooltip.
#[must_use]
pub fn preview(text: &str) -> String {
    let text = text.trim();
    let head: String = text.chars().take(30).collect();
    if text.chars().count() > 30 {
        format!("{head}...")
    } else {
        head
    }
}

/// A message to send, with what it is sent with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageDraft {
    pub text: String,
    pub conversation_id: String,
    pub project_id: Option<String>,
    pub instance_id: String,
    pub search_provider: String,
    pub file_ids: Vec<String>,
}

impl MessageDraft {
    /// # Errors
    /// `ui_chat_error_empty` when there is nothing to say, `ui_chat_no_model_available` when no model is chosen.
    pub fn body(&self) -> Result<SendBody, DraftError> {
        let message = self.text.trim();
        if message.is_empty() {
            return Err(DraftError("ui_chat_error_empty"));
        }
        if self.instance_id.is_empty() {
            return Err(DraftError("ui_chat_no_model_available"));
        }
        Ok(SendBody {
            instance_id: self.instance_id.clone(),
            message: message.to_string(),
            conversation_id: self.conversation_id.clone(),
            project_id: self.project_id.clone(),
            search_provider: (!self.search_provider.is_empty())
                .then(|| self.search_provider.clone()),
            file_ids: self.file_ids.join(","),
        })
    }
}

/// The new-project dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectDraft {
    pub name: String,
}

impl ProjectDraft {
    /// # Errors
    /// `ui_projects_error_name` when the name is empty or too long.
    pub fn body(&self) -> Result<NewProject, DraftError> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > PROJECT_NAME_MAX {
            return Err(DraftError("ui_projects_error_name"));
        }
        Ok(NewProject {
            name: name.to_string(),
        })
    }
}

/// What the shell's upload hook reports for a file: its name, the status of the upload and the answer's body.
#[must_use]
pub fn upload_result(raw: &str) -> Option<(String, u16, String)> {
    #[derive(quench_serde::Deserialize)]
    struct Raw {
        name: String,
        status: u16,
        body: String,
    }
    quench_serde::json::from_str::<Raw>(raw)
        .ok()
        .map(|r| (r.name, r.status, r.body))
}

/// A size as the chips show it: `512 B`, `1.5 KB`, `3.2 MB`.
#[must_use]
pub fn human_bytes(size: i64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let size = size.max(0) as f64;
    if size < 1024.0 {
        format!("{size:.0} B")
    } else if size < 1024.0 * 1024.0 {
        format!("{:.1} KB", size / 1024.0)
    } else if size < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", size / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size / (1024.0 * 1024.0 * 1024.0))
    }
}

/// A file as the service answered an upload with it.
#[must_use]
pub fn parse_file(body: &str) -> Option<crate::FileInfo> {
    quench_serde::json::from_str(body).ok()
}
