---
title: Metrics and alerts
parent: Gauge
grand_parent: Services
nav_order: 7
---

What the collector reads, how a scrape becomes a view, and when an alert exists. Sources: `service/src/domain/{collector,sources/*,prom,store}.rs`, `core/src/{model,forecast}.rs`.

## Collector

`collector::spawn` starts one tokio task per target. Each loop: scrape, `store.record(name, result)`, sleep `interval_secs`. A failed scrape is logged at warn once per outage (`<name>: scrape failed: <error>`). The first scrape of a host has no rates, so no history point is drawn until the second succeeds. After a failure the host keeps its last view and error; its previous reading is discarded (counters would span the gap) and one gap point is appended to the history if the last point was not already a gap. A host is `up` when it has no error and a live reading (its last scrape answered); a host that is not up may still have a view, its last one, shown as not live. Before the first scrape its error is `waiting for the first scrape`.

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
| Disk health (SMART) | not read | `smartmon_device_smart_{healthy,available}`, `smartmon_device_info`, and the raw values `smartmon_{temperature_celsius,airflow_temperature_cel,power_on_hours,reallocated_sector_ct,current_pending_sector,offline_uncorrectable}_raw_value`, from the textfile collector (see below) |
| NVMe health | not read | `node_nvme_info` (model) and `node_hwmon_temp_celsius{chip="nvme_<controller>"}` (temperature) from node_exporter itself; `nvme_{critical_warning,percentage_used_ratio,available_spare_ratio,available_spare_threshold_ratio,media_errors_total,power_on_hours_total,temperature_celsius}` and `nvme_device_info` from the `nvme_metrics.py` textfile script (see below) |

PCP values are scaled using the units in the comment line of the exposition, not a table. Whole disks only (`is_whole_disk`: not partitions, `loop`, `ram`, `dm-`, `zram`, `sr`, `md`; `nvme0n1` and `mmcblk0` are disks, `nvme0n1p2` a partition) and physical-looking interfaces only (`is_real_nic`: not `lo`, `veth`, `docker`, `br-`, `cni`, `flannel`, `cali`, `virbr`, `tun`, `tap`). An empty node_exporter answer is an error: `no metrics in the response (is this node_exporter?)`. A non-2xx answer is `<url>: <status>`.

With `also`, the second scrape's GPUs, temperatures, fans and faults are appended, and PSI, memory split, VM counters, CPU frequency, RAPL, blocked processes, disk IO, links and disk health fill what the first lacks (`Reading::merge`).

### Disk health on a host

node_exporter has no SMART collector. The host runs `smartmon.sh` from prometheus-community's `node-exporter-textfile-collector-scripts` as root on a systemd timer (every 5 minutes), writing `smartmon.prom` (via a temp file and `mv`) into the directory given to node_exporter as `--collector.textfile.directory=/var/lib/node_exporter/textfile`; `node_textfile_scrape_error 0` shows the directory is read. On Debian, `prometheus-node-exporter-collectors` ships the script and `prometheus-node-exporter-smartmon.timer`. One `DiskHealth` per drive with `smartmon_device_smart_healthy` (drives with `smart_available 0` are skipped), named by `disk` without `/dev/` (`sda`, `nvme0`: the NVMe controller, not the `nvme0n1` namespace of the throughput rows). The model is `device_model`, else `product`, else `model_family`. `smartmon.sh` exports only the overall verdict for NVMe drives; the rest of an NVMe row (`nvme_health`) comes from elsewhere. A PCP host takes the data from its `also` node_exporter. Gauge does not check how old the files are: if the timer stops, the last values stay.

NVMe drives, by source:

| Field | Taken from (first one present) |
|---|---|
| verdict | `smartmon_device_smart_healthy`; a row with no smartmon verdict is healthy. Either way a non-zero `nvme_critical_warning` (spare under threshold, too hot, reliability degraded, read-only, backup failed) makes it failing |
| model | `smartmon_device_info` (empty for NVMe in practice), `node_nvme_info{device="nvme0"}.model`, `nvme_device_info.model` |
| temperature | `nvme_temperature_celsius`, then the hwmon chip `nvme_<controller>`: sensor `temp1` (the composite; `temp2`... are hotter controller sensors, not used), else its first sensor |
| power-on hours | `nvme_power_on_hours_total` |
| wear, spare, spare threshold | `nvme_percentage_used_ratio`, `nvme_available_spare_ratio`, `nvme_available_spare_threshold_ratio`, times 100 |
| media errors | `nvme_media_errors_total` |

`nvme_metrics.py` (prometheus-community, same repository; needs `nvme-cli` and Python's `prometheus_client`, runs as root) labels by namespace (`device="nvme0n1"`); its series are matched to the controller (`nvme_controller`: `nvme0n1` → `nvme0`). An NVMe controller that `nvme_metrics.py` reports and smartmon does not gets a row of its own. Without `nvme_metrics.py` an NVMe row has the verdict, model and temperature, and dashes for hours and wear. SATA SSDs are ATA drives to `smartmon.sh` and get the ATA columns (pending and uncorrectable are often not reported, and show as 0). Their wear is 100 minus the normalized value (`smartmon_<attr>_value`, the life left) of the first of `wear_leveling_count` (Samsung, 177), `media_wearout_indicator` (Intel, 233) and `percent_lifetime_remain` (Crucial, Micron, 202) the drive reports; other vendors' life attributes are not read. A spinning disk has no wear.

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
| `smart_failed` | `smart_failed:<drive>` | the drive's SMART overall verdict is not passed | always |
| `smart_sectors` | `smart_sectors:<drive>` | reallocated, pending or uncorrectable sectors are not zero; `detail` is `<drive> · <n> reallocated, <n> pending, <n> uncorrectable` (non-zero ones only) | pending or uncorrectable not zero |
| `smart_wear` | `smart_wear:<drive>` | a drive's wear (NVMe percentage used, SATA SSD life used) is 80 percent or more; `detail` is `<drive> · <n>% used` | 100 or more (rated endurance used up) |
| `smart_media` | `smart_media:<drive>` | an NVMe drive's media errors are not zero; `detail` is `<drive> · <n> media errors` | always |
| `hot` | `hot:<drive>` | a drive's temperature is 55 degrees C or more (NVMe: 70) | 60 degrees C or more (NVMe: 80) |
| `fills_soon` | `fills_soon:<mount>` | the forecast says a filesystem of this host fills in under 14 days | under 3 days |
| `down` | `down` | the scrape failed; `detail` is the error | always |

`key` is stable while the wording in `detail` changes, which is how an alert that opens and closes is told from one that merely changes. `fills_soon` is added by the store from the forecast the history refreshes every ten minutes, so it appears on the scrape after the refresh; its detail is `<mount> · <n> d` or `<mount> · <1 d`.

The store feeds each scrape's alerts to an `AlertLog`, which returns only changes: `Opened` for a key that was not open, `Closed` for one no longer present. While a host is silent its other alerts stay as they were, neither confirmed nor cleared, and one `down` alert is open. The changes go to the history writer (see [History](history.md)).
