//! What a host looks like, whichever agent reported it.
//!
//! A [`Reading`] is one scrape, in plain units (bytes, seconds, degrees C) with counters left as totals;
//! [`View`] is that turned into rates by comparing two readings.

use quench_serde::{Deserialize, Serialize};

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
    pub psi: Option<Psi>,
    pub mem_parts: Option<MemParts>,
    pub vm: Option<Vm>,
    pub disk_io: Vec<DiskIo>,
    pub links: Vec<Link>,
    /// Mean scaling frequency of the cores, in Hz.
    pub cpu_freq_hz: Option<f64>,
    /// CPU package energy, joules since boot (RAPL).
    pub cpu_joules: Option<f64>,
    pub procs_blocked: Option<f64>,
    pub fans: Vec<Fan>,
    pub faults: Vec<Fault>,
}

impl Reading {
    /// Adds what a side agent knows: its GPUs, sensors and error counters, and whatever this reading lacks.
    pub fn merge(&mut self, extra: Self) {
        self.gpus.extend(extra.gpus);
        self.temps.extend(extra.temps);
        self.fans.extend(extra.fans);
        self.faults.extend(extra.faults);
        self.psi = self.psi.take().or(extra.psi);
        self.mem_parts = self.mem_parts.take().or(extra.mem_parts);
        self.vm = self.vm.take().or(extra.vm);
        self.cpu_freq_hz = self.cpu_freq_hz.or(extra.cpu_freq_hz);
        self.cpu_joules = self.cpu_joules.or(extra.cpu_joules);
        self.procs_blocked = self.procs_blocked.or(extra.procs_blocked);
        if self.disk_io.is_empty() {
            self.disk_io = extra.disk_io;
        }
        if self.links.is_empty() {
            self.links = extra.links;
        }
    }
}

/// Pressure stall totals in seconds since boot: time some (or every) task waited for the resource.
#[derive(Debug, Clone, Default)]
pub struct Psi {
    pub cpu_some: f64,
    pub mem_some: f64,
    pub mem_full: f64,
    pub io_some: f64,
    pub io_full: f64,
}

/// What is inside "used" memory, in bytes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemParts {
    pub anon: f64,
    pub cached: f64,
    pub buffers: f64,
    pub dirty: f64,
}

/// Kernel counters since boot.
#[derive(Debug, Clone, Default)]
pub struct Vm {
    /// Pages swapped in and out.
    pub swap_in: f64,
    pub swap_out: f64,
    pub oom_kills: f64,
    pub tcp_retrans: f64,
}

/// Block-device totals; times are seconds, `queue` is the requests in flight now.
#[derive(Debug, Clone, Default)]
pub struct DiskIo {
    pub name: String,
    pub busy_secs: f64,
    pub reads: f64,
    pub writes: f64,
    pub read_secs: f64,
    pub write_secs: f64,
    pub queue: f64,
}

/// A network interface's state; `errors` counts errors and drops of both directions.
#[derive(Debug, Clone, Default)]
pub struct Link {
    pub name: String,
    pub up: bool,
    /// Negotiated speed in bytes per second.
    pub speed: Option<f64>,
    pub errors: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fan {
    pub label: String,
    pub rpm: f64,
}

/// A hardware error counter that is not zero (btrfs device errors, ECC).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fault {
    pub what: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filesystem {
    pub device: String,
    pub mount: Option<String>,
    pub size: f64,
    pub avail: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Gpu {
    pub name: String,
    pub busy_pct: Option<f64>,
    pub vram_used: Option<f64>,
    pub vram_total: Option<f64>,
    pub temp_c: Option<f64>,
    pub power_w: Option<f64>,
    pub power_cap_w: Option<f64>,
    pub fan_rpm: Option<f64>,
    pub fan_max_rpm: Option<f64>,
    pub sclk_mhz: Option<f64>,
    pub mclk_mhz: Option<f64>,
    pub gtt_used: Option<f64>,
    pub gtt_total: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Temp {
    pub label: String,
    pub celsius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub name: String,
    pub a_per_sec: f64,
    pub b_per_sec: f64,
}

/// Time waiting for a resource, as a share of the interval.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PsiPct {
    pub cpu: f64,
    pub mem: f64,
    pub mem_full: f64,
    pub io: f64,
    pub io_full: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskStat {
    pub name: String,
    /// Share of the interval the device had requests in flight.
    pub util_pct: f64,
    pub iops: f64,
    /// Mean time per request, milliseconds; `None` when there were none.
    pub read_ms: Option<f64>,
    pub write_ms: Option<f64>,
    pub queue: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkView {
    pub name: String,
    pub up: bool,
    pub speed: Option<f64>,
    pub errors_per_sec: f64,
}

/// Something that needs a look. `kind` picks the wording, `detail` is what it is about (and changes as it gets
/// worse), `key` is what it is *of* - the same alert has the same key from one scrape to the next, which is how an
/// alert that opens and closes is told from one that merely changes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub key: String,
    pub kind: String,
    pub detail: String,
    pub severe: bool,
}

/// A reading with the rates that need a previous one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    pub psi: Option<PsiPct>,
    pub mem_parts: Option<MemParts>,
    /// Swap traffic, bytes per second.
    pub swap_in_per_sec: Option<f64>,
    pub swap_out_per_sec: Option<f64>,
    /// OOM kills since boot.
    pub oom_kills: Option<f64>,
    pub tcp_retrans_per_sec: Option<f64>,
    pub disk_stats: Vec<DiskStat>,
    pub links: Vec<LinkView>,
    pub cpu_freq_mhz: Option<f64>,
    pub cpu_watts: Option<f64>,
    pub procs_blocked: Option<f64>,
    pub fans: Vec<Fan>,
    pub faults: Vec<Fault>,
    pub alerts: Vec<Alert>,
}

/// One sample of a host's history, in the units the charts use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub t: f64,
    pub cpu: Option<f64>,
    pub mem: Option<f64>,
    pub rx: Option<f64>,
    pub tx: Option<f64>,
    pub read: Option<f64>,
    pub write: Option<f64>,
    pub gpu: Option<f64>,
    pub vram: Option<f64>,
    /// Pressure: the share of time some task waited for CPU, memory and IO.
    pub psi_cpu: Option<f64>,
    pub psi_mem: Option<f64>,
    pub psi_io: Option<f64>,
    /// The busiest disk's utilisation, percent.
    pub disk_util: Option<f64>,
    /// The hungriest GPU's power draw, watts.
    pub power: Option<f64>,
}

impl Point {
    /// A break in the line: the host stopped answering.
    #[must_use]
    pub const fn gap(t: f64) -> Self {
        Self {
            t,
            cpu: None,
            mem: None,
            rx: None,
            tx: None,
            read: None,
            write: None,
            gpu: None,
            vram: None,
            psi_cpu: None,
            psi_mem: None,
            psi_io: None,
            disk_util: None,
            power: None,
        }
    }

    #[must_use]
    pub const fn is_gap(&self) -> bool {
        self.cpu.is_none() && self.mem.is_none() && self.rx.is_none()
    }
}

fn pct(prev: &Cpu, cur: &Cpu) -> Option<f64> {
    let total = cur.total_secs - prev.total_secs;
    let busy = cur.busy_secs - prev.busy_secs;
    (total > 0.0 && busy >= 0.0).then(|| (busy / total * 100.0).clamp(0.0, 100.0))
}

/// Bytes in a page of swap traffic.
const PAGE: f64 = 4096.0;

/// Per-second rate of a counter taken from two readings; `None` without both, or after a reset.
fn counter_rate(
    prev: Option<&Reading>,
    cur: &Reading,
    dt: f64,
    get: impl Fn(&Reading) -> Option<f64>,
) -> Option<f64> {
    let (a, b) = (get(prev?)?, get(cur)?);
    (b >= a && dt > 0.0).then(|| (b - a) / dt)
}

fn psi_pct(prev: &Psi, cur: &Psi, dt: f64) -> Option<PsiPct> {
    let pct = |a: f64, b: f64| (b >= a).then(|| ((b - a) / dt * 100.0).clamp(0.0, 100.0));
    if dt <= 0.0 {
        return None;
    }
    Some(PsiPct {
        cpu: pct(prev.cpu_some, cur.cpu_some)?,
        mem: pct(prev.mem_some, cur.mem_some)?,
        mem_full: pct(prev.mem_full, cur.mem_full)?,
        io: pct(prev.io_some, cur.io_some)?,
        io_full: pct(prev.io_full, cur.io_full)?,
    })
}

fn disk_stats(prev: &[DiskIo], cur: &[DiskIo], dt: f64) -> Vec<DiskStat> {
    cur.iter()
        .filter_map(|c| {
            let p = prev.iter().find(|p| p.name == c.name)?;
            if dt <= 0.0 || c.reads < p.reads || c.writes < p.writes || c.busy_secs < p.busy_secs {
                return None;
            }
            let (reads, writes) = (c.reads - p.reads, c.writes - p.writes);
            let per = |secs: f64, n: f64| (n > 0.0).then(|| secs / n * 1000.0);
            Some(DiskStat {
                name: c.name.clone(),
                util_pct: ((c.busy_secs - p.busy_secs) / dt * 100.0).clamp(0.0, 100.0),
                iops: (reads + writes) / dt,
                read_ms: per(c.read_secs - p.read_secs, reads),
                write_ms: per(c.write_secs - p.write_secs, writes),
                queue: c.queue,
            })
        })
        .collect()
}

fn link_views(prev: Option<&Reading>, cur: &Reading, dt: f64) -> Vec<LinkView> {
    cur.links
        .iter()
        .map(|l| {
            let before = prev.and_then(|p| p.links.iter().find(|q| q.name == l.name));
            LinkView {
                name: l.name.clone(),
                up: l.up,
                speed: l.speed,
                errors_per_sec: before
                    .filter(|q| l.errors >= q.errors && dt > 0.0)
                    .map_or(0.0, |q| (l.errors - q.errors) / dt),
            }
        })
        .collect()
}

/// Thresholds are deliberately blunt: a number that is high for a minute is not an alert, a counter that moved is.
fn alerts(view: &View, prev: Option<&Reading>, cur: &Reading) -> Vec<Alert> {
    let mut out = Vec::new();
    let mut add = |key: String, kind: &str, detail: String, severe: bool| {
        out.push(Alert {
            key,
            kind: kind.into(),
            detail,
            severe,
        });
    };
    for f in &view.faults {
        add(
            format!("fault:{}", f.what),
            "fault",
            format!("{}: {}", f.what, f.count),
            true,
        );
    }
    let kills_before = prev.and_then(|p| p.vm.as_ref()).map(|v| v.oom_kills);
    if let Some((before, now)) = kills_before.zip(cur.vm.as_ref().map(|v| v.oom_kills))
        && now > before
    {
        add("oom".into(), "oom", format!("{now} since boot"), true);
    }
    for f in &view.filesystems {
        let used = (f.size - f.avail) / f.size * 100.0;
        if f.size > 0.0 && used >= 90.0 {
            let name = f.mount.as_deref().unwrap_or(&f.device);
            add(
                format!("disk_full:{name}"),
                "disk_full",
                format!("{name} {used:.0}%"),
                used >= 97.0,
            );
        }
    }
    for t in &view.temps {
        if t.celsius >= 90.0 {
            add(
                format!("hot:{}", t.label),
                "hot",
                format!("{} {:.0} °C", t.label, t.celsius),
                true,
            );
        }
    }
    if let Some(p) = &view.psi {
        if p.mem >= 10.0 {
            add(
                "pressure_mem".into(),
                "pressure_mem",
                format!("{:.0}%", p.mem),
                p.mem_full >= 5.0,
            );
        }
        if p.io_full >= 20.0 {
            add(
                "pressure_io".into(),
                "pressure_io",
                format!("{:.0}%", p.io_full),
                false,
            );
        }
    }
    let swapping = view.swap_in_per_sec.unwrap_or(0.0) + view.swap_out_per_sec.unwrap_or(0.0);
    if swapping >= 1024.0 * 1024.0 {
        add(
            "swapping".into(),
            "swapping",
            format!("{:.0} MiB/s", swapping / 1048576.0),
            false,
        );
    }
    out
}

/// Days left under which a filling disk is an alert, and under which it is a severe one.
pub const FILLS_WARN_DAYS: f64 = 14.0;
pub const FILLS_SEVERE_DAYS: f64 = 3.0;

/// The alerts the stored history adds: a filesystem of `host` that the forecast says fills soon. The forecast comes
/// from the database, so these are added by the service and not by [`View::from_readings`].
#[must_use]
pub fn fill_alerts(host: &str, forecast: &[crate::DiskForecast]) -> Vec<Alert> {
    forecast
        .iter()
        .filter(|d| d.host == host)
        .filter_map(|d| {
            let left = d.days_left.filter(|l| *l < FILLS_WARN_DAYS)?;
            Some(Alert {
                key: format!("fills_soon:{}", d.mount),
                kind: "fills_soon".into(),
                detail: if left < 1.0 {
                    format!("{} · <1 d", d.mount)
                } else {
                    format!("{} · {left:.0} d", d.mount)
                },
                severe: left < FILLS_SEVERE_DAYS,
            })
        })
        .collect()
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
            psi: prev.and_then(|p| psi_pct(p.psi.as_ref()?, cur.psi.as_ref()?, dt)),
            mem_parts: cur.mem_parts.clone(),
            swap_in_per_sec: counter_rate(prev, cur, dt, |r| r.vm.as_ref().map(|v| v.swap_in))
                .map(|pages| pages * PAGE),
            swap_out_per_sec: counter_rate(prev, cur, dt, |r| r.vm.as_ref().map(|v| v.swap_out))
                .map(|pages| pages * PAGE),
            oom_kills: cur.vm.as_ref().map(|v| v.oom_kills),
            tcp_retrans_per_sec: counter_rate(prev, cur, dt, |r| {
                r.vm.as_ref().map(|v| v.tcp_retrans)
            }),
            disk_stats: prev.map_or_else(Vec::new, |p| disk_stats(&p.disk_io, &cur.disk_io, dt)),
            links: link_views(prev, cur, dt),
            cpu_freq_mhz: cur.cpu_freq_hz.map(|hz| hz / 1e6),
            cpu_watts: counter_rate(prev, cur, dt, |r| r.cpu_joules),
            procs_blocked: cur.procs_blocked,
            fans: cur.fans.clone(),
            faults: cur.faults.clone(),
            alerts: Vec::new(),
        }
        .with_alerts(prev, cur)
    }

    fn with_alerts(mut self, prev: Option<&Reading>, cur: &Reading) -> Self {
        self.alerts = alerts(&self, prev, cur);
        self
    }

    #[must_use]
    pub fn severe_alerts(&self) -> usize {
        self.alerts.iter().filter(|a| a.severe).count()
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
