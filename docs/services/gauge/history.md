---
title: History
parent: Gauge
grand_parent: Services
nav_order: 6
---

Optional persistence of what the store sees. Source: `service/src/domain/history.rs`. Without `DATABASE_URL` the task only drains the channel; with one it connects in the background and retries every 30 seconds (`history: cannot reach the database (<error>), trying again`), so the database may start after Gauge. A database that is down costs history, never a scrape.

## Flow

The store sends two kinds of notes down an unbounded channel: a `Tick` per scrape that drew a point (host, point, filesystems) and an alert change (opened or closed). One task (`keep`) consumes them and wakes every 30 seconds.

1. Minutes. A `Rollup` per host folds points into one-minute buckets (mean and maximum of each of the 13 series; gap points are ignored; non-finite values skipped). A minute is written when a later minute starts or at a sweep whose clock minute is later. Write: one `INSERT ... ON CONFLICT (host, t) DO NOTHING` per host per minute. A failed write logs `history: <host>: could not store a minute: <error>` and is dropped.
2. Filesystems. At most every 600 seconds per host the hour's row of every filesystem is upserted.
3. Forecast. Every 600 seconds the forecast is recomputed from the last seven days of `filesystem_samples` and stored in the shared cell that the store reads for `fills_soon` alerts and the API serves.
4. Alerts. `Opened` inserts a row (ignored if one is open for that host and key); `Closed` sets `closed` on the open row.
5. Pruning. At the first sweep and then at most hourly: deletes older than `HISTORY_DAYS` (default 30, 1..365) from `samples`, `filesystem_samples` and `alert_events`; logs `history: dropped N samples older than D days` when something went.
6. At start, after connecting: every `alert_events` row with `closed IS NULL` is closed at the current time (`history: closed N alerts a previous run left open`). An alert that spans a restart is therefore two rows and the downtime counts as part of the first.

## Reading a range

`GET /api/v1/hosts/{name}/history?range=24h|7d|30d[&stat=max]` aligns the end to the step containing now and returns `window / step` slots, each the mean (or, with `stat=max`, the maximum of the `_max` columns) of the samples that fall into the step, grouped in SQL by `floor(epoch / step)`. A slot with no row is a gap point.

| Range | Window | Step | Points |
|---|---|---|---|
| `1h` (live, memory only) | 3600 s | 10 s | up to 360 |
| `24h` | 86 400 s | 300 s | 288 |
| `7d` | 604 800 s | 1800 s | 336 |
| `30d` | 2 592 000 s | 7200 s | 360 |

`Range::Live` ("1h") is not accepted by the history endpoint (400). The browser never sends `stat`.

## Disk forecast

`gauge-core::forecast::fit`: a least-squares line through (time, used bytes) of the last seven days' hourly samples. It returns nothing with fewer than 24 samples or a span under 12 hours. `per_day` is the slope times 86 400. `days_left` is `(size - used) / per_day` only when `per_day` is above 1 048 576 bytes a day and `used < size`, and only when at most 3650 days. A level or emptying disk has `days_left: null`. Each `DiskForecast.used` is the last sample, `size` the last size seen.

## Alert log

`GET /api/v1/alerts/history?limit=N` returns rows whose `COALESCE(closed, opened)` is within the retention, ordered `opened DESC, id DESC`, limit clamped to 1..1000 (default 200).
