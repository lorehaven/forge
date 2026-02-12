use super::utils::{clean_path, resolve_dir, resolve_parent_for_write};
use crate::core::Indexer;
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fs::{self, Metadata, create_dir_all};
use std::io::Write;
use std::path::Path;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

pub fn get_file_info(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;

    let meta: Metadata = full
        .metadata()
        .with_context(|| format!("Cannot get metadata for {path}"))?;

    let mut lines = vec![
        format!("Path: {path}"),
        format!("Exists: {}", full.exists()),
        format!("Is directory: {}", meta.is_dir()),
        format!("Is file: {}", meta.is_file()),
        format!("Size: {} bytes", meta.len()),
    ];

    if let Ok(modified) = meta.modified()
        && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
    {
        let secs = duration.as_secs();
        lines.push(format!("Last modified: {secs} (unix timestamp)"));
    }

    if meta.is_file()
        && full
            .extension()
            .is_some_and(|e| e == "rs" || e == "toml" || e == "md" || e == "txt")
        && let Ok(content) = std::fs::read_to_string(&full)
    {
        let line_count = content.lines().count();
        lines.push(format!("Line count: {line_count}"));
    }

    Ok(lines.join("\n"))
}

pub fn file_exists(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path);
    Ok(if full.is_ok() && full.unwrap().exists() {
        "true".to_string()
    } else {
        "false".to_string()
    })
}

pub fn list_files_recursive(cwd: &Path, args: &Value) -> Result<String> {
    let base = args["path"].as_str().unwrap_or(".");
    let ext_filter = args["extension"].as_str();

    let full = resolve_dir(cwd, &clean_path(base))?;

    let mut files = Vec::new();

    for entry in WalkDir::new(&full)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && ![".git", "target", "node_modules"].contains(&&*name)
        })
        .filter_map(std::result::Result::ok)
    {
        if entry.file_type().is_file() {
            if let Some(ext) = ext_filter {
                if let Some(file_ext) = entry.path().extension().and_then(|s| s.to_str()) {
                    if file_ext != ext {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            let rel = entry
                .path()
                .strip_prefix(cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()))
                .unwrap_or_else(|_| entry.path());
            files.push(rel.to_string_lossy().into_owned());
        }
    }

    files.sort();

    if files.is_empty() {
        Ok("No matching files found.".to_string())
    } else {
        Ok(format!(
            "Found {} files:\n{}",
            files.len(),
            files.join("\n")
        ))
    }
}

pub fn read_file(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;
    fs::read_to_string(full).map_err(Into::into)
}

pub fn read_multiple_files(cwd: &Path, args: &Value) -> Result<String> {
    let paths: Vec<String> = serde_json::from_value(args["paths"].clone())?;
    let mut results = Vec::new();
    for raw_path in paths {
        let path = clean_path(&raw_path);
        match resolve_dir(cwd, &path) {
            Ok(full) => {
                if let Ok(content) = fs::read_to_string(&full) {
                    results.push(format!("File: {path}\nContent:\n{content}\n---\n"));
                } else {
                    results.push(format!("Failed to read {path}: file not readable\n---\n"));
                }
            }
            Err(e) => {
                results.push(format!("Failed to resolve {path}: {e}\n---\n"));
            }
        }
    }
    if results.is_empty() {
        Ok("No files read.".to_string())
    } else {
        Ok(results.join(""))
    }
}

pub fn write_file(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let content: String = serde_json::from_value(args["content"].clone())?;
    let full_path = resolve_parent_for_write(cwd, &path)?;
    if let Some(parent) = full_path.parent() {
        create_dir_all(parent)?;
    }
    fs::write(&full_path, content)?;
    Ok(format!("Successfully wrote {path}"))
}

pub fn list_directory(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path = args["path"].as_str().unwrap_or(".");
    let path = clean_path(raw_path);
    let full = resolve_dir(cwd, &path)?;
    let mut entries = vec![];
    for e in fs::read_dir(full)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if e.file_type()?.is_dir() && ["target", ".git", "node_modules"].contains(&name.as_str()) {
            continue;
        }
        let suffix = if e.file_type()?.is_dir() { "/" } else { "" };
        entries.push(format!("{name}{suffix}"));
    }
    entries.sort();
    Ok(entries.join("\n"))
}

pub fn get_directory_tree(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path = args["path"].as_str().unwrap_or(".");
    let path = clean_path(raw_path);
    let full = resolve_dir(cwd, &path)?;
    let mut lines = vec![if path == "." { ".".to_string() } else { path }];

    let mut entries: Vec<_> = WalkDir::new(&full)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if name.starts_with('.') {
                return false;
            }
            if e.file_type().is_dir() && ["target", ".git", "node_modules"].contains(&&*name) {
                return false;
            }
            true
        })
        .filter_map(std::result::Result::ok)
        .collect();

    entries.sort_by_key(|e| e.path().to_path_buf());

    for entry in entries {
        let depth = entry.depth();
        let indent = "  ".repeat(depth);
        let name = entry.file_name().to_string_lossy();
        let suffix = if entry.file_type().is_dir() { "/" } else { "" };
        lines.push(format!("{indent}{name}{suffix}"));
    }
    Ok(lines.join("\n"))
}

pub fn create_directory(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full_path = resolve_parent_for_write(cwd, &path)?;

    if full_path.is_dir() {
        return Ok(format!("Directory already exists: {path}"));
    }
    if full_path.exists() {
        return Err(anyhow!(
            "Cannot create directory '{path}': path already exists and is a file"
        ));
    }

    create_dir_all(&full_path).with_context(|| format!("Failed to create directory '{path}'"))?;
    Ok(format!("Created directory: {path}"))
}

pub fn append_to_file(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let content: String = serde_json::from_value(args["content"].clone())?;
    let path = clean_path(&raw_path);
    let full_path = resolve_parent_for_write(cwd, &path)?;

    if let Some(parent) = full_path.parent() {
        create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&full_path)
        .with_context(|| format!("Cannot open/append to {path}"))?;

    writeln!(file, "{content}").with_context(|| format!("Failed to append to {path}"))?;
    Ok(format!("Appended {} bytes to {path}", content.len()))
}

pub fn replace_in_file(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let search: String = serde_json::from_value(args["search"].clone())?;
    let replace: String = serde_json::from_value(args["replace"].clone())?;

    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;

    // PRE-FLIGHT VALIDATION: Check if search string exists before attempting replacement
    super::validators::validate_search_exists(&full, &search)?;

    let old_content = fs::read_to_string(&full).context("Cannot read file for replacement")?;

    let new_content = old_content.replace(&search, &replace);
    if old_content == new_content {
        return Ok(format!(
            "No changes made in {path}. This usually means the 'search' string was not found exactly as provided."
        ));
    }

    fs::write(&full, &new_content)?;
    Ok(format!(
        "Replaced '{}' → '{}' in {}\n({} → {} bytes)",
        search.escape_default(),
        replace.escape_default(),
        path,
        old_content.len(),
        new_content.len()
    ))
}

pub fn search_text(cwd: &Path, args: &Value) -> Result<String> {
    let pattern: String = serde_json::from_value(args["pattern"].clone())?;
    let path = args["path"].as_str().unwrap_or(".").to_string();
    let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);

    let path_clean = clean_path(&path);
    let full_path = resolve_dir(cwd, &path_clean)?;

    let mut cmd = std::process::Command::new("grep");
    cmd.arg("-r")
        .arg("--color=never")
        .arg("-n")
        .arg("-I")
        .arg("-F")
        .arg(&pattern)
        .arg(&full_path);

    if !case_sensitive {
        cmd.arg("-i");
    }

    let output = cmd.output().context("grep command failed")?;

    let result = if output.status.success() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        String::from_utf8_lossy(&output.stderr).to_string()
    };

    if result.trim().is_empty() {
        Ok(format!("No matches found for '{pattern}'."))
    } else {
        Ok(format!("Search results:\n{result}"))
    }
}

pub fn find_file(cwd: &Path, args: &Value) -> Result<String> {
    let pattern: String = serde_json::from_value(args["pattern"].clone())?;
    let base = args["path"].as_str().unwrap_or(".");
    let full_base = resolve_dir(cwd, &clean_path(base))?;

    let mut matches = Vec::new();

    for entry in WalkDir::new(&full_base)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && ![".git", "target", "node_modules"].contains(&&*name)
        })
        .filter_map(std::result::Result::ok)
    {
        let name = entry.file_name().to_string_lossy();
        if name.contains(&pattern) {
            let rel = entry
                .path()
                .strip_prefix(cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()))
                .unwrap_or_else(|_| entry.path());
            matches.push(rel.to_string_lossy().into_owned());
        }
    }

    matches.sort();

    if matches.is_empty() {
        Ok(format!("No files found matching '{pattern}'."))
    } else {
        Ok(format!(
            "Found {} matches:\n{}",
            matches.len(),
            matches.join("\n")
        ))
    }
}

pub fn search_code_semantic(indexer: Option<&Indexer>, args: &Value) -> Result<String> {
    let query: String = serde_json::from_value(args["query"].clone())?;
    let limit = usize::try_from(args["limit"].as_u64().unwrap_or(5))?;

    let Some(indexer) = indexer else {
        return Ok("Indexing is disabled or failed to initialize.".to_string());
    };

    let results = indexer.search(&query, limit)?;

    if results.is_empty() {
        Ok(format!("No semantic matches found for '{query}'."))
    } else {
        use std::fmt::Write as _;
        let mut out = format!("Semantic search results for '{query}':\n\n");
        for (path, content) in results {
            let _ = writeln!(out, "--- {path} ---");
            let snippet: String = content.lines().take(30).collect::<Vec<_>>().join("\n");
            let _ = writeln!(out, "{snippet}");
            if content.lines().count() > 30 {
                let _ = writeln!(out, "\n...(truncated)");
            }
            let _ = writeln!(out, "\n");
        }
        Ok(out)
    }
}
