//! Keeping yt-dlp current.
//!
//! YouTube changes often enough that a yt-dlp a few months old is refused ("HTTP Error 403: Forbidden" on every download),
//! and the copy a distribution packs into an image only ages. So the service keeps a copy of its own: the official
//! standalone build, downloaded to `$MUSIC_HOME/bin/yt-dlp` (checked against the `SHA2-256SUMS` the release publishes) and
//! brought up to date with `yt-dlp -U` before the jobs that use it, at most every 12 hours. When that cannot be done the
//! job goes on with what there is: the copy it has, or the `yt-dlp` on the `PATH`. `YTDLP_UPDATE=false` turns it off.

use crate::config::Config;
use crate::jobs::Log;
use crate::youtube::run_tool;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Where the releases are published; `latest` follows the newest stable one.
const BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";

/// How long a check stays good.
const EVERY: Duration = Duration::from_hours(12);

/// The copy the service keeps, once it has one.
static BINARY: Mutex<Option<PathBuf>> = Mutex::new(None);
static CHECKED: Mutex<Option<Instant>> = Mutex::new(None);

/// The release file for this platform: the self-contained build that matches the C library the service was built for (the
/// image is musl, a developer's machine usually glibc; neither build runs on the other).
#[must_use]
pub const fn asset() -> Option<&'static str> {
    if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "musl"
    )) {
        Some("yt-dlp_musllinux")
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "aarch64",
        target_env = "musl"
    )) {
        Some("yt-dlp_musllinux_aarch64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("yt-dlp_linux")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("yt-dlp_linux_aarch64")
    } else {
        None
    }
}

/// What to run for `yt-dlp`: the copy the service keeps, else the one on the `PATH`.
#[must_use]
pub fn program() -> PathBuf {
    BINARY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| PathBuf::from("yt-dlp"))
}

/// The hash a `SHA2-256SUMS` file lists for `asset` (`<hash>  <name>`, the name optionally starting with `*`).
#[must_use]
pub fn checksum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == asset).then(|| hash.to_lowercase())
    })
}

fn enabled() -> bool {
    !matches!(
        quench_core::env::get_or("YTDLP_UPDATE", "true")
            .trim()
            .to_lowercase()
            .as_str(),
        "false" | "0" | "no" | "off"
    )
}

fn fresh() -> bool {
    CHECKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some_and(|at| at.elapsed() < EVERY)
}

fn remember(binary: &Path) {
    *BINARY.lock().unwrap_or_else(PoisonError::into_inner) = Some(binary.to_path_buf());
    *CHECKED.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
}

async fn version(binary: &Path) -> Option<String> {
    let out = run_tool(
        &binary.to_string_lossy(),
        &["--version".to_string()],
        Duration::from_secs(60),
    )
    .await
    .ok()?;
    out.ok.then(|| out.stdout.trim().to_string())
}

/// Makes sure the service has a current yt-dlp, telling the log what it did. Never fails a job: a problem is a line in the
/// log and the job goes on with the yt-dlp it can reach.
pub async fn ensure(config: &Config, log: &Log) {
    if !enabled() || fresh() {
        return;
    }
    let Some(asset) = asset() else {
        log.line("yt-dlp: no standalone build for this platform; using the one on the PATH");
        return;
    };
    let binary = config.home.join("bin").join("yt-dlp");
    if !binary.is_file() {
        log.line("yt-dlp: fetching the current release (the service keeps its own copy)");
        if let Err(e) = install(asset, &binary).await {
            log.line(format!(
                "yt-dlp: cannot fetch it ({e}); using the one on the PATH"
            ));
            return;
        }
    }
    let before = version(&binary).await;
    match run_tool(
        &binary.to_string_lossy(),
        &["-U".to_string()],
        Duration::from_secs(180),
    )
    .await
    {
        Ok(out) if out.ok => {
            let after = version(&binary).await;
            match (before, after) {
                (Some(a), Some(b)) if a != b => log.line(format!("yt-dlp: updated {a} -> {b}")),
                (_, Some(b)) => log.line(format!("yt-dlp {b} is current")),
                _ => log.line("yt-dlp: updated"),
            }
        }
        Ok(out) => log.line(format!(
            "yt-dlp: the update failed ({}); going on with {}",
            out.stderr.trim().lines().last().unwrap_or("no message"),
            before.as_deref().unwrap_or("the copy it has")
        )),
        Err(e) => log.line(format!("yt-dlp: the update failed ({e})")),
    }
    remember(&binary);
}

/// Downloads `asset` of the latest release to `destination`, after checking it against the published checksum.
async fn install(asset: &str, destination: &Path) -> Result<(), String> {
    let client = quench_client::transport::Client::builder()
        .timeout(Duration::from_secs(300))
        .user_agent("songbook/1.0")
        .build()
        .unwrap_or_default();
    let fetch = |name: String| {
        let client = client.clone();
        async move {
            let response = client
                .get(format!("{BASE}/{name}"))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("{name}: HTTP {}", response.status().as_u16()));
            }
            response.bytes().await.map_err(|e| e.to_string())
        }
    };
    let sums = fetch("SHA2-256SUMS".to_string()).await?;
    let wanted = checksum_for(&String::from_utf8_lossy(&sums), asset)
        .ok_or_else(|| format!("SHA2-256SUMS does not list {asset}"))?;
    let bytes = fetch(asset.to_string()).await?;
    let got = quench_core::hex::encode(quench_crypto::hash::sha256(&bytes));
    if got != wanted {
        return Err(format!("checksum mismatch for {asset}"));
    }
    if let Some(dir) = destination.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let partial = destination.with_extension("part");
    std::fs::write(&partial, &bytes).map_err(|e| format!("{}: {e}", partial.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&partial, destination).map_err(|e| e.to_string())
}
