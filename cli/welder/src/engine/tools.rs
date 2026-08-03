use anyhow::{Context, Result, anyhow, bail};
use grep::regex::RegexMatcher;
use grep::searcher::Searcher;
use grep::searcher::sinks::UTF8;
use ignore::WalkBuilder;
use serde_json::Value;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path};
use std::process::Command;

#[derive(Debug)]
pub struct ToolResult {
    pub output: String,
}

pub fn run_tool(tool: &str, args: &Value, run_cmd_allowlist: &[String]) -> Result<ToolResult> {
    let output = match tool {
        "list_dir" => list_dir(args)?,
        "read_file" => read_file(args)?,
        "write_file" => write_file(args)?,
        "replace_in_file" => replace_in_file(args)?,
        "search" => search(args)?,
        "index_project" => index_project(args)?,
        "run_cmd" => run_cmd(args, run_cmd_allowlist)?,
        _ => bail!("unknown tool: {tool}"),
    };

    Ok(ToolResult { output })
}

#[must_use]
pub fn tool_help(tools: &[String], run_cmd_allowlist: &[String]) -> String {
    let mut docs = Vec::new();

    for tool in tools {
        let doc = match tool.as_str() {
            "list_dir" => "- list_dir: args = {\"path\": \".\"}",
            "read_file" => {
                "- read_file: args = {\"path\": \"src/main.rs\", \"start_line\": 1, \"end_line\": 200}"
            }
            "write_file" => "- write_file: args = {\"path\": \"file.txt\", \"content\": \"...\"}",
            "replace_in_file" => {
                "- replace_in_file: args = {\"path\": \"src/lib.rs\", \"find\": \"old\", \"replace\": \"new\"}"
            }
            "search" => "- search: args = {\"pattern\": \"execute\", \"path\": \".\"}",
            "index_project" => "- index_project: args = {\"path\": \".\"}",
            "run_cmd" => {
                "- run_cmd: args = {\"cmd\": \"cargo check -p welder\"} (must match allowlist prefix)"
            }
            _ => continue,
        };
        docs.push(doc.to_string());
    }

    if tools.iter().any(|tool| tool == "run_cmd") {
        docs.push(format!(
            "  run_cmd allowlist prefixes: {run_cmd_allowlist:?}"
        ));
    }

    docs.join("\n")
}

fn list_dir(args: &Value) -> Result<String> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let safe = safe_rel_path(path)?;

    let mut entries = fs::read_dir(safe)
        .with_context(|| format!("failed to read directory {}", safe.display()))?
        .map(|entry| {
            let entry = entry?;
            let ty = entry.file_type()?;
            let kind = if ty.is_dir() { "dir" } else { "file" };
            Ok(format!("{kind}\t{}", entry.file_name().to_string_lossy()))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()
        .context("failed to list directory")?;

    entries.sort_unstable();
    Ok(entries.join("\n"))
}

fn read_file(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let safe = safe_rel_path(path)?;

    let content = fs::read_to_string(safe)
        .with_context(|| format!("failed to read file {}", safe.display()))?;
    let lines: Vec<&str> = content.lines().collect();

    let start_line = args
        .get("start_line")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(1);
    let end_line = args
        .get("end_line")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(lines.len());

    if start_line == 0 || end_line < start_line {
        bail!("invalid line range");
    }

    let start_idx = start_line.saturating_sub(1).min(lines.len());
    let end_idx = end_line.min(lines.len());
    let mut out = String::new();

    for (idx, line) in lines[start_idx..end_idx].iter().enumerate() {
        let line_no = start_idx + idx + 1;
        writeln!(&mut out, "{line_no:>5} | {line}").expect("writing to String should not fail");
    }

    Ok(out)
}

fn write_file(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let content = required_str(args, "content")?;
    let safe = safe_rel_path(path)?;

    if let Some(parent) = safe.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }

    fs::write(safe, content).with_context(|| format!("failed to write file {}", safe.display()))?;
    Ok(format!("wrote {}", safe.display()))
}

fn replace_in_file(args: &Value) -> Result<String> {
    let path = required_str(args, "path")?;
    let find = required_str(args, "find")?;
    let replace = required_str(args, "replace")?;
    let safe = safe_rel_path(path)?;

    let content = fs::read_to_string(safe)
        .with_context(|| format!("failed to read file {}", safe.display()))?;
    let count = content.matches(find).count();
    let updated = content.replace(find, replace);
    fs::write(safe, updated).with_context(|| format!("failed to write file {}", safe.display()))?;

    Ok(format!(
        "replaced {count} occurrence(s) in {}",
        safe.display()
    ))
}

fn search(args: &Value) -> Result<String> {
    let pattern = required_str(args, "pattern")?;
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let safe = safe_rel_path(path)?;

    let matcher =
        RegexMatcher::new(pattern).with_context(|| format!("invalid pattern: {pattern}"))?;
    let mut out = String::new();

    for entry in WalkBuilder::new(safe).build() {
        let entry = entry.context("failed to walk directory")?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let display = entry.path().display().to_string();
        let mut searcher = Searcher::new();
        let search_result = searcher.search_path(
            &matcher,
            entry.path(),
            UTF8(|line_num, line| {
                write!(&mut out, "{display}:{line_num}:{line}").map_err(std::io::Error::other)?;
                Ok(true)
            }),
        );

        // Binary/unreadable files are skipped rather than failing the whole
        // search, matching how `rg` quietly moves past them.
        if let Err(err) = search_result
            && err.kind() != std::io::ErrorKind::InvalidData
        {
            bail!("search failed on {display}: {err}");
        }
    }

    Ok(out)
}

fn index_project(args: &Value) -> Result<String> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let safe = safe_rel_path(path)?;

    let mut out = String::new();

    for entry in WalkBuilder::new(safe).build() {
        let entry = entry.context("failed to walk directory")?;
        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            writeln!(&mut out, "{}", entry.path().display())
                .expect("writing to String should not fail");
        }
    }

    Ok(out)
}

fn run_cmd(args: &Value, run_cmd_allowlist: &[String]) -> Result<String> {
    let cmd = required_str(args, "cmd")?;

    let cmd_parts =
        shlex::split(cmd).ok_or_else(|| anyhow!("invalid shell-like command syntax"))?;
    if cmd_parts.is_empty() {
        bail!("command is empty");
    }

    let executable = &cmd_parts[0];
    if executable.contains('/') || executable.contains('\\') {
        bail!("only bare executable names are allowed");
    }

    if !is_command_allowed(&cmd_parts, run_cmd_allowlist) {
        bail!(
            "command blocked by run_cmd allowlist. command='{cmd}', allowlist={run_cmd_allowlist:?}"
        );
    }

    let output = Command::new(executable)
        .args(&cmd_parts[1..])
        .output()
        .with_context(|| format!("failed to execute command: {cmd}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    Ok(format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status, stdout, stderr
    ))
}

/// Whether `cmd_parts` (already shell-split) matches at least one allowlist
/// entry by whitespace-tokenized prefix, e.g. allowlist entry "cargo check"
/// matches `cmd_parts` of `["cargo", "check", "-p", "welder"]`.
fn is_command_allowed(cmd_parts: &[String], run_cmd_allowlist: &[String]) -> bool {
    run_cmd_allowlist.iter().any(|pattern| {
        let tokens: Vec<&str> = pattern.split_whitespace().collect();
        !tokens.is_empty()
            && tokens.len() <= cmd_parts.len()
            && cmd_parts
                .iter()
                .take(tokens.len())
                .zip(tokens.iter())
                .all(|(actual, expected)| actual == expected)
    })
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing or invalid '{key}'"))
}

fn safe_rel_path(path: &str) -> Result<&Path> {
    let p = Path::new(path);

    if p.is_absolute() {
        bail!("absolute paths are not allowed");
    }

    for component in p.components() {
        if matches!(component, Component::ParentDir) {
            bail!("path traversal is not allowed");
        }
    }

    ensure_no_symlink_escape(p)?;

    Ok(p)
}

/// Rejects paths that escape the working directory through a symlink.
///
/// The absolute/`..` checks above only look at path syntax; a symlink
/// already present on disk (e.g. `link -> /etc`) would otherwise let a
/// model-driven tool read or write outside the project root even though the
/// path string itself looks perfectly relative.
fn ensure_no_symlink_escape(p: &Path) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let root = fs::canonicalize(&cwd)
        .with_context(|| format!("failed to canonicalize {}", cwd.display()))?;

    // Only existing entries can be symlinks, so it's enough to find the
    // longest prefix of `p` that already exists and canonicalize just that:
    // canonicalize resolves every symlink along the whole prefix chain in
    // one call. Anything past that prefix doesn't exist yet (e.g. a file
    // `write_file` is about to create) and so can't be a symlink either.
    let mut prefix = cwd;
    for component in p.components() {
        let candidate = prefix.join(component.as_os_str());
        if !candidate.exists() {
            break;
        }
        prefix = candidate;
    }

    let canonical_prefix = fs::canonicalize(&prefix)
        .with_context(|| format!("failed to canonicalize {}", prefix.display()))?;

    if !canonical_prefix.starts_with(&root) {
        bail!("path escapes the working directory");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `safe_rel_path` reads the process's current directory, which is
    /// global state shared across every test thread. Any test that reaches
    /// the symlink check (i.e. doesn't bail out on the absolute/`..` checks
    /// first) must hold this lock for its duration so it can't observe a
    /// cwd another test is mid-way through changing.
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn rejects_absolute_paths() {
        assert!(safe_rel_path("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        assert!(safe_rel_path("../secrets").is_err());
        assert!(safe_rel_path("a/../../b").is_err());
    }

    #[test]
    fn allows_plain_relative_paths() {
        let _guard = CWD_LOCK.lock().unwrap();
        assert!(safe_rel_path("src/main.rs").is_ok());
        assert!(safe_rel_path(".").is_ok());
    }

    #[test]
    fn allows_paths_that_do_not_exist_yet() {
        let _guard = CWD_LOCK.lock().unwrap();
        // A brand-new file under a directory that doesn't exist yet either
        // (write_file creates parents) must still be allowed.
        assert!(safe_rel_path("brand/new/file.txt").is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn rejects_symlink_escape() {
        let _guard = CWD_LOCK.lock().unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "hi").unwrap();
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("escape")).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = safe_rel_path("escape/secret.txt");
        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_err(), "symlink escape should be rejected");
    }

    #[test]
    fn allowlist_matches_by_whitespace_tokenized_prefix() {
        let allowlist = vec!["cargo check".to_string(), "npm test".to_string()];

        let allowed: Vec<String> = ["cargo", "check", "-p", "welder"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(is_command_allowed(&allowed, &allowlist));

        let disallowed: Vec<String> = ["cargo", "publish"].into_iter().map(String::from).collect();
        assert!(!is_command_allowed(&disallowed, &allowlist));
    }

    #[test]
    fn allowlist_rejects_shorter_command_than_pattern() {
        let allowlist = vec!["cargo check".to_string()];
        let short: Vec<String> = vec!["cargo".to_string()];
        assert!(!is_command_allowed(&short, &allowlist));
    }

    #[test]
    fn allowlist_empty_matches_nothing() {
        let cmd: Vec<String> = ["cargo", "check"].into_iter().map(String::from).collect();
        assert!(!is_command_allowed(&cmd, &[]));
    }
}
