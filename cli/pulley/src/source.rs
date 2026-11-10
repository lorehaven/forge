//! Whether a job's source is there to be synced from.
//!
//! A source can be absent in two ways. A host that is switched off cannot be reached, and rsync fails without touching the
//! destination. A host that is up but whose data is not (a drive that has not been mounted yet) is worse: the source answers,
//! and a sync that deletes would make the destination match what it finds. So the source has to answer and be a directory
//! that exists; and for a job that **deletes**, it also has to hold something, because an empty source is what an unmounted
//! drive looks like. A job may add a `marker`: any path that is always present in the source (an existing file or directory;
//! nothing has to be created for it).
//!
//! Nothing here looks at how much would change: a project deleted on purpose, thousands of files and all, is propagated.
//!
//! The check is one `rsync --list-only` over the same `ssh` and the same configuration the sync itself uses, so aliases,
//! ports and jump hosts mean what they do for the sync. A local source is looked at directly.

use crate::config::Job;
use crate::rsync;
use std::path::{Component, Path};
use std::process::{Command, Stdio};

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// The source answered and, if there is a marker, it is there.
    Ready,
    /// Nothing answered: a host that is off or cut off, a refused connection, no credentials. The text is why.
    Unreachable(String),
    /// Something answered, but what a sync needs is not there: no such directory, or no marker in it.
    NotReady(String),
}

impl Availability {
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Both are the same *kind* of finding: a source that stays unreachable is not news every time the words differ.
    #[must_use]
    pub fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// Why it is not ready; empty when it is.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Ready => "",
            Self::Unreachable(why) | Self::NotReady(why) => why,
        }
    }
}

/// Looks at the source of `job`.
#[must_use]
pub fn check(job: &Job) -> Availability {
    if let Some(marker) = job.marker.as_deref()
        && !is_plain_relative(marker)
    {
        return Availability::NotReady(format!(
            "marker `{marker}` must be a relative path inside the source"
        ));
    }
    if rsync::is_remote(&job.src) {
        check_remote(job)
    } else {
        check_local(job)
    }
}

/// A path that stays inside the directory it is joined to.
fn is_plain_relative(path: &str) -> bool {
    !path.trim().is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

/// Whether the source of `job` must hold something to count as there: a job that deletes, and has no marker to say so.
const fn needs_content(job: &Job) -> bool {
    job.delete && job.marker.is_none()
}

fn empty_source(src: &str) -> Availability {
    Availability::NotReady(format!(
        "the source directory {src} is empty, as an unmounted drive is; nothing is synced, so nothing is deleted"
    ))
}

fn check_local(job: &Job) -> Availability {
    let src = Path::new(&job.src);
    if !src.is_dir() {
        return Availability::NotReady(format!("source directory {} does not exist", job.src));
    }
    match job.marker.as_deref() {
        Some(marker) if !src.join(marker).exists() => {
            Availability::NotReady(format!("`{marker}` is not in {}", job.src))
        }
        None if needs_content(job)
            && std::fs::read_dir(src).is_ok_and(|mut entries| entries.next().is_none()) =>
        {
            empty_source(&job.src)
        }
        _ => Availability::Ready,
    }
}

/// What to list: the marker if there is one; otherwise the source directory, with its contents (a trailing slash) when it must
/// hold something and just its own entry (none) when it only has to be there.
#[must_use]
pub fn probe_target(src: &str, marker: Option<&str>, contents: bool) -> String {
    let base = src.trim_end_matches('/');
    match (marker, contents) {
        (Some(marker), _) => format!("{base}/{}", marker.trim_start_matches("./")),
        (None, true) => format!("{base}/"),
        (None, false) => base.to_string(),
    }
}

/// Whether a listing of a directory (`rsync --list-only dir/`) shows anything besides the directory itself, which it lists as
/// `.`. A line is `drwxr-xr-x  size  date time  name`, and the name may hold spaces.
#[must_use]
pub fn has_entries(listing: &str) -> bool {
    listing.lines().any(|line| {
        let mut rest = line;
        for _ in 0..4 {
            rest = rest.trim_start();
            match rest.find(char::is_whitespace) {
                Some(end) => rest = &rest[end..],
                None => return false,
            }
        }
        let name = rest.trim_start();
        !name.is_empty() && name != "."
    })
}

fn check_remote(job: &Job) -> Availability {
    let mut cmd = Command::new("rsync");
    cmd.arg("--list-only").arg("--timeout=15");
    if let Some(shell) = rsync::remote_shell(true) {
        cmd.arg("-e").arg(shell);
    }
    let contents = needs_content(job);
    cmd.arg(probe_target(&job.src, job.marker.as_deref(), contents));
    match cmd.stdin(Stdio::null()).output() {
        Ok(out) => {
            let found = classify_probe(out.status.code(), &String::from_utf8_lossy(&out.stderr));
            if found.is_ready() && contents && !has_entries(&String::from_utf8_lossy(&out.stdout)) {
                empty_source(&job.src)
            } else {
                found
            }
        }
        Err(error) => Availability::Unreachable(format!("could not run rsync: {error}")),
    }
}

/// What a finished `rsync --list-only` means.
///
/// Exit 23 with "No such file" is rsync's answer for a source it reached but could not find the thing in; 255 is `ssh` failing
/// to connect or to log in. Anything else counts as not reachable.
#[must_use]
pub fn classify_probe(code: Option<i32>, stderr: &str) -> Availability {
    let why = reason_line(stderr);
    match code {
        Some(0) => Availability::Ready,
        Some(23) if stderr.contains("No such file") || stderr.contains("Permission denied") => {
            Availability::NotReady(why.to_string())
        }
        Some(255) => Availability::Unreachable(why.to_string()),
        Some(code) => Availability::Unreachable(format!("rsync exited with {code}: {why}")),
        None => Availability::Unreachable("the check was interrupted".to_string()),
    }
}

/// The line of rsync's stderr that says why, not the boilerplate around it. For a host that is off that is `ssh`'s own line
/// (`ssh: connect to host ... No route to host`), which rsync follows with two lines about the closed connection and its exit code;
/// for a missing path it is rsync's `link_stat ... No such file`, before its summary. Falls back to the last line, then to a stock phrase.
fn reason_line(stderr: &str) -> &str {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .copied()
        .find(|line| {
            !line.starts_with("rsync error:")
                && !line.starts_with("rsync: connection unexpectedly closed")
        })
        .or_else(|| lines.last().copied())
        .unwrap_or("no message")
}
