//! A form's text, checked into the body the API wants.

use crate::model::{RegisterRepo, TriggerRun, UpdateRepo};

/// Why a form is not ready to send; `key` is the i18n key of the message (the ones the old pages used).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    OwnerNameEmpty,
    ProjectRequired,
    BadCloneUrl,
    NoStage,
}

impl DraftError {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::OwnerNameEmpty => "ui_repos_err_owner_name_empty",
            Self::ProjectRequired => "ui_repos_err_project_required",
            Self::BadCloneUrl => "ui_repos_err_bad_clone_url",
            Self::NoStage => "ui_launch_err_none",
        }
    }
}

fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The repository form, for a new repository (`Default`) and for an open one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoDraft {
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    pub default_branch: String,
    pub project_id: String,
    pub provider: String,
    pub enabled: bool,
}

impl Default for RepoDraft {
    fn default() -> Self {
        Self {
            owner: String::new(),
            name: String::new(),
            clone_url: String::new(),
            default_branch: String::new(),
            project_id: String::new(),
            provider: "github".to_string(),
            enabled: true,
        }
    }
}

impl RepoDraft {
    fn check(&self) -> Result<(), DraftError> {
        if self.owner.trim().is_empty() || self.name.trim().is_empty() {
            return Err(DraftError::OwnerNameEmpty);
        }
        if self.project_id.trim().is_empty() {
            return Err(DraftError::ProjectRequired);
        }
        if self.clone_url.trim().is_empty() {
            return Err(DraftError::BadCloneUrl);
        }
        Ok(())
    }

    /// # Errors
    /// What is missing: owner and name, the project, the clone URL.
    pub fn to_register(&self) -> Result<RegisterRepo, DraftError> {
        self.check()?;
        Ok(RegisterRepo {
            provider: optional(&self.provider),
            owner: self.owner.trim().to_string(),
            name: self.name.trim().to_string(),
            clone_url: self.clone_url.trim().to_string(),
            default_branch: optional(&self.default_branch),
            project_id: self.project_id.trim().to_string(),
        })
    }

    /// # Errors
    /// What is missing: owner and name, the project, the clone URL.
    pub fn to_update(&self) -> Result<UpdateRepo, DraftError> {
        self.check()?;
        Ok(UpdateRepo {
            owner: Some(self.owner.trim().to_string()),
            name: Some(self.name.trim().to_string()),
            clone_url: Some(self.clone_url.trim().to_string()),
            default_branch: optional(&self.default_branch),
            project_id: Some(self.project_id.trim().to_string()),
            enabled: Some(self.enabled),
        })
    }
}

/// The "run with parameters" form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchDraft {
    pub git_ref: String,
    /// The ticked stages.
    pub ticked: Vec<String>,
    /// Stage names typed by hand, comma separated.
    pub extra: String,
}

impl LaunchDraft {
    /// Every ticked box, then the typed names, trimmed and without repeats.
    #[must_use]
    pub fn stages(&self) -> Vec<String> {
        let mut chosen: Vec<String> = Vec::new();
        let typed = self.extra.split(',').map(str::to_string);
        for name in self.ticked.iter().cloned().chain(typed) {
            let name = name.trim();
            if !name.is_empty() && !chosen.iter().any(|c| c == name) {
                chosen.push(name.to_string());
            }
        }
        chosen
    }

    /// A bare branch name is what people type; the run records the full ref.
    #[must_use]
    pub fn full_ref(&self) -> Option<String> {
        optional(&self.git_ref).map(|value| {
            if value.starts_with("refs/") {
                value
            } else {
                format!("refs/heads/{value}")
            }
        })
    }

    /// # Errors
    /// [`DraftError::NoStage`] when no stage is ticked or typed.
    pub fn to_trigger(&self) -> Result<TriggerRun, DraftError> {
        let stages = self.stages();
        if stages.is_empty() {
            return Err(DraftError::NoStage);
        }
        Ok(TriggerRun {
            git_ref: self.full_ref(),
            sha: None,
            stages: Some(stages),
        })
    }
}
