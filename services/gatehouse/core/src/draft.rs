//! Forms that go through `fetch` (the others are plain form posts the service answers with a redirect).

use crate::model::MfaEnrollRequest;

/// Why a form was not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    /// The code box is empty.
    CodeRequired,
}

impl DraftError {
    /// The translation key of the message.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::CodeRequired => "ui_account_mfa_code_required",
        }
    }
}

/// The enrolment form: the code from the authenticator.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MfaDraft {
    pub code: String,
}

impl MfaDraft {
    /// The request for `secret`, or why the form cannot be sent.
    ///
    /// # Errors
    /// [`DraftError::CodeRequired`] when the code is blank.
    pub fn validate(&self, secret: &str) -> Result<MfaEnrollRequest, DraftError> {
        let code: String = self.code.chars().filter(|c| !c.is_whitespace()).collect();
        if code.is_empty() {
            return Err(DraftError::CodeRequired);
        }
        Ok(MfaEnrollRequest {
            secret: secret.to_string(),
            code,
        })
    }
}

/// The name of a chosen file for the picker's read-only box: a browser reports `C:\fakepath\me.png`.
#[must_use]
pub fn file_name(value: &str) -> &str {
    value.rsplit(['\\', '/']).next().unwrap_or(value)
}

/// One path segment, percent-encoded.
#[must_use]
pub fn segment(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The service's path prefix from the page's own path: `/gatehouse/ui/home` is `/gatehouse`, `/ui/home` is no prefix.
#[must_use]
pub fn base_of(path: &str) -> String {
    let at = path
        .find("/ui/")
        .or_else(|| path.strip_suffix("/ui").map(str::len));
    at.map_or_else(String::new, |i| path[..i].trim_end_matches('/').to_string())
}
