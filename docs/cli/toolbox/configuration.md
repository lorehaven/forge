---
title: Configuration
parent: toolbox
grand_parent: Command-line tools
nav_order: 2
---

Forge Toolbox has **no configuration file, environment variables or flags**. Everything is compiled in (`forge_toolbox::lib`):

| Constant | Value |
|---|---|
| `REGISTRY_NAME` | `ennor` (the registry name `cargo` must know: `[registries.ennor]` in `~/.cargo/config.toml`) |
| `REGISTRY_INDEX` | `sparse+https://ennor.ddns.net/index/` (displayed only) |

Monitored crates (`MONITORED_CRATES`, in display order):

| Package | Binary |
|---|---|
| `anvil` | `anvil` |
| `conveyor-cli` | `conveyor` |
| `foreman` | `foreman` |
| `pulley` | `pulley` |
| `riveter` | `riveter` |
| `welder` | `welder` |
| `warehouse-cli` | `warehouse` |
| `quench-tool` | `quench` |

`forge-toolbox` itself is not in the table (a running binary cannot replace itself); it appears only in the `note:` line. The services under `services/` are absent because nothing installs them with `cargo install`.

Credentials: Toolbox passes nothing to cargo. If the registry requires authentication (`CRATES_REQUIRE_AUTH_FOR_READS=true`), configure Cargo's credential provider, for example the one `anvil registry-token` provides (see the anvil pages).

Child processes used: `cargo install --list`, `cargo search <package> --limit 10 --registry ennor`, `cargo install <package> --registry ennor [--force]`.
