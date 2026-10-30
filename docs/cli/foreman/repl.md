---
title: REPL (picker)
parent: foreman
grand_parent: Command-line tools
nav_order: 2
---

`foreman repl [SERVICES]...` (alias `pick`) opens an interactive picker: tick services by number or name, then start or stop the selection. Not run for the book (it can start processes); documented from `repl.rs`.

```text
┌─────────────────────────────────────────────┐
│ example picker                              │
│ pick services, then `up`                    │
└─────────────────────────────────────────────┘
picker       `help` for the rest, `quit` to leave

  [ ] 1) api          stopped
  [ ] 2) web          stopped

foreman>
```

*Unverified:* layout from the source.

* **Prompt**: `foreman> ` with an empty selection, `foreman (api,web)> ` with one.
* Line editing and in-memory history from the `quench-cli` editor (arrows, Ctrl-A/E/B/F/K/U/W/L/P/N/T); no completion. Ctrl-C and Ctrl-D leave the picker.
* A failing command prints `error  <message>` and keeps the session.
* After leaving: `picker  left the picker; foreman status shows what is up`. Services started from the picker keep running.

## Commands

| Input | Does |
|---|---|
| `help`, `h`, `?` | Prints the list below. |
| `<n>` or `<name>` (several allowed: `1 2`, `api web`) | Toggles each. A number is the 1-based index in the printed list; unknown: `no service N` / `unknown service 'x'`. The selection is kept in config order. |
| `list`, `ls`, `l` | Prints the list again (`[x]` selected, `running on <port>` / `stopped`). |
| `all` | Select every service. |
| `none`, `clear` | Clear the selection. |
| `running` | Select what is up right now. |
| `up`, `start` | `start` the selection (with dependencies; they join the selection). Empty: `nothing selected - all for the whole estate`. |
| `down`, `stop` | Stop the selected services (no dependency expansion) and report strays. Empty: `nothing selected`. |
| `status` | As `foreman status`. |
| `logs <name>` | `tail -f` the log; Ctrl-C returns to the picker. |
| `db`, `reset` | As the commands. |
| `run <task>` | Run a task with the current selection. |
| `quit`, `exit`, `q` | Leave. |
