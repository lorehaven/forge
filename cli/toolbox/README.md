# Forge Toolbox

Forge Toolbox is an interactive terminal UI for monitoring and updating installable Forge crates from the configured registry.

## Run

```bash
cargo run -p forge-toolbox
```

## Controls

- `Up` / `Down`: move selection
- `Enter`: install or update selected crate (depending on current state)
- `r`: refresh versions and installation state
- `q`: quit

Toolbox monitors:

- `anvil`
- `pulley`
- `riveter`
- `welder`
- `warehouse-cli` (binary name: `warehouse`)
