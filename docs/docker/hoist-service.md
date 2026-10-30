# Hoist Service

The public catalogue behind hoist's Public tab: the apps of **F-Droid**, **IzzyOnDroid** and **Aptoide** merged into one row per package, kept in Postgres, refreshed every hour and served to signed-in users. It replaces the standalone Entmoot service (same merge rules, same data; the API moved to `/hoist/v1` and the store from SQLite to Postgres).

It lives in the hoist project, `services/hoist/service` (crate `hoist-service`), which is a member of forge's cargo workspace like every service, beside the app's crates (the project also builds the app for Android and the desktop; `anvil release` sees the service as a docker package, and the windowed desktop host, the Android surface and the browser server are left out of the images). The schema belongs to foundry's `hoist` module (`services/foundry/service/migrations/hoist`).

## API

Everything is under the base path (`/hoist`) and needs a valid realm login (any identity; nothing is per user). Health probes are outside `/v1` and stay open.

| Route | Purpose |
| --- | --- |
| `GET /v1/apps` | browse or search, a page at a time: `query` (label or package, any case), `category`, `min_votes` (fewest ratings an app must have), `sort` = `downloads` (default), `rating` or `alpha`, `offset`, `limit` (1-100, default 30). Answers `{total, offset, limit, items}`; what has no figures sorts last |
| `GET /v1/categories` | each category with its app count, the biggest first: `{items: [{name, count}]}` |
| `GET /v1/apps/{package}` | one app, or `404` |

An item carries `package_name`, `label`, `summary`, `description`, `icon_url`, `category`, `version_code`, `version_name`, `size_bytes`, `download_url`, `sha256` (F-Droid and IzzyOnDroid builds; an Aptoide build may have none), `md5`, `install_source` (`fdroid`, `izzyondroid` or `aptoide`), `rating_avg`, `rating_count`, `downloads` and `malware_rank`. The file itself is **not** served from here: `download_url` is the store's own.

## How it works

- `connectors/` fetch: F-Droid and IzzyOnDroid share the index-v2 format (`<repo>/index-v2.json`, parsed on a blocking thread because it is tens of megabytes); Aptoide is browsed `HOIST_APTOIDE_PAGES` pages of its most downloaded apps and each surviving package is resolved with one `app/getMeta` call (bounded concurrency).
- `merge.rs` decides: within Aptoide one listing per package (trusted by its malware scan first, then downloads, then ratings), across sources one row per package by field (F-Droid, then IzzyOnDroid, then Aptoide wins the install slot; Aptoide's ratings and download counts still show on an F-Droid row).
- `store.rs` keeps it: the whole table is rewritten in one transaction, so readers never see a half-merged catalogue. A source that fails counts as empty; when **every** source fails nothing is written and last hour's data stays.
- `refresh.rs` runs the first refresh at start (readiness does not wait for it) and then every `HOIST_REFRESH_MINUTES`.

## Configuration

| Variable | Default | |
| --- | --- | --- |
| `BASE_PATH` | `/` | `/hoist` when deployed |
| `DATABASE_URL`, `DB_SCHEMA` | | `DB_SCHEMA` defaults to `hoist` |
| `GATEHOUSE_URL`, `REDIS_URL`, `AUTH_DB_SCHEMA` | | the realm's, as every service |
| `HOIST_FDROID_URL` | `https://f-droid.org/repo` | |
| `HOIST_IZZYONDROID_URL` | `https://apt.izzysoft.de/fdroid/repo` | |
| `HOIST_APTOIDE_URL` | `https://ws75.aptoide.com/api/7/` | |
| `HOIST_REFRESH_MINUTES` | `60` | |
| `HOIST_APTOIDE_PAGES` | `100` | 30 apps a page |

## Running, testing and shipping

- `foreman start hoist` runs it locally on `https://localhost:14443/hoist` (a short Aptoide crawl, `HOIST_APTOIDE_PAGES=5`); `foreman db` installs its schema.
- Tests: `cargo test -p hoist-service`. The store, refresh and API tests need Postgres: set `HOIST_TEST_DATABASE_URL` to a throwaway database with the `hoist` module installed (`foreman db` makes one: `postgres://postgres:postgres@localhost:5432/postgres`); without it they pass without checking anything. `HOIST_LIVE_SOURCES=1` also fetches the real stores once.
- `anvil release` builds and pushes `ossiriand.arda:8080/forge/hoist/hoist:<version>` like the other services (it is a docker package in `.anvil/docker.toml`; its `config/` is empty because the image wants one); the deployment is `homecloud/overlays/forge/hoist`.
