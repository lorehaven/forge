use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fs::{self, create_dir_all};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use walkdir::WalkDir;

pub async fn execute_tool(name: &str, args: Value) -> Result<String> {
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
        "replace_in_file" => {
            let raw_path: String = serde_json::from_value(args["path"].clone())?;
            let search: String = serde_json::from_value(args["search"].clone())?;
            let replace: String = serde_json::from_value(args["replace"].clone())?;

            let path = clean_path(&raw_path);
            let full = resolve_dir(&cwd, &path)?;

            let mut content =
                fs::read_to_string(&full).context("Cannot read file for replacement")?;

            let old_len = content.len();
            content = content.replace(&search, &replace);
            let changed = old_len != content.len() || content.contains(&search);

            if !changed {
                return Ok(format!("No changes needed in {}", path));
            }

            fs::write(&full, &content)?;
            Ok(format!(
                "Replaced '{}' → '{}' in {}\n({} → {} bytes)",
                search.escape_default(),
                replace.escape_default(),
                path,
                old_len,
                content.len()
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
        "search_text" => {
            let pattern: String = serde_json::from_value(args["pattern"].clone())?;
            let path = args["path"].as_str().unwrap_or(".");
            let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);

            let mut cmd = Command::new("grep");
            cmd.arg("-r")
                .arg("--color=never")
                .arg(if case_sensitive { "-i" } else { "" })
                .arg(&pattern)
                .arg(path);

            let output = cmd
                .output()
                .context("grep command failed – is grep installed?")?;

            let result = if output.status.success() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                format!(
                    "No matches or error:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                )
            };

            Ok(result)
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
                Err(anyhow!("git status failed:\n{}", String::from_utf8_lossy(&output.stderr)))
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
                Err(anyhow!("git diff failed:\n{}", String::from_utf8_lossy(&output.stderr)))
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
                Err(anyhow!("git add failed:\n{}", String::from_utf8_lossy(&output.stderr)))
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
                Err(anyhow!("git commit failed:\n{}", String::from_utf8_lossy(&output.stderr)))
            }
        }
        // ──────────────────────────────────────────────
        _ => Err(anyhow!("Unknown tool: {}", name)),
    }
}
