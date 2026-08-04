# Forge Toolbox

Forge Toolbox (package `forge-toolbox`, binary `forge-toolbox`) is an interactive terminal UI for monitoring and updating installable Forge crates from the workspace's Cargo registry. Instead of running `cargo install --list` and `cargo search` by hand for each CLI in the workspace, it shows installed vs. latest versions in one table and lets you install or update a crate with a keypress.

## Features

- Table view of every monitored crate's package name, binary name, installed version, latest registry version, and whether an update is available.
- `Enter` runs `cargo install <package> --registry ennor` for the selected crate — a plain install if it isn't installed yet, `--force` if it's out of date.
- Background execution with a spinner so the UI stays responsive while `cargo install` runs.
- `r` refreshes installed/latest state on demand.
- A status note about `forge-toolbox` itself, since it cannot appear in its own monitored list (it can't replace its own running binary).

## Requirements

- `cargo`, configured with the `ennor` registry (`sparse+https://ennor.ddns.net/index/`).

## Usage

```bash
cargo run -p forge-toolbox
```

### Controls

| Key | Action |
|---|---|
| `Up` / `Down` | Move selection |
| `Enter` | Install or update the selected crate, depending on its current state |
| `r` | Refresh versions and installation state |
| `q` | Quit |

### Monitored crates

| Package | Binary |
|---|---|
| `anvil` | `anvil` |
| `conveyor-cli` | `conveyor` |
| `foreman` | `foreman` |
| `pulley` | `pulley` |
| `riveter` | `riveter` |
| `welder` | `welder` |
| `warehouse-cli` | `warehouse` |

Docker-only services under `docker/` are not listed, since nothing installs them with `cargo install`.

[Home](../README.md)
