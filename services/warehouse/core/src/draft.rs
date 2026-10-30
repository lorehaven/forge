//! A form's text, checked into the body the API wants.

use crate::fmt::{gib_text, mib_text};
use crate::model::{DynamicStorage, NewStorage, StorageChanges};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

/// Why a form is not ready to send. The keys are the translations the old pages used for the same errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    InvalidStorageName,
    OwnerRequired,
    InvalidQuota,
    InvalidMaxFile,
}

impl DraftError {
    /// The translation key of the message.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::InvalidStorageName => "api_error_invalid_storage_name",
            Self::OwnerRequired => "api_error_storage_owner_required",
            Self::InvalidQuota => "api_error_invalid_quota",
            Self::InvalidMaxFile => "api_error_invalid_max_file",
        }
    }
}

/// Whether `name` is safe as a storage name (a URL path segment): letters, digits, `-` and `_`.
#[must_use]
pub fn valid_storage_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `raw` in units of `scale` bytes: blank is `None`, a non-negative number is bytes, anything else is an error.
#[allow(clippy::cast_possible_truncation)]
fn scaled(raw: &str, scale: f64) -> Result<Option<i64>, ()> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let value: f64 = raw.parse().map_err(|_| ())?;
    if !value.is_finite() || value < 0.0 {
        return Err(());
    }
    // A float to the nearest whole byte; `as` saturates.
    Ok(Some((value * scale).round() as i64))
}

/// The "new storage" form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewStorageDraft {
    pub name: String,
    pub owner: String,
    pub quota_gib: String,
    pub max_file_mib: String,
    pub sync: bool,
}

impl NewStorageDraft {
    /// # Errors
    /// The first thing wrong with the form.
    pub fn validate(&self) -> Result<NewStorage, DraftError> {
        let name = self.name.trim();
        if !valid_storage_name(name) {
            return Err(DraftError::InvalidStorageName);
        }
        let owner = self.owner.trim();
        if owner.is_empty() {
            return Err(DraftError::OwnerRequired);
        }
        let quota_bytes = scaled(&self.quota_gib, GIB).map_err(|()| DraftError::InvalidQuota)?;
        let max_file_bytes =
            scaled(&self.max_file_mib, MIB).map_err(|()| DraftError::InvalidMaxFile)?;
        Ok(NewStorage {
            name: name.to_string(),
            owner: owner.to_string(),
            quota_bytes,
            max_file_bytes,
            sync_enabled: self.sync,
        })
    }
}

/// The "edit storage" form, opened on a storage's current values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditStorageDraft {
    pub quota_gib: String,
    pub max_file_mib: String,
    pub clear_max_file: bool,
    pub sync: bool,
}

impl EditStorageDraft {
    #[must_use]
    pub fn from_storage(storage: &DynamicStorage) -> Self {
        Self {
            quota_gib: gib_text(storage.quota_bytes),
            max_file_mib: storage.max_file_bytes.map(mib_text).unwrap_or_default(),
            clear_max_file: false,
            sync: storage.sync_enabled,
        }
    }

    /// # Errors
    /// The first thing wrong with the form.
    pub fn validate(&self) -> Result<StorageChanges, DraftError> {
        let quota_bytes = scaled(&self.quota_gib, GIB).map_err(|()| DraftError::InvalidQuota)?;
        let max_file_bytes = if self.clear_max_file {
            None
        } else {
            scaled(&self.max_file_mib, MIB).map_err(|()| DraftError::InvalidMaxFile)?
        };
        Ok(StorageChanges {
            quota_bytes,
            max_file_bytes,
            clear_max_file: self.clear_max_file,
            sync_enabled: self.sync,
        })
    }
}
