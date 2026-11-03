//! convert: turn the downloaded media in the audio folder into mp3, in place. The source goes once its mp3 is complete; a
//! leftover source whose mp3 already exists goes too when both last the same (`prune`).

use crate::files::{MEDIA, TEMP_SUFFIXES};
use crate::jobs::Log;
use crate::youtube::run_tool;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Media files below `root`, sorted.
#[must_use]
pub fn media_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    // A folder like `.replace` is somebody's work in progress.
                    if !name.starts_with('.') {
                        walk(&path, out);
                    }
                } else if !TEMP_SUFFIXES.iter().any(|s| name.ends_with(s))
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| MEDIA.contains(&e.to_lowercase().as_str()))
                {
                    out.push(path);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

async fn probe(args: &[&str]) -> Result<String, String> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_tool("ffprobe", &args, Duration::from_secs(60))
        .await
        .map(|o| o.stdout)
}

/// Seconds the file plays, per ffprobe.
pub async fn duration(path: &Path) -> Option<f64> {
    probe(&[
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "csv=p=0",
        &path.to_string_lossy(),
    ])
    .await
    .ok()?
    .trim()
    .parse()
    .ok()
}

async fn has_audio(path: &Path) -> bool {
    probe(&[
        "-v",
        "error",
        "-select_streams",
        "a",
        "-show_entries",
        "stream=index",
        "-of",
        "csv=p=0",
        &path.to_string_lossy(),
    ])
    .await
    .map_or(true, |o| !o.trim().is_empty()) // when ffprobe cannot tell, let ffmpeg try
}

/// `Some(true)` when both last the same (within 2 s), `Some(false)` when they differ, `None` when one cannot be read.
pub async fn same_length(a: &Path, b: &Path) -> Option<bool> {
    Some((duration(a).await? - duration(b).await?).abs() <= 2.0)
}

/// The ffmpeg arguments that write `destination` as mp3 (VBR quality 0) from `source`.
#[must_use]
pub fn ffmpeg_args(source: &Path, destination: &Path) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-y", "-i"]
        .map(String::from)
        .to_vec();
    args.push(source.to_string_lossy().into_owned());
    args.extend(
        [
            "-vn",
            "-map",
            "0:a:0",
            "-map_metadata",
            "0",
            "-codec:a",
            "libmp3lame",
            "-q:a",
            "0",
            "-f",
            "mp3",
            "-nostats",
        ]
        .map(String::from),
    );
    args.push(destination.to_string_lossy().into_owned());
    args
}

pub async fn run(root: &Path, prune: bool, log: &Log) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!(
            "Input directory does not exist: {}",
            root.display()
        ));
    }
    let all = media_files(root);
    // An mp3 is what the others become: nothing to do for it, so it is counted, not walked.
    let (done, files): (Vec<PathBuf>, Vec<PathBuf>) = all.into_iter().partition(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mp3"))
    });
    if files.is_empty() {
        log.line(format!(
            "Nothing to convert in {} ({} mp3 file(s) already)",
            root.display(),
            done.len()
        ));
        return Ok(());
    }
    log.line(format!(
        "Found {} file(s) to convert to mp3 ({} mp3 file(s) already)",
        files.len(),
        done.len()
    ));
    let (mut converted, mut skipped, mut failed, mut pruned, mut silent) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    for (i, source) in files.iter().enumerate() {
        let at = format!("[{}/{}]", i + 1, files.len());
        let destination = source.with_extension("mp3");
        let shown = source
            .strip_prefix(root)
            .unwrap_or(source)
            .display()
            .to_string();
        if destination.is_file() {
            skipped += 1;
            if prune {
                match same_length(source, &destination).await {
                    Some(true) => {
                        if std::fs::remove_file(source).is_ok() {
                            pruned += 1;
                            log.line(format!("{at} removed {shown}: the mp3 exists"));
                        }
                    }
                    Some(false) => log.line(format!(
                        "{at} keeping {shown}: the mp3 differs in length (partial?)"
                    )),
                    None => log.line(format!("{at} keeping {shown}: cannot read the lengths")),
                }
            }
            continue;
        }
        if !has_audio(source).await {
            silent += 1;
            log.line(format!(
                "{at} SKIP {shown}: no audio stream (a video-only download); delete it"
            ));
            continue;
        }
        log.line(format!(
            "{at} {shown} -> {}",
            destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        let tmp = PathBuf::from(format!("{}.part", destination.display()));
        match run_tool(
            "ffmpeg",
            &ffmpeg_args(source, &tmp),
            Duration::from_mins(30),
        )
        .await
        {
            Ok(out) if out.ok => match std::fs::rename(&tmp, &destination) {
                Ok(()) => {
                    converted += 1;
                    let _ = std::fs::remove_file(source);
                }
                Err(e) => {
                    failed += 1;
                    let _ = std::fs::remove_file(&tmp);
                    log.line(format!("  ERROR: {e}"));
                }
            },
            Ok(out) => {
                failed += 1;
                let _ = std::fs::remove_file(&tmp);
                log.line(format!(
                    "  ERROR: ffmpeg: {}",
                    out.stderr.trim().lines().last().unwrap_or("failed")
                ));
            }
            Err(e) => {
                failed += 1;
                let _ = std::fs::remove_file(&tmp);
                log.line(format!("  ERROR: {e}"));
            }
        }
    }
    log.line(format!("Finished: {converted} converted, {skipped} skipped, {silent} without audio, {failed} failed, {pruned} redundant sources deleted."));
    Ok(())
}
