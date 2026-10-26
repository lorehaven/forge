//! The scanners and command helpers behind the security gate (a copy of `quench-security`'s: that
//! crate is not published, and forge builds against published quench crates. Keep the two in step.)
//!
//! Pure functions over text and the filesystem, so each can be tested against known-bad input - a
//! scanner that has never been shown what it should catch proves nothing when it reports "clean".
//!
//! Patterns are assembled from pieces at run time, so this file does not itself contain what it
//! searches for (and does not trip its own scan).

// Source files are matched on their (lower-case) extensions on purpose.
#![allow(clippy::case_sensitive_file_extension_comparisons)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Directories never scanned: build output and version control.
const SKIP_DIRS: [&str; 4] = ["target", ".git", "node_modules", "dist"];

/// The files git considers part of the project - tracked, or new and not ignored - as `./`-prefixed
/// relative paths. `None` outside a git checkout.
///
/// This is what "in the sources" means: a virtualenv, a build directory or a gitignored local key is
/// not, and scanning them buries real findings under noise.
fn project_files(root: &Path) -> Option<Vec<String>> {
    let listing = run(
        root,
        "git",
        &["ls-files", "--cached", "--others", "--exclude-standard"],
        &[],
    );
    if !listing.success {
        return None;
    }
    let mut files: Vec<String> = listing
        .output
        .lines()
        .filter(|line| !line.is_empty())
        // Deleted but not yet committed paths are still listed; they cannot be read. A symlink is not a file of
        // the project either: scanning through it reads whatever it points at, which may be a local,
        // untracked file (and is reported by `symlinks_leaving_the_project` if it should not be there).
        .filter(|line| {
            root.join(line)
                .symlink_metadata()
                .is_ok_and(|meta| meta.file_type().is_file())
        })
        .map(|line| format!("./{line}"))
        .collect();
    files.sort();
    files.dedup();
    (!files.is_empty()).then_some(files)
}

/// Where a symlink in the project points, when that is somewhere a checkout elsewhere cannot follow.
///
/// An absolute target names a path on one machine; a relative one that climbs out of the project does too.
/// Either breaks the first clone made anywhere else - CI, a release tag's baseline check - and a symlink
/// into a developer's own files can also leak whatever it reaches.
fn escapes(path: &str, target: &Path) -> bool {
    use std::path::Component;
    if target.is_absolute() {
        return true;
    }
    let mut depth = Path::new(path.trim_start_matches("./"))
        .parent()
        .map_or(0, |parent| parent.components().count());
    for component in target.components() {
        match component {
            Component::ParentDir => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            Component::Normal(_) => depth += 1,
            _ => {}
        }
    }
    false
}

/// Symlinks of the project that point outside it, as `(path, target)`.
#[must_use]
pub fn symlinks_leaving_the_project(root: &Path) -> Vec<(String, String)> {
    let listing = run(
        root,
        "git",
        &["ls-files", "--cached", "--others", "--exclude-standard"],
        &[],
    );
    if !listing.success {
        return Vec::new();
    }
    let mut found: Vec<(String, String)> = listing
        .output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let target = std::fs::read_link(root.join(line)).ok()?;
            escapes(line, &target).then(|| (format!("./{line}"), target.display().to_string()))
        })
        .collect();
    found.sort();
    found
}

/// Every `.rs` file of the project (relative paths, `./`-prefixed, sorted).
#[must_use]
pub fn rust_files(root: &Path) -> Vec<String> {
    if let Some(files) = project_files(root) {
        return files.into_iter().filter(|f| f.ends_with(".rs")).collect();
    }
    let mut found = Vec::new();
    walk(root, root, &mut found, &|name| name.ends_with(".rs"));
    found.sort();
    found
}

/// Every file under `root` worth looking for secrets in: text of any kind, skipping build output.
#[must_use]
pub fn text_files(root: &Path) -> Vec<String> {
    let is_text = |name: &str| {
        [
            "rs", "toml", "yml", "yaml", "json", "env", "sh", "fish", "md", "txt", "pem", "key",
            "cfg", "conf", "ini",
        ]
        .iter()
        .any(|ext| name.rsplit('.').next() == Some(*ext))
            && !name.ends_with("Cargo.lock")
    };
    if let Some(files) = project_files(root) {
        return files.into_iter().filter(|f| is_text(f)).collect();
    }
    let mut found = Vec::new();
    walk(root, root, &mut found, &is_text);
    found.sort();
    found
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>, wanted: &dyn Fn(&str) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(root, &path, out, wanted);
            }
        } else if wanted(&name) {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            out.push(format!("./{}", relative.display()));
        }
    }
}

fn piece(parts: &[&str]) -> String {
    parts.concat()
}

/// Whether a line of Rust uses `unsafe`, or switches the lint that forbids it off.
#[must_use]
pub fn line_uses_unsafe(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    let keyword = piece(&["uns", "afe"]);
    let lint_name = piece(&["uns", "afe_code"]);
    [
        format!("allow({lint_name})"),
        format!("forbid({lint_name})"),
        format!("{keyword} {{"),
        format!("{keyword}{{"),
        format!("{keyword} fn"),
        format!("{keyword} impl"),
        format!("{keyword} trait"),
        format!("#[{keyword}("),
    ]
    .iter()
    .any(|pattern| line.contains(pattern.as_str()))
}

/// Files under `root` that use `unsafe`, as `./relative/path.rs`.
#[must_use]
pub fn files_using_unsafe(root: &Path) -> Vec<String> {
    rust_files(root)
        .into_iter()
        .filter(|file| {
            fs::read_to_string(root.join(file.trim_start_matches("./")))
                .is_ok_and(|text| text.lines().any(line_uses_unsafe))
        })
        .collect()
}

/// Entries of an allowlist file: one path per line, `#` comments and blanks ignored.
#[must_use]
pub fn allowlist(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// What a line of text gives away: key material and well-known cloud tokens.
#[must_use]
pub fn line_leaks_secret(line: &str) -> Option<&'static str> {
    let begin = piece(&["-----BE", "GIN "]);
    let private = piece(&["PRIVATE", " KEY-----"]);
    if line.contains(&begin) && line.contains(&private) {
        return Some("a private key");
    }
    if token_after(line, "AKIA", 16, |c| {
        c.is_ascii_uppercase() || c.is_ascii_digit()
    }) {
        return Some("an AWS access key id");
    }
    if token_after(line, "ghp_", 36, |c| c.is_ascii_alphanumeric()) {
        return Some("a GitHub personal access token");
    }
    for kind in ['b', 'a', 'p', 'r', 's'] {
        let prefix = format!("xox{kind}-");
        if token_after(line, &prefix, 10, |c| c.is_ascii_alphanumeric() || c == '-') {
            return Some("a Slack token");
        }
    }
    None
}

fn token_after(line: &str, prefix: &str, length: usize, ok: impl Fn(char) -> bool) -> bool {
    line.match_indices(prefix).any(|(at, _)| {
        line[at + prefix.len()..]
            .chars()
            .take_while(|c| ok(*c))
            .count()
            >= length
    })
}

/// A file's secrets: `(path, line number, what)`.
#[must_use]
pub fn secrets_in(root: &Path) -> Vec<(String, usize, &'static str)> {
    let mut found = Vec::new();
    for file in text_files(root) {
        // Fixtures and documentation legitimately contain example keys.
        if file.contains("/tests/")
            || file.contains("/testdata/")
            || file.contains("/fixtures/")
            || file.ends_with(".md")
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(root.join(file.trim_start_matches("./"))) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if let Some(what) = line_leaks_secret(line) {
                found.push((file.clone(), index + 1, what));
            }
        }
    }
    found
}

/// The result of running a command to completion.
#[derive(Debug)]
pub struct Ran {
    pub success: bool,
    pub output: String,
}

impl Ran {
    /// The last `lines` lines of output, for a failure message.
    #[must_use]
    pub fn tail(&self, lines: usize) -> String {
        let all: Vec<&str> = self.output.lines().collect();
        all[all.len().saturating_sub(lines)..].join("\n")
    }
}

/// Runs `program args` in `dir` with `env`, stdout and stderr interleaved into one string.
#[must_use]
pub fn run(dir: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> Ran {
    let mut command = Command::new(program);
    command.args(args).current_dir(dir).stdin(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    match command.output() {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            Ran {
                success: output.status.success(),
                output: text,
            }
        }
        Err(error) => Ran {
            success: false,
            output: format!("could not start `{program}`: {error} (is it installed?)"),
        },
    }
}

/// The forge workspace root (this crate lives in `tests/forge-sectest`).
#[must_use]
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
