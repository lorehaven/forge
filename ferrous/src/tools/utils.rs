use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

#[must_use]
pub fn clean_path(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

pub fn resolve_dir(cwd: &Path, input: &str) -> Result<PathBuf> {
    let cwd_canonical = cwd.canonicalize().context("Failed to canonicalize CWD")?;
    let Ok(full) = cwd_canonical.join(input).canonicalize() else {
        return Err(anyhow!("Invalid path: {input}"));
    };

    if !full.starts_with(&cwd_canonical) {
        return Err(anyhow!("Path traversal attempt"));
    }
    Ok(full)
}

pub fn resolve_parent_for_write(cwd: &Path, input: &str) -> Result<PathBuf> {
    let cwd_canonical = cwd.canonicalize().context("Failed to canonicalize CWD")?;
    let path = cwd_canonical.join(input);
    let full_parent = path.parent().map_or_else(
        || cwd_canonical.clone(),
        |p| p.canonicalize().unwrap_or_else(|_| cwd_canonical.clone()),
    );
    if !full_parent.starts_with(&cwd_canonical) {
        return Err(anyhow!("Path traversal attempt"));
    }
    Ok(path)
}
