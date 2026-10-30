---
title: Configuration
parent: conveyor-cli
grand_parent: Command-line tools
nav_order: 2
---

Settings are resolved per run, lowest to highest priority: config file, environment variable, command-line flag.

## Config file

Optional `$XDG_CONFIG_HOME/conveyor/config.toml`, else `$HOME/.config/conveyor/config.toml` (empty values of those variables are ignored). A missing file is fine; a file that cannot be read or parsed is an error (`failed to parse <path> as TOML`). `validate` does not read it.

```toml
url = "https://localhost:9443/conveyor"
username = "admin"
password = "..."                  # plain text: keep the file private (chmod 600)
gatehouse_url = "https://localhost:5443/gatehouse"
insecure = true
```

| Key | Type | Default | Flag / env |
|---|---|---|---|
| `url` | string | none | `--url` / `CONVEYOR_URL` |
| `username` | string | none | `--username` / `CONVEYOR_USERNAME` |
| `password` | string | none | `--password` / `CONVEYOR_PASSWORD` |
| `gatehouse_url` | string | none | `--gatehouse-url` / `GATEHOUSE_URL` |
| `insecure` | bool | `false` | `--insecure` / `CONVEYOR_INSECURE` |

Unknown keys are ignored (no `deny_unknown_fields`). Environment values that are empty or whitespace are treated as unset.

## Authentication flow

If a username is resolved, the CLI needs `gatehouse_url` (`CONVEYOR_USERNAME is set but gatehouse's address is not: pass --gatehouse-url or set GATEHOUSE_URL ...`), posts `{"username","password"}` to `{gatehouse}/api/v1/auth/login` once, and uses the returned `access_token` as a bearer token for every Conveyor call. A rejected login: `gatehouse rejected <user>'s credentials (set CONVEYOR_USERNAME and CONVEYOR_PASSWORD)`. Without a username no token is sent and Conveyor decides whether that is allowed.

`conveyor-cli` does not use the `gatehouse-auth` application-token secret file (`.<tool>/secret.toml`) that `anvil` and `riveter` use.

## Environment variables

`CONVEYOR_URL`, `CONVEYOR_USERNAME`, `CONVEYOR_PASSWORD`, `GATEHOUSE_URL`, `CONVEYOR_INSECURE` (as in the table above), `XDG_CONFIG_HOME`, `HOME` (config location). A `.env` file in the working directory is loaded first.
