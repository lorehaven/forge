---
title: REPL
parent: pulley
grand_parent: Command-line tools
nav_order: 2
---

`pulley` with no arguments opens the interactive shell. Verified with piped input in a scratch environment (without running a sync).

```text
$ pulley
pulley loaded configuration from:
  - ./a.pulley.toml
config loaded successfully. 3 job(s) found.

┌─────────────────────────────────────────────┐
│ Pulley REPL                                 │
│ backup and sync jobs                        │
└─────────────────────────────────────────────┘
hint type `help` for available commands

pulley(repl)> help
help available commands
  list                    - List all configured jobs
  run <job_id> [...]      - Run specific job(s) by ID
  run all                 - Run all jobs
  reload                  - Reload configuration file
  help                    - Show this help message
  quit, exit              - Exit the REPL
pulley(repl)>
```

* **Prompt**: `pulley(repl)> `.
* Line editing and in-memory history from the `quench-cli` editor (arrows, Home/End, Ctrl-A/E/B/F/K/U/W/L/P/N/T); no completion. Ctrl-C and Ctrl-D end the session (`repl session closed`).
* Words are split on whitespace; an empty line is ignored.

| Command | Does |
|---|---|
| `help` | Prints the table above. |
| `list` | As `pulley list`. |
| `run <job_id> [...]`, `run all` | As `pulley run`. `run` alone prints `Usage: run <job_id> [...] \| run all`; no match prints `No matching jobs found`. Asks `Continue? (y/n): ` per job unless `no-confirm`. An rsync error is shown as `error <message>` and the session continues. |
| `reload` | Re-reads all config files (`Reloading configuration...`, `Configuration reloaded successfully. N job(s) found.`). If no job loads, the error is shown and the old configuration is kept. |
| `quit`, `exit` | `repl goodbye`, then `repl session closed`. |
| anything else | `command unknown command <x>. type help` |
