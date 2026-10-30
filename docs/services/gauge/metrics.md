---
title: Metrics and alerts
parent: Gauge
grand_parent: Services
nav_order: 7
---

What the collector reads, how a scrape becomes a view, and when an alert exists. Sources: `service/src/domain/{collector,sources/*,prom,store}.rs`, `core/src/{model,forecast}.rs`.

## Collector

`collector::spawn` starts one tokio task per target. Each loop: scrape, `store.record(name, result)`, sleep `interval_secs`. A failed scrape is logged at warn once per outage (`<name>: scrape failed: <error>`). The first scrape of a host has no rates, so no history point is drawn until the second succeeds. After a failure the host keeps its last view and error; its previous reading is discarded (counters would span the gap) and one gap point is appended to the history if the last point was not already a gap. A host is `up` when it has no error and has a view; before the first scrape its error is `waiting for the first scrape`.

## What is read

| | PCP (`GET {url}/metrics?names=a,b,c`) | node_exporter (`GET {url}/metrics`) |
|---|---|---|
| CPU total and per core | `kernel.all.cpu.{user,nice,sys,idle,wait.total,irq.hard,irq.soft,steal}`, `kernel.percpu.cpu.*` | `node_cpu_seconds_total` (idle and iowait are idle; the rest is busy) |
| Load, uptime | `kernel.all.load`, `kernel.all.uptime` | `node_load*`, `node_boot_time_seconds`, `node_time_seconds` |
| Memory, swap | `mem.physmem`, `mem.util.available`, `mem.util.swapTotal`, `mem.util.swapFree` | `node_memory_*` |
| Filesystems | `filesys.capacity`, `filesys.avail` (devices; no mount points) | `node_filesystem_size_bytes`, `node_filesystem_avail_bytes` for `/dev/*` devices, skipping the fstypes `tmpfs devtmpfs overlay squashfs ramfs efivarfs proc sysfs cgroup2 devpts fuse.portal nsfs` |
| Disk and network throughput | `disk.dev.{read,write}_bytes`, `network.interface.{in,out}.bytes` | `node_disk_{read,written}_bytes_total`, `node_network_{receive,transmit}_bytes_total` |
| GPU | `nvidia.{gpuactive,memused,memtotal,temperature}` and `amdgpu.gpu.{load,temperature}`, `amdgpu.memory.{used,total}`, asked separately; absence is fine | `node_drm_gpu_busy_percent`, `node_drm_memory_{vram,gtt}_{used,size}_bytes`, hwmon power, cap, fan, frequency (run node_exporter with `--collector.drm`) |
| Temperatures, fans | not read | `node_hwmon_temp_celsius`, `node_hwmon_chip_names`, `node_hwmon_fan_rpm`, `node_hwmon_fan_max_rpm` |
| Pressure | not read | `node_pressure_{cpu_waiting,memory_waiting,memory_stalled,io_waiting,io_stalled}_seconds_total` |
| Memory split, kernel counters | not read | `node_memory_*` (anon, cached, buffers, dirty), `node_vmstat_{pswpin,pswpout,oom_kill}`, `node_netstat_*` (TCP retransmits) |
| Disk latency, queue | not read | `node_disk_io_time_seconds_total`, `node_disk_io_now`, `node_disk_{reads,writes}_completed_total`, `node_disk_{read,write}_time_seconds_total` |
| Link state, speed, errors | not read | `node_network_up`, `node_network_speed_bytes`, `node_network_{receive,transmit}_{errs,drop}_total` |
| CPU frequency, power | not read | `node_cpu_scaling_frequency_hertz`, `node_rapl_package_joules_total`, `node_procs_blocked` |
| Hardware error counters | not read | `node_btrfs_device_errors_total`, `node_edac_{correctable,uncorrectable}_errors_total` |

PCP values are scaled using the units in the comment line of the exposition, not a table. Whole disks only (`is_whole_disk`: not partitions, `loop`, `ram`, `dm-`, `zram`, `sr`, `md`; `nvme0n1` and `mmcblk0` are disks, `nvme0n1p2` a partition) and physical-looking interfaces only (`is_real_nic`: not `lo`, `veth`, `docker`, `br-`, `cni`, `flannel`, `cali`, `virbr`, `tun`, `tap`). An empty node_exporter answer is an error: `no metrics in the response (is this node_exporter?)`. A non-2xx answer is `<url>: <status>`.

With `also`, the second scrape's GPUs, temperatures, fans and faults are appended, and PSI, memory split, VM counters, CPU frequency, RAPL, blocked processes, disk IO and links fill what the first lacks (`Reading::merge`).

## Derived values

* CPU percent: busy / total seconds between two readings, clamped to 0..100. Same per core.
* Memory used: `mem_total - mem_available`. Swap used: total - free.
* Rates (disk, network, swap, TCP retransmits, errors, RAPL watts): counter difference over the time between readings; a counter that went backwards gives no rate (reset).
* Swap traffic: pages times 4096 bytes.
* PSI percent: stalled seconds over the interval times 100, clamped.
* Disk stats: utilisation (`io_time` delta over interval), IOPS, mean read and write latency in ms (`None` without requests), queue.
* One history point per scrape (`Point`): CPU, memory percent, rx/tx and read/write summed over interfaces and disks, busiest GPU, fullest VRAM percent, PSI cpu/mem/io, busiest disk utilisation, hungriest GPU watts. The in-memory ring holds 360 points (an hour at 10 s).

## Alerts

Evaluated on every successful scrape (`model::alerts`); thresholds are fixed in code.

| `kind` | `key` | Condition | `severe` |
|---|---|---|---|
| `fault` | `fault:<what>` | a hardware error counter is not zero (btrfs, ECC) | always |
| `oom` | `oom` | OOM kills since boot increased since the previous reading | always |
| `disk_full` | `disk_full:<mount or device>` | a filesystem is 90 percent used or more | 97 percent or more |
| `hot` | `hot:<sensor label>` | a temperature of 90 degrees C or more | always |
| `pressure_mem` | `pressure_mem` | memory pressure (some) 10 percent or more | memory full-stall 5 percent or more |
| `pressure_io` | `pressure_io` | IO full-stall 20 percent or more | never |
| `swapping` | `swapping` | swap in plus out at least 1 MiB/s | never |
| `fills_soon` | `fills_soon:<mount>` | the forecast says a filesystem of this host fills in under 14 days | under 3 days |
| `down` | `down` | the scrape failed; `detail` is the error | always |

`key` is stable while the wording in `detail` changes, which is how an alert that opens and closes is told from one that merely changes. `fills_soon` is added by the store from the forecast the history refreshes every ten minutes, so it appears on the scrape after the refresh; its detail is `<mount> · <n> d` or `<mount> · <1 d`.

The store feeds each scrape's alerts to an `AlertLog`, which returns only changes: `Opened` for a key that was not open, `Closed` for one no longer present. While a host is silent its other alerts stay as they were, neither confirmed nor cleared, and one `down` alert is open. The changes go to the history writer (see [History](history.md)).
