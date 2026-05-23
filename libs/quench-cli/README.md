# Quench CLI Library

Shared terminal UI library used by Forge CLI/REPL modules to keep one style and prompt system.

## Exposed API

- `quench_cli::terminal::print_box_banner`
- `quench_cli::terminal::print_status`
- `quench_cli::terminal::repl_prompt`
- `quench_cli::terminal::{Tone, RESET, BOLD, DIM, CYAN, BLUE, GREEN, YELLOW, WHITE, SEP, SEP_THIN}`

## Integration Checks

```bash
cargo run -p anvil -- --help
cargo run -p riveter -- repl
cargo run -p pulley
cargo run -p warehouse-cli -- --help
cargo run -p forge-toolbox
cargo run -p welder -- --workflow ./welder/samples/agent.toml
```
