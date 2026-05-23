//! Garbage collection for the crates registry storage.
//!
//! ## What counts as garbage
//!
//! | Category | Action |
//! |---|---|
//! | `.crate` tarball whose index entry is **yanked** | Deleted |
//! | `.crate` tarball with **no index entry** at all (orphan) | Deleted |
//! | Version sub-directory that is now empty after tarball removal | Removed |
//! | Index entry that references a **missing** `.crate` file | Entry removed from index (index rebuilt) |
//! | `owners.json` whose parent crate directory has no index file | Deleted |
//!
//! Blobs that are still referenced by a non-yanked index entry are kept.
//! The index files themselves are never deleted; they are only repaired when
//! they contain entries pointing to missing tarballs.

use crate::routers::CRATES_STORAGE_ROOT;
use crate::routers::crates::{crate_file_path, validate_crate_name, validate_version};
use actix_web::{HttpResponse, Responder, post};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Response type
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, ToSchema)]
pub struct CratesGcReport {
    /// Number of `.crate` tarballs deleted (yanked or orphaned)
    pub deleted_crates: usize,
    /// Number of `.crate` tarballs kept
    pub kept_crates: usize,
    /// Number of index entries removed because their tarball was missing
    pub removed_index_entries: usize,
    /// Number of orphaned `owners.json` files deleted
    pub deleted_owner_files: usize,
    /// Number of empty directories removed
    pub removed_empty_dirs: usize,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/crates/gc",
    operation_id = "run_crates_garbage_collection",
    tags = ["admin"],
    responses(
        (status = 200, description = "Garbage collection completed", body = CratesGcReport, content_type = "application/json"),
        (status = 500, description = "GC failure"),
    )
)]
#[post("/crates/gc")]
pub async fn handle() -> impl Responder {
    match garbage_collect().await {
        Ok(report) => HttpResponse::Ok().json(report),
        Err(e) => {
            tracing::error!("crates GC failed: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

// ---------------------------------------------------------------------------
// Core GC logic
// ---------------------------------------------------------------------------

async fn garbage_collect() -> std::io::Result<CratesGcReport> {
    let root = PathBuf::from(CRATES_STORAGE_ROOT.as_str());
    let mut report = CratesGcReport::default();

    // Iterate over each crate directory  (<root>/<crate-name>/)
    let mut crate_dirs = tokio::fs::read_dir(&root).await?;

    while let Some(entry) = crate_dirs.next_entry().await? {
        let crate_dir = entry.path();
        let file_type = entry.file_type().await?;

        if !file_type.is_dir() {
            continue;
        }

        let crate_name = entry.file_name().to_string_lossy().to_ascii_lowercase();

        // Skip the index sub-tree entirely
        if crate_name == "index" {
            continue;
        }

        if !validate_crate_name(&crate_name) {
            continue;
        }

        // ------------------------------------------------------------------
        // 1. Read the index file to learn which versions exist and which are
        //    yanked.  Build two sets:
        //      • indexed_versions  – every version mentioned in the index
        //      • yanked_versions   – versions whose entry has yanked=true
        // ------------------------------------------------------------------
        let (indexed_versions, yanked_versions, index_path, index_lines) =
            read_index_state(&crate_name).await;

        // ------------------------------------------------------------------
        // 2. Walk the version sub-directories and decide fate of each tarball
        // ------------------------------------------------------------------
        let mut version_dirs = tokio::fs::read_dir(&crate_dir).await?;

        while let Some(v_entry) = version_dirs.next_entry().await? {
            let v_path = v_entry.path();
            let v_type = v_entry.file_type().await?;

            if !v_type.is_dir() {
                // owners.json or other metadata files – handled separately below
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
                // Nothing to delete; the version directory might still be empty
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

        // ------------------------------------------------------------------
        // 3. Repair the index: remove entries whose tarball is now gone
        // ------------------------------------------------------------------
        if let Some(path) = &index_path {
            report.removed_index_entries += repair_index(path, &index_lines, &crate_name).await;
        }

        // ------------------------------------------------------------------
        // 4. Orphaned owners.json: crate dir exists but no index file at all
        // ------------------------------------------------------------------
        if index_path.is_none() {
            let owners_file = crate_dir.join("owners.json");
            if tokio::fs::metadata(&owners_file).await.is_ok()
                && tokio::fs::remove_file(&owners_file).await.is_ok()
            {
                report.deleted_owner_files += 1;
                tracing::debug!("GC: deleted orphaned owners.json for {crate_name}");
            }
            // The crate dir itself may now be empty
            try_remove_empty_dir(&crate_dir, &mut report).await;
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Index helpers
// ---------------------------------------------------------------------------

/// Reads the index file for a crate and returns:
/// - set of all indexed versions
/// - set of yanked versions
/// - path to the index file (None if it doesn't exist)
/// - raw lines (for later repair pass)
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

/// Removes index entries whose `.crate` tarball no longer exists on disk.
/// Returns the number of entries removed.
async fn repair_index(index_path: &Path, lines: &[String], crate_name: &str) -> usize {
    let mut removed = 0usize;
    let mut kept_lines: Vec<&str> = Vec::new();

    for line in lines {
        let keep = if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            match v.get("vers").and_then(|x| x.as_str()) {
                Some(vers) => {
                    // Keep the entry if the tarball exists
                    crate_file_path(crate_name, vers)
                        .map(|p| std::path::Path::new(&p).exists())
                        .unwrap_or(false)
                }
                None => true, // can't parse version → preserve to be safe
            }
        } else {
            true // malformed line → preserve
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

// ---------------------------------------------------------------------------
// Directory helpers
// ---------------------------------------------------------------------------

/// Removes `dir` if it is empty, incrementing the report counter on success.
async fn try_remove_empty_dir(dir: &Path, report: &mut CratesGcReport) {
    // A directory is "empty" if it has no entries at all, or only empty
    // sub-directories (we only do one level here – version dirs are shallow).
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
