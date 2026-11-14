# gauge-cli
Gauge's host dashboard in a terminal.

## What it is
`gauge` is a full-screen terminal monitor of the hosts Gauge watches: CPU, memory, disk and GPU meters, alerts and disk forecasts, refreshed every 5 seconds and driven entirely by the keyboard. It is styled like `forge-toolbox`.

## Use cases
- See at a glance which host is down or running hot.
- Open a host for its full page: cores, memory, GPUs, filesystems, disk health, network, sensors.
- See the active alerts under the host list, the fleet side by side, and which filesystem fills first.
- Read the GPU, network and power pages, and the log of ended alerts.

## Example
```sh
GAUGE_PASSWORD=... gauge login --url https://ennor.ddns.net --username me   # once: writes ~/.config/gauge/config.toml
gauge
```

Keys: `←↑↓→`/`hjkl` move between host cards or scroll a page, `↵` open a host, `Esc` back, `Tab`/`1`–`7` pages (Hosts, Fleet, Storage, GPUs, Network, Power, Alerts), `PgUp PgDn Home End` jump, `r` refresh now, `q` quit.

## Documentation
Full documentation: [cli/gauge-cli](../../docs/cli/gauge-cli/index.md)
