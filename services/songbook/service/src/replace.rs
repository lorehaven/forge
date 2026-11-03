//! replace: put the song of a new YouTube link in place of the one a song has.
//!
//! For a song that is wrong, or the wrong version. The new audio is downloaded and converted to mp3 beside the audio folder's
//! files, in a folder of its own; only when that worked do the old files of the song go and the new mp3 take their name, so a
//! link that does not work costs nothing. The link is then the song's (as if set by hand).

use crate::app::App;
use crate::index::{browse, str_at};
use crate::jobs::Log;
use crate::{convert, download, files};
use std::path::Path;

/// The folder (inside the audio folder) a replacement is made in. A name no song file can have.
const WORK: &str = ".replace";

/// Replaces the audio of song `track_id` with the one at `url`.
pub async fn run(app: &App, track_id: i64, url: &str, log: &Log) -> Result<(), String> {
    let url = url.trim().to_string();
    if !browse::is_link(&url) {
        return Err("the link must start with http:// or https://".into());
    }
    let row = app
        .index
        .run(move |db| {
            Ok(db
                .query(
                    "SELECT title, filename FROM songs WHERE id = ?",
                    &[crate::index::int(track_id)],
                )?
                .rows
                .first()
                .map(|r| (str_at(r, 0), str_at(r, 1))))
        })
        .await
        .map_err(|e| e.to_string())?;
    let Some((title, name)) = row else {
        return Err("unknown track".into());
    };
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err("the song has no usable file name".into());
    }
    let data = app.config.data.clone();
    let work = data.join(WORK);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    log.line(format!("Replacing \"{title}\" ({name}) with {url}"));
    crate::ytdlp::ensure(&app.config, log).await;
    let outcome = make(app, &data, &work, &name, &url, log).await;
    let _ = std::fs::remove_dir_all(&work);
    outcome?;
    // The link now is the song's own, so that the next download run does not look for another.
    let kept = url.clone();
    app.index
        .run(move |db| browse::set_link(db, track_id, &kept))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| "the new link was refused".to_string())?;
    log.line("Replaced. Devices that kept the old song offline keep it until they drop and keep it again.");
    Ok(())
}

/// Downloads and converts into `work`, then swaps the files of `name` in `data`.
async fn make(
    app: &App,
    data: &Path,
    work: &Path,
    name: &str,
    url: &str,
    log: &Log,
) -> Result<(), String> {
    download::fetch_one(app, work, name, url, log).await?;
    convert::run(work, false, log).await?;
    let fresh = work.join(format!("{name}.mp3"));
    if !fresh.is_file() {
        return Err("the download gave no audio (see the lines above)".into());
    }
    for line in swap(data, &fresh, name)? {
        log.line(line);
    }
    Ok(())
}

/// Removes the files of `name` in `data` (any extension) and moves `fresh` in as `<name>.mp3`; what it did, line by line.
pub fn swap(data: &Path, fresh: &Path, name: &str) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    let old: Vec<String> = files::file_index(data).remove(name).unwrap_or_default();
    for file in &old {
        std::fs::remove_file(data.join(file)).map_err(|e| format!("cannot remove {file}: {e}"))?;
        done.push(format!("removed {file}"));
    }
    let target = data.join(format!("{name}.mp3"));
    std::fs::rename(fresh, &target).map_err(|e| format!("cannot move the new file in: {e}"))?;
    done.push(format!("now {}", target.display()));
    Ok(done)
}
