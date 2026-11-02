//! The whole pipeline as one job: sync, find links (looking again at songs that had no match), download, convert, images.
//!
//! The steps are the ones the buttons of the Pipeline page start, one after the other, writing to one log. The first step
//! that fails stops the run, since the later ones would work on what it did not deliver. `artist` limits find links and
//! download, as it does when they are started on their own; sync always reads every followed artist.

use crate::app::App;
use crate::jobs::Log;
use crate::{convert, download, images, sync, youtube};
use songbook_core::JobStart;

/// The steps, in the order they run.
pub const STEPS: [&str; 5] = ["sync", "find links", "download", "convert", "images"];

fn header(log: &Log, step: usize) {
    log.line(format!(
        "\n== {}/{} {} ==",
        step + 1,
        STEPS.len(),
        STEPS[step]
    ));
}

/// Runs every step; `Err` names the step that failed.
pub async fn run(app: &App, job: &JobStart, log: &Log) -> Result<(), String> {
    header(log, 0);
    sync::run(app, job.singles, log)
        .await
        .map_err(|e| format!("{}: {e}", STEPS[0]))?;
    header(log, 1);
    youtube::run(app, &job.artist, job.limit, true, log)
        .await
        .map_err(|e| format!("{}: {e}", STEPS[1]))?;
    header(log, 2);
    download::run(app, &job.artist, job.limit_download, 3, 5, log)
        .await
        .map_err(|e| format!("{}: {e}", STEPS[2]))?;
    header(log, 3);
    convert::run(&app.config.data, true, log)
        .await
        .map_err(|e| format!("{}: {e}", STEPS[3]))?;
    header(log, 4);
    images::run(app, false, log)
        .await
        .map_err(|e| format!("{}: {e}", STEPS[4]))?;
    log.line("\n== pipeline finished ==");
    Ok(())
}
