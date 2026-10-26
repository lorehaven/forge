//! `node_exporter`: `GET <url>/metrics`.

use crate::domain::model::{Cpu, Filesystem, Gpu, Reading, Temp, is_real_nic, is_whole_disk};
use crate::domain::prom::{Exposition, parse};
use quench_client::transport::Client;
use std::collections::BTreeMap;

pub async fn scrape(http: &Client, base: &str) -> Result<Reading, String> {
    let text = super::get_text(http, &format!("{base}/metrics")).await?;
    let exposition = parse(&text);
    if exposition.samples.is_empty() {
        return Err("no metrics in the response (is this node_exporter?)".into());
    }
    Ok(reading(&exposition, super::now()))
}

const SKIP_FS: [&str; 12] = [
    "tmpfs",
    "devtmpfs",
    "overlay",
    "squashfs",
    "ramfs",
    "efivarfs",
    "proc",
    "sysfs",
    "cgroup2",
    "devpts",
    "fuse.portal",
    "nsfs",
];

#[must_use]
pub fn reading(x: &Exposition, at: f64) -> Reading {
    let mut cpu = Cpu::default();
    let mut cores: BTreeMap<String, Cpu> = BTreeMap::new();
    for s in x.named("node_cpu_seconds_total") {
        let idle = matches!(s.label("mode"), Some("idle" | "iowait"));
        let core = cores
            .entry(s.label("cpu").unwrap_or("0").to_string())
            .or_default();
        core.total_secs += s.value;
        cpu.total_secs += s.value;
        if !idle {
            core.busy_secs += s.value;
            cpu.busy_secs += s.value;
        }
    }
    let mut cores: Vec<_> = cores.into_iter().collect();
    cores.sort_by_key(|(id, _)| id.parse::<u32>().unwrap_or(u32::MAX));

    let mut seen = std::collections::HashSet::new();
    let filesystems = x
        .named("node_filesystem_size_bytes")
        .filter(|s| {
            let fstype = s.label("fstype").unwrap_or_default();
            s.label("device").is_some_and(|d| d.starts_with("/dev/"))
                && !SKIP_FS.contains(&fstype)
                && s.value > 0.0
        })
        .filter_map(|s| {
            let device = s.label("device")?.to_string();
            let mount = s.label("mountpoint").map(str::to_string);
            let avail = x
                .named("node_filesystem_avail_bytes")
                .find(|a| {
                    a.label("mountpoint") == s.label("mountpoint")
                        && a.label("device") == s.label("device")
                })?
                .value;
            // A bind mount shows the same device twice; the first mount is enough.
            seen.insert(device.clone()).then_some(Filesystem {
                device,
                mount,
                size: s.value,
                avail,
            })
        })
        .collect();

    let disks = x
        .named("node_disk_read_bytes_total")
        .filter_map(|r| {
            let dev = r.label("device")?;
            if !is_whole_disk(dev) {
                return None;
            }
            let written = x
                .named("node_disk_written_bytes_total")
                .find(|w| w.label("device") == Some(dev))?;
            Some((dev.to_string(), r.value, written.value))
        })
        .collect();

    let nics = x
        .named("node_network_receive_bytes_total")
        .filter_map(|r| {
            let dev = r.label("device")?;
            if !is_real_nic(dev) {
                return None;
            }
            let sent = x
                .named("node_network_transmit_bytes_total")
                .find(|w| w.label("device") == Some(dev))?;
            Some((dev.to_string(), r.value, sent.value))
        })
        .collect();

    // hwmon: one row per chip (the hottest sensor), named by `node_hwmon_chip_names` when it has one.
    let mut chips: BTreeMap<String, f64> = BTreeMap::new();
    for s in x.named("node_hwmon_temp_celsius") {
        let chip = s.label("chip").unwrap_or("?").to_string();
        let entry = chips.entry(chip).or_insert(f64::MIN);
        *entry = entry.max(s.value);
    }
    let temps: Vec<Temp> = chips
        .iter()
        .map(|(chip, celsius)| {
            let label = x
                .named("node_hwmon_chip_names")
                .find(|n| n.label("chip") == Some(chip.as_str()))
                .and_then(|n| n.label("chip_name"))
                .unwrap_or(chip)
                .to_string();
            Temp {
                label,
                celsius: *celsius,
            }
        })
        .collect();
    let gpu_temp = temps
        .iter()
        .find(|t| t.label == "amdgpu")
        .map(|t| t.celsius);

    // The `drm` collector (off by default in node_exporter) reports AMD GPUs.
    let cards = x.named("node_drm_memory_vram_size_bytes").count();
    let amd_chips = temps.iter().filter(|t| t.label == "amdgpu").count();
    let gpu_temp = if cards == 1 && amd_chips == 1 {
        gpu_temp
    } else {
        None
    };
    let gpus = x
        .named("node_drm_memory_vram_size_bytes")
        .map(|size| {
            let card = size.label("card");
            let by_card = |name: &str| {
                x.named(name)
                    .find(|s| s.label("card") == card)
                    .map(|s| s.value)
            };
            Gpu {
                name: card.unwrap_or("gpu").to_string(),
                busy_pct: by_card("node_drm_gpu_busy_percent"),
                vram_used: by_card("node_drm_memory_vram_used_bytes"),
                vram_total: Some(size.value),
                temp_c: gpu_temp,
            }
        })
        .collect();

    let uptime_secs = x
        .one("node_boot_time_seconds")
        .zip(x.one("node_time_seconds"))
        .map(|(boot, now)| now - boot);

    Reading {
        at,
        hostname: x
            .named("node_uname_info")
            .next()
            .and_then(|s| s.label("nodename"))
            .map(str::to_string),
        uptime_secs,
        cpu,
        cores,
        load: x
            .one("node_load1")
            .zip(x.one("node_load5"))
            .zip(x.one("node_load15"))
            .map(|((a, b), c)| [a, b, c]),
        mem_total: x.one("node_memory_MemTotal_bytes"),
        mem_available: x.one("node_memory_MemAvailable_bytes"),
        swap_total: x.one("node_memory_SwapTotal_bytes"),
        swap_free: x.one("node_memory_SwapFree_bytes"),
        filesystems,
        disks,
        nics,
        gpus,
        temps,
    }
}
