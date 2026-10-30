---
title: Client
parent: Gauge
grand_parent: Services
nav_order: 5
---

The browser client is `gauge-app` (screens, protocol) compiled with `gauge-web` to WebAssembly and served by the service at `{base}/ui/assets/app.wasm`. General pattern: [The browser-client pattern](../../guides/browser-client.md). Service routes that start it are listed in the [API](api.md) under `ui`.

## Routes (owned by the app)

`gauge_app::live::shell` registers, with `base` the service prefix:

| Path | Screen | Data it asks for | Refresh |
|---|---|---|---|
| `{base}/ui`, `{base}/ui/`, `{base}/ui/home`, `{base}/ui/home/` | Hosts | `Hosts` | every 10 s |
| `{base}/ui/hosts/:name` | Host | `Host(name)`; `History(name, range)` once a stored range is chosen | host every 10 s; stored range every 60 s |
| `{base}/ui/fleet` | Fleet | `Hosts` | 10 s |
| `{base}/ui/storage` | Storage | `Hosts`, `Forecast` | hosts 10 s, forecast 60 s |
| `{base}/ui/gpus` | GPUs | `Hosts` | 10 s |
| `{base}/ui/network` | Network | `Hosts` | 10 s |
| `{base}/ui/power` | Power | `Hosts` | 10 s |
| `{base}/ui/alerts` | Alerts | `Hosts`, `AlertLog` | hosts 10 s, log 30 s |
| anything else | not found screen (`ui_host_not_found`) | | |

The service serves page routes for `/ui`, `/ui/`, `/ui/home`, `/ui/home/`, `/ui/hosts/{name}`, `/ui/fleet`, `/ui/storage`, `/ui/gpus`, `/ui/network`, `/ui/power`, `/ui/alerts` (the host list is `/ui/home`; there is no `/ui/hosts`). The refresh period is `Setup.refresh_ms`, set to 10 000 by the web crate; polls stop when the screen is left.

## Request and Msg

`Request` (app to host) and the HTTP call `gauge-web` makes for it (`protocol::url_for`; `GET` with `Accept: application/json` and the session cookie):

| Request | Call | Reply `Msg` |
|---|---|---|
| `Hosts` | `GET {base}/api/v1/hosts` | `Hosts(Vec<HostCard>)` |
| `Host(name)` | `GET {base}/api/v1/hosts/{name}` (name percent-encoded) | `Detail(HostDetail)` |
| `History(name, range)` | `GET {base}/api/v1/hosts/{name}/history?range={1h\|24h\|7d\|30d}` | `History(HostHistory)` |
| `Forecast` | `GET {base}/api/v1/forecast` | `Forecast(Vec<DiskForecast>)` |
| `AlertLog` | `GET {base}/api/v1/alerts/history` | `AlertLog(Vec<AlertEvent>)` |
| `Language(code)` | none: stored in browser storage key `language` | none |

Status mapping (`protocol::interpret`): 2xx parses the body (failure: `Failed("the service answered something unreadable: ...")`); 401 `SignedOut`; 403 `Failed("this account may not see Gauge: ask for access in gatehouse")`; 503 on history, forecast or alert log `NoHistory`; 404 on `Host` or `History` `NotFound(name)`; other statuses `Failed("the service answered <n>")`; transport error `Failed("cannot reach the service: <why>")`. Note: the app can emit `History(name, Live)` only through `url_for`'s `range=1h`, which the service rejects with 400; the host screen never asks for it (the Live range uses the host answer).

`Setup`: `base` (derived from the page path: text before `/ui/`), `language` (storage `language`, default `en-US`), `login_url` (`{base}/ui/login`), `history` (browser history), `now`, `refresh_ms`.

State shown in banners: `Failed(why)` as `p#problem.fui-notice.fui-notice-error`; `SignedOut` as the same element with the text "Your session ended." and a link `a#sign-in` to the login URL (these two strings are not translated).

## Screens

* Hosts (`screens::hosts`): grid of cards sorted by name, `a#host-{name}.gg-host` (class `gg-stale` when down): name, alert badge (`N alerts`, critical style when one is severe), up/unreachable state, `cores · kind · url`, outage notice, meters for CPU, memory, GPU (when a GPU exists, busiest) and disk (fullest filesystem), a CPU sparkline image, and a footer with the load and `down/up` network rates. Empty list: `p#no-hosts`; before the first answer: `p#waiting`.
* Host (`screens::host`): back link, name, `kind · url`, state; outage notice; overview stats (CPU, memory, load, uptime, GPU); the alerts section `div#alerts`; the range bar `div#ranges` with buttons `button#range-1h`, `range-24h`, `range-7d`, `range-30d` (`aria-pressed`) and the note `p#range-note` (no history, waiting, or "Showing the last 24h · 5m per point"); sections for CPU (and cores), memory (stack of programs, cache, buffers, dirty; swap), pressure, one section per GPU, disks, network, temperatures (if any), fans (if any). Charts are SVG images in `img` elements with a relative time axis (`-1 h` ... `now`). Choosing a stored range sets the range signal and asks `History`; choosing `1h` shows the live points.
* Fleet: table of hosts with CPU, memory, pressure (worst of cpu/mem/io), disk, GPU, hottest sensor, alert count.
* Storage: stats (capacity, free, fills first, fullest) then per host a filesystem table (used bar, size, growth, full in) and a disk IO table (busy, IOPS, latency read/write), plus hardware faults.
* GPUs: per GPU meters (busy, VRAM, power) and facts; `p#no-gpus` when none.
* Network: totals received and sent, per interface table (link, rx, tx, errors), TCP retransmits.
* Power: measured draw (CPU package plus GPUs) per host with the note that it is not wall power; fans table.
* Alerts: `div#alerts-page`: current alerts of all hosts (down hosts first as a severe `down`), sorted severe first then host, each with how long it has been open (from the log), `p#all-clear` when none; `div#alert-log` lists up to 30 ended alerts with "ago" and "lasted".
* Not found: `p#not-found`.

Level classes: a bar is `gg-ok` below 75 percent, `gg-warn` from 75, `gg-crit` from 90.

## Navigation and shell

The sidebar items (id, path, label key, icon) are `nav-hosts` `/home` `ui_nav_hosts` server; `nav-fleet` `/fleet` `ui_nav_fleet` grid; `nav-storage` `/storage` `ui_nav_storage` database; `nav-gpus` `/gpus` `ui_nav_gpus` cpu; `nav-network` `/network` `ui_nav_network` activity; `nav-power` `/power` `ui_nav_power` layers; `nav-alerts` `/alerts` `ui_nav_alerts` shield. The Hosts item is current for `/ui`, `/ui/` and `/ui/hosts/...`. The top bar is `forge_shell::Bar` with brand `Gauge`, icon `eye`, logout `{base}/ui/logout`, the dashboard button, and the language picker.

## Languages and i18n keys

Locales (first is the fallback): `en-US` English, `de-DE` Deutsch, `es-ES` Español, `fr-FR` Français, `pl-PL` Polski. Files `services/gauge/service/i18n/<locale>.ftl`, compiled into the module with `include_str!`; each has the same 116 entries as `en-US`. The key set (see the file for the English texts): shell and frame (`header_label`, `footer_label`, `locale_label`, `theme_label`, `ui_header_home`, `ui_home_button`, `ui_dashboard_button`, `ui_theme_toggle`, `ui_theme_mode`, `ui_theme_light`, `ui_theme_dark`, `ui_theme_palette`, `ui_palette_green|teal|blue|purple|rose|orange`, `ui_user_menu`, `ui_profile_edit`, `ui_logout`); navigation (`ui_nav_hosts|fleet|storage|gpus|network|power|alerts`); hosts (`ui_hosts_none`, `ui_host_not_found`, `ui_state_up`, `ui_state_down`, `ui_back`, `ui_waiting`); metrics (`ui_cpu`, `ui_memory`, `ui_swap`, `ui_gpu`, `ui_gpu_busy`, `ui_vram`, `ui_disk`, `ui_network`, `ui_load`, `ui_uptime`, `ui_temps`, `ui_power`, `ui_fan`, `ui_fans`, `ui_clocks`, `ui_gtt`, `ui_io`, `ui_pressure`, `ui_pressure_hint`, `ui_memory_full`, `ui_io_full`, `ui_mem_anon|cache|buffers|dirty`, `ui_swap_in`, `ui_swap_out`, `ui_oom`, `ui_tcp_retrans`); columns (`ui_col_device|used|size|read|write|rx|tx|name|temp|util|iops|latency|link|errors|speed|host|growth|full_in`); alerts (`ui_alerts`, `ui_alerts_title`, `ui_alerts_none`, `ui_alert_fault|oom|disk_full|hot|pressure_mem|pressure_io|swapping|down|fills_soon`, `ui_alert_for`, `ui_alert_lasted`, `ui_alert_log`, `ui_link_down`); fleet screens (`ui_storage_capacity|free|fullest|soonest`, `ui_power_cpu|gpu|total|note`, `ui_gpus_none`, `ui_hottest`, `ui_fleet_hint`); history (`ui_range_live`, `ui_range_unavailable`, `ui_range_last`, `ui_range_per_point`, `ui_not_growing`, `ui_day_less`, `ui_days`, `ui_now`). Alert wording looks up `ui_alert_<kind>`; a kind without a key shows the key text. Chosen language: the picker sends `Language(code)`, stored in `localStorage["language"]` (and `shell.js` mirrors the `qlocale` cookie).

## Styling hooks

Stylesheet `app/src/style.css` (served inside `app.css` after the shell styles). Root: `div#gauge.fui-app.fui-client`, content in `main.gg-main`. Classes: `gg-grid-hosts`, `gg-host`, `gg-stale`, `gg-head`, `gg-name`, `gg-sub`, `gg-meter`, `gg-bar` (+ `gg-ok|gg-warn|gg-crit`), `gg-foot`, `gg-page`, `gg-section`, `gg-stats`, `gg-badge` (+ `gg-badge-crit|gg-badge-warn`), `gg-alerts`, `gg-alert` (+ `gg-alert-crit|gg-alert-warn`), `gg-ranges`, `gg-range`, `gg-range-on`, `gg-range-bar`, `gg-log`, `gg-log-row`, `gg-log-when`, `gg-current`, `gg-muted`, `gg-back`.
