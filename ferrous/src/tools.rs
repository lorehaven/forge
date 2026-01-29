use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fs::{self, Metadata, create_dir_all};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, UNIX_EPOCH};
use walkdir::WalkDir;

pub async fn execute_tool(name: &str, args: Value) -> Result<String> {
    let cwd = std::env::current_dir()?;

    fn clean_path(s: &str) -> String {
        s.trim().trim_matches('"').trim().to_string()
    }

    fn resolve_dir(cwd: &Path, input: &str) -> Result<PathBuf> {
        let cwd_canonical = cwd.canonicalize().context("Failed to canonicalize CWD")?;
        let full = cwd_canonical
            .join(input)
            .canonicalize()
            .context("Invalid path")?;
        if !full.starts_with(&cwd_canonical) {
            return Err(anyhow!("Path traversal attempt"));
        }
        Ok(full)
    }

    fn resolve_parent_for_write(cwd: &Path, input: &str) -> Result<PathBuf> {
        let cwd_canonical = cwd.canonicalize().context("Failed to canonicalize CWD")?;
        let path = cwd_canonical.join(input);
        let full_parent = path.parent().map_or_else(
            || Ok(cwd_canonical.clone()),
            |p| p.canonicalize().map_err(anyhow::Error::from),
        )?;
        if !full_parent.starts_with(&cwd_canonical) {
            return Err(anyhow!("Path traversal attempt"));
        }
        Ok(path)
    }

    match name {
        // ──────────────────────────────────────────────
        "analyze_project" => {
            let output = Command::new("cargo")
                .arg("clippy")
                .arg("--all-targets")
                .arg("--")
                .arg("-W")
                .arg("clippy::all")
                .arg("-W")
                .arg("clippy::pedantic")
                .arg("-W")
                .arg("clippy::nursery")
                .current_dir(&cwd)
                .output()
                .context("Failed to run cargo clippy. Is clippy installed?")?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let status_msg = if output.status.success() {
                "Analysis completed successfully.".to_string()
            } else {
                format!(
                    "Analysis completed with warnings (exit code {})",
                    output.status.code().unwrap_or(-1)
                )
            };

            let combined = if stdout.trim().is_empty() && stderr.trim().is_empty() {
                "No issues found.".to_string()
            } else if stdout.trim().is_empty() {
                format!("stderr:\n{}", stderr)
            } else if stderr.trim().is_empty() {
                format!("stdout:\n{}", stdout)
            } else {
                format!("stdout:\n{}\n\nstderr:\n{}", stdout, stderr)
            };

            Ok(format!("{}\n\n{}", status_msg, combined))
        }
        // ──────────────────────────────────────────────
        "get_file_info" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let path = clean_path(&raw_path);
            let full = resolve_dir(&cwd, &path)?;

            let meta: Metadata = full
                .metadata()
                .with_context(|| format!("Cannot get metadata for {}", path))?;

            let mut lines = vec![
                format!("Path: {}", path),
                format!("Exists: {}", full.exists()),
                format!("Is directory: {}", meta.is_dir()),
                format!("Is file: {}", meta.is_file()),
                format!("Size: {} bytes", meta.len()),
            ];

            if let Ok(modified) = meta.modified()
                && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
            {
                let secs = duration.as_secs();
                lines.push(format!("Last modified: {} (unix timestamp)", secs));
            }

            if meta.is_file()
                && full
                    .extension()
                    .is_some_and(|e| e == "rs" || e == "toml" || e == "md" || e == "txt")
                && let Ok(content) = fs::read_to_string(&full)
            {
                let line_count = content.lines().count();
                lines.push(format!("Line count: {}", line_count));
            }

            Ok(lines.join("\n"))
        }
        // ──────────────────────────────────────────────
        "file_exists" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let path = clean_path(&raw_path);
            let full = resolve_dir(&cwd, &path)?;
            Ok(if full.exists() {
                "true".to_string()
            } else {
                "false".to_string()
            })
        }
        // ──────────────────────────────────────────────
        "list_files_recursive" => {
            let base = args["path"].as_str().unwrap_or(".");
            let ext_filter = args["extension"].as_str();

            let full = resolve_dir(&cwd, &clean_path(base))?;

            let mut files = Vec::new();

            for entry in WalkDir::new(&full)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    !name.starts_with('.') && ![".git", "target", "node_modules"].contains(&&*name)
                })
                .filter_map(|e| e.ok())
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
                        .strip_prefix(cwd.canonicalize().unwrap_or(cwd.clone()))
                        .unwrap_or(entry.path());
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
        // ──────────────────────────────────────────────
        "replace_in_file" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let search: String = serde_json::from_value(args["search"].clone())?;
            let replace: String = serde_json::from_value(args["replace"].clone())?;

            let path = clean_path(&raw_path);
            let full = resolve_dir(&cwd, &path)?;

            let old_content =
                fs::read_to_string(&full).context("Cannot read file for replacement")?;

            let new_content = old_content.replace(&search, &replace);
            if old_content == new_content {
                return Ok(format!(
                    "No changes made in {}. This usually means the 'search' string was not found exactly as provided. Check indentation and whitespace!",
                    path
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
        // ──────────────────────────────────────────────
        "read_file" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let path = clean_path(&raw_path);
            let full = resolve_dir(&cwd, &path)?;
            fs::read_to_string(full).map_err(Into::into)
        }
        // ──────────────────────────────────────────────
        "read_multiple_files" => {
            let paths: Vec<String> = serde_json::from_value(args["paths"].clone())?;
            let mut results = Vec::new();
            for raw_path in paths {
                let path = clean_path(&raw_path);
                match resolve_dir(&cwd, &path) {
                    Ok(full) => {
                        if let Ok(content) = fs::read_to_string(&full) {
                            results.push(format!("File: {}\nContent:\n{}\n---\n", path, content));
                        } else {
                            results
                                .push(format!("Failed to read {}: file not readable\n---\n", path));
                        }
                    }
                    Err(e) => {
                        results.push(format!("Failed to resolve {}: {}\n---\n", path, e));
                    }
                }
            }
            if results.is_empty() {
                Ok("No files read.".to_string())
            } else {
                Ok(results.join(""))
            }
        }
        // ──────────────────────────────────────────────
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
        // ──────────────────────────────────────────────
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
        // ──────────────────────────────────────────────
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
        // ──────────────────────────────────────────────
        "create_directory" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let path = clean_path(&raw_path);

            // We allow creating nested paths → use the same safety check as write_file
            let full_path = resolve_parent_for_write(&cwd, &path)?;

            // If the path already exists as a directory, we consider it success (idempotent)
            if full_path.is_dir() {
                return Ok(format!("Directory already exists: {}", path));
            }

            // If it exists but is a file → error (protect against accidental overwrite)
            if full_path.exists() {
                return Err(anyhow!(
                    "Cannot create directory '{}': path already exists and is a file",
                    path
                ));
            }

            create_dir_all(&full_path)
                .with_context(|| format!("Failed to create directory '{}'", path))?;

            Ok(format!("Created directory: {}", path))
        }
        // ──────────────────────────────────────────────
        "append_to_file" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let content: String = serde_json::from_value(args["content"].clone())?;

            let path = clean_path(&raw_path);
            let full_path = resolve_parent_for_write(&cwd, &path)?;

            // Create parent directories if needed
            if let Some(parent) = full_path.parent() {
                create_dir_all(parent)?;
            }

            use std::fs::OpenOptions;
            use std::io::Write;

            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&full_path)
                .with_context(|| format!("Cannot open/append to {}", path))?;

            writeln!(file, "{}", content)
                .with_context(|| format!("Failed to append to {}", path))?;

            Ok(format!("Appended {} bytes to {}", content.len(), path))
        }
        // ──────────────────────────────────────────────
        "search_text" => {
            let pattern: String = serde_json::from_value(args["pattern"].clone())?;
            let path = args["path"].as_str().unwrap_or(".").to_string();
            let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);

            let path_clean = clean_path(&path);
            let full_path = resolve_dir(&cwd, &path_clean)?;

            // Use `grep` via subprocess — most reliable and fast
            let mut cmd = Command::new("grep");
            cmd.arg("-r") // recursive
                .arg("--color=never") // plain text output
                .arg("-n") // show line numbers
                .arg("-I") // skip binary files
                .arg("-F") // fixed string (not regex) — safer for model
                .arg(&pattern)
                .arg(&full_path);

            if !case_sensitive {
                cmd.arg("-i");
            }

            let output = cmd
                .output()
                .context("grep command failed. Is grep installed?")?;

            let result = if output.status.success() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                String::from_utf8_lossy(&output.stderr).to_string()
            };

            if result.trim().is_empty() {
                Ok(format!(
                    "No matches found for '{}'. Note that this is a fixed-string search (not regex) and is case-{}sensitive.",
                    pattern,
                    if case_sensitive { "" } else { "in" }
                ))
            } else {
                Ok(format!("Search results:\n{}", result))
            }
        }
        // ──────────────────────────────────────────────
        "execute_shell_command" => {
            let command: String = serde_json::from_value(args["command"].clone())
                .context("Missing or invalid 'command' argument")?;

            let command_trim = command.trim();

            // ── Very important: strict allow-list ────────────────────────────────
            let allowed_prefixes = vec![
                "cargo check",
                "cargo fmt",
                "cargo clippy",
                "cargo build",
                "cargo run",
                "cargo test",
                "cargo bench",
                "cargo doc",
                "cargo metadata",
                "cargo tree",
                "cargo audit",     // if cargo-audit is installed
                "rustfmt --check", // sometimes used separately
                "cargo +nightly ", // allow nightly toolchains
            ];

            let is_allowed = allowed_prefixes.iter().any(|prefix| {
                command_trim.starts_with(prefix)
                    || command_trim.starts_with(&format!("{} ", prefix))
            });

            if !is_allowed {
                return Err(anyhow!(
                    "Command rejected for safety reasons.\n\nAllowed prefixes:\n  {}\n\nGot: {:?}",
                    allowed_prefixes.join("\n  "),
                    command_trim
                ));
            }

            // ── Execute with timeout & output capture ────────────────────────────
            let output = tokio::time::timeout(Duration::from_secs(120), async {
                tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&command)
                    .current_dir(&cwd)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
            })
            .await
            .context("Command execution timed out after 120 seconds")??;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let combined = if stderr.is_empty() {
                stdout
            } else if stdout.is_empty() {
                format!("(stderr only)\n{}", stderr)
            } else {
                format!("stdout:\n{}\n\nstderr:\n{}", stdout, stderr)
            };

            let status_msg = if output.status.success() {
                "Command finished successfully".to_string()
            } else {
                format!(
                    "Command failed with exit code {}",
                    output.status.code().unwrap_or(-1)
                )
            };

            Ok(format!("{}\n\nOutput:\n{}", status_msg, combined))
        }
        // ──────────────────────────────────────────────
        "git_status" => {
            let output = Command::new("git")
                .arg("status")
                .arg("--short")
                .current_dir(&cwd)
                .output()
                .context("Failed to run git status. Is git installed? Is this a git repository?")?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();

            if output.status.success() {
                if stdout.trim().is_empty() {
                    Ok("Working tree clean. Nothing to commit.".to_string())
                } else {
                    Ok(format!("Git status:\n{}", stdout))
                }
            } else {
                Err(anyhow!(
                    "git status failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        }
        // ──────────────────────────────────────────────
        "git_diff" => {
            let mut cmd = Command::new("git");
            cmd.arg("diff");

            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                let path_clean = clean_path(path);
                let full = resolve_dir(&cwd, &path_clean)?;
                cmd.arg(full);
            }

            let output = cmd
                .current_dir(&cwd)
                .output()
                .context("Failed to run git diff")?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();

            if output.status.success() {
                if stdout.trim().is_empty() {
                    Ok("No changes.".to_string())
                } else {
                    Ok(format!("Git diff:\n{}", stdout))
                }
            } else {
                Err(anyhow!(
                    "git diff failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        }
        // ──────────────────────────────────────────────
        "git_add" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let path = clean_path(&raw_path);
            let full = resolve_dir(&cwd, &path)?;

            let output = Command::new("git")
                .arg("add")
                .arg(&full)
                .current_dir(&cwd)
                .output()
                .context("Failed to run git add")?;

            if output.status.success() {
                Ok(format!("Staged: {}", path))
            } else {
                Err(anyhow!(
                    "git add failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        }
        // ──────────────────────────────────────────────
        "git_commit" => {
            let message: String = serde_json::from_value(args["message"].clone())?;

            if message.trim().is_empty() {
                return Err(anyhow!("Commit message cannot be empty"));
            }

            let output = Command::new("git")
                .arg("commit")
                .arg("-m")
                .arg(&message)
                .current_dir(&cwd)
                .output()
                .context("Failed to run git commit")?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                Ok(format!("Commit created.\n{}", stdout))
            } else {
                Err(anyhow!(
                    "git commit failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        }
        // ──────────────────────────────────────────────
        _ => Err(anyhow!("Unknown tool: {}", name)),
    }
}
