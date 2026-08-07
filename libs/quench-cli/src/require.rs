//! A single up-front check for the external binaries a CLI is about to shell
//! out to, so a missing tool fails with one clear line instead of a raw OS
//! error however many steps later.

use std::fmt;

pub struct MissingBinary {
    name: String,
    hint: String,
}

impl fmt::Display for MissingBinary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` not found on PATH. {}", self.name, self.hint)
    }
}

// `main`'s default error path prints `{:?}`, not `{}` — this keeps that
// output the same one-line message rather than a derived struct dump.
impl fmt::Debug for MissingBinary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for MissingBinary {}

/// Fails with a clear, named error unless `name` resolves on `PATH`. `hint`
/// should say what needs it and, where there is one, how to install it.
pub fn require_binary(name: &str, hint: &str) -> Result<(), MissingBinary> {
    if which::which(name).is_ok() {
        Ok(())
    } else {
        Err(MissingBinary {
            name: name.to_string(),
            hint: hint.to_string(),
        })
    }
}
