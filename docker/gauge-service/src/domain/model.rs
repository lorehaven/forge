//! What a host looks like, whichever agent reported it.
//!
//! A [`Reading`] is one scrape, in plain units (bytes, seconds, degrees C) with counters left as totals;
//! [`View`] is that turned into rates by comparing two readings.

use quench_serde::Serialize;

#[derive(Debug, Clone, Default)]
pub struct Cpu {
    pub busy_secs: f64,
    pub total_secs: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Reading {
    /// Unix seconds when the scrape finished.
    pub at: f64,
    pub hostname: Option<String>,
    pub uptime_secs: Option<f64>,
    pub cpu: Cpu,
    pub cores: Vec<(String, Cpu)>,
    pub load: Option<[f64; 3]>,
    pub mem_total: Option<f64>,
    pub mem_available: Option<f64>,
    pub swap_total: Option<f64>,
    pub swap_free: Option<f64>,
    pub filesystems: Vec<Filesystem>,
    /// (device, bytes read, bytes written) totals.
    pub disks: Vec<(String, f64, f64)>,
    /// (interface, bytes received, bytes sent) totals.
    pub nics: Vec<(String, f64, f64)>,
    pub gpus: Vec<Gpu>,
    pub temps: Vec<Temp>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Filesystem {
    pub device: String,
    pub mount: Option<String>,
    pub size: f64,
    pub avail: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    pub busy_pct: Option<f64>,
    pub vram_used: Option<f64>,
    pub vram_total: Option<f64>,
    pub temp_c: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Temp {
    pub label: String,
    pub celsius: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rate {
    pub name: String,
    pub a_per_sec: f64,
    pub b_per_sec: f64,
}

/// A reading with the rates that need a previous one.
#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub at: f64,
    pub hostname: Option<String>,
    pub uptime_secs: Option<f64>,
    pub cpu_pct: Option<f64>,
    pub cores_pct: Vec<(String, f64)>,
    pub load: Option<[f64; 3]>,
    pub mem_total: Option<f64>,
    pub mem_used: Option<f64>,
    pub swap_total: Option<f64>,
    pub swap_used: Option<f64>,
    pub filesystems: Vec<Filesystem>,
    /// Disk throughput: `a` is read, `b` is write, bytes per second.
    pub disks: Vec<Rate>,
    /// Network throughput: `a` is received, `b` is sent, bytes per second.
    pub nics: Vec<Rate>,
    pub gpus: Vec<Gpu>,
    pub temps: Vec<Temp>,
}

fn pct(prev: &Cpu, cur: &Cpu) -> Option<f64> {
    let total = cur.total_secs - prev.total_secs;
    let busy = cur.busy_secs - prev.busy_secs;
    (total > 0.0 && busy >= 0.0).then(|| (busy / total * 100.0).clamp(0.0, 100.0))
}

fn rates(prev: &[(String, f64, f64)], cur: &[(String, f64, f64)], dt: f64) -> Vec<Rate> {
    cur.iter()
        .filter_map(|(name, a, b)| {
            let (_, pa, pb) = prev.iter().find(|(n, _, _)| n == name)?;
            // A counter that went backwards was reset (the agent or the host restarted).
            (a >= pa && b >= pb && dt > 0.0).then(|| Rate {
                name: name.clone(),
                a_per_sec: (a - pa) / dt,
                b_per_sec: (b - pb) / dt,
            })
        })
        .collect()
}

impl View {
    #[must_use]
    pub fn from_readings(prev: Option<&Reading>, cur: &Reading) -> Self {
        let dt = prev.map_or(0.0, |p| cur.at - p.at);
        let cores_pct = prev
            .map(|p| {
                cur.cores
                    .iter()
                    .filter_map(|(id, c)| {
                        let (_, pc) = p.cores.iter().find(|(pid, _)| pid == id)?;
                        pct(pc, c).map(|v| (id.clone(), v))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            at: cur.at,
            hostname: cur.hostname.clone(),
            uptime_secs: cur.uptime_secs,
            cpu_pct: prev.and_then(|p| pct(&p.cpu, &cur.cpu)),
            cores_pct,
            load: cur.load,
            mem_total: cur.mem_total,
            mem_used: cur
                .mem_total
                .zip(cur.mem_available)
                .map(|(t, a)| (t - a).max(0.0)),
            swap_total: cur.swap_total,
            swap_used: cur
                .swap_total
                .zip(cur.swap_free)
                .map(|(t, f)| (t - f).max(0.0)),
            filesystems: cur.filesystems.clone(),
            disks: prev.map_or_else(Vec::new, |p| rates(&p.disks, &cur.disks, dt)),
            nics: prev.map_or_else(Vec::new, |p| rates(&p.nics, &cur.nics, dt)),
            gpus: cur.gpus.clone(),
            temps: cur.temps.clone(),
        }
    }

    #[must_use]
    pub fn mem_pct(&self) -> Option<f64> {
        self.mem_total
            .zip(self.mem_used)
            .filter(|(t, _)| *t > 0.0)
            .map(|(t, u)| u / t * 100.0)
    }
}

/// Whole disks only: not partitions, loop devices, RAM disks or device-mapper nodes.
#[must_use]
pub fn is_whole_disk(name: &str) -> bool {
    if ["loop", "ram", "dm-", "zram", "sr", "md"]
        .iter()
        .any(|p| name.starts_with(p))
    {
        return false;
    }
    if name.starts_with("nvme") || name.starts_with("mmcblk") {
        // nvme0n1 is a disk, nvme0n1p2 a partition
        return !name.rsplit_once('p').is_some_and(|(head, tail)| {
            !tail.is_empty()
                && tail.chars().all(|c| c.is_ascii_digit())
                && head.ends_with(|c: char| c.is_ascii_digit())
        });
    }
    !name.ends_with(|c: char| c.is_ascii_digit())
}

/// Interfaces worth a row: physical-looking ones, not loopback, bridges, veths or overlays.
#[must_use]
pub fn is_real_nic(name: &str) -> bool {
    ![
        "lo", "veth", "docker", "br-", "cni", "flannel", "cali", "virbr", "tun", "tap",
    ]
    .iter()
    .any(|p| name.starts_with(p))
}
