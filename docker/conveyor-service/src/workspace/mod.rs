//! The directory a run happens in.

pub mod checkout;

pub use checkout::{CheckoutError, CheckoutRequest, checkout};

use std::path::{Path, PathBuf};

/// A checkout on local disk, owned by one run.
///
/// Cleanup is [`Workspace::remove`] rather than `Drop`: removing a directory
/// tree can fail and takes long enough to be worth awaiting, and a `Drop` that
/// can do neither would swallow both.
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

    /// Resolves a path the pipeline named, refusing anything outside the
    /// checkout.
    ///
    /// A pipeline can say `artifacts = ["../../etc/passwd"]`, and collecting it
    /// would hand a repository author whatever the service account can read.
    /// Symlinks are followed before the check, so a link planted in the
    /// repository does not get around it.
    pub fn resolve(&self, relative: &str) -> Option<PathBuf> {
        let candidate = self.root.join(relative);

        // The path may not exist yet, so canonicalize what does exist and keep
        // the rest: `canonicalize` on a missing file is an error, not a verdict.
        let (existing, remainder) = split_at_existing(&candidate);
        let base = existing.canonicalize().ok()?;
        let root = self.root.canonicalize().ok()?;

        // `join` on an empty remainder appends a trailing separator, and
        // `stat("/a/file/")` is ENOTDIR - so a path that fully exists would
        // come back looking like it did not. Only join when there is something
        // left to join.
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
