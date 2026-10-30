---
title: Data
parent: Hoist
grand_parent: Services
nav_order: 3
---

## Postgres: schema `hoist`

Owned by Foundry's `hoist` module (`services/foundry/service/migrations/hoist`, module version `0.1.0`, `default_schema = "hoist"`, requires `quench-core@^0.1`). One migration, `0001-packages` (author `hoist-service`, since `0.1.0`), in five changes:

```sql
CREATE TABLE IF NOT EXISTS ${schema}.packages (
    package_name   TEXT PRIMARY KEY,
    label          TEXT NOT NULL,
    summary        TEXT,
    description    TEXT,
    icon_url       TEXT,
    category       TEXT,
    version_code   BIGINT NOT NULL,
    version_name   TEXT NOT NULL,
    size_bytes     BIGINT NOT NULL,
    download_url   TEXT NOT NULL,
    sha256         TEXT,
    md5            TEXT,
    install_source TEXT NOT NULL,
    rating_avg     DOUBLE PRECISION,
    rating_count   BIGINT,
    downloads      BIGINT,
    malware_rank   TEXT,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS packages_label_idx     ON ${schema}.packages (LOWER(label));
CREATE INDEX IF NOT EXISTS packages_category_idx  ON ${schema}.packages (category);
CREATE INDEX IF NOT EXISTS packages_downloads_idx ON ${schema}.packages (downloads DESC NULLS LAST);
CREATE INDEX IF NOT EXISTS packages_rating_idx    ON ${schema}.packages (rating_avg DESC NULLS LAST);
```

The table is rewritten as a whole on every refresh, in one transaction: `DELETE FROM packages`, then `INSERT` in chunks of 200 rows (17 parameters per row). Readers never see a half-merged catalogue. `updated_at` is set by the default and is never read.

No Redis keys of its own (the realm's session store is only read by the auth middleware). No files.

## App files (desktop and Android)

Directory: `$XDG_DATA_HOME/hoist`. All writes are plain `std::fs` writes (no temp-and-rename, unlike Forklift).

| File | Written by | Format |
|---|---|---|
| `session` | `Store::write` | Mode `0600`. Two lines: `<server>\n<refresh token>\n`. |
| `installed` | `Store::record` | One line per program, tab separated: `program \t version_code \t version_name` (tabs and newlines in the name become spaces). The ledger of what this app handed to the system installer. |
| `announced` | `Store::set_announced` | One line: the announcement key, i.e. `program@version_code` joined with `,` after sorting; empty when no updates. |
| `preferences` | `Store::save_preferences` | `auto_check=true` or `auto_check=false`. Absent means on; only the line `auto_check=false` turns it off. |
| `downloads/` | `Session::download*` | Downloaded packages, kept. Registry builds under their `filename` (directories stripped); public ones as `<package>-<version_code>.apk`. |

## Registry row (`hoist_core::Artifact`)

Deserialized from Warehouse's `GET /warehouse/api/v1/artifacts?platform=<os>` (a JSON array, or an object with `items` or `artifacts`).

| Field | Type | Notes |
|---|---|---|
| `program` | string | Application id on Android. |
| `platform` | string | `android`, `linux`, `windows`, `macos`. |
| `arch` | string, optional | `x86_64`, `aarch64`, `universal`; absent for Android. |
| `format` | string | `apk`, `deb`, `rpm`, `pkg.tar.zst`, `tar.gz`, ... |
| `version_code`, `size_bytes` | integer | |
| `version_name`, `filename`, `sha256`, `uploaded_by`, `created_at` | string | `sha256` is bare lowercase hex; `created_at` RFC 3339. |
| `label` | string, optional | |
| `min_sdk_version`, `target_sdk_version` | integer, optional | |
| `permissions` | string array, default empty | |
| `yanked` | boolean, default false | |

## Announcement key and background-check output

`hoist_desktop::check::report` prints one JSON line. `status` is one of `off`, `signed_out`, `none`, `updates`, `error`:

```json
{"status":"updates","notify":true,"count":2,"title":"2 updates available","body":"Mathom 1.2.0, Daybook 0.7.1"}
```

`none` and `updates` carry `notify`, `count`, `title`, `body`; `error` carries `error`. A body names at most 3 titles, then `and N more`.
