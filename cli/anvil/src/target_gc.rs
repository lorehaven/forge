//! Keeps cargo's `target/` under a size limit.
//!
//! Cargo never prunes it, so a workspace built with several profiles, features and targets grows without bound.
//! [`prune`] removes what is cheapest to make again, until the directory fits: first every `incremental/` cache, then
//! whole profile directories (the ones holding a `.fingerprint/`), the least recently built first. Profiles named in
//! `keep` go last of all and only when `spare_kept` is false: a build that has just finished must not delete its own
//! output.

use quench_core::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const GIB: u64 = 1024 * 1024 * 1024;

/// What a [`prune`] found and did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub before: u64,
    pub after: u64,
    pub removed: Vec<PathBuf>,
    /// Still over the limit with nothing left that may be removed.
    pub over: bool,
}

/// `20`, `20G`, `512M`, `1.5G` as bytes (a bare number is gigabytes).
///
/// # Errors
/// The text is not a size.
pub fn parse_size(text: &str) -> Result<u64> {
    let text = text.trim();
    let (number, unit) = match text.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
        Some((i, _)) => text.split_at(i),
        None => (text, "G"),
    };
    let value: f64 = number
        .trim()
        .parse()
        .with_context(|| format!("`{text}` is not a size (try 20G)"))?;
    let unit = match unit
        .to_ascii_lowercase()
        .trim_end_matches('b')
        .trim_end_matches('i')
    {
        "k" => 1024,
        "m" => 1024 * 1024,
        "g" | "" => GIB,
        "t" => 1024 * GIB,
        _ => quench_core::bail!("`{text}` is not a size (try 20G)"),
    };
    if !value.is_finite() || value < 0.0 {
        quench_core::bail!("`{text}` is not a size (try 20G)");
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    Ok((value * unit as f64) as u64)
}

/// The bytes under `path` (symlinks are not followed).
#[must_use]
pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries.flatten().map(|e| dir_size(&e.path())).sum()
    })
}

fn mtime(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// A profile directory: `target/<profile>` or `target/<triple>/<profile>`.
fn profile_dirs(target: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(target) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join(".fingerprint").is_dir() {
            found.push(path);
        } else if path.is_dir()
            && let Ok(inner) = fs::read_dir(&path)
        {
            found.extend(
                inner
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.join(".fingerprint").is_dir()),
            );
        }
    }
    found
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn remove(path: &Path, dry_run: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    fs::remove_dir_all(path).with_context(|| format!("removing {}", path.display()))
}

/// Brings `target` to at most `limit` bytes, or as near as the rules allow.
///
/// `keep` names profile directories (`debug`, `release`) that are removed only as a last resort when `spare_kept` is
/// false. With `spare_kept` they are never removed, only their `incremental/` is. `dry_run` removes nothing but reports
/// what would go.
///
/// # Errors
/// A directory cannot be removed.
pub fn prune(
    target: &Path,
    limit: u64,
    keep: &[&str],
    spare_kept: bool,
    dry_run: bool,
) -> Result<Report> {
    let mut report = Report {
        before: dir_size(target),
        ..Report::default()
    };
    let mut size = report.before;
    let mut profiles = profile_dirs(target);

    // 1. incremental caches, biggest first: the cheapest thing to lose.
    let mut caches: Vec<(u64, PathBuf)> = profiles
        .iter()
        .map(|p| p.join("incremental"))
        .filter(|p| p.is_dir())
        .map(|p| (dir_size(&p), p))
        .collect();
    caches.sort_by_key(|(bytes, _)| std::cmp::Reverse(*bytes));
    for (bytes, cache) in caches {
        if size <= limit {
            break;
        }
        remove(&cache, dry_run)?;
        size = size.saturating_sub(bytes);
        report.removed.push(cache);
    }

    // 2. whole profiles, least recently built first; the kept ones last (or never).
    profiles.sort_by_key(|p| (keep.contains(&name_of(p).as_str()), mtime(p)));
    for profile in profiles {
        if size <= limit {
            break;
        }
        if spare_kept && keep.contains(&name_of(&profile).as_str()) {
            continue;
        }
        let bytes = dir_size(&profile);
        remove(&profile, dry_run)?;
        size = size.saturating_sub(bytes);
        report.removed.push(profile);
    }

    report.after = size;
    report.over = size > limit;
    Ok(report)
}

/// Where cargo puts the build output for the workspace in the working directory.
#[must_use]
pub fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from)
}

fn gib(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let value = bytes as f64 / GIB as f64;
    format!("{value:.1} GiB")
}

/// Prunes the workspace's `target/` and says what it did.
///
/// # Errors
/// As [`prune`].
pub fn run(limit: u64, keep: &[&str], spare_kept: bool, dry_run: bool) -> Result<Report> {
    use quench_cli::prelude::{Tone, print_status};
    let target = target_dir();
    let report = prune(&target, limit, keep, spare_kept, dry_run)?;
    if report.removed.is_empty() {
        if report.over {
            print_status(
                Tone::Warn,
                "anvil",
                &format!(
                    "{} is {} (limit {}): nothing left to prune",
                    target.display(),
                    gib(report.before),
                    gib(limit)
                ),
            );
        }
        return Ok(report);
    }
    let verb = if dry_run { "would remove" } else { "removed" };
    for path in &report.removed {
        print_status(Tone::Info, "anvil", &format!("{verb} {}", path.display()));
    }
    let tone = if report.over {
        Tone::Warn
    } else {
        Tone::Success
    };
    print_status(
        tone,
        "anvil",
        &format!(
            "{}: {} -> {} (limit {})",
            target.display(),
            gib(report.before),
            gib(report.after),
            gib(limit)
        ),
    );
    Ok(report)
}
