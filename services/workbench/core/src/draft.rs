//! A form's text, checked into the body the API wants.

use crate::model::{CreateIssue, CreateProject, NewComment, UpdateIssue};

/// Why a form is not ready to send. The message keys are the ones the old pages used for their errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    EmptyFields,
    TitleRequired,
    InvalidEstimate,
    CommentRequired,
    InvalidIssueKey,
}

impl DraftError {
    /// The text for a person, in English: operational, like the old banners, not run through the translations.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::EmptyFields => "a key and a name are required",
            Self::TitleRequired => "a title is required",
            Self::InvalidEstimate => "the estimate must be a whole number, 0 or more",
            Self::CommentRequired => "a comment cannot be empty",
            Self::InvalidIssueKey => "an issue key looks like WB-4",
        }
    }
}

/// Text to the optional field the API wants: `None` when there is only whitespace.
fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectDraft {
    pub key: String,
    pub name: String,
    pub description: String,
}

impl ProjectDraft {
    /// # Errors
    /// [`DraftError::EmptyFields`] when the key or the name is blank.
    pub fn validate(&self) -> Result<CreateProject, DraftError> {
        if self.key.trim().is_empty() || self.name.trim().is_empty() {
            return Err(DraftError::EmptyFields);
        }
        Ok(CreateProject {
            key: self.key.trim().to_string(),
            name: self.name.trim().to_string(),
            description: optional(&self.description),
        })
    }
}

/// The issue form, for a new issue (`Default`) and for an open one (`from_issue`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueDraft {
    pub title: String,
    pub description: String,
    pub kind: String,
    pub priority: String,
    pub assignee: String,
    pub estimate: String,
    pub status: String,
}

impl Default for IssueDraft {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            kind: "task".into(),
            priority: "medium".into(),
            assignee: String::new(),
            estimate: String::new(),
            status: "todo".into(),
        }
    }
}

impl IssueDraft {
    #[must_use]
    pub fn from_issue(issue: &crate::model::Issue) -> Self {
        Self {
            title: issue.title.clone(),
            description: issue.description.clone().unwrap_or_default(),
            kind: issue.kind.clone(),
            priority: issue.priority.clone(),
            assignee: issue.assignee.clone().unwrap_or_default(),
            estimate: issue.estimate.map(|e| e.to_string()).unwrap_or_default(),
            status: issue.status.clone(),
        }
    }

    fn estimate(&self) -> Result<Option<i32>, DraftError> {
        match self.estimate.trim() {
            "" => Ok(None),
            text => match text.parse::<i32>() {
                Ok(points) if points >= 0 => Ok(Some(points)),
                _ => Err(DraftError::InvalidEstimate),
            },
        }
    }

    /// The body that creates the issue.
    ///
    /// # Errors
    /// A blank title, or an estimate that is not a whole number of 0 or more.
    pub fn to_create(&self) -> Result<CreateIssue, DraftError> {
        if self.title.trim().is_empty() {
            return Err(DraftError::TitleRequired);
        }
        Ok(CreateIssue {
            kind: self.kind.clone(),
            title: self.title.trim().to_string(),
            description: optional(&self.description),
            priority: self.priority.clone(),
            assignee: optional(&self.assignee),
            estimate: self.estimate()?,
        })
    }

    /// The body that replaces the issue's fields (the status is a transition of its own).
    ///
    /// # Errors
    /// As [`IssueDraft::to_create`].
    pub fn to_update(&self) -> Result<UpdateIssue, DraftError> {
        let create = self.to_create()?;
        Ok(UpdateIssue {
            title: create.title,
            description: create.description,
            kind: create.kind,
            priority: create.priority,
            assignee: create.assignee,
            estimate: create.estimate,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentDraft {
    pub body: String,
}

impl CommentDraft {
    /// # Errors
    /// [`DraftError::CommentRequired`] when there is only whitespace.
    pub fn validate(&self) -> Result<NewComment, DraftError> {
        optional(&self.body)
            .map(|body| NewComment { body })
            .ok_or(DraftError::CommentRequired)
    }
}

/// The add-link form: an issue's key and the kind of link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkDraft {
    pub key: String,
    pub kind: String,
}

impl Default for LinkDraft {
    fn default() -> Self {
        Self {
            key: String::new(),
            kind: "blocks".into(),
        }
    }
}

impl LinkDraft {
    /// The key as the API's `?key=` wants it.
    ///
    /// # Errors
    /// [`DraftError::InvalidIssueKey`] when it is not `KEY-number`.
    pub fn target(&self) -> Result<String, DraftError> {
        split_issue_key(&self.key)
            .map(|(project, seq)| format!("{project}-{seq}"))
            .ok_or(DraftError::InvalidIssueKey)
    }
}

/// `"WB-4"` is `("WB", 4)`; splits from the right in case a key contains `-`.
#[must_use]
pub fn split_issue_key(key: &str) -> Option<(String, i32)> {
    let (project, seq) = key.trim().rsplit_once('-')?;
    let project = project.trim();
    if project.is_empty() {
        return None;
    }
    Some((project.to_string(), seq.trim().parse().ok()?))
}
