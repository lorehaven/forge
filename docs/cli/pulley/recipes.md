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

## Back up a host that is sometimes off

Pulley runs on the machine that keeps the backup and pulls from the one that is switched off for hours a day. On the backup machine (`nogrod`), a job for the source host (`lorien`):

```toml
[[jobs]]
id = "lorien"
desc = "Backup from lorien"
src = "me@lorien:/mnt/data/projects"
dest = "/srv/backup/lorien/projects"
delete = true                      # what is deleted on lorien is deleted here too
no-confirm = true
interval = 900
```

Nothing is created on lorien. What happens:

* **lorien is off:** the check fails, one line `source not available (ssh: connect to host lorien ... No route to host)` is logged, the source is looked at again about every minute, and nothing is synced or deleted. When lorien is up again `source is available again` is logged and the backup runs straight away.
* **lorien is up but the drive is not mounted yet:** the folder does not exist (or, if the source is the mount point itself, it is empty), so the job is skipped as above and the backup keeps everything it has.
* **lorien is up and the drive is mounted:** a normal sync, `--delete` included. Remove a whole project on lorien and it is removed from the backup on the next run, however many files it had.

Put `Host lorien` settings (`HostName`, `User`, `IdentityFile`) in `~/.ssh/config` of the account that runs the daemon; the check and the sync use them, and the daemon cannot answer a password prompt, so use a key.
