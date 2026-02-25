use crate::routers::CRATES_STORAGE_ROOT;
use crate::routers::crates::validate_crate_name;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// A single version entry as stored in the sparse index file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecord {
    pub name: String,
    pub vers: String,
    pub deps: Vec<IndexDep>,
    pub cksum: String,
    pub features: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub features2: Option<HashMap<String, Vec<String>>>,
    pub yanked: bool,
    #[serde(default)]
    pub links: Option<String>,
    #[serde(default)]
    pub rust_version: Option<String>,
    #[serde(default = "default_v")]
    pub v: u8,
}

fn default_v() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDep {
    pub name: String,
    pub req: String,
    pub features: Vec<String>,
    pub optional: bool,
    pub default_features: bool,
    pub target: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub registry: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
}

// ---------------------------------------------------------------------------
// Listing helpers
// ---------------------------------------------------------------------------

/// Returns a sorted list of all crate names that have an index file.
pub fn list_crates() -> Vec<String> {
    let root = PathBuf::from(CRATES_STORAGE_ROOT.as_str()).join("index");
    let mut names: Vec<String> = Vec::new();

    // The index tree is <root>/index/<prefix>/<name> — we walk recursively and
    // collect leaf files (which are the index files, one per crate name).
    collect_index_names(&root, &mut names);
    names.sort();
    names
}

fn collect_index_names(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_index_names(&path, out);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && validate_crate_name(name)
        {
            out.push(name.to_string());
        }
    }
}

/// Reads all version records for a crate from its index file.
/// Returns them in published order (oldest first, as written to the file).
pub fn list_versions(crate_name: &str) -> Vec<IndexRecord> {
    let Some(path) = crate::routers::crates::index_file_path(crate_name) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}
