//! PCP's `pmproxy`: `GET <url>/metrics?names=a,b,c`.
//!
//! It answers in the Prometheus text format, with each
//! metric's units in a comment (see [`crate::domain::prom`]). The core metrics must be there; GPU metrics
//! come from PMDAs that may not be installed, so they are asked for separately and their absence is fine.

use crate::domain::model::{Cpu, Filesystem, Gpu, Reading, is_real_nic, is_whole_disk};
use crate::domain::prom::{Exposition, Sample, parse};
use quench_client::transport::Client;

const CORE: &[&str] = &[
    "kernel.all.cpu.user",
    "kernel.all.cpu.nice",
    "kernel.all.cpu.sys",
    "kernel.all.cpu.idle",
    "kernel.all.cpu.wait.total",
    "kernel.all.cpu.irq.hard",
    "kernel.all.cpu.irq.soft",
    "kernel.all.cpu.steal",
    "kernel.percpu.cpu.user",
    "kernel.percpu.cpu.nice",
    "kernel.percpu.cpu.sys",
    "kernel.percpu.cpu.idle",
    "kernel.percpu.cpu.wait.total",
    "kernel.percpu.cpu.irq.hard",
    "kernel.percpu.cpu.irq.soft",
    "kernel.percpu.cpu.steal",
    "kernel.all.load",
    "kernel.all.uptime",
    "mem.physmem",
    "mem.util.available",
    "mem.util.swapTotal",
    "mem.util.swapFree",
    "filesys.capacity",
    "filesys.avail",
    "disk.dev.read_bytes",
    "disk.dev.write_bytes",
    "network.interface.in.bytes",
    "network.interface.out.bytes",
];

const NVIDIA: &[&str] = &[
    "nvidia.gpuactive",
    "nvidia.memused",
    "nvidia.memtotal",
    "nvidia.temperature",
];

const AMDGPU: &[&str] = &[
    "amdgpu.gpu.load",
    "amdgpu.memory.used",
    "amdgpu.memory.total",
    "amdgpu.gpu.temperature",
];

async fn fetch(http: &Client, base: &str, names: &[&str]) -> Result<Exposition, String> {
    let text = super::get_text(http, &format!("{base}/metrics?names={}", names.join(","))).await?;
    Ok(parse(&text))
}

pub async fn scrape(http: &Client, base: &str) -> Result<Reading, String> {
    let core = fetch(http, base, CORE).await?;
    if core.samples.is_empty() {
        return Err("no metrics in the response (is this pmproxy?)".into());
    }
    let mut gpus = Vec::new();
    if let Ok(x) = fetch(http, base, NVIDIA).await {
        gpus.extend(gpus_from(
            &x,
            "nvidia_gpuactive",
            "nvidia_memused",
            "nvidia_memtotal",
            "nvidia_temperature",
        ));
    }
    if let Ok(x) = fetch(http, base, AMDGPU).await {
        gpus.extend(gpus_from(
            &x,
            "amdgpu_gpu_load",
            "amdgpu_memory_used",
            "amdgpu_memory_total",
            "amdgpu_gpu_temperature",
        ));
    }
    let mut reading = reading(&core, super::now());
    reading.gpus = gpus;
    Ok(reading)
}

fn instance(s: &Sample) -> String {
    s.label("instname")
        .or_else(|| s.label("instid"))
        .unwrap_or("0")
        .to_string()
}

fn gpus_from(x: &Exposition, busy: &str, used: &str, total: &str, temp: &str) -> Vec<Gpu> {
    let find = |name: &str, inst: &str| x.named(name).find(|s| instance(s) == inst);
    let mut cards: Vec<String> = x.named(busy).chain(x.named(total)).map(instance).collect();
    cards.sort();
    cards.dedup();
    cards
        .into_iter()
        .map(|card| Gpu {
            busy_pct: find(busy, &card).map(|s| s.value),
            vram_used: find(used, &card).map(|s| s.value * x.byte_scale(used)),
            vram_total: find(total, &card).map(|s| s.value * x.byte_scale(total)),
            temp_c: find(temp, &card).map(|s| s.value),
            name: card,
        })
        .collect()
}

/// Summed CPU times (seconds) of every sample named `<prefix>_<state>` that belongs to `pick`.
fn cpu_of(x: &Exposition, prefix: &str, pick: impl Fn(&Sample) -> bool) -> Cpu {
    let mut cpu = Cpu::default();
    for (state, busy) in [
        ("user", true),
        ("nice", true),
        ("sys", true),
        ("irq_hard", true),
        ("irq_soft", true),
        ("steal", true),
        ("idle", false),
        ("wait_total", false),
    ] {
        let name = format!("{prefix}_{state}");
        let scale = x.second_scale(&name);
        for s in x.named(&name).filter(|s| pick(s)) {
            cpu.total_secs += s.value * scale;
            if busy {
                cpu.busy_secs += s.value * scale;
            }
        }
    }
    cpu
}

#[must_use]
pub fn reading(x: &Exposition, at: f64) -> Reading {
    let cpu = cpu_of(x, "kernel_all_cpu", |_| true);

    let mut ids: Vec<String> = x.named("kernel_percpu_cpu_idle").map(instance).collect();
    ids.sort_by_key(|id| {
        id.trim_start_matches("cpu")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    let cores = ids
        .into_iter()
        .map(|id| {
            let cpu = cpu_of(x, "kernel_percpu_cpu", |s| instance(s) == id);
            (id.trim_start_matches("cpu").to_string(), cpu)
        })
        .collect();

    let mut load = [None; 3];
    for s in x.named("kernel_all_load") {
        match s.label("instid") {
            Some("1") => load[0] = Some(s.value),
            Some("5") => load[1] = Some(s.value),
            Some("15") => load[2] = Some(s.value),
            _ => {}
        }
    }

    let bytes = |name: &str| x.one(name).map(|v| v * x.byte_scale(name));

    let mut seen = std::collections::HashSet::new();
    let scale = x.byte_scale("filesys_capacity");
    let filesystems = x
        .named("filesys_capacity")
        .filter(|s| instance(s).starts_with("/dev/") && s.value > 0.0)
        .filter_map(|s| {
            let device = instance(s);
            let avail = x.named("filesys_avail").find(|a| instance(a) == device)?;
            seen.insert(device.clone()).then(|| Filesystem {
                device,
                // pmproxy leaves out string metrics, so the mount point is not known.
                mount: None,
                size: s.value * scale,
                avail: avail.value * x.byte_scale("filesys_avail"),
            })
        })
        .collect();

    let paired = |a: &str, b: &str, keep: fn(&str) -> bool| -> Vec<(String, f64, f64)> {
        let (sa, sb) = (x.byte_scale(a), x.byte_scale(b));
        x.named(a)
            .filter_map(|r| {
                let dev = instance(r);
                if !keep(&dev) {
                    return None;
                }
                let w = x.named(b).find(|w| instance(w) == dev)?;
                Some((dev, r.value * sa, w.value * sb))
            })
            .collect()
    };

    let hostname = x
        .samples
        .iter()
        .find_map(|s| s.label("hostname"))
        .map(str::to_string);

    Reading {
        at,
        hostname,
        uptime_secs: x.one("kernel_all_uptime"),
        cpu,
        cores,
        load: load[0]
            .zip(load[1])
            .zip(load[2])
            .map(|((a, b), c)| [a, b, c]),
        mem_total: bytes("mem_physmem"),
        mem_available: bytes("mem_util_available"),
        swap_total: bytes("mem_util_swapTotal"),
        swap_free: bytes("mem_util_swapFree"),
        filesystems,
        disks: paired("disk_dev_read_bytes", "disk_dev_write_bytes", is_whole_disk),
        nics: paired(
            "network_interface_in_bytes",
            "network_interface_out_bytes",
            is_real_nic,
        ),
        gpus: Vec::new(),
        temps: Vec::new(),
    }
}
