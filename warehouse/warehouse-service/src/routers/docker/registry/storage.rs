use crate::routers::DOCKER_STORAGE_ROOT;
use crate::routers::docker::repository_path;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagListError {
    InvalidName,
    NotFound,
}

pub(crate) fn list_repositories() -> Vec<String> {
    let mut repos = Vec::new();
    let root = PathBuf::from(DOCKER_STORAGE_ROOT.as_str());
    collect_repositories(&root, "", &mut repos);
    repos.sort();
    repos
}

pub(crate) fn list_tags_for_repository(name: &str) -> Result<Vec<String>, TagListError> {
    let Some(repo_root) = repository_path(name) else {
        return Err(TagListError::InvalidName);
    };

    let repo_path = repo_root.join("tags");
    if !repo_path.exists() {
        return Err(TagListError::NotFound);
    }

    let mut tags = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&repo_path) {
        for entry in entries.flatten() {
            if let Some(tag) = entry.file_name().to_str() {
                tags.push(tag.to_string());
            }
        }
    }

    tags.sort_by(|a, b| compare_tags_desc(a, b));
    Ok(tags)
}

#[derive(Debug, Clone)]
pub(crate) struct TagMetadata {
    pub tag: String,
    pub digest: String,
    pub media_type: Option<String>,
    pub size_bytes: Option<u64>,
}

pub(crate) fn list_tag_metadata_for_repository(
    name: &str,
) -> Result<Vec<TagMetadata>, TagListError> {
    let Some(repo_root) = repository_path(name) else {
        return Err(TagListError::InvalidName);
    };

    let tags_path = repo_root.join("tags");
    if !tags_path.exists() {
        return Err(TagListError::NotFound);
    }

    let manifests_root = PathBuf::from(DOCKER_STORAGE_ROOT.as_str())
        .join("manifests")
        .join("sha256");

    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&tags_path) {
        for entry in entries.flatten() {
            let Some(tag) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };

            let digest = match std::fs::read_to_string(entry.path()) {
                Ok(s) => s.trim().to_string(),
                Err(_) => String::new(),
            };

            let mut media_type = None;
            let mut size_bytes = None;
            if let Some(hex) = digest.strip_prefix("sha256:") {
                let manifest_path = manifests_root.join(hex);
                if let Ok(bytes) = std::fs::read(&manifest_path) {
                    size_bytes = Some(bytes.len() as u64);
                    media_type = detect_media_type(&bytes);
                }
            }

            items.push(TagMetadata {
                tag,
                digest,
                media_type,
                size_bytes,
            });
        }
    }

    items.sort_by(|a, b| compare_tags_desc(&a.tag, &b.tag));
    Ok(items)
}

fn collect_repositories(dir: &Path, prefix: &str, repos: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            if prefix.is_empty()
                && matches!(
                    entry.file_name().to_str(),
                    Some("blobs" | "manifests" | "_uploads")
                )
            {
                continue;
            }

            let name = entry.file_name();
            let name = name.to_string_lossy();
            let repo_name = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{}/{}", prefix, name)
            };

            if path.join("tags").exists() {
                repos.push(repo_name.clone());
            }

            collect_repositories(&path, &repo_name, repos);
        }
    }
}

fn detect_media_type(bytes: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    value
        .get("mediaType")
        .and_then(|m| m.as_str())
        .map(str::to_string)
}

fn compare_tags_desc(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_version_tag(a), parse_version_tag(b)) {
        (Some(va), Some(vb)) => compare_version_tags_desc(&va, &vb),
        _ => b.cmp(a),
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct VersionTag {
    major: u64,
    minor: u64,
    patch: u64,
    suffix: Option<String>,
}

fn parse_version_tag(tag: &str) -> Option<VersionTag> {
    let (version, suffix) = match tag.split_once('-') {
        Some((version, suffix)) => (version, Some(suffix.to_string())),
        None => (tag, None),
    };

    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }

    Some(VersionTag {
        major,
        minor,
        patch,
        suffix,
    })
}

fn compare_version_tags_desc(a: &VersionTag, b: &VersionTag) -> std::cmp::Ordering {
    b.major
        .cmp(&a.major)
        .then_with(|| b.minor.cmp(&a.minor))
        .then_with(|| b.patch.cmp(&a.patch))
        .then_with(|| match (&a.suffix, &b.suffix) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a_suffix), Some(b_suffix)) => b_suffix.cmp(a_suffix),
        })
}
