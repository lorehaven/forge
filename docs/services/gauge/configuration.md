---
title: Configuration
parent: Gauge
grand_parent: Services
nav_order: 2
---

## Service environment

Read in `services/gauge/service/src` (file in parentheses). Platform variables (`BASE_PATH`, `SERVER_ADDR`, `SERVICE_NAME`, `GATEHOUSE_*`, `REDIS_URL`, `RUST_LOG`, ...) are in [Common service runtime](../common-runtime.md); Gauge uses all of those that apply to a browser service with sessions.

| Variable | Default | Where | Meaning |
|---|---|---|---|
| `GAUGE_TARGETS` | `config/targets.toml` | `domain/config.rs` | Path of the targets file, relative to the working directory. Failing to read or parse it panics at start (`configuration: <reason>`). |
| `DATABASE_URL`, then `POSTGRES_URL` | empty | `main.rs` | History database. Empty: no history (info log `history: no DATABASE_URL, so no history beyond the last hour`). |
| `DB_SCHEMA` | `gauge` | `domain/history.rs` | Schema the history queries address. Not validated: it is interpolated into SQL. |
| `HISTORY_DAYS` | `30` | `history::retention_days` | Days kept, clamped to 1..365; an unparsable value gives 30. |
| `WEB_DIR` | `web` | `forge-web` | Directory holding `app.wasm`. |
| `SERVICE_AUTH_ENABLED` etc. | | `JwtConfig::init` | See common runtime. |

Under foreman the service gets the platform defaults plus `GATEHOUSE_CLIENT_SECRET` and `SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:13080`; `DATABASE_URL` is `postgres://postgres:postgres@localhost:5432/postgres` (the `gauge` schema exists only if foundry installed the `gauge` module; `config/install.toml` of foundry does not list it, so install it with `FOUNDRY_INSTALL` or `--install gauge`, or the history writes fail with a logged warning per minute).

## `targets.toml`

```toml
interval_secs = 10        # optional, default 10, clamped to 1..3600

[[target]]
name = "ossiriand"        # required, non-empty, unique
kind = "pcp"              # "pcp" or "node"
url  = "http://ossiriand:44322"
also = "http://ossiriand:9100"   # optional node_exporter on the same machine
```

| Key | Type | Default | Meaning |
|---|---|---|---|
| `interval_secs` | integer | 10 | Seconds between scrapes of each host. Clamped to 1..=3600. |
| `[[target]].name` | string | required | Shown name and the `{name}` of the host routes. Empty or repeated names are rejected: ``target name `<n>` is empty or repeated``. |
| `[[target]].kind` | `pcp` or `node` | required | `pcp`: `pmproxy`, scraped at `{url}/metrics?names=...`. `node`: `node_exporter`, scraped at `{url}/metrics`. |
| `[[target]].url` | string | required | Base URL; trailing `/` removed. |
| `[[target]].also` | string | none | A `node_exporter` base URL whose GPUs, sensors and error counters are merged into the host (for a PCP host without GPU modules). If it is down the host is still up without them. |

Unknown keys are not rejected. The shipped file lists `ossiriand` (pcp with also, :44322 and :9100), `nogrod` (node, `192.168.0.8:9100`) and `lorien` (pcp with also, `192.168.0.2`). Hosts are listed by hand; there is no discovery. The HTTP client has an 8 second timeout and the user agent `gauge`; certificates are verified with the default settings.

## Secrets

`GATEHOUSE_CLIENT_SECRET` (the OAuth client `gauge`, shared with Gatehouse's `CLIENT_SECRET_GAUGE`). Names only.
