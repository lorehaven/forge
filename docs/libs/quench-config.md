# Quench Config

`quench-config` (crate `quench_config`) is Forge's small shared configuration-loading helper: reading JSON/YAML/TOML files and typed environment variables through one API instead of every service hand-rolling `std::env::var` + `.parse()`. This page is the full reference (based on the single source file `libs/quench-config/src/lib.rs`); the crate's own README is a short pointer here. It's used by gatehouse-service (loading `permissions.toml` and its clients file via `ConfigLoader::from_toml_file`), sage-service (`config/settings.rs`, environment-var reads via `ConfigLoader`), and warehouse-service (`lib.rs` and `routers/files/mod.rs`).

## Public API / Key Types

- **`ConfigLoader`** — the crate's main type. `ConfigLoader::new(env_prefix)` builds one bound to a prefix (e.g. `"WAREHOUSE"`) used by its `env_*` methods.
  - File loaders (associated functions, not tied to a prefix): `from_json_file::<T>(path)`, `from_yaml_file::<T>(path)`, `from_toml_file::<T>(path)` — each reads the file and deserializes it via `serde_json`/`serde_yaml`/`toml` respectively into any `T: DeserializeOwned`.
  - `load_with_fallback::<T>(file_path, env_var, default)` — tries a file (dispatching on its extension among `.json`/`.yaml`/`.yml`/`.toml`) if `file_path` is given and exists, then a JSON-encoded environment variable, then falls back to `default`, erring with `ConfigError::MissingValue` if none of those produce a value.
  - Prefixed environment readers (instance methods): `env_string(key, default)` (checks `{PREFIX}_{key}` first, then bare `key`, then `default`), `env_u64(key, default)`, `env_bool(key, default)` (accepts `"true"`/`"1"`/`"yes"`/`"on"`, case-insensitive), `env_list(key, default: &[&str])` (comma-separated, trimmed, empty entries dropped).
- **`AppConfig`** — a generic `service_name`/`port`/`database_url`/`log_level`/`debug` struct with `AppConfig::from_env()` (reads `APP_SERVICE_NAME`, `APP_PORT`, etc. via a `ConfigLoader::new("APP")`). Present in the crate but not currently instantiated by any downstream service found in the workspace — consumers use `ConfigLoader` directly for their own config structs instead.
- **`ConfigError`** / **`Result<T>`** — `ReadError`, `ParseError`, `MissingValue`, `InvalidValue` variants (`thiserror`-derived).

## Configuration

The crate itself reads no fixed environment variables; `env_prefix` and every key name are supplied by the caller. A typical consumer prefixes its own service name, e.g. warehouse-service's `ConfigLoader::new("WAREHOUSE")` makes `env_string("MAX_UPLOAD_MB", "100")` check `WAREHOUSE_MAX_UPLOAD_MB` before the bare `MAX_UPLOAD_MB`.

## Testing

`libs/quench-config/tests/unit/lib_tests.rs` covers the loader's file-parsing and environment-variable behavior.

## Usage example

```rust
use quench_config::ConfigLoader;

let loader = ConfigLoader::new("WAREHOUSE");
let max_upload_mb = loader.env_u64("MAX_UPLOAD_MB", 100);
let debug = loader.env_bool("DEBUG", false);

let permissions: PermissionsFile = ConfigLoader::from_toml_file("config/permissions.toml")?;
```

[Home](../README.md)
