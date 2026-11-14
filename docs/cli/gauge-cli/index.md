---
title: gauge-cli
parent: Command-line tools
nav_order: 10
permalink: /cli/gauge-cli/
---

`gauge` is a full-screen terminal monitor of the hosts [Gauge](../../services/gauge/index.md) watches. It is drawn with `forge-tui`, so it looks like `forge-toolbox`, it fetches again every 5 seconds, and every function is on the keyboard. Package `gauge-cli`, binary **`gauge`**, library crate `gauge_cli` (directory `forge/cli/gauge-cli`).

Verified by unit tests (keys, layout at several widths, tiles, configuration and the config file). Not run against a live Gauge or Gatehouse for this page.

## Install

```bash
cd forge/cli/gauge-cli
cargo install --path .
```

## Configuration

Sign in once and `gauge` needs no arguments:

```bash
GAUGE_PASSWORD=... gauge login --url https://ennor.ddns.net --username me
gauge
```

`gauge login` signs in to Gatehouse (`{url}/gatehouse/api/v1/auth/login`) and writes `~/.config/gauge/config.toml` (under `$XDG_CONFIG_HOME` when set), readable by its owner only. The password is not kept, and the token is never printed.

```toml
url = "https://ennor.ddns.net"
token = "…"         # a Gatehouse refresh token: written by `gauge login`
path = "/gauge"     # optional: Gauge's path under the server
```

Gatehouse rotates a refresh token each time it is used, so every run writes the new one back before it goes on, and the stored session keeps going for as long as it is used. If it has ended (unused too long, or signed out in Gatehouse), `gauge` says so and `gauge login` signs in again. Gatehouse has no long-lived API tokens, so the token cannot be made by hand.

Each setting comes from the first of: flag, environment, file. A password sign-in (flags or environment) is used instead of a stored token.

| Flag | Environment | File | Meaning |
|---|---|---|---|
| `--url` | `GAUGE_URL` | `url` | The server Gauge and Gatehouse are on (required). |
| `--path` | `GAUGE_PATH` | `path` | Gauge's path under the server; default `/gauge`. |
| `--username` | `GAUGE_USERNAME` | | Account to sign in as, for `login` or instead of the stored session. |
| `--password` | `GAUGE_PASSWORD` | | Its password. |
| | `GAUGE_TOKEN` | `token` | A refresh token to resume. |

The sign-in happens before the screen opens, so a wrong password or an ended session is an error line and exit code 1. The account needs access to Gauge.

## Pages

The tabs and their order are the web version's. Press the number or `Tab`.

The Hosts page and an opened host start with stat tiles (each a value, a bar to the eighth of a cell and a sparkline of the last hour); they give way on a short terminal. The host page, and the Storage, GPUs, Network, Power and Alerts pages, are sets of rounded cards, one per section, in two columns on a wide terminal; they scroll as a whole. An opened host also fetches its live history (`GET {path}/api/v1/hosts/{name}`) with every refresh.

| Key | Page | Shows |
|---|---|---|
| `1` | Hosts | One rounded card per host, as in the web grid: name, alert badge, up or unreachable, cores · kind · url, meters for CPU, memory, GPU and disk, a two-line CPU chart, and the load and network rates. As many cards to a row as fit (at least 44 columns each); the selected card has the brand-coloured border; on a short terminal the cards shrink (no URL line, no chart). Under the cards, **Active alerts**: up to five open alerts (down hosts first, then severe) with how long each has been open, "and N more" past that, or "All clear". |
| `2` | Fleet | The fleet tiles on top (as on Hosts), then a table with a row per host: status, CPU with its last-hour trend, memory, pressure (worst of cpu/memory/io), disk, GPU, hottest sensor (amber from 75 °C, red from 85 °C) and alert count (red when severe or the host is down). Columns drop from the right of the table on a narrow terminal, the trend first. `↑↓` select, `↵` or `→` open the host. |
| `3` | Storage | Summary (capacity, free, which filesystem fills first, the fullest), then per host every filesystem with growth per day and days until full, disk IO (busy, IOPS, latency) and hardware faults. |
| `4` | GPUs | Per GPU: busy, VRAM, GTT and power meters; temperature, clocks, fan. |
| `5` | Network | Totals received and sent, TCP retransmits, then every interface with link state, speed and errors. |
| `6` | Power | Measured draw per host (CPU package plus GPUs; not wall power) and the fans. |
| `7` | Alerts | Alerts now and the 30 most recently ended, with how long each lasted. |
| `↵` on a host (Hosts or Fleet) | Host | Overview, alerts, CPU and cores, memory and swap, pressure, GPUs, filesystems, disk activity and health, network, sensors, faults. |

The API calls are `GET {path}/api/v1/hosts` (required), `/forecast` and `/alerts/history` (optional: without stored history the pages are shorter). A failed refresh keeps the last numbers on screen and shows the reason in the status line.

## Keys

| Key | Action |
|---|---|
| `↑` `↓`, `k` `j` | Move by a row of host cards / scroll a page |
| `←` `→`, `h` `l` | Move along a row of host cards |
| `PgUp` `PgDn`, `Space` | A page up / down |
| `Home` `End`, `g` `G` | First / last |
| `↵` (`→`, `l` on Fleet) | Open the selected host |
| `Esc`, `←`, `h`, `Backspace` | Back from an open host |
| `Tab`, `Shift+Tab`, `1`–`7` | Next / previous page, or a page by number |
| `r` | Refresh now |
| `q`, `Ctrl+C` | Quit |

`FORGE_THEME` (`dark`/`light`), `FORGE_PALETTE` and `NO_COLOR` apply as in `forge-toolbox`.
