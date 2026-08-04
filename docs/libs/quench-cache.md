# Quench Cache

`quench-cache` (crate `quench_cache`) is Forge's shared caching layer: in-process by default, Redis (standalone or cluster) behind the `redis` feature, exposed through one API (`CacheStore`) so switching backend is a configuration change, not a rewrite. This page is the full reference; the crate's own README is a short pointer here. `quench-auth` depends on it (with the `redis` feature enabled) for `SessionDb`, and gatehouse-service and sage-service depend on it directly — gatehouse for its token/JWKS caching (`tokens.rs`), sage for caching web-fetch tool results (`tools/web_fetch.rs`).

## Public API / Key Types

Gated by Cargo features (`default = ["request-cache", "data-cache"]`):

- **`store::CacheStore`** (feature `data-cache`) — the main entry point. An enum over `InMemory` (wraps `DataCache`) and, with the `redis` feature, `Redis` (wraps `RedisStore`). Construct with `CacheStore::in_memory()` or `CacheStore::from_env(key_prefix)`, which reads `CACHE_URL`/`REDIS_URL` and falls back to in-process when neither is set. Methods: `get`, `set` (with optional TTL), `take` (atomic get-and-delete), `remove`, `add_to_set`/`set_members`/`remove_from_set` (Redis-set semantics for values written concurrently, e.g. session ids per user), `clear`, `is_shared()`, `topology()`.
- **`store::RedisStore`** (feature `redis`) — the Redis-backed implementation `CacheStore::Redis` wraps. Supports both a single server (via `redis::aio::ConnectionManager`, which reconnects automatically) and a cluster (comma-separated seed URLs, or `CACHE_CLUSTER=true`/`REDIS_CLUSTER=true` to force cluster mode for a single seed). `clear()` walks the keyspace with `SCAN` (not `KEYS`, so it doesn't block the server), and on a cluster sweeps every primary in turn.
- **`data_cache::DataCache`** (feature `data-cache`) — the in-process backend, a `DashMap<String, CacheEntry>` with per-entry TTL. Methods: `set`, `set_with_ttl`, `get`, `has`, `delete`, `add_to_set`/`set_members`/`remove_from_set`, `clear`, `stats()` (returns `CacheStats`), `cleanup_expired()`, `len`/`is_empty`.
- **`request_cache::RequestCache`** (feature `request-cache`) — a separate, simpler cache keyed for caching request/response pairs (e.g. LLM calls): `CachedResponse { content, model, timestamp, tokens_used }`, a single cache-wide TTL (`new()` defaults to 24h, or `with_ttl(secs)`), `generate_key(parts)` to hash inputs into a key, `get`/`set`/`has`/`clear`/`stats`/`cleanup_expired`.
- **`CacheError`** / **`Result<T>`** — `Miss`, `SerializationError`, `Backend` variants (`thiserror`-derived).

All three are re-exported from `quench_cache::prelude`.

## Features

- `data-cache` (default) — `DataCache`, `CacheStore`, `store` module.
- `request-cache` (default) — `RequestCache`.
- `redis` — adds the `Redis` variant to `CacheStore` and the `RedisStore` type; implies `data-cache`. Without this feature, pointing `CACHE_URL`/`REDIS_URL` at a cache is a hard error from `CacheStore::from_env` rather than a silent downgrade to in-process.

## Configuration

- `CACHE_URL` or `REDIS_URL` — comma-separated for cluster seed nodes; unset means in-process.
- `CACHE_CLUSTER` or `REDIS_CLUSTER` (`true`/`false`) — forces cluster mode even for a single seed.

## Testing

`libs/quench-cache/tests/` has `unit.rs` (covering `data_cache` and `request_cache` behavior, and `store` unit-level logic) and a separate `redis_store.rs` integration test that exercises `RedisStore` against a real Redis instance.

## Usage example

```rust
use quench_cache::CacheStore;
use serde_json::json;

let store = CacheStore::from_env("my-service").await?;
store.set("greeting", json!({ "text": "hello" }), Some(60)).await?;
if let Some(value) = store.get("greeting").await? {
    println!("{value}");
}
```

[Home](../README.md)
