//! images: find the pictures of artists and albums (singles are albums of the kind `single`) and keep them as files.
//!
//! One file per picture in `meta/images`, named after the Deezer id: `album-<id>.jpg`, `artist-<id>.jpg` (500 pixels, as
//! Deezer gives them). What Deezer has no picture for gets an empty `<name>.none` beside it, so the next run does not ask
//! again; `retry_missing` removes those. What is on disk is left alone, so a run only fetches what is new.

use crate::app::App;
use crate::config::Config;
use crate::index::{int_at, str_at};
use crate::jobs::Log;
use std::path::{Path, PathBuf};

/// How many pictures in a row may fail before the run stops: the network is down, not the pictures.
const MAX_FAILED: u32 = 10;

#[must_use]
pub fn dir(config: &Config) -> PathBuf {
    config.meta().join("images")
}

#[must_use]
pub fn album_name(deezer_id: i64) -> String {
    format!("album-{deezer_id}.jpg")
}

#[must_use]
pub fn artist_name(deezer_id: i64) -> String {
    format!("artist-{deezer_id}.jpg")
}

fn marker(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.none"))
}

/// Whether the bytes are a JPEG or a PNG (an error page is not a picture).
#[must_use]
pub fn is_picture(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

/// Writes a file whole, so a run stopped half-way leaves none.
fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let temp = dir.join(format!("{name}.tmp"));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, dir.join(name))
}

/// What one run did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub saved: u32,
    pub none: u32,
    pub had: u32,
    pub failed: u32,
}

pub async fn run(app: &App, retry_missing: bool, log: &Log) -> Result<(), String> {
    let dir = dir(&app.config);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    if retry_missing {
        let mut cleared = 0;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().ends_with(".none")
                    && std::fs::remove_file(entry.path()).is_ok()
                {
                    cleared += 1;
                }
            }
        }
        log.line(format!(
            "asking again for {cleared} pictures Deezer had none for"
        ));
    }
    let (albums, artists) = app
        .index
        .run(|db| {
            let albums: Vec<(i64, String)> = db
                .query(
                    "SELECT deezer_id, title FROM albums WHERE deezer_id IS NOT NULL ORDER BY id",
                    &[],
                )?
                .rows
                .iter()
                .map(|r| (int_at(r, 0), str_at(r, 1)))
                .collect();
            let artists: Vec<(i64, String)> = db
                .query(
                    "SELECT deezer_id, name FROM artists WHERE deezer_id IS NOT NULL ORDER BY id",
                    &[],
                )?
                .rows
                .iter()
                .map(|r| (int_at(r, 0), str_at(r, 1)))
                .collect();
            Ok((albums, artists))
        })
        .await
        .map_err(|e| e.to_string())?;
    log.line(format!(
        "{} albums and singles, {} artists",
        albums.len(),
        artists.len()
    ));

    log.line("\nalbums and singles");
    let a = fetch(app, &dir, &albums, album_name, log, |app, id| async move {
        app.deezer.cover_url(id).await
    })
    .await?;
    log.line(format!(
        "albums: {} saved, {} already there, {} with no picture, {} failed",
        a.saved, a.had, a.none, a.failed
    ));
    log.line("\nartists");
    let b = fetch(
        app,
        &dir,
        &artists,
        artist_name,
        log,
        |app, id| async move { app.deezer.picture_url(id).await },
    )
    .await?;
    log.line(format!(
        "artists: {} saved, {} already there, {} with no picture, {} failed",
        b.saved, b.had, b.none, b.failed
    ));
    if a.failed + b.failed > 0 {
        log.line("Some pictures failed: run it again to fetch them.");
    }
    Ok(())
}

async fn fetch<F, Fut>(
    app: &App,
    dir: &Path,
    items: &[(i64, String)],
    name_of: fn(i64) -> String,
    log: &Log,
    address: F,
) -> Result<Counts, String>
where
    F: Fn(App, i64) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<String, String>> + Send,
{
    let mut counts = Counts::default();
    let mut in_a_row = 0u32;
    for (i, (id, title)) in items.iter().enumerate() {
        let name = name_of(*id);
        if dir.join(&name).exists() {
            counts.had += 1;
            continue;
        }
        if marker(dir, &name).exists() {
            counts.none += 1;
            continue;
        }
        let step = async {
            let url = address(app.clone(), *id).await?;
            if url.is_empty() {
                return Ok(None);
            }
            let bytes = app.deezer.image(&url).await?;
            if is_picture(&bytes) {
                Ok(Some(bytes))
            } else {
                Err(format!("{url}: not a picture"))
            }
        };
        match step.await {
            Ok(Some(bytes)) => {
                write(dir, &name, &bytes).map_err(|e| format!("{name}: {e}"))?;
                counts.saved += 1;
                in_a_row = 0;
            }
            Ok(None) => {
                let _ = std::fs::write(marker(dir, &name), b"");
                counts.none += 1;
                in_a_row = 0;
                log.line(format!("no picture: {title}"));
            }
            Err(e) => {
                counts.failed += 1;
                in_a_row += 1;
                log.line(format!("failed: {title}: {e}"));
                if in_a_row >= MAX_FAILED {
                    return Err(format!("{MAX_FAILED} pictures in a row failed: {e}"));
                }
            }
        }
        if (i + 1) % 100 == 0 {
            log.line(format!("{}/{}", i + 1, items.len()));
        }
    }
    Ok(counts)
}
