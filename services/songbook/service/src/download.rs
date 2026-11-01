//! download: fetch the linked songs that have no file yet into the flat audio folder, as `<filename>.<ext>`.

use crate::app::App;
use crate::files;
use crate::index::{browse, int_at, str_at};
use crate::jobs::Log;
use crate::youtube::{Auth, age_restricted, run_tool};
use std::time::Duration;

/// The yt-dlp output template for a file name: `%` would start a template field, so it is doubled.
#[must_use]
pub fn output_template(dir: &std::path::Path, filename: &str) -> String {
    dir.join(format!("{}.%(ext)s", filename.replace('%', "%%")))
        .to_string_lossy()
        .into_owned()
}

/// How many downloads in a row YouTube may refuse with 403 before the run stops: the next ones would be refused too.
const MAX_REFUSED: u32 = 5;

/// Does yt-dlp's output say the video data was refused (HTTP 403)?
#[must_use]
pub fn is_forbidden(output: &str) -> bool {
    output.contains("HTTP Error 403")
}

pub async fn run(
    app: &App,
    artist: &str,
    limit: Option<u32>,
    retries: u32,
    retry_delay: u64,
    log: &Log,
) -> Result<(), String> {
    let name = artist.to_string();
    let mut rows = app
        .index
        .run(move |db| {
            let rows = db
                .query(
                    "SELECT id, title, filename, yt_url FROM songs WHERE yt_url != '' ORDER BY id",
                    &[],
                )?
                .rows;
            let mine = if name.is_empty() {
                None
            } else {
                Some(browse::songs_of(db, &name)?)
            };
            Ok(rows
                .into_iter()
                .filter(|r| mine.as_ref().is_none_or(|s| s.contains(&int_at(r, 0))))
                .map(|r| (str_at(&r, 2), str_at(&r, 3)))
                .collect::<Vec<_>>())
        })
        .await
        .map_err(|e| e.to_string())?;
    if let Some(n) = limit.filter(|n| *n > 0) {
        rows.truncate(n as usize);
    }
    if rows.is_empty() {
        log.line("No songs with a YouTube link found in the index.");
        log.line("Run sync, then Find links, first.");
        return Ok(());
    }
    std::fs::create_dir_all(&app.config.data)
        .map_err(|e| format!("cannot create {}: {e}", app.config.data.display()))?;
    let on_disk = files::file_index(&app.config.data);
    log.line(format!(
        "{} songs with links; output: {}",
        rows.len(),
        app.config.data.display()
    ));
    crate::ytdlp::ensure(&app.config, log).await;
    let auth = Auth::new(&app.config);
    let (mut succeeded, mut failed, mut skipped) = (0u32, 0u32, 0u32);
    // Songs in a row that YouTube refused with 403: it is not the songs but the tool or the login.
    let mut refused = 0u32;
    for (i, (name, url)) in rows.iter().enumerate() {
        let at = format!("[{}/{}]", i + 1, rows.len());
        if on_disk.contains_key(name) {
            skipped += 1;
            log.line(format!("{at} SKIP already downloaded/converted: {name}"));
            continue;
        }
        log.line(format!("{at} {name}"));
        // Audio only: the pictures of a music video are the bulk of the download and `convert` throws them away.
        let mut args: Vec<String> = [
            "--no-overwrites",
            "--continue",
            "--windows-filenames",
            "-f",
            "bestaudio/best",
        ]
        .map(String::from)
        .to_vec();
        args.extend(auth.args(log).await);
        args.extend([
            "--output".to_string(),
            output_template(&app.config.data, name),
            url.trim().to_string(),
        ]);
        let mut attempt = 0;
        loop {
            match run_tool("yt-dlp", &args, Duration::from_mins(30)).await {
                Ok(out) if out.ok => {
                    succeeded += 1;
                    refused = 0;
                    break;
                }
                Ok(out) => {
                    let all = format!("{}\n{}", out.stderr, out.stdout);
                    if age_restricted(&all) {
                        failed += 1;
                        log.line("  ERROR: YouTube age/authentication restriction. Configure cookies; retries will not fix missing authentication.");
                        break;
                    }
                    // A 403 on the video data does not go away by waiting, so it gets one more try, not all of them.
                    let forbidden = is_forbidden(&all);
                    if attempt >= if forbidden { retries.min(1) } else { retries } {
                        failed += 1;
                        refused = if forbidden { refused + 1 } else { 0 };
                        log.line(format!(
                            "  ERROR: yt-dlp failed after {} attempt(s): {}",
                            attempt + 1,
                            out.stderr.trim().lines().last().unwrap_or("")
                        ));
                        if refused >= MAX_REFUSED {
                            return Err(format!(
                                "YouTube refused {MAX_REFUSED} downloads in a row (HTTP 403): yt-dlp is out of date or the cookies are no longer accepted. \
                                 The service updates its own yt-dlp (see the first lines of this log); push fresh cookies and run again."
                            ));
                        }
                        break;
                    }
                    let delay = (retry_delay << attempt).min(300);
                    log.line(format!(
                        "  retrying in {delay}s (attempt {}/{retries})",
                        attempt + 1
                    ));
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    attempt += 1;
                }
                Err(e) => {
                    failed += 1;
                    log.line(format!("  ERROR: {e}"));
                    break;
                }
            }
        }
    }
    log.line(format!(
        "download run finished: {succeeded} succeeded, {skipped} skipped, {failed} failed"
    ));
    Ok(())
}
