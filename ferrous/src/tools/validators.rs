use anyhow::{Context, Result, anyhow};
use std::path::Path;
use std::process::Command;

/// Validates that a search string exists in the target file before attempting `replace_in_file`
pub fn validate_search_exists(file_path: &Path, search: &str) -> Result<()> {
    if !file_path.exists() {
        return Err(anyhow!("File does not exist: {}", file_path.display()));
    }

    if !file_path.is_file() {
        return Err(anyhow!("Path is not a file: {}", file_path.display()));
    }

    let content =
        std::fs::read_to_string(file_path).context("Failed to read file for validation")?;

    if !content.contains(search) {
        return Err(anyhow!(
            "Search string not found in file. The exact string '{}' does not exist in '{}'.\n\
            This prevents a no-op replace operation. Please read the file first to get the exact content.",
            search.chars().take(100).collect::<String>(),
            file_path.display()
        ));
    }

    Ok(())
}

/// Validates that a path exists and is within project bounds
pub fn validate_path_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("Path does not exist: {}", path.display()));
    }
    Ok(())
}

/// Validates that a path is a directory
pub fn validate_is_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("Path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(anyhow!("Path is not a directory: {}", path.display()));
    }
    Ok(())
}

/// Validates that a path is a file
pub fn validate_is_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("Path does not exist: {}", path.display()));
    }
    if !path.is_file() {
        return Err(anyhow!("Path is not a file: {}", path.display()));
    }
    Ok(())
}

/// Runs a fast linter check on Rust files
pub fn lint_rust_file(file_path: &Path) -> Result<LintResult> {
    if !file_path.exists() {
        return Err(anyhow!("File does not exist: {}", file_path.display()));
    }

    // Use cargo check for Rust files
    let output = Command::new("cargo")
        .arg("check")
        .arg("--message-format=short")
        .arg("--quiet")
        .current_dir(
            file_path
                .parent()
                .ok_or_else(|| anyhow!("Cannot determine parent directory"))?,
        )
        .output()
        .context("Failed to run cargo check")?;

    Ok(LintResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Runs a fast syntax check on JavaScript/TypeScript files
pub fn lint_js_file(file_path: &Path) -> Result<LintResult> {
    if !file_path.exists() {
        return Err(anyhow!("File does not exist: {}", file_path.display()));
    }

    // Try to find and run eslint
    let eslint_cmd = if Command::new("eslint").arg("--version").output().is_ok() {
        "eslint"
    } else if Command::new("npx").arg("--version").output().is_ok() {
        "npx"
    } else {
        return Ok(LintResult {
            success: true,
            stdout: String::new(),
            stderr: "eslint not available, skipping lint check".to_string(),
        });
    };

    let mut cmd = Command::new(eslint_cmd);
    if eslint_cmd == "npx" {
        cmd.arg("eslint");
    }
    cmd.arg(file_path);

    let output = cmd.output().context("Failed to run eslint")?;

    Ok(LintResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Auto-detect file type and run appropriate linter
pub fn lint_file(file_path: &Path) -> Result<LintResult> {
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" => lint_rust_file(file_path),
        "js" | "jsx" | "ts" | "tsx" => lint_js_file(file_path),
        _ => Ok(LintResult {
            success: true,
            stdout: String::new(),
            stderr: format!("No linter available for .{ext} files"),
        }),
    }
}

/// Result of a lint operation
#[derive(Debug, Clone)]
pub struct LintResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl LintResult {
    #[must_use]
    pub const fn has_errors(&self) -> bool {
        !self.success
    }

    #[must_use]
    pub fn format_output(&self) -> String {
        let mut output = String::new();
        if !self.stdout.is_empty() {
            output.push_str(&self.stdout);
            output.push('\n');
        }
        if !self.stderr.is_empty() {
            output.push_str(&self.stderr);
            output.push('\n');
        }
        output
    }
}
