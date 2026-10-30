---
title: Configuration
parent: Hoist
grand_parent: Services
nav_order: 2
---

## hoist-service

Plain environment variables, no config file. `BASE_PATH`, `DATABASE_URL`, `DB_SCHEMA` handling of the framework and the auth settings are read by `quench-starter`/`quench-auth`; the `HOIST_*` ones by `hoist_service::config::Settings::from_env`.

| Variable | Type | Default | Meaning | Read in |
|---|---|---|---|---|
| `BASE_PATH` | path | `/` | Prefix of every route (`/hoist` when deployed). Normalised by `normalize_base_path`. | `main.rs` |
| `DATABASE_URL` | URL | none | Postgres (`DbWrapper::init_env`). Without Postgres the API answers `503`. | `main.rs` |
| `DB_SCHEMA` | string | `hoist` | Schema holding `packages`. | `store::schema` |
| `GATEHOUSE_URL`, `REDIS_URL`, `AUTH_DB_SCHEMA` | | | The realm's, as every service (`JwtConfig::init`, `SessionDb::from_env`, `gatehouse_health_url`). | `main.rs` |
| `HOIST_FDROID_URL` | URL | `https://f-droid.org/repo` | F-Droid repository; `<url>/index-v2.json` is read. | `config.rs` |
| `HOIST_IZZYONDROID_URL` | URL | `https://apt.izzysoft.de/fdroid/repo` | IzzyOnDroid repository. | `config.rs` |
| `HOIST_APTOIDE_URL` | URL | `https://ws75.aptoide.com/api/7/` | Aptoide web service base. | `config.rs` |
| `HOIST_REFRESH_MINUTES` | integer > 0 | `60` | Refresh interval. A value that is not a positive integer falls back to 60. | `config.rs` |
| `HOIST_APTOIDE_PAGES` | integer > 0 | `100` | Pages (30 apps each) of Aptoide's most downloaded apps. Invalid falls back to 100. | `config.rs` |

Test-only: `HOIST_TEST_DATABASE_URL` (a throwaway database with the `hoist` module installed) and `HOIST_LIVE_SOURCES=1` (also fetch the real stores once). Without the first, the store, refresh and API tests pass without checking anything.

Feature flags: none. Secrets: none of the service's own (the realm settings are shared).

In `foreman.toml` the service is `hoist`: package `hoist-service`, port `14443`, `base_path = "/hoist"`, `needs = ["gatehouse"]`, env `DB_SCHEMA=hoist`, `SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:14080`, `HOIST_APTOIDE_PAGES=5`.

## The apps

Both hosts keep their state in `$XDG_DATA_HOME/hoist` (else `~/.local/share/hoist`, else the temp dir; on Android `HOME` is the app's `files` directory, so `files/.local/share/hoist`). See [Data](data.md) for the files.

| Setting | Where | Default | Meaning |
|---|---|---|---|
| Server | Settings screen / sign-in | `https://ennor.ddns.net` (`hoist_desktop::DEFAULT_SERVER`) | Address of the estate (Gatehouse at `/gatehouse`, Warehouse at `/warehouse`, catalogue at `/hoist`). Must start with `https://` (validated in the form). |
| Check for updates in the background | Settings screen | on | `auto_check` in the `preferences` file. |
| `HOIST_TRACE` | environment | unset | Switches on request tracing in `gatehouse-client` (`trace_var("HOIST_TRACE")`); on the desktop worker its presence is read too. |

Timings set by the hosts (`Setup`): background look every 6 hours (`check_every_ms = 21_600_000`), toast 4000 ms, loader minimum 500 ms. The desktop shows no Public tab (`public: false`); the Android program does (`public: true`).
