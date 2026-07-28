//! Getting the commit onto disk.
//!
//! Conveyor shells out to `git` rather than linking a git library. The estate
//! already shells out to `docker` and `kubectl`, the operations here are four
//! commands, and a repository that clones from the command line clones from
//! conveyor - which is not true of every library's protocol support.
//!
//! The ref and the sha arrive in a webhook payload, so they are validated
//! before they reach an argument list. `git` reads a leading `-` as an option,
//! and `--upload-pack=...` in the position where a ref was expected runs a
//! program of the sender's choosing.

use crate::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum CheckoutError {
    #[error("git is not on PATH: conveyor cannot check anything out without it")]
    GitMissing,

    #[error("invalid commit sha {sha:?}: {reason}")]
    InvalidSha { sha: String, reason: &'static str },

    #[error("invalid git ref {git_ref:?}: {reason}")]
    InvalidRef {
        git_ref: String,
        reason: &'static str,
    },

    #[error("invalid clone url {url:?}: {reason}")]
    InvalidUrl { url: String, reason: &'static str },

    #[error("could not prepare {path}: {source}")]
    Workspace {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("git {command} failed ({code}): {stderr}")]
    Git {
        command: String,
        code: String,
        stderr: String,
    },

    #[error("git {command} timed out after {seconds}s")]
    Timeout { command: String, seconds: u64 },

    #[error("could not run git: {0}")]
    Spawn(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug)]
pub struct CheckoutRequest<'a> {
    pub clone_url: &'a str,
    /// Full (`refs/heads/master`) or bare (`master`).
    pub git_ref: &'a str,
    /// The commit to check out. Every later decision reads this rather than the
    /// ref, which can move while the run is queued.
    pub sha: &'a str,
    pub timeout: Duration,
}

/// Checks `request` out into a directory of its own beneath `work_dir`.
///
/// The directory is named after the run, so a stale one left by a crashed
/// worker is identifiable rather than anonymous. An existing directory of the
/// same name is removed first: a retry of the same run must not inherit
/// whatever the previous attempt left behind.
pub async fn checkout(
    work_dir: &Path,
    run_id: &str,
    request: &CheckoutRequest<'_>,
) -> Result<Workspace, CheckoutError> {
    validate_sha(request.sha)?;
    validate_ref(request.git_ref)?;
    validate_url(request.clone_url)?;

    if which::which("git").is_err() {
        return Err(CheckoutError::GitMissing);
    }

    let root = work_dir.join(format!("run-{}", sanitize_component(run_id)));
    if tokio::fs::try_exists(&root).await.unwrap_or(false) {
        tokio::fs::remove_dir_all(&root)
            .await
            .map_err(|source| CheckoutError::Workspace {
                path: root.clone(),
                source,
            })?;
    }
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|source| CheckoutError::Workspace {
            path: root.clone(),
            source,
        })?;

    let workspace = Workspace::new(root);
    let root = workspace.root().to_path_buf();

    git(&root, &["init", "--quiet"], request.timeout).await?;
    git(
        &root,
        &["remote", "add", "origin", request.clone_url],
        request.timeout,
    )
    .await?;

    fetch(&root, request).await?;

    // Detached on purpose: a run builds a commit, not a branch. Checking out
    // the branch would leave the workspace pointing at a ref that can move, and
    // a step that runs `git describe` would report something other than what is
    // being built.
    git(
        &root,
        &["checkout", "--quiet", "--detach", request.sha],
        request.timeout,
    )
    .await?;

    Ok(workspace)
}

/// Fetches the commit, cheaply if the server allows it.
///
/// Asking for the sha directly fetches one commit and nothing else, which is
/// what GitHub and anything with `uploadpack.allowReachableSHA1InWant` will
/// serve. Plenty of servers do not - a bare `file://` repository does not by
/// default - so the fallback fetches the ref in full. Shallow-fetching the ref
/// instead is not an option: the ref may have moved on since the webhook, and
/// the depth-1 tip would be the wrong commit.
async fn fetch(root: &Path, request: &CheckoutRequest<'_>) -> Result<(), CheckoutError> {
    let shallow = git(
        root,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--depth",
            "1",
            "origin",
            request.sha,
        ],
        request.timeout,
    )
    .await;

    match shallow {
        Ok(()) => Ok(()),
        Err(error @ (CheckoutError::Timeout { .. } | CheckoutError::Spawn(_))) => Err(error),
        Err(_) => {
            tracing::debug!(
                "server would not serve {} directly; fetching {} in full",
                request.sha,
                request.git_ref
            );
            git(
                root,
                &["fetch", "--quiet", "--no-tags", "origin", request.git_ref],
                request.timeout,
            )
            .await
        }
    }
}

async fn git(root: &Path, args: &[&str], timeout: Duration) -> Result<(), CheckoutError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        // Without these, a private repository with no usable credentials makes
        // git ask for a password. There is no terminal to ask on, so it would
        // hang until the checkout timeout rather than failing immediately.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .env("GIT_CONFIG_NOSYSTEM", "1");

    let described = args.first().copied().unwrap_or("git").to_string();

    let output = match tokio::time::timeout(timeout, command.output()).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(CheckoutError::Timeout {
                command: described,
                seconds: timeout.as_secs(),
            });
        }
    };

    if output.status.success() {
        return Ok(());
    }

    Err(CheckoutError::Git {
        command: described,
        code: output
            .status
            .code()
            .map_or_else(|| "signal".to_string(), |code| code.to_string()),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A commit sha, as git writes them.
pub fn validate_sha(sha: &str) -> Result<(), CheckoutError> {
    let invalid = |reason| {
        Err(CheckoutError::InvalidSha {
            sha: sha.to_string(),
            reason,
        })
    };

    if sha.len() < 7 {
        return invalid("too short to identify a commit");
    }
    if sha.len() > 64 {
        return invalid("longer than any git hash");
    }
    if !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return invalid("not hexadecimal");
    }
    Ok(())
}

/// A ref name, checked against the rules `git check-ref-format` applies, plus a
/// refusal of anything git would read as an option.
pub fn validate_ref(git_ref: &str) -> Result<(), CheckoutError> {
    let invalid = |reason| {
        Err(CheckoutError::InvalidRef {
            git_ref: git_ref.to_string(),
            reason,
        })
    };

    if git_ref.is_empty() {
        return invalid("empty");
    }
    // The one that matters: `--upload-pack=/tmp/x` in a ref position runs
    // /tmp/x. Everything below is hygiene; this is the exploit.
    if git_ref.starts_with('-') {
        return invalid("starts with '-', which git would read as an option");
    }
    if git_ref
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\u{7f}')
    {
        return invalid("contains whitespace or a control character");
    }
    if let Some(bad) = git_ref
        .chars()
        .find(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
    {
        return invalid(match bad {
            '~' => "contains '~'",
            '^' => "contains '^'",
            ':' => "contains ':'",
            '?' => "contains '?'",
            '*' => "contains '*'",
            '[' => "contains '['",
            _ => "contains a backslash",
        });
    }
    if git_ref.contains("..") {
        return invalid("contains '..'");
    }
    if git_ref.contains("@{") {
        return invalid("contains '@{'");
    }
    if git_ref.contains("//") {
        return invalid("contains an empty path component");
    }
    if git_ref.starts_with('/') || git_ref.ends_with('/') {
        return invalid("starts or ends with '/'");
    }
    if git_ref.ends_with('.') || git_ref.ends_with(".lock") {
        return invalid("ends with '.' or '.lock'");
    }
    Ok(())
}

/// The clone url is administrator-supplied rather than attacker-supplied, so
/// this only rules out the shapes that would confuse `git` itself.
pub fn validate_url(url: &str) -> Result<(), CheckoutError> {
    let invalid = |reason| {
        Err(CheckoutError::InvalidUrl {
            url: url.to_string(),
            reason,
        })
    };

    if url.trim().is_empty() {
        return invalid("empty");
    }
    if url.starts_with('-') {
        return invalid("starts with '-', which git would read as an option");
    }
    if url.chars().any(|c| c.is_control()) {
        return invalid("contains a control character");
    }
    Ok(())
}

/// Keeps a run id to characters that are safe in a directory name.
///
/// Ids are generated, so this never fires in practice; it is here so that a
/// future id scheme cannot turn into a path traversal by being adopted.
fn sanitize_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if cleaned.is_empty() {
        "unnamed".to_string()
    } else {
        cleaned
    }
}
