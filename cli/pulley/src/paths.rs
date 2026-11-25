//! The two spellings of a local path on Windows.
//!
//! The rsync that Chocolatey installs (cwRsync, and any other Cygwin build) knows drive `C:` as `/cygdrive/c`, and reads
//! `C:\data` as the directory `\data` on a host called `C`. Windows, which pulley asks whether a source is there, knows
//! `C:\data` and not `/cygdrive/c`. A job may use either spelling; each side is given its own.

use std::borrow::Cow;

/// `path` as Windows itself reads it: `/cygdrive/c/data` is `C:/data`. Unchanged on other systems, and for any other path.
#[must_use]
pub fn native(path: &str) -> Cow<'_, str> {
    if cfg!(windows)
        && let Some(converted) = cygdrive_to_drive(path)
    {
        return Cow::Owned(converted);
    }
    Cow::Borrowed(path)
}

/// `path` as a Cygwin rsync reads it: `C:\data` is `/cygdrive/c/data`. Unchanged on other systems, and for any other path.
#[must_use]
pub fn for_rsync(path: &str) -> Cow<'_, str> {
    if cfg!(windows)
        && let Some(converted) = drive_to_cygdrive(path)
    {
        return Cow::Owned(converted);
    }
    Cow::Borrowed(path)
}

/// `/cygdrive/c/data` as `C:/data`, or `None` for a path that is not below a `/cygdrive` drive.
#[must_use]
pub fn cygdrive_to_drive(path: &str) -> Option<String> {
    let mut rest = path.strip_prefix("/cygdrive/")?.chars();
    let drive = rest.next().filter(char::is_ascii_alphabetic)?;
    let rest = rest.as_str();
    if !(rest.is_empty() || rest.starts_with('/')) {
        return None;
    }
    // `C:` alone is the current directory on that drive; the root needs its slash.
    let rest = if rest.is_empty() { "/" } else { rest };
    Some(format!("{}:{rest}", drive.to_ascii_uppercase()))
}

/// `C:\data` or `C:/data` as `/cygdrive/c/data`, or `None` for a path that does not start at a drive.
///
/// A drive is one letter and a colon, followed by a slash of either kind or by nothing: `c:data` stays what rsync takes it
/// for, a path on the host `c`.
#[must_use]
pub fn drive_to_cygdrive(path: &str) -> Option<String> {
    let mut chars = path.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    let rest = chars.as_str().strip_prefix(':')?;
    if !(rest.is_empty() || rest.starts_with(['/', '\\'])) {
        return None;
    }
    Some(format!(
        "/cygdrive/{}{}",
        drive.to_ascii_lowercase(),
        rest.replace('\\', "/")
    ))
}
