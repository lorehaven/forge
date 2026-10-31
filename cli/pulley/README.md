# pulley
Backup and sync jobs powered by rsync.

## What it is
`pulley` runs rsync-backed jobs described in TOML: interactively (a small shell), once from the command line, continuously as a foreground daemon, or as a background service (systemd user unit, runit, or a Windows scheduled task). Jobs come from `~/.config/pulley/*.toml` and `*.pulley.toml` in the current directory.

## Use cases
- Keep folders backed up to another machine or disk.
- Run a named job once (`pulley run <job>`).
- Install the daemon as a user service (`pulley service install`).

## Example
```sh
pulley list
pulley run all
pulley service install
```

## Documentation
Full documentation: [cli/pulley](../../docs/cli/pulley/index.md)
