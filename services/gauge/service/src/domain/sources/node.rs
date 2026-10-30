//! `node_exporter`: `GET <url>/metrics`.

use crate::domain::model::{
    Cpu, DiskIo, Fan, Fault, Filesystem, Gpu, Link, MemParts, Psi, Reading, Temp, Vm, is_real_nic,
    is_whole_disk,
};
use crate::domain::prom::{Exposition, Sample, parse};
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
    let gpus = gpus(x, &chips_of_gpus(x));
    let fans = x
        .named("node_hwmon_fan_rpm")
        .filter(|f| f.value > 0.0 && !is_amd_chip(x, f.label("chip").unwrap_or_default()))
        .map(|f| {
            let chip = f.label("chip").unwrap_or("?");
            Fan {
                label: format!(
                    "{} {}",
                    chip_label(x, chip),
                    f.label("sensor").unwrap_or_default()
                ),
                rpm: f.value,
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
        psi: psi(x),
        mem_parts: x.one("node_memory_AnonPages_bytes").map(|anon| MemParts {
            anon,
            cached: x.one("node_memory_Cached_bytes").unwrap_or(0.0),
            buffers: x.one("node_memory_Buffers_bytes").unwrap_or(0.0),
            dirty: x.one("node_memory_Dirty_bytes").unwrap_or(0.0),
        }),
        vm: x.one("node_vmstat_pswpin").map(|swap_in| Vm {
            swap_in,
            swap_out: x.one("node_vmstat_pswpout").unwrap_or(0.0),
            oom_kills: x.one("node_vmstat_oom_kill").unwrap_or(0.0),
            tcp_retrans: x.one("node_netstat_Tcp_RetransSegs").unwrap_or(0.0),
        }),
        disk_io: disk_io(x),
        links: links(x),
        cpu_freq_hz: mean(x.named("node_cpu_scaling_frequency_hertz").map(|s| s.value)),
        cpu_joules: {
            let joules: Vec<f64> = x
                .named("node_rapl_package_joules_total")
                .map(|s| s.value)
                .collect();
            (!joules.is_empty()).then(|| joules.iter().sum())
        },
        procs_blocked: x.one("node_procs_blocked"),
        fans,
        faults: faults(x),
    }
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0.0), |(s, n), v| (s + v, n + 1.0));
    (n > 0.0).then(|| sum / n)
}

fn psi(x: &Exposition) -> Option<Psi> {
    let cpu_some = x.one("node_pressure_cpu_waiting_seconds_total")?;
    Some(Psi {
        cpu_some,
        mem_some: x.one("node_pressure_memory_waiting_seconds_total")?,
        mem_full: x.one("node_pressure_memory_stalled_seconds_total")?,
        io_some: x.one("node_pressure_io_waiting_seconds_total")?,
        io_full: x.one("node_pressure_io_stalled_seconds_total")?,
    })
}

fn disk_io(x: &Exposition) -> Vec<DiskIo> {
    let by = |name: &str, dev: &str| {
        x.named(name)
            .find(|s| s.label("device") == Some(dev))
            .map(|s| s.value)
    };
    x.named("node_disk_io_time_seconds_total")
        .filter_map(|s| {
            let dev = s.label("device").filter(|d| is_whole_disk(d))?;
            Some(DiskIo {
                name: dev.to_string(),
                busy_secs: s.value,
                reads: by("node_disk_reads_completed_total", dev)?,
                writes: by("node_disk_writes_completed_total", dev)?,
                read_secs: by("node_disk_read_time_seconds_total", dev)?,
                write_secs: by("node_disk_write_time_seconds_total", dev)?,
                queue: by("node_disk_io_now", dev).unwrap_or(0.0),
            })
        })
        .collect()
}

fn links(x: &Exposition) -> Vec<Link> {
    x.named("node_network_up")
        .filter_map(|s| {
            let dev = s.label("device").filter(|d| is_real_nic(d))?;
            let sum = |names: [&str; 4]| -> f64 {
                names
                    .iter()
                    .filter_map(|n| {
                        x.named(n)
                            .find(|e| e.label("device") == Some(dev))
                            .map(|e| e.value)
                    })
                    .sum()
            };
            // Negative means the driver does not know (a link that is down reports -1).
            let speed = x
                .named("node_network_speed_bytes")
                .find(|e| e.label("device") == Some(dev))
                .map(|e| e.value)
                .filter(|v| *v > 0.0);
            Some(Link {
                name: dev.to_string(),
                up: s.value > 0.0,
                speed,
                errors: sum([
                    "node_network_receive_errs_total",
                    "node_network_transmit_errs_total",
                    "node_network_receive_drop_total",
                    "node_network_transmit_drop_total",
                ]),
            })
        })
        .collect()
}

/// Counters that moved off zero: btrfs device errors and ECC corrections. A healthy host reports none.
fn faults(x: &Exposition) -> Vec<Fault> {
    let mut out = Vec::new();
    for s in x
        .named("node_btrfs_device_errors_total")
        .filter(|s| s.value > 0.0)
    {
        out.push(Fault {
            what: format!(
                "btrfs {} {}",
                s.label("device").unwrap_or("?"),
                s.label("type").unwrap_or("error")
            ),
            count: s.value,
        });
    }
    for (name, kind) in [
        ("node_edac_correctable_errors_total", "correctable"),
        ("node_edac_uncorrectable_errors_total", "uncorrectable"),
    ] {
        for s in x.named(name).filter(|s| s.value > 0.0) {
            out.push(Fault {
                what: format!(
                    "ecc {kind} (controller {})",
                    s.label("controller").unwrap_or("0")
                ),
                count: s.value,
            });
        }
    }
    out
}

fn chip_label(x: &Exposition, chip: &str) -> String {
    x.named("node_hwmon_chip_names")
        .find(|n| n.label("chip") == Some(chip))
        .and_then(|n| n.label("chip_name"))
        .unwrap_or(chip)
        .to_string()
}

fn is_amd_chip(x: &Exposition, chip: &str) -> bool {
    chip_label(x, chip) == "amdgpu"
}

/// What one amdgpu hwmon chip reports.
#[derive(Debug, Default)]
struct Chip {
    discrete: bool,
    temp: Option<f64>,
    power: Option<f64>,
    cap: Option<f64>,
    fan: Option<f64>,
    fan_max: Option<f64>,
    sclk: Option<f64>,
    mclk: Option<f64>,
}

fn chips_of_gpus(x: &Exposition) -> Vec<Chip> {
    let mut names: Vec<String> = x
        .named("node_hwmon_chip_names")
        .filter(|n| n.label("chip_name") == Some("amdgpu"))
        .filter_map(|n| n.label("chip").map(str::to_string))
        .collect();
    names.sort();
    let on = |name: &str, chip: &str| {
        x.named(name)
            .filter(|s| s.label("chip") == Some(chip))
            .map(|s| s.value)
            .reduce(f64::max)
    };
    let mhz = |chip: &str, sensor: &str| {
        x.named("node_hwmon_freq_freq_mhz")
            .find(|s| s.label("chip") == Some(chip) && s.label("sensor") == Some(sensor))
            .map(|s| s.value)
    };
    let mut chips: Vec<Chip> = names
        .iter()
        .map(|c| {
            let cap = on("node_hwmon_power_cap_watt", c);
            Chip {
                // An integrated GPU has no power cap and no fan; a card has both.
                discrete: cap.is_some(),
                temp: on("node_hwmon_temp_celsius", c),
                power: on("node_hwmon_power_average_watt", c)
                    .or_else(|| on("node_hwmon_power_watt", c)),
                cap,
                fan: on("node_hwmon_fan_rpm", c),
                fan_max: on("node_hwmon_fan_max_rpm", c),
                sclk: mhz(c, "sclk"),
                mclk: mhz(c, "mclk"),
            }
        })
        .collect();
    chips.sort_by_key(|c| !c.discrete);
    chips
}

/// The `drm` collector names cards, `hwmon` names PCI paths, and nothing joins the two. With as many amdgpu
/// chips as cards they are paired by size: the card with the most VRAM gets the discrete chip.
fn gpus(x: &Exposition, chips: &[Chip]) -> Vec<Gpu> {
    let mut cards: Vec<&Sample> = x.named("node_drm_memory_vram_size_bytes").collect();
    cards.sort_by(|a, b| b.value.total_cmp(&a.value));
    let paired = chips.len() == cards.len();
    let mut out: Vec<Gpu> = cards
        .iter()
        .enumerate()
        .map(|(i, size)| {
            let card = size.label("card");
            let by_card = |name: &str| {
                x.named(name)
                    .find(|s| s.label("card") == card)
                    .map(|s| s.value)
            };
            let chip = chips.get(i).filter(|_| paired);
            Gpu {
                name: card.unwrap_or("gpu").to_string(),
                busy_pct: by_card("node_drm_gpu_busy_percent"),
                vram_used: by_card("node_drm_memory_vram_used_bytes"),
                vram_total: Some(size.value),
                temp_c: chip.and_then(|c| c.temp),
                power_w: chip.and_then(|c| c.power),
                power_cap_w: chip.and_then(|c| c.cap),
                fan_rpm: chip.and_then(|c| c.fan),
                fan_max_rpm: chip.and_then(|c| c.fan_max),
                sclk_mhz: chip.and_then(|c| c.sclk),
                mclk_mhz: chip.and_then(|c| c.mclk),
                gtt_used: by_card("node_drm_memory_gtt_used_bytes"),
                gtt_total: by_card("node_drm_memory_gtt_size_bytes"),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
