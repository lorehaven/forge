---
title: API
parent: Gauge
grand_parent: Services
nav_order: 1
---

The HTTP API of Gauge 0.2.3, rendered from [openapi.yaml](openapi.yaml) (OpenAPI 3.1.0). Host metrics dashboard. Scrapes PCP (pmproxy) and node_exporter, keeps the last hour in memory and, with a database, a month of one-minute samples. Everything under /api/v1 is behind the realm's Auth middleware (bearer token or session cookie); no per-action permission is checked by the service.

Paths below are relative to the base path (`BASE_PATH`, server variable `base`).

## hosts

Live state and stored history of the watched hosts.

### `GET /api/v1/hosts`

Every watched host with the CPU history its card draws. Operation id `listHosts`.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`HostCard`](#hostcard) | One entry per configured target, in configuration order. |
| 401 |  |  | No valid token or session (Auth middleware, also for unmapped /api/v1 paths). Empty or redirect for browsers. |

Example `200`:

```json
[
  {
    "name": "nogrod",
    "kind": "node",
    "url": "http://192.168.0.8:9100",
    "up": true,
    "error": null,
    "last_ok": 1761800000.5,
    "view": null,
    "cpu": [
      12.5,
      14.0,
      null,
      9.8
    ]
  }
]
```

### `GET /api/v1/hosts/{name}`

One host with its in-memory history (the last hour). Operation id `getHost`.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| name | path | yes | string | The `name` of a target in targets.toml. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`HostDetail`](#hostdetail) | The host and up to 360 points. |
| 401 |  |  | No valid token or session (Auth middleware, also for unmapped /api/v1 paths). Empty or redirect for browsers. |
| 404 | application/json | [`Error`](#error) | No target of that name. |

Example `404`:

```json
{
  "error": "no such host"
}
```

### `GET /api/v1/hosts/{name}/history`

Stored history of one host. Operation id `getHostHistory`.

Evenly spaced points from the one-minute samples in Postgres, grouped to the step of the range; a step with no samples is a gap (a point with every field null).

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| name | path | yes | string | The `name` of a target in targets.toml. |
| range | query | no | string (`24h`, `7d`, `30d`) | `1h` is a valid Range but is rejected here: the last hour is in the host's own answer. |
| stat | query | no | string | `max` selects the highest value of each step; any other value (or none) selects the mean. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`HostHistory`](#hosthistory) | 288 (24h), 336 (7d) or 360 (30d) points. |
| 400 | application/json | [`Error`](#error) | Unknown range, or `1h`. |
| 401 |  |  | No valid token or session (Auth middleware, also for unmapped /api/v1 paths). Empty or redirect for browsers. |
| 404 | application/json | [`Error`](#error) | No target of that name (checked first). |
| 500 | application/json | [`Error`](#error) | The query failed. |
| 503 | application/json | [`Error`](#error) | Gauge has no database connection. |

Example `200`:

```json
{
  "name": "nogrod",
  "range": "24h",
  "step_secs": 300,
  "points": [
    {
      "t": 1761720000.0,
      "cpu": 11.2,
      "mem": 40.1,
      "rx": 1200.0,
      "tx": 800.0,
      "read": 0.0,
      "write": 5000.0,
      "gpu": null,
      "vram": null,
      "psi_cpu": 0.1,
      "psi_mem": 0.0,
      "psi_io": 0.0,
      "disk_util": 1.5,
      "power": null
    },
    {
      "t": 1761720300.0,
      "cpu": null,
      "mem": null,
      "rx": null,
      "tx": null,
      "read": null,
      "write": null,
      "gpu": null,
      "vram": null,
      "psi_cpu": null,
      "psi_mem": null,
      "psi_io": null,
      "disk_util": null,
      "power": null
    }
  ]
}
```

Example `400`:

```json
{
  "error": "range is one of 24h, 7d, 30d (the last hour is in the host's own answer)"
}
```

Example `500`:

```json
{
  "error": "could not read the history"
}
```

Example `503`:

```json
{
  "error": "history is not available: Gauge has no database connection"
}
```

## storage

Filesystem forecast.

### `GET /api/v1/forecast`

How full each filesystem is and when it fills. Operation id `getForecast`.

A straight line fitted to the last seven days of hourly samples (see data.md).

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`DiskForecast`](#diskforecast) | One entry per host and mount seen in the last seven days; empty when none. |
| 401 |  |  | No valid token or session (Auth middleware, also for unmapped /api/v1 paths). Empty or redirect for browsers. |
| 500 | application/json | [`Error`](#error) | The query failed. |
| 503 | application/json | [`Error`](#error) | Gauge has no database connection. |

Example `200`:

```json
[
  {
    "host": "ossiriand",
    "mount": "/data",
    "size": 4000000000000.0,
    "used": 3100000000000.0,
    "per_day": 21000000000.0,
    "days_left": 42.8
  },
  {
    "host": "nogrod",
    "mount": "/",
    "size": 500000000000.0,
    "used": 120000000000.0,
    "per_day": null,
    "days_left": null
  }
]
```

Example `500`:

```json
{
  "error": "could not read the history"
}
```

Example `503`:

```json
{
  "error": "history is not available: Gauge has no database connection"
}
```

## alerts

Alert log.

### `GET /api/v1/alerts/history`

Alerts that opened or were open within the retention, newest first. Operation id `getAlertLog`.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| limit | query | no | integer `int64` | Clamped to 1..1000. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`AlertEvent`](#alertevent) | Alert events ordered by `opened` descending, then by id descending. |
| 401 |  |  | No valid token or session (Auth middleware, also for unmapped /api/v1 paths). Empty or redirect for browsers. |
| 500 | application/json | [`Error`](#error) | The query failed. |
| 503 | application/json | [`Error`](#error) | Gauge has no database connection. |

Example `200`:

```json
[
  {
    "host": "lorien",
    "key": "down",
    "kind": "down",
    "detail": "http://192.168.0.2:44322/metrics?names=...: error sending request",
    "severe": true,
    "opened": 1761790000.0,
    "closed": null
  },
  {
    "host": "ossiriand",
    "key": "hot:k10temp",
    "kind": "hot",
    "detail": "k10temp 92 °C",
    "severe": true,
    "opened": 1761700000.0,
    "closed": 1761701200.0
  }
]
```

Example `500`:

```json
{
  "error": "could not read the alert log"
}
```

Example `503`:

```json
{
  "error": "history is not available: Gauge has no database connection"
}
```

## ui

Browser pages, assets and SSO routes.

### `GET /ui`

Redirect to the home page, or to login without a session. Operation id `uiRoot`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/`

Same as /ui. Operation id `uiRootSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/home`

Client page, hosts screen. Operation id `uiHome`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/home/`

Same as /ui/home. Operation id `uiHomeSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/hosts/{name}`

Client page, host screen. Operation id `uiHost`.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| name | path | yes | string | The `name` of a target in targets.toml. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/fleet`

Client page, fleet screen. Operation id `uiFleet`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/storage`

Client page, storage screen. Operation id `uiStorage`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/gpus`

Client page, GPUs screen. Operation id `uiGpus`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/network`

Client page, network screen. Operation id `uiNetwork`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/power`

Client page, power screen. Operation id `uiPower`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/alerts`

Client page, alerts screen. Operation id `uiAlerts`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/assets/quench.js`

The quench-ui loader script. Operation id `uiAssetScript`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | JavaScript. |

### `GET /ui/assets/app.css`

Shell stylesheet followed by Gauge's stylesheet. Operation id `uiAssetStyle`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/css | string | CSS. |

### `GET /ui/assets/shell.js`

The forge-shell script. Operation id `uiAssetShell`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | JavaScript. |

### `GET /ui/assets/favicon.png`

The service icon. Operation id `uiAssetFavicon`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | image/png | string `binary` | PNG. |

### `GET /ui/assets/app.wasm`

The browser client module. Operation id `uiAssetModule`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/wasm | string `binary` | WebAssembly module. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/login`

Start sign-in (SSO redirect to Gatehouse). Operation id `uiLogin`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home (refresh cookie accepted) or to Gatehouse's /api/v1/authorize; sets forge_authorize_state. |
| 503 |  |  | Text `gatehouse is not configured`. |

### `GET /ui/login/`

Same as /ui/login. Operation id `uiLoginSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | See /ui/login. |
| 503 |  |  | Text `gatehouse is not configured`. |

### `GET /ui/auth/callback`

Complete the authorization-code exchange. Operation id `uiAuthCallback`.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| code | query | no | string |  |
| state | query | no | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home with session cookies, or to {base}/ui/login on any failure. |
| 503 |  |  | Text `gatehouse is not configured`. |

### `GET /ui/logout`

Realm logout through Gatehouse. Operation id `uiLogout`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To Gatehouse's /ui/logout with a redirect back to {base}/ui/login. |
| 503 |  |  | Text `gatehouse is not configured`. |

### `GET /ui/status`

Session status for the page shell. Operation id `uiStatus`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | Always 200. |

### `POST /ui/refresh`

Exchange the refresh cookie for new cookies. Operation id `uiRefresh`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | New session and refresh cookies set. |
| 401 |  |  | No refresh cookie |

Example `200`:

```json
{
  "authenticated": true,
  "username": null,
  "roles": []
}
```

## Schemas

### Error

| Field | Type | Required | Description |
|---|---|---|---|
| error | string | yes |  |

### HostSummary

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| kind | string (`pcp`, `node`) | yes |  |
| url | string | yes |  |
| up | boolean | yes |  |
| error | string or null | yes | Why the last scrape failed. |
| last_ok | number or null | yes | Unix seconds of the last good scrape. |
| view | [`View`](#view) or null | yes |  |

### HostCard

HostSummary fields flattened, plus cpu.

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| kind | string (`pcp`, `node`) | yes |  |
| url | string | yes |  |
| up | boolean | yes |  |
| error | string or null | yes | Why the last scrape failed. |
| last_ok | number or null | yes | Unix seconds of the last good scrape. |
| view | [`View`](#view) or null | yes |  |
| cpu | array of number or null | yes | CPU busy percent per history point, oldest first. |

### HostDetail

HostSummary fields flattened, plus history.

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| kind | string (`pcp`, `node`) | yes |  |
| url | string | yes |  |
| up | boolean | yes |  |
| error | string or null | yes | Why the last scrape failed. |
| last_ok | number or null | yes | Unix seconds of the last good scrape. |
| view | [`View`](#view) or null | yes |  |
| history | array of [`Point`](#point) | yes |  |

### HostHistory

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| range | string (`24h`, `7d`, `30d`) | yes |  |
| step_secs | integer | yes | 300, 1800 or 7200. |
| points | array of [`Point`](#point) | yes |  |

### Point

| Field | Type | Required | Description |
|---|---|---|---|
| t | number | yes | Unix seconds. |
| cpu | number or null | yes |  |
| mem | number or null | yes |  |
| rx | number or null | yes | Bytes per second received |
| tx | number or null | yes |  |
| read | number or null | yes | Bytes per second read |
| write | number or null | yes |  |
| gpu | number or null | yes | Busiest GPU |
| vram | number or null | yes | Fullest GPU memory |
| psi_cpu | number or null | yes |  |
| psi_mem | number or null | yes |  |
| psi_io | number or null | yes |  |
| disk_util | number or null | yes | Busiest disk |
| power | number or null | yes | Hungriest GPU |

### View

The latest numbers of a host (rates need two scrapes).

| Field | Type | Required | Description |
|---|---|---|---|
| at | number | yes |  |
| hostname | string or null | yes |  |
| uptime_secs | number or null | yes |  |
| cpu_pct | number or null | yes |  |
| cores_pct | array of array | yes | Pairs [core id, percent]. |
| load | array of number | yes |  |
| mem_total | number or null | yes |  |
| mem_used | number or null | yes |  |
| swap_total | number or null | yes |  |
| swap_used | number or null | yes |  |
| filesystems | array of [`Filesystem`](#filesystem) | yes |  |
| disks | array of [`Rate`](#rate) | yes | a = read, b = write, bytes per second. |
| nics | array of [`Rate`](#rate) | yes | a = received, b = sent, bytes per second. |
| gpus | array of [`Gpu`](#gpu) | yes |  |
| temps | array of [`Temp`](#temp) | yes |  |
| psi | [`PsiPct`](#psipct) or null | yes |  |
| mem_parts | [`MemParts`](#memparts) or null | yes |  |
| swap_in_per_sec | number or null | yes |  |
| swap_out_per_sec | number or null | yes |  |
| oom_kills | number or null | yes |  |
| tcp_retrans_per_sec | number or null | yes |  |
| disk_stats | array of [`DiskStat`](#diskstat) | yes |  |
| links | array of [`LinkView`](#linkview) | yes |  |
| cpu_freq_mhz | number or null | yes |  |
| cpu_watts | number or null | yes |  |
| procs_blocked | number or null | yes |  |
| fans | array of [`Fan`](#fan) | yes |  |
| faults | array of [`Fault`](#fault) | yes |  |
| alerts | array of [`Alert`](#alert) | yes |  |

### Filesystem

| Field | Type | Required | Description |
|---|---|---|---|
| device | string | yes |  |
| mount | string or null | yes |  |
| size | number | yes |  |
| avail | number | yes |  |

### Rate

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| a_per_sec | number | yes |  |
| b_per_sec | number | yes |  |

### Gpu

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| busy_pct | number or null | yes |  |
| vram_used | number or null | yes |  |
| vram_total | number or null | yes |  |
| temp_c | number or null | yes |  |
| power_w | number or null | yes |  |
| power_cap_w | number or null | yes |  |
| fan_rpm | number or null | yes |  |
| fan_max_rpm | number or null | yes |  |
| sclk_mhz | number or null | yes |  |
| mclk_mhz | number or null | yes |  |
| gtt_used | number or null | yes |  |
| gtt_total | number or null | yes |  |

### Temp

| Field | Type | Required | Description |
|---|---|---|---|
| label | string | yes |  |
| celsius | number | yes |  |

### PsiPct

| Field | Type | Required | Description |
|---|---|---|---|
| cpu | number | yes |  |
| mem | number | yes |  |
| mem_full | number | yes |  |
| io | number | yes |  |
| io_full | number | yes |  |

### MemParts

| Field | Type | Required | Description |
|---|---|---|---|
| anon | number | yes |  |
| cached | number | yes |  |
| buffers | number | yes |  |
| dirty | number | yes |  |

### DiskStat

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| util_pct | number | yes |  |
| iops | number | yes |  |
| read_ms | number or null | yes |  |
| write_ms | number or null | yes |  |
| queue | number | yes |  |

### LinkView

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| up | boolean | yes |  |
| speed | number or null | yes | Bytes per second. |
| errors_per_sec | number | yes |  |

### Fan

| Field | Type | Required | Description |
|---|---|---|---|
| label | string | yes |  |
| rpm | number | yes |  |

### Fault

| Field | Type | Required | Description |
|---|---|---|---|
| what | string | yes |  |
| count | number | yes |  |

### Alert

| Field | Type | Required | Description |
|---|---|---|---|
| key | string | yes |  |
| kind | string | yes | fault, oom, disk_full, hot, pressure_mem, pressure_io, swapping, fills_soon (and down in the log). |
| detail | string | yes |  |
| severe | boolean | yes |  |

### DiskForecast

| Field | Type | Required | Description |
|---|---|---|---|
| host | string | yes |  |
| mount | string | yes | The mount point |
| size | number | yes |  |
| used | number | yes |  |
| per_day | number or null | yes | Bytes added a day; null without enough history. |
| days_left | number or null | yes |  |

### AlertEvent

| Field | Type | Required | Description |
|---|---|---|---|
| host | string | yes |  |
| key | string | yes |  |
| kind | string | yes |  |
| detail | string | yes |  |
| severe | boolean | yes |  |
| opened | number | yes | Unix seconds. |
| closed | number or null | yes |  |

### AuthStatus

| Field | Type | Required | Description |
|---|---|---|---|
| authenticated | boolean | yes |  |
| username | string or null | yes |  |
| roles | array of string | yes |  |

