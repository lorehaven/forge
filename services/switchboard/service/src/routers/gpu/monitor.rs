//! The GPU's video memory, read from sysfs.
//!
//! The `amdgpu` driver publishes it as `mem_info_vram_total` and `mem_info_vram_used` (bytes) under each card. No
//! `rocm-smi` is needed, so the service runs in the ordinary image; it only needs `/sys/class/drm` (the pod is
//! privileged with `/dev/kfd` and `/dev/dri`, which includes it).

use std::io;
use std::path::Path;

use quench_serde::Serialize;

/// Where the kernel lists the graphics cards.
pub const DRM_ROOT: &str = "/sys/class/drm";

#[derive(Debug, Default, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
}

/// The video memory of this machine's GPU.
///
/// # Errors
/// [`io::ErrorKind::NotFound`] when no card reports video memory (no AMD GPU, or `/sys` is not visible).
pub fn get_gpu_info() -> io::Result<GpuInfo> {
    read_gpu_info(Path::new(DRM_ROOT))
}

/// As [`get_gpu_info`], for the cards listed under `root`. With several, the one with the most memory is the one models run
/// on: an integrated GPU only has a slice of main memory set aside.
///
/// # Errors
/// As [`get_gpu_info`]; or the listing cannot be read.
pub fn read_gpu_info(root: &Path) -> io::Result<GpuInfo> {
    let mut best: Option<(String, u64, u64)> = None;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // `card0`, `card1`: not `card0-DP-1`, the connectors under them.
        let is_card = name
            .strip_prefix("card")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !is_card {
            continue;
        }
        let device = entry.path().join("device");
        let (Some(total), Some(used)) = (
            read_bytes(&device.join("mem_info_vram_total")),
            read_bytes(&device.join("mem_info_vram_used")),
        ) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(n, t, _)| total > *t || (total == *t && name < *n))
        {
            best = Some((name, total, used));
        }
    }
    let (name, total, used) = best.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no card with video memory counters under {}",
                root.display()
            ),
        )
    })?;
    let (total_gb, used_gb) = (gigabytes(total), gigabytes(used));
    Ok(GpuInfo {
        name,
        total_gb: round2(total_gb),
        used_gb: round2(used_gb),
        free_gb: round2(total_gb - used_gb),
    })
}

fn read_bytes(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[allow(clippy::cast_precision_loss)] // a video memory size is far below 2^53 bytes
fn gigabytes(bytes: u64) -> f64 {
    bytes as f64 / 1024.0_f64.powi(3)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
