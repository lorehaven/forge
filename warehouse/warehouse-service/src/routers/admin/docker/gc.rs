use crate::routers::DOCKER_STORAGE_ROOT;
use actix_web::{HttpResponse, Responder, post};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
struct GcReport {
    deleted: usize,
    kept: usize,
}

#[utoipa::path(
    post,
    path = "/docker/gc",
    operation_id = "run_garbage_collection",
    tags = ["admin"],
    responses(
        (status = 200, description = "Garbage collection completed", body = GcReport),
        (status = 500, description = "GC failure")
    )
)]
#[post("/docker/gc")]
pub async fn handle() -> impl Responder {
    match garbage_collect().await {
        Ok(report) => HttpResponse::Ok().json(report),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

async fn garbage_collect() -> std::io::Result<GcReport> {
    let root = PathBuf::from(DOCKER_STORAGE_ROOT.as_str());

    let manifest_entries = collect_digest_files(&root, "manifests").await?;
    let blob_entries = collect_digest_files(&root, "blobs").await?;

    let manifest_paths: HashMap<String, PathBuf> = manifest_entries.into_iter().collect();
    let mut referenced_blobs = HashSet::new();
    let mut to_visit: VecDeque<String> = manifest_paths.keys().cloned().collect();
    let mut visited = HashSet::new();

    while let Some(digest) = to_visit.pop_front() {
        if !visited.insert(digest.clone()) {
            continue;
        }

        let Some(path) = manifest_paths.get(&digest) else {
            continue;
        };

        let data = tokio::fs::read(path).await?;
        mark_manifest_references(&data, &mut referenced_blobs, &mut to_visit);
    }

    let mut deleted = 0usize;
    let mut kept = 0usize;

    for (digest, path) in blob_entries {
        if referenced_blobs.contains(&digest) {
            kept += 1;
        } else {
            tokio::fs::remove_file(path).await?;
            deleted += 1;
        }
    }

    Ok(GcReport { deleted, kept })
}

async fn collect_digest_files(root: &Path, kind: &str) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if !is_digest_file_path(&path, kind) {
                continue;
            }

            let Some(hex) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            if !is_sha256_hex(hex) {
                continue;
            }

            files.push((format!("sha256:{hex}"), path));
        }
    }

    Ok(files)
}

fn is_digest_file_path(path: &Path, kind: &str) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(sha_dir) = parent.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(kind_dir) = parent
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    else {
        return false;
    };

    sha_dir == "sha256" && kind_dir == kind
}

fn is_sha256_hex(v: &str) -> bool {
    v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())
}

fn mark_manifest_references(
    manifest_data: &[u8],
    referenced_blobs: &mut HashSet<String>,
    manifests_to_visit: &mut VecDeque<String>,
) {
    let Ok(v) = serde_json::from_slice::<Value>(manifest_data) else {
        return;
    };

    if let Some(d) = v
        .get("config")
        .and_then(|c| c.get("digest"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
    {
        referenced_blobs.insert(d);
    }

    for key in ["layers", "blobs"] {
        if let Some(items) = v.get(key).and_then(|m| m.as_array()) {
            for item in items {
                if let Some(d) = item.get("digest").and_then(|x| x.as_str()) {
                    referenced_blobs.insert(d.to_string());
                }
            }
        }
    }

    if let Some(subject) = v.get("subject")
        && let Some(d) = subject.get("digest").and_then(|x| x.as_str())
    {
        manifests_to_visit.push_back(d.to_string());
    }

    if let Some(manifests) = v.get("manifests").and_then(|m| m.as_array()) {
        for m in manifests {
            let Some(d) = m.get("digest").and_then(|x| x.as_str()) else {
                continue;
            };

            // In indexes, this points to child manifests; treat unknown media types as manifest refs.
            let media_type = m.get("mediaType").and_then(|x| x.as_str()).unwrap_or("");
            if media_type.contains("manifest")
                || media_type.contains("index")
                || media_type.is_empty()
            {
                manifests_to_visit.push_back(d.to_string());
            } else {
                referenced_blobs.insert(d.to_string());
            }
        }
    }
}
