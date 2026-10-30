---
title: Recipes
parent: pulley
grand_parent: Command-line tools
nav_order: 4
---

Verified: `list` and the REPL commands. Syncing, the daemon and the service were not run.

## First job

```bash
cat > docs.pulley.toml <<'TOML'
[[jobs]]
id = "documents"
desc = "Backup documents"
src = "/home/me/Documents"
dest = "/mnt/backup/documents"
delete = true
skip = ["tmp", ".cache"]
TOML
pulley list
pulley run documents          # shows the dry-run, asks before syncing
```

## Share jobs across projects

Put jobs in `~/.config/pulley/personal.toml`; override one in a project with a `*.pulley.toml` that reuses its `id`.

## Unattended sync

```toml
[[jobs]]
id = "documents"
desc = "Backup documents"
src = "/home/me/Documents"
dest = "/mnt/backup/documents"
interval = 600
```

```bash
pulley daemon                 # foreground, Ctrl-C to stop
pulley service install        # background (systemd --user, runit as root, or a Windows task)
pulley service status
tail -f ~/.config/pulley/logs/documents/$(date +%F).log
```

The daemon syncs without asking; it passes `--delete` to the update only if the job sets `delete = true`.
