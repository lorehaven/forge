# Pulley

Pulley is an interactive, REPL-based backup/sync tool built on `rsync`, configured through TOML files. Jobs are declared once, previewed with a dry run, and re-run from a persistent REPL session without re-typing flags. Jobs can also be listed and run non-interactively (`pulley list`, `pulley run <job_id>`), and pulley runs as a background service (`pulley daemon`, installable via `pulley service install`) that continuously syncs any job with an `interval` set.

See [docs/cli/pulley.md](../../docs/cli/pulley.md) for full documentation.
