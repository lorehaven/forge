# Gauge Service

Gauge is the estate's host metrics dashboard. It scrapes the [PCP](https://pcp.io) (`pmproxy`) and `node_exporter` endpoints of the hosts it is told about, keeps the last hour in memory and shows CPU, memory, GPU, disks, network and temperatures per host. Binary and crate: `gauge-service` (`services/gauge/service`). No database: a restart starts the charts over.

## Hosts

`config/targets.toml` (or the file `GAUGE_TARGETS` names):

```toml
interval_secs = 10          # 1-3600

[[target]]
name = "ossiriand"          # unique
kind = "pcp"                # pcp (pmproxy, :44322) or node (node_exporter, :9100)
url  = "http://ossiriand:44322"
also = "http://ossiriand:9100"   # optional: a node_exporter on the same machine, for its GPUs and temperatures
```

There is no discovery yet; hosts are listed by hand.

## What is read

| | PCP (`/metrics?names=...`) | node_exporter (`/metrics`) |
|---|---|---|
| CPU, per core | `kernel.all.cpu.*`, `kernel.percpu.cpu.*` | `node_cpu_seconds_total` |
| Load, uptime | `kernel.all.load`, `kernel.all.uptime` | `node_load*`, `node_boot_time_seconds` |
| Memory, swap | `mem.physmem`, `mem.util.available`, `swap*` | `node_memory_*` |
| Filesystems | `filesys.capacity`, `filesys.avail` (device only; pmproxy omits mount points) | `node_filesystem_*` |
| Disk, network I/O | `disk.dev.*_bytes`, `network.interface.*.bytes` | `node_disk_*`, `node_network_*` |
| GPU | `nvidia.*`, `amdgpu.*` PMDAs, when installed | `node_drm_*` (`--collector.drm`) |
| Temperatures | not read | `node_hwmon_temp_celsius` |

PCP units come from the metric's own header, so values are scaled without a table. Partitions, loop and device-mapper disks, and bridge/veth/CNI interfaces are left out.

## UI and API

`/ui/home`: one card per host. `/ui/hosts/{name}`: the full page. Both refresh every 10 s. `GET /api/v1/hosts` and `/api/v1/hosts/{name}` (with history) return JSON behind the realm's `Auth`. The catalog entry `gauge` has one action, `read`.

## Running it

`foreman start gauge` (port 13443, `/gauge`), or without a realm: see the crate's `README.md`.

Without a realm (auth off), from `services/gauge/service`:

```text
SERVICE_AUTH_ENABLED=false BASE_PATH=/gauge SERVER_ADDR=127.0.0.1:11444 SERVER_CERT_PATH=/nonexistent \
GATEHOUSE_URL=http://127.0.0.1:1 GATEHOUSE_CLIENT_ID=gauge GATEHOUSE_CLIENT_SECRET=local \
SERVICE_REALM=forge SERVICE_NAME=gauge LOG_DIR=/tmp/gauge-logs ../../target/debug/gauge-service
```

then <http://localhost:11444/gauge/ui/home>.
