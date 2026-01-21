use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fs::{self, create_dir_all};
use std::path::PathBuf;
use walkdir::WalkDir;

pub fn execute_tool(name: &str, args: Value) -> Result<String> {
    let cwd = std::env::current_dir()?;

    fn clean_path(s: &str) -> String {
        s.trim().trim_matches('"').trim().to_string()
    }

    fn resolve_dir(cwd: &PathBuf, input: &str) -> Result<PathBuf> {
        let full = cwd
            .join(input)
            .canonicalize()
            .context("Invalid directory path")?;
        if !full.starts_with(cwd) {
            return Err(anyhow!("Path traversal attempt"));
        }
        Ok(full)
    }

    fn resolve_parent_for_write(cwd: &PathBuf, input: &str) -> Result<PathBuf> {
        let path = cwd.join(input);
        let full_parent = path.parent().map_or_else(
            || cwd.canonicalize().map_err(anyhow::Error::from),
            |p| p.canonicalize().map_err(anyhow::Error::from),
        )?;
        if !full_parent.starts_with(cwd) {
            return Err(anyhow!("Path traversal attempt"));
        }
        Ok(path)
    }

    match name {
        "read_file" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let path = clean_path(&raw_path);
            let full = resolve_dir(&cwd, &path)?;
            fs::read_to_string(full).map_err(Into::into)
        }
        "write_file" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let path = clean_path(&raw_path);
            let content: String = serde_json::from_value(args["content"].clone())?;
            let full_path = resolve_parent_for_write(&cwd, &path)?;
            if let Some(parent) = full_path.parent() {
                create_dir_all(parent)?;
            }
            fs::write(&full_path, content)?;
            Ok(format!("Successfully wrote {}", path))
        }
        "list_directory" => {
            let raw_path = args["path"].as_str().unwrap_or(".");
            let path = clean_path(raw_path);
            let full = resolve_dir(&cwd, &path)?;
            let mut entries = vec![];
            for e in fs::read_dir(full)? {
                let e = e?;
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                if e.file_type()?.is_dir()
                    && ["target", ".git", "node_modules"].contains(&name.as_str())
                {
                    continue;
                }
                let suffix = if e.file_type()?.is_dir() { "/" } else { "" };
                entries.push(format!("{}{}", name, suffix));
            }
            entries.sort();
            Ok(entries.join("\n"))
        }
        "get_directory_tree" => {
            let raw_path = args["path"].as_str().unwrap_or(".");
            let path = clean_path(raw_path);
            let full = resolve_dir(&cwd, &path)?;
            let mut lines = vec![if path == "." { ".".to_string() } else { path }];

            let mut entries: Vec<_> = WalkDir::new(&full)
                .min_depth(1)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    if name.starts_with('.') {
                        return false;
                    }
                    if e.file_type().is_dir()
                        && ["target", ".git", "node_modules"].contains(&&*name)
                    {
                        return false;
                    }
                    true
                })
                .filter_map(|e| e.ok())
                .collect();

            entries.sort_by_key(|e| e.path().to_path_buf());

            for entry in entries {
                let depth = entry.depth();
                let indent = "  ".repeat(depth);
                let name = entry.file_name().to_string_lossy();
                let suffix = if entry.file_type().is_dir() { "/" } else { "" };
                lines.push(format!("{}{}{}", indent, name, suffix));
            }
            Ok(lines.join("\n"))
        }
        _ => Err(anyhow!("Unknown tool: {}", name)),
    }
}
