//! fix links: find songs whose YouTube link does not match the song, and give them a new link and a new file.
//!
//! A link found by the search (not one set by hand) is suspect when the song's title, without `(feat. X)` and remaster
//! tags, is not in the title of the linked video (compared without case or punctuation). A suspect song's audio
//! file is moved to `.mismatch-old` in the audio folder (nothing is deleted), its link is cleared, and then find links,
//! download and convert run for it. This is not a step of the pipeline: it is started on its own, and a preview
//! (`dry_run`) lists the suspects and changes nothing.
//!
//! Find links looks at every song without a link, not only the cleared ones; songs it already marked as having no match are
//! left alone.

use crate::app::App;
use crate::index::{self, Db, Result, int, int_at, opt_str_at, str_at};
use crate::jobs::Log;
use crate::{convert, download, files, youtube};
use songbook_core::JobStart;
use songbook_core::naming::{clean_title, norm};
use std::path::Path;

/// Where the audio files of the songs being fixed are put aside, inside the audio folder.
pub const ASIDE: &str = ".mismatch-old";

/// A song whose link does not look like it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspect {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub filename: String,
    pub yt_title: String,
    pub yt_channel: String,
}

/// Does the video's title contain the song's title? A song with no usable title is never in doubt.
#[must_use]
pub fn matches(title: &str, video_title: &str) -> bool {
    if title.trim().is_empty() {
        return true;
    }
    let wanted = norm(&clean_title(title));
    wanted.is_empty() || norm(video_title).contains(&wanted)
}

/// The songs with an automatic link whose video title does not contain the song's title; `artist` limits it to the songs of
/// one followed artist.
pub async fn suspects(db: &mut Db, artist: &str) -> Result<Vec<Suspect>> {
    let rows = db.query(
        "SELECT s.id, s.title, s.filename, s.yt_title, s.yt_channel, a.name FROM songs s \
         JOIN song_artist sa ON sa.song_id = s.id AND sa.position = 0 JOIN artists a ON a.id = sa.artist_id \
         WHERE s.yt_manual = 0 AND s.yt_url != '' AND s.yt_title != '' ORDER BY s.id",
        &[],
    ).await?;
    let mine = if artist.is_empty() {
        None
    } else {
        Some(index::browse::songs_of(db, artist).await?)
    };
    Ok(rows
        .rows
        .iter()
        .filter(|r| mine.as_ref().is_none_or(|set| set.contains(&int_at(r, 0))))
        .filter(|r| !matches(&str_at(r, 1), &str_at(r, 3)))
        .map(|r| Suspect {
            id: int_at(r, 0),
            title: str_at(r, 1),
            filename: str_at(r, 2),
            yt_title: str_at(r, 3),
            yt_channel: opt_str_at(r, 4).unwrap_or_default(),
            artist: str_at(r, 5),
        })
        .collect())
}

/// Clears the links of `ids` so that the next find links looks at them again.
pub async fn clear(db: &mut Db, ids: &[i64]) -> Result<()> {
    for id in ids {
        db.execute(
            "UPDATE songs SET yt_url = '', yt_channel = '', yt_title = '', yt_official = 0, yt_score = NULL, yt_views = NULL, \
             yt_manual = 0, yt_status = NULL, yt_candidate = '' WHERE id = ?",
            &[int(*id)],
        ).await?;
    }
    Ok(())
}

/// Moves the files named `stem.*` from `data` into `data/.mismatch-old`; returns how many.
fn put_aside(
    data: &Path,
    stem: &str,
    on_disk: &std::collections::HashMap<String, Vec<String>>,
) -> std::io::Result<u32> {
    let mut moved = 0;
    for name in on_disk.get(stem).into_iter().flatten() {
        std::fs::create_dir_all(data.join(ASIDE))?;
        std::fs::rename(data.join(name), data.join(ASIDE).join(name))?;
        moved += 1;
    }
    Ok(moved)
}

/// Lists the suspects; unless `job.dry_run`, fixes them. `job.artist` and `job.limit` narrow it.
pub async fn run(app: &App, job: &JobStart, log: &Log) -> std::result::Result<(), String> {
    let artist = job.artist.clone();
    let mut found = app
        .index
        .run(async move |db| suspects(db, &artist).await)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(n) = job.limit.filter(|n| *n > 0) {
        found.truncate(n as usize);
    }
    for s in &found {
        log.line(format!(
            "  {} - {}  <-  {:?} [{}]",
            s.artist, s.title, s.yt_title, s.yt_channel
        ));
    }
    log.line(format!("{} songs whose link does not match", found.len()));
    if job.dry_run {
        if !found.is_empty() {
            log.line("Nothing was changed. Run \"Fix links\" to fix these.");
        }
        return Ok(());
    }
    if found.is_empty() {
        return Ok(());
    }
    let data = app.config.data.clone();
    let on_disk = files::file_index(&data);
    let (mut moved, mut failed) = (0, 0);
    for s in &found {
        match put_aside(&data, &s.filename, &on_disk) {
            Ok(n) => moved += n,
            Err(e) => {
                log.line(format!(
                    "  ERROR: cannot move the file of {}: {e}",
                    s.filename
                ));
                failed += 1;
            }
        }
    }
    if failed > 0 {
        // A link cleared with its old file still in place would never be looked up (find links skips downloaded songs).
        return Err(format!(
            "{failed} files could not be moved; nothing else was changed"
        ));
    }
    let ids: Vec<i64> = found.iter().map(|s| s.id).collect();
    app.index
        .run(async move |db| clear(db, &ids).await)
        .await
        .map_err(|e| e.to_string())?;
    log.line(format!(
        "cleared {} links; moved {moved} files to {}",
        found.len(),
        data.join(ASIDE).display()
    ));
    log.line("\n== find links ==");
    youtube::run(app, "", None, false, log)
        .await
        .map_err(|e| format!("find links: {e}"))?;
    log.line("\n== download ==");
    download::run(app, "", None, 3, 5, log)
        .await
        .map_err(|e| format!("download: {e}"))?;
    log.line("\n== convert ==");
    convert::run(&data, true, log)
        .await
        .map_err(|e| format!("convert: {e}"))?;
    log.line(format!(
        "\n== fix links finished ==\nThe old files are in {}; delete that folder when the new ones are right.",
        data.join(ASIDE).display()
    ));
    Ok(())
}
