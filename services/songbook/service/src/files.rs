//! The audio folder: one flat directory, one file per song, named after the index.

use std::collections::HashMap;
use std::path::Path;

/// Names of files yt-dlp and ffmpeg leave while they work.
pub const TEMP_SUFFIXES: [&str; 3] = [".part", ".ytdl", ".temp"];

/// Media extensions `convert` reads.
pub const MEDIA: [&str; 18] = [
    "webm", "mkv", "mp4", "m4a", "mp3", "opus", "ogg", "oga", "flac", "wav", "aac", "mpg", "mpeg",
    "mov", "avi", "m4v", "3gp", "ts",
];

/// `{file stem: [file names]}` for the finished files directly in `root` (any extension; temporary files ignored).
#[must_use]
pub fn file_index(root: &Path) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().is_ok_and(|t| t.is_file())
                && !TEMP_SUFFIXES.iter().any(|s| name.ends_with(s))
            {
                let stem = name
                    .rsplit_once('.')
                    .map_or(name.as_str(), |(s, _)| s)
                    .to_string();
                out.entry(stem).or_default().push(name);
            }
        }
    }
    out
}
