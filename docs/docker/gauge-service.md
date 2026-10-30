# Gauge Service

Gauge is the estate's host metrics dashboard. It scrapes the [PCP](https://pcp.io) (`pmproxy`) and `node_exporter` endpoints of the hosts it is told about, keeps the last hour in memory and a month of one-minute samples in Postgres, and shows CPU, memory, pressure, GPU, disks, network, power and temperatures per host and across hosts. Binary and crate: `gauge-service` (`services/gauge/service`). The database is optional: without `DATABASE_URL` Gauge runs as before, with the last hour only.

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

Beyond that node_exporter gives pressure stall info, the memory split (programs, cache, buffers), swap traffic and OOM kills, per-disk utilisation and latency, GPU power, fan, clocks and GTT, CPU frequency and RAPL watts, link state, speed and errors, fans, and btrfs / ECC error counters. A PCP host with an `also` node_exporter gets all of it. The thresholds that raise an alert are in `gauge-core` (`model.rs`, `alerts`). The plan and what is not done are in `plans/GAUGE_METRICS.md`.

PCP units come from the metric's own header, so values are scaled without a table. Partitions, loop and device-mapper disks, and bridge/veth/CNI interfaces are left out.

## UI and API

`/ui/home`: one card per host. `/ui/hosts/{name}`: the full page, with buttons for 1h (live), 24h, 7d and 30d. `/ui/fleet`, `/ui/storage`, `/ui/gpus`, `/ui/network`, `/ui/power`, `/ui/alerts`: the screens across hosts. They refresh every 10 s (the stored ranges every minute).

`GET /api/v1/hosts`, `/api/v1/hosts/{name}` (with the last hour), `/api/v1/hosts/{name}/history?range=24h|7d|30d[&stat=max]` and `/api/v1/forecast` and `/api/v1/alerts/history[?limit=]` return JSON behind the realm's `Auth`; the history, forecast and alert log answer 503 when there is no database. The catalog entry `gauge` has one action, `read`.

## History

One row per host per minute in `gauge.samples` (the mean and the highest of 13 series, NULL where a host does not report one), made by foundry's `gauge` module (`services/foundry/service/migrations/gauge`). The service folds the live points into minutes, writes each finished minute from a task of its own (a failed write is logged and dropped), and deletes what is older than `HISTORY_DAYS` (default 30) once an hour. Five hosts are 7 200 rows a day. The ranges are 288, 336 and 360 evenly spaced points; a step with nothing stored is a gap in the line. Environment: `DATABASE_URL` (or `POSTGRES_URL`), `DB_SCHEMA` (default `gauge`), `HISTORY_DAYS` (1-365).

`gauge.filesystem_samples` holds how full each filesystem was, once an hour (the hour's row is refreshed every ten minutes); the Storage screen's "growth" and "full in" come from a straight line fitted to the last seven days (`gauge-core`'s `forecast`). It needs at least 24 samples spanning half a day, gives no date for a disk that is level or emptying, and none further than ten years off. A PCP host reports devices, not mount points, so its rows are named by device.

**Alerts.** Each alert has a `key` (what it is of: `hot:k10temp`, `disk_full:/data`, `fills_soon:/data`, `down`) that stays the same while its wording follows the number. A filesystem the forecast fills within 14 days is an alert (`fills_soon`, severe under 3 days); the forecast is refreshed every ten minutes and the alert appears on the next scrape. The store hands every change to the history writer, which keeps `gauge.alert_events`: a row opens when an alert appears and closes when it is gone, a host that stops answering is one `down` alert (its other alerts wait and are neither confirmed nor cleared), and at most one row per host and key is open. Rows a previous run left open are closed at start, so an alert that spans a Gauge restart is two rows and the downtime counts as part of the first. The Alerts screen shows how long each current alert has been going and lists the last thirty that ended.

**Charts** have a time axis: how long ago each quarter of the window is (`−1 h`, `−45 min` ... `now`; `−7 d` ... for the stored ranges). It is relative, not clock times: the app does not know the browser's time zone.

The tests that need Postgres run when `GAUGE_TEST_DATABASE_URL` is set (see the header of `service/tests/integration/history_tests.rs`).

## Running it

`foreman start gauge` (port 13443, `/gauge`), or without a realm: see the crate's `README.md`.

Without a realm (auth off), from `services/gauge/service`:

```text
SERVICE_AUTH_ENABLED=false BASE_PATH=/gauge SERVER_ADDR=127.0.0.1:11444 SERVER_CERT_PATH=/nonexistent \
GATEHOUSE_URL=http://127.0.0.1:1 GATEHOUSE_CLIENT_ID=gauge GATEHOUSE_CLIENT_SECRET=local \
SERVICE_REALM=forge SERVICE_NAME=gauge LOG_DIR=/tmp/gauge-logs ../../target/debug/gauge-service
```

then <http://localhost:11444/gauge/ui/home>.
