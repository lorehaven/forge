---
title: Data
parent: Warehouse
grand_parent: Services
nav_order: 3
---

Foundry module `warehouse` (version 0.5.0 in `module.toml`, `default_schema = "warehouse"`, `requires = ["auth@^0.2", "quench-core@^0.1"]`, variable `auth_schema = "auth"`) from `services/foundry/service/migrations/warehouse`. `${schema}` is `warehouse` unless `DB_SCHEMA` differs; users live in the realm schema `${auth_schema}`.

## Migrations in order

| Id | `since` | Change |
|---|---|---|
| `0001-dynamic-storages` | 0.2.1 | tables `storages`, `blobs`, `storage_files`, `storage_sync_log` and indexes `storages_owner_idx`, `storage_sync_log_storage_id_idx` |
| `0002-apk-versions` | 0.2.1 | table `apk_versions` (later dropped) and index `apk_versions_package_version_idx` |
| `0003-storage-files-prefix-index` | 0.2.1 | index `storage_files_storage_path_c_idx` on `(storage, path COLLATE "C") INCLUDE (size)` for prefix listings |
| `0004-artifact-versions` | 0.3.0 | table `artifact_versions`, index `artifact_versions_program_platform_version_idx`; copies `apk_versions` rows as `android`/`apk` rows; drops `apk_versions` |
| `0005-rivet-packages` | 0.4.0 | table `rivet_packages`, index `rivet_packages_name_idx` |
| `0006-artifact-format-key` | 0.5.0 | drops the unique constraint `artifact_versions_program_platform_version_code_key`, adds unique index `artifact_versions_program_platform_format_version_key` on `(program, platform, format, version_code)`, rewrites ids to `<program>/<platform>/<format>@<version_code>` |

## Tables

### `storages`

`name` TEXT primary key; `owner` TEXT not null references `${auth_schema}.users(username)` on delete cascade; `max_file_bytes` BIGINT null (null = the global default); `quota_bytes` BIGINT not null; `used_bytes` BIGINT not null default 0 (maintained by the domain layer with every insert and delete, never recomputed); `sync_enabled` BOOLEAN not null default false; `created_at` TIMESTAMPTZ not null default `NOW()`. Index `storages_owner_idx (owner)`.

### `blobs`

Content-addressed, shared by all dynamic storages: `sha256` TEXT primary key; `size` BIGINT not null; `ref_count` BIGINT not null default 0 (zero allows the file to be deleted); `created_at` TIMESTAMPTZ not null default `NOW()`.

### `storage_files`

`storage` TEXT not null references `storages(name)` on delete cascade; `path` TEXT not null; `sha256` TEXT not null references `blobs(sha256)` **on delete restrict**; `size` BIGINT not null; `created_at`, `updated_at` TIMESTAMPTZ not null default `NOW()`; primary key `(storage, path)`. Index `storage_files_storage_path_c_idx`.

### `storage_sync_log`

Append-only change feed, written only for storages with `sync_enabled`: `id` BIGSERIAL primary key (the client's cursor); `storage` TEXT not null references `storages(name)` on delete cascade; `path` TEXT not null; `op` TEXT not null; `sha256` TEXT null; `size` BIGINT null; `at` TIMESTAMPTZ not null default `NOW()`. Index `(storage, id)`.

### `artifact_versions`

`id` TEXT primary key (`<program>/<platform>/<format>@<version_code>`); `program` TEXT not null; `platform` TEXT not null check in (`android`, `linux`, `windows`, `macos`); `arch` TEXT null; `format` TEXT not null; `version_code` BIGINT not null; `version_name` TEXT not null; `filename` TEXT not null; `size_bytes` BIGINT not null; `sha256` TEXT not null; `label` TEXT null; `metadata` JSONB not null default `'{}'` (`min_sdk_version`, `target_sdk_version`, `permissions`); `uploaded_by` TEXT not null references `${auth_schema}.users(username)` on delete cascade; `yanked` BOOLEAN not null default false; `created_at` TIMESTAMPTZ not null default `NOW()`. Unique `(program, platform, format, version_code)`; index `(program, platform, version_code DESC)`.

### `rivet_packages`

`id` TEXT primary key (`<name>@<version>`); `name`, `version` TEXT not null; `description`, `namespace` TEXT null; `filename` TEXT not null; `size_bytes` BIGINT not null; `sha256` TEXT not null; `manifest` JSONB not null default `'{}'` (the parsed `rivet.toml`); `uploaded_by` TEXT not null (**no foreign key**); `yanked` BOOLEAN not null default false; `created_at` TIMESTAMPTZ not null default `NOW()`; unique `(name, version)`; index `(name, created_at DESC)`.

## Files

| Root (env) | Layout |
|---|---|
| `CRATES_STORAGE_PATH` | `<name>/<version>/<name>-<version>.crate`, `<name>/owners.json` (pretty JSON list of `{id, login, name?}`), `index/<prefix>/<name>` (one JSON line per version) |
| `STORAGE_PATH` | `blobs/sha256/<hex>`, `manifests/sha256/<hex>`, `<repo>/tags/<tag>`, `<repo>/_uploads/<uuid>` |
| `ARTIFACT_STORAGE_PATH` | `<program>/<platform>/<version_code>/<filename>`; staging `<file>.part.<pid>.<n>` |
| `RIVET_STORAGE_PATH` | `<name>/<version>/<name>-<version>.rivet`; staging `.part.<pid>.<n>` |
| `FILE_STORAGES` entries | the operator's directories, written and read as-is (uploads stage `<target>.part...` and rename) |
| `DYNAMIC_STORAGE_ROOT` | `.blobs/<aa>/<bb>/<sha256>`, `.tmp/<pid>.<n>.part` |

## Cache store

The realm's session keys (see Gatehouse [data](../gatehouse/data.md#cache-store)) and the JWKS cache (prefix `jwks`, entries `kid:{kid}`). The Docker auth-failure counters are in process memory.
