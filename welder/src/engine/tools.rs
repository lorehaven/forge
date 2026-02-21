use anyhow::{Context, Result, anyhow, bail};
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

    let output = Command::new("rg")
        .arg("-n")
        .arg(pattern)
        .arg(safe.as_os_str())
        .output()
        .context("failed to execute rg")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            Ok(String::new())
        } else {
            bail!("search failed: {stderr}");
        }
    }
}

fn index_project(args: &Value) -> Result<String> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let safe = safe_rel_path(path)?;

    let output = Command::new("rg")
        .arg("--files")
        .arg(safe.as_os_str())
        .output()
        .context("failed to execute rg --files")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("index_project failed: {stderr}");
    }
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

    let allowed = run_cmd_allowlist.iter().any(|pattern| {
        let tokens: Vec<&str> = pattern.split_whitespace().collect();
        !tokens.is_empty()
            && tokens.len() <= cmd_parts.len()
            && cmd_parts
                .iter()
                .take(tokens.len())
                .zip(tokens.iter())
                .all(|(actual, expected)| actual == expected)
    });

    if !allowed {
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

    Ok(p)
}
