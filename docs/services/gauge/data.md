---
title: Data
parent: Gauge
grand_parent: Services
nav_order: 3
---

Gauge owns the schema `gauge` (foundry module `gauge` 0.4.0, requires `quench-core@^0.1`; default schema `gauge`). Migrations are in `services/foundry/service/migrations/gauge`, in order. The service has no migrations of its own. In memory: per host the last reading, the latest view and up to 360 points (`HISTORY`); a restart empties them. Redis holds only the realm sessions. No files.

## Migrations

| Id | since | Creates |
|---|---|---|
| `0001-samples` | 0.2.0 | `samples` and index `samples_t_idx` |
| `0002-filesystem-samples` | 0.3.0 | `filesystem_samples` and index `filesystem_samples_t_idx` |
| `0003-alert-events` | 0.4.0 | `alert_events`, unique partial index `alert_events_open_idx`, index `alert_events_opened_idx` |

## `gauge.samples`

One row per host per minute; `t` is the start of the minute. Every series has a mean column and a `_max` column, both nullable (NULL where the host does not report it).

| Column | Type | Notes |
|---|---|---|
| `host` | TEXT NOT NULL | target name |
| `t` | TIMESTAMPTZ NOT NULL | primary key `(host, t)` |
| `scrapes` | INTEGER NOT NULL | scrapes folded into the minute (six at 10 s) |
| `cpu`, `cpu_max` | DOUBLE PRECISION | percent |
| `mem`, `mem_max` | DOUBLE PRECISION | percent |
| `net_rx`, `net_rx_max`, `net_tx`, `net_tx_max` | DOUBLE PRECISION | bytes per second |
| `disk_read`, `disk_read_max`, `disk_write`, `disk_write_max` | DOUBLE PRECISION | bytes per second |
| `gpu`, `gpu_max` | DOUBLE PRECISION | busiest GPU, percent |
| `vram`, `vram_max` | DOUBLE PRECISION | fullest GPU memory, percent |
| `psi_cpu`, `psi_cpu_max`, `psi_mem`, `psi_mem_max`, `psi_io`, `psi_io_max` | DOUBLE PRECISION | pressure, percent |
| `disk_util`, `disk_util_max` | DOUBLE PRECISION | busiest disk, percent |
| `gpu_power`, `gpu_power_max` | DOUBLE PRECISION | hungriest GPU, watts |

The writer inserts with `ON CONFLICT (host, t) DO NOTHING`: a restart inside a minute keeps the first row. The ledger comment sizes it at about 7 200 rows a day for five hosts.

## `gauge.filesystem_samples`

| Column | Type | Notes |
|---|---|---|
| `host` | TEXT NOT NULL | |
| `mount` | TEXT NOT NULL | the mount point, or the device when the agent (PCP) reports none |
| `t` | TIMESTAMPTZ NOT NULL | start of the hour; primary key `(host, mount, t)` |
| `size_bytes` | DOUBLE PRECISION NOT NULL | |
| `used_bytes` | DOUBLE PRECISION NOT NULL | `size - avail` |

Upserted (`ON CONFLICT ... DO UPDATE`) so the hour's row holds the latest values.

## `gauge.alert_events`

| Column | Type | Notes |
|---|---|---|
| `id` | BIGSERIAL | primary key |
| `host` | TEXT NOT NULL | |
| `key` | TEXT NOT NULL | what the alert is of: `hot:k10temp`, `fills_soon:/data`, `down` |
| `kind` | TEXT NOT NULL | |
| `detail` | TEXT NOT NULL | the wording when it opened |
| `severe` | BOOLEAN NOT NULL | |
| `opened` | TIMESTAMPTZ NOT NULL | index `alert_events_opened_idx` |
| `closed` | TIMESTAMPTZ | NULL while going |

`alert_events_open_idx` is `UNIQUE (host, key) WHERE closed IS NULL`: at most one open row per host and key (`INSERT ... ON CONFLICT (host, key) WHERE closed IS NULL DO NOTHING`).

## Retention

Once an hour (and at the first sweep after start) the service deletes `samples` and `filesystem_samples` rows with `t` older than `HISTORY_DAYS`, and `alert_events` rows whose `COALESCE(closed, opened)` is older. See [History](history.md).
