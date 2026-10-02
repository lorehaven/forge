//! Entry paths. A package is unpacked into a directory the caller chose, so a
//! path is only acceptable if it is relative and stays inside it.

use crate::error::PackageError;

/// Longest whole path accepted.
pub const MAX_PATH_LEN: usize = 512;
const MAX_COMPONENT_LEN: usize = 255;

/// Checks a `/`-separated relative path: no empty, `.` or `..` component, no
/// leading `/`, no backslash or control byte.
pub fn validate(path: &str) -> Result<(), PackageError> {
    let unsafe_path = |why| Err(PackageError::UnsafePath(path.to_string(), why));

    if path.is_empty() {
        return unsafe_path("empty");
    }
    if path.len() > MAX_PATH_LEN {
        return unsafe_path("too long");
    }
    if path.starts_with('/') {
        return unsafe_path("absolute");
    }
    if path
        .chars()
        .any(|c| c == '\\' || c == '\0' || c.is_control())
    {
        return unsafe_path("contains a backslash or control character");
    }
    for component in path.split('/') {
        match component {
            "" => return unsafe_path("empty component"),
            "." | ".." => return unsafe_path("relative component"),
            c if c.len() > MAX_COMPONENT_LEN => return unsafe_path("component too long"),
            _ => {}
        }
    }
    Ok(())
}
