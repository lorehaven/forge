---
title: Configuration
parent: toolbox
grand_parent: Command-line tools
nav_order: 2
---

Forge Toolbox has **no configuration file or flags**. Everything is compiled in (`forge_toolbox::lib`), except how the screen looks, which is read from the environment (see [forge-tui](../../libs/forge-tui/index.md)):

| Variable | Effect |
|---|---|
| `FORGE_THEME` | `dark` (default) or `light`. Without it, `COLORFGBG` (a background of 7 or 15 is light) decides. |
| `FORGE_PALETTE` | Brand colour: `green` (default), `purple`, `blue`, `teal`, `orange`, `rose`, as on the web. |
| `COLORTERM` | `truecolor` or `24bit` gives 24-bit colour; otherwise the 256-colour palette is used. |
| `NO_COLOR` | Any non-empty value turns all styling off (also `TERM=dumb`). |

Compiled in:

| Constant | Value |
|---|---|
| `REGISTRY_NAME` | `ennor` (the registry name `cargo` must know: `[registries.ennor]` in `~/.cargo/config.toml`) |
| `REGISTRY_INDEX` | `sparse+https://ennor.ddns.net/warehouse/index/` (displayed only) |

The crates are discovered from the registry (see [Commands](commands.md)). `MONITORED_CRATES` is only the fallback used when the registry cannot be asked, in display order:

| Package | Binary |
|---|---|
| `anvil` | `anvil` |
| `conveyor-cli` | `conveyor` |
| `foreman` | `foreman` |
| `pulley` | `pulley` |
| `riveter` | `riveter` |
| `loupe` | `loupe` |
| `welder` | `welder` |
| `warehouse-cli` | `warehouse` |
| `quench-tool` | `quench` |

`forge-toolbox` itself is not in the table (a running binary cannot replace itself); it appears only in the `note:` line. The services under `services/` are absent because nothing installs them with `cargo install`.

Credentials: Toolbox passes nothing to cargo, and its one direct request (the tool listing) is sent without a token, so with `CRATES_REQUIRE_AUTH_FOR_READS=true` it falls back to the fixed list. If the registry requires authentication (`CRATES_REQUIRE_AUTH_FOR_READS=true`), configure Cargo's credential provider, for example the one `anvil registry-token` provides (see the anvil pages).

The one direct HTTP request is `GET <registry>/api/v1/crates?bins=true&per_page=100` (15 s timeout). Child processes used: `cargo install --list`, `cargo search <package> --limit 10 --registry ennor`, `cargo install <package> --registry ennor [--force]`.
