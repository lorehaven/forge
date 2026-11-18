//! Keeping the house tidy: files nobody sent, attachment bytes nothing points at, the trash, old delivery records.

use crate::app::App;
use crate::error::{Result, now};

/// Uploads not sent within a day are dropped.
const UPLOAD_DAYS: i64 = 1;
/// The trash is emptied after this many days.
pub const TRASH_DAYS: i64 = 30;
/// Delivery records are kept this many days.
pub const DELIVERY_DAYS: i64 = 30;
const DAY: i64 = 86_400;

/// What one pass removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Swept {
    pub uploads: u64,
    pub trash: u64,
    pub blobs: u64,
    pub deliveries: u64,
}

/// One pass.
///
/// # Errors
///
/// A database failure.
pub async fn sweep(app: &App, at: i64) -> Result<Swept> {
    let uploads = app.store.uploads_prune(at - UPLOAD_DAYS * DAY).await?;
    let trash = app.store.trash_prune(at - TRASH_DAYS * DAY).await?;
    // A blob is younger than the delivery that writes it; an hour is generous.
    let blobs = app.store.blobs_prune(at - 3600).await?;
    let deliveries = app.store.deliveries_prune(at - DELIVERY_DAYS * DAY).await?;
    Ok(Swept {
        uploads,
        trash,
        blobs,
        deliveries,
    })
}

/// Runs [`sweep`] every `maintenance_interval`, for as long as the process lives.
pub fn spawn(app: &App) {
    let app = app.clone();
    tokio::spawn(async move {
        loop {
            match sweep(&app, now()).await {
                Ok(swept) if swept != Swept::default() => {
                    quench_log::info!("maintenance: {swept:?}");
                }
                Ok(_) => {}
                Err(e) => quench_log::warn!("maintenance failed: {e}"),
            }
            tokio::time::sleep(app.config.maintenance_interval).await;
        }
    });
}
