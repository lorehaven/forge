# Quench CLI

`quench-cli` (crate `quench_cli`) is a small shared terminal-UI library used by Forge's CLI and REPL tools, so they present one consistent style (colors, banners, status lines, prompts) instead of each rolling its own ANSI-code handling. It depends only on `crossterm` for styling. Consumers across the workspace include `anvil`, `riveter`, `pulley`, `warehouse-cli`, `forge-toolbox`, `welder`, `conveyor-cli`, `foreman`, and the `quench-starter`/`quench-client` libraries.

## Public API / Key Types

All in `quench_cli::terminal` (`libs/quench-cli/src/terminal.rs`):

- `print_box_banner(title, subtitle)` — draws a bordered box banner.
- `print_status(tone, label, message)` — prints a colored status line; `tone` is a `Tone` (`Info`, `Success`, `Warn`, `Error`).
- `print_component_preview(name, description)` — prints a `[name] description` line.
- `repl_prompt(app, context) -> String` — formats a REPL prompt like `app(context)> `.
- `print_line(message)` / `print_error_line(message)` — println/eprintln wrappers taking `impl AsRef<str>`.
- `print_inline(message)` — prints without a trailing newline and flushes stdout immediately.
- Color/formatting constants: `RESET`, `BOLD`, `DIM`, `CYAN`, `BLUE`, `GREEN`, `YELLOW`, `WHITE` (raw ANSI escape sequences), plus `SEP` and `SEP_THIN` (horizontal separator strings).

`quench_cli::prelude` re-exports the subset most callers need: `print_box_banner`, `print_status`, `print_line`, `repl_prompt`, `Tone`, and the color/separator constants. `print_component_preview`, `print_error_line`, and `print_inline` are reachable via `quench_cli::terminal::*` but not re-exported from `prelude`.

## Usage example

```rust
use quench_cli::prelude::*;

print_box_banner("Riveter", "interactive REPL");
print_status(Tone::Success, "OK", "connected to switchboard");
let prompt = repl_prompt("riveter", "default");
```

## Integration checks

The crate has no automated test suite of its own; confidence comes from running the consumers that exercise its terminal output:

```bash
cargo run -p anvil -- --help
cargo run -p riveter -- repl
cargo run -p pulley
cargo run -p warehouse-cli -- --help
cargo run -p forge-toolbox
cargo run -p welder -- --workflow ./welder/samples/agent.toml
```

[Home](../README.md)
