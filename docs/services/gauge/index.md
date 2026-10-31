---
title: Gauge
parent: Services
nav_order: 7
has_children: true
---

Gauge is the host metrics dashboard of the estate. It polls the PCP (`pmproxy`) and `node_exporter` endpoints of a fixed list of hosts, keeps the last hour of each host in memory, folds the points into one-minute samples in Postgres (optional) and shows CPU, memory, pressure, GPU, disks, network, power, temperatures and alerts per host and across hosts.

## Crates

| Crate | Path | Version | Role |
|---|---|---|---|
| `gauge-core` | `services/gauge/core` | 0.1.0 | Wire model (`HostSummary`, `HostCard`, `HostDetail`, `HostHistory`, `DiskForecast`, `AlertEvent`, `Point`, `View`, `Range`), unit formatting, chart geometry, the disk forecast fit, alert thresholds. No I/O. |
| `gauge-app` | `services/gauge/app` | 0.1.0 | Screens and the `Request`/`Msg` protocol (`quench-ui`). |
| `gauge-web` | `services/gauge/web` | 0.1.0 | `wasm32` module: `fetch` transport, `Request` to URL mapping, `Msg` from the answer. |
| `gauge` | `services/gauge/service` | 0.2.3 | The server: collector, store, history, API, SSO and page routes. Binary `gauge-service`. |

The foundry catalog module `gauge` (0.4.0, schema `gauge`) owns the tables.

## Facts

* Default port under foreman: HTTPS 13443, redirect 13080, `BASE_PATH=/gauge` (`foreman.toml`).
* Dependencies: Gatehouse (JWKS, SSO, readiness); Redis for sessions (in-process store when `REDIS_URL` is unset); Postgres for history (optional). The monitored hosts are read over HTTP.
* Authentication: all of `/api/v1` is behind `Auth` (bearer token or session cookie, audience `gauge`). The service checks no `gauge:read` grant itself (the Gatehouse catalog lists the action `read`; see [Authentication and permissions](../../guides/authentication.md)). Browser pages need a session cookie and redirect to `/ui/login` otherwise.
* The service runs without a database: the live views work, the history, forecast and alert-log endpoints answer `503`.

## Pages

| Page | Content |
|---|---|
| [API](api.md), [openapi.yaml](openapi.yaml) | The 5 API routes and the 22 browser routes. |
| [Configuration](configuration.md) | Environment, `targets.toml`. |
| [Metrics and alerts](metrics.md) | What is read from PCP and node_exporter, how rates and alerts are derived. |
| [History](history.md) | Minute samples, retention, forecast, alert log. |
| [Data](data.md) | Tables of the `gauge` schema. |
| [Operations](operations.md) | Running, health, failure modes. |
| [Client](client.md) | Screens, requests, i18n keys. |

Shared behaviour (health routes, SSO routes, cookies, platform environment variables): [Common service runtime](../common-runtime.md).
