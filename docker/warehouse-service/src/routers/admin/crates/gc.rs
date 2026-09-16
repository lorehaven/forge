//! Garbage collection for the crates registry storage: deletes yanked/orphaned tarballs and
//! their empty dirs, repairs index entries pointing at missing tarballs, drops orphan owners.json.

use crate::routers::crates::{crate_file_path, validate_crate_name, validate_version};
use crate::routers::crates_storage_root;
use quench_http::prelude::{Response, http::StatusCode, post};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize)]
pub struct CratesGcReport {
    pub deleted_crates: usize,
    pub kept_crates: usize,
    pub removed_index_entries: usize,
    pub deleted_owner_files: usize,
    pub removed_empty_dirs: usize,
}

#[post("/admin/crates/gc")]
pub async fn handle() -> Response {
    match garbage_collect().await {
        Ok(report) => Response::json(StatusCode::OK, &report)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => {
            tracing::error!("crates GC failed: {e}");
            Response::new(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn garbage_collect() -> std::io::Result<CratesGcReport> {
    let root = PathBuf::from(crates_storage_root());
    let mut report = CratesGcReport::default();

    let mut crate_dirs = tokio::fs::read_dir(&root).await?;

    while let Some(entry) = crate_dirs.next_entry().await? {
        let crate_dir = entry.path();
        let file_type = entry.file_type().await?;

        if !file_type.is_dir() {
            continue;
        }

        let crate_name = entry.file_name().to_string_lossy().to_ascii_lowercase();

        if crate_name == "index" {
            continue;
        }

        if !validate_crate_name(&crate_name) {
            continue;
        }

        let (indexed_versions, yanked_versions, index_path, index_lines) =
            read_index_state(&crate_name).await;

        let mut version_dirs = tokio::fs::read_dir(&crate_dir).await?;

        while let Some(v_entry) = version_dirs.next_entry().await? {
            let v_path = v_entry.path();
            let v_type = v_entry.file_type().await?;

            if !v_type.is_dir() {
                continue;
            }

            let version = v_entry.file_name().to_string_lossy().into_owned();
            if !validate_version(&version) {
                continue;
            }

            let Some(tarball) = crate_file_path(&crate_name, &version) else {
                continue;
            };

            let tarball_exists = tokio::fs::metadata(&tarball).await.is_ok();

            if !tarball_exists {
                try_remove_empty_dir(&v_path, &mut report).await;
                continue;
            }

            let should_delete =
                yanked_versions.contains(&version) || !indexed_versions.contains(&version);

            if should_delete {
                if tokio::fs::remove_file(&tarball).await.is_ok() {
                    report.deleted_crates += 1;
                    tracing::debug!(
                        "GC: deleted {crate_name}-{version}.crate (yanked={})",
                        yanked_versions.contains(&version)
                    );
                }
                try_remove_empty_dir(&v_path, &mut report).await;
            } else {
                report.kept_crates += 1;
            }
        }

        if let Some(path) = &index_path {
            report.removed_index_entries += repair_index(path, &index_lines, &crate_name).await;
        }

        if index_path.is_none() {
            let owners_file = crate_dir.join("owners.json");
            if tokio::fs::metadata(&owners_file).await.is_ok()
                && tokio::fs::remove_file(&owners_file).await.is_ok()
            {
                report.deleted_owner_files += 1;
                tracing::debug!("GC: deleted orphaned owners.json for {crate_name}");
            }
            try_remove_empty_dir(&crate_dir, &mut report).await;
        }
    }

    Ok(report)
}

/// Indexed versions, yanked versions, the index file path (if any), and its raw lines.
async fn read_index_state(
    crate_name: &str,
) -> (
    HashSet<String>,
    HashSet<String>,
    Option<PathBuf>,
    Vec<String>,
) {
    let Some(path) = crate::routers::crates::index_file_path(crate_name) else {
        return (HashSet::new(), HashSet::new(), None, Vec::new());
    };

    let content = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(_) => return (HashSet::new(), HashSet::new(), None, Vec::new()),
    };

    let mut indexed = HashSet::new();
    let mut yanked = HashSet::new();
    let mut lines: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        lines.push(trimmed.to_string());

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed)
            && let Some(vers) = v.get("vers").and_then(|x| x.as_str())
        {
            indexed.insert(vers.to_string());
            if v.get("yanked").and_then(|y| y.as_bool()).unwrap_or(false) {
                yanked.insert(vers.to_string());
            }
        }
    }

    (indexed, yanked, Some(path), lines)
}

/// Removes index entries whose `.crate` tarball no longer exists; returns the count removed.
async fn repair_index(index_path: &Path, lines: &[String], crate_name: &str) -> usize {
    let mut removed = 0usize;
    let mut kept_lines: Vec<&str> = Vec::new();

    for line in lines {
        let keep = if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            match v.get("vers").and_then(|x| x.as_str()) {
                Some(vers) => crate_file_path(crate_name, vers)
                    .map(|p| std::path::Path::new(&p).exists())
                    .unwrap_or(false),
                None => true, // can't parse version, preserve to be safe
            }
        } else {
            true // malformed line, preserve
        };

        if keep {
            kept_lines.push(line.as_str());
        } else {
            removed += 1;
            tracing::debug!("GC: removing index entry for {crate_name} from {index_path:?}");
        }
    }

    if removed > 0 {
        let new_content = kept_lines.join("\n") + if kept_lines.is_empty() { "" } else { "\n" };
        if let Err(e) = tokio::fs::write(index_path, new_content.as_bytes()).await {
            tracing::error!("GC: failed to rewrite index {index_path:?}: {e}");
        }
    }

    removed
}

/// Removes `dir` if it is empty, incrementing the report counter on success.
async fn try_remove_empty_dir(dir: &Path, report: &mut CratesGcReport) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    if entries.next_entry().await.ok().flatten().is_none()
        && tokio::fs::remove_dir(dir).await.is_ok()
    {
        report.removed_empty_dirs += 1;
        tracing::debug!("GC: removed empty dir {dir:?}");
    }
}

pub fn register_routes() {
    let _ = handle as fn() -> _;
}
