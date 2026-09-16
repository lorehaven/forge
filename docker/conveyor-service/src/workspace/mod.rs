//! The directory a run happens in.

pub mod checkout;

pub use checkout::{CheckoutError, CheckoutRequest, HttpCredential, checkout};

use std::path::{Path, PathBuf};

/// A checkout on local disk, owned by one run. Cleanup is [`Workspace::remove`],
/// not `Drop` - removal can fail and is worth awaiting.
#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Wraps an existing directory. Normally produced by [`checkout`]; public
    /// so a test can point an executor at a directory it prepared itself.
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Where the repository is checked out. Steps run with this as their
    /// working directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves a pipeline-named path, refusing anything outside the checkout
    /// (symlinks followed first, so a planted link can't escape it).
    pub fn resolve(&self, relative: &str) -> Option<PathBuf> {
        let candidate = self.root.join(relative);

        // Canonicalize only the part that exists - `canonicalize` on a missing file errors.
        let (existing, remainder) = split_at_existing(&candidate);
        let base = existing.canonicalize().ok()?;
        let root = self.root.canonicalize().ok()?;

        // Only join when non-empty - joining an empty remainder adds a trailing
        // separator, making an existing file look missing (ENOTDIR).
        let resolved = if remainder.as_os_str().is_empty() {
            base
        } else {
            base.join(remainder)
        };

        resolved.starts_with(&root).then_some(resolved)
    }

    /// Removes the checkout. Called when the run finishes, however it finished.
    pub async fn remove(self) -> std::io::Result<()> {
        tokio::fs::remove_dir_all(&self.root).await
    }
}

/// Splits `path` into its longest existing ancestor and the rest.
fn split_at_existing(path: &Path) -> (PathBuf, PathBuf) {
    let mut existing = path.to_path_buf();
    let mut remainder = Vec::new();

    while !existing.exists() {
        let Some(name) = existing.file_name().map(std::ffi::OsString::from) else {
            break;
        };
        remainder.push(name);
        if !existing.pop() {
            break;
        }
    }

    let mut tail = PathBuf::new();
    for name in remainder.iter().rev() {
        tail.push(name);
    }
    (existing, tail)
}
