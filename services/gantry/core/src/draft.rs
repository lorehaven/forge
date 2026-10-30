//! A form's text, checked into the body the API wants.

use crate::model::EditBody;
use crate::model::ResourceRef;

/// Why a form is not ready to send. The text is for a person, in English: operational, like the old banners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    EmptyYaml,
}

impl DraftError {
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::EmptyYaml => "the YAML is empty",
        }
    }
}

/// The editor's text for one resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditDraft {
    pub resource: ResourceRef,
    pub yaml: String,
}

impl EditDraft {
    /// # Errors
    /// [`DraftError::EmptyYaml`] when there is only whitespace.
    pub fn validate(&self) -> Result<EditBody, DraftError> {
        if self.yaml.trim().is_empty() {
            return Err(DraftError::EmptyYaml);
        }
        Ok(self.resource.edit_body(self.yaml.clone()))
    }
}
