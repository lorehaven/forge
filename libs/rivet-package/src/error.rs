//! What can be wrong with a package.

/// Everything that makes a manifest or an archive unacceptable.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// `rivet.toml` is malformed or breaks a rule.
    #[error("invalid manifest: {0}")]
    Manifest(String),
    /// The archive is not a well-formed `tar.zst` of regular files.
    #[error("invalid archive: {0}")]
    Archive(String),
    /// An entry path could escape the package root or is otherwise unsafe.
    #[error("unsafe path `{0}`: {1}")]
    UnsafePath(String, &'static str),
    /// The archive is bigger than the limits allow.
    #[error("archive exceeds limits: {0}")]
    Limit(String),
    /// A file every package needs is absent.
    #[error("package is missing `{0}`")]
    Missing(&'static str),
    /// `SHA256SUMS` disagrees with the archive's contents.
    #[error("checksum mismatch: {0}")]
    Checksum(String),
    /// Reading or writing failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
