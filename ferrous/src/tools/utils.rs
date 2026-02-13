use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

#[must_use]
pub fn clean_path(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

pub fn resolve_dir(cwd: &Path, input: &str) -> Result<PathBuf> {
    let cwd_canonical = cwd.canonicalize().context("Failed to canonicalize CWD")?;
    let full = resolve_existing_path(&cwd_canonical, input)
        .ok_or_else(|| anyhow!("Invalid path: {input}"))?;

    if !full.starts_with(&cwd_canonical) {
        return Err(anyhow!("Path traversal attempt"));
    }
    Ok(full)
}

fn resolve_existing_path(cwd_canonical: &Path, input: &str) -> Option<PathBuf> {
    let direct = cwd_canonical.join(input);
    if let Ok(path) = direct.canonicalize() {
        return Some(path);
    }

    // Fallback for workspace layouts: if caller passes "src/foo.rs" from repo root,
    // try "<module>/src/foo.rs" and select it only when unambiguous.
    let mut candidates = Vec::new();
    let Ok(entries) = std::fs::read_dir(cwd_canonical) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let nested = path.join(input);
        if let Ok(canonical) = nested.canonicalize() {
            candidates.push(canonical);
        }
    }

    if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        None
    }
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
