---
title: gatehouse-auth
parent: Libraries
has_children: true
permalink: /libs/gatehouse-auth/
---

`gatehouse-auth` (library `gatehouse_auth`) gives command-line tools their credentials for Gatehouse. A tool keeps a long-lived, revocable **application token** (`app_token`) and the Gatehouse address in a git-ignored secret file; on every run it exchanges the token for a short-lived access token (`POST /api/v1/token`, `grant_type=app_token`), so nothing short-lived is stored and nothing has to be copied by hand. The same crate performs the interactive `login` that mints the application token.

## Facts

| | |
|---|---|
| Version | 0.1.7 |
| Edition | 2024 (workspace) |
| License | MIT |
| Lints | workspace lints |
| Test harness | `harness = false`, `quench-test` (`tests/unit.rs` includes `tests/unit/lib_tests.rs`) |
| Unsafe | none |
| Used by | `cli/anvil`, `cli/riveter` |

## Install

```toml
[dependencies]
gatehouse-auth = { version = "0.1.7", registry = "ennor" }
```

In the workspace: `gatehouse-auth = { workspace = true }` (version `0.1.0` constraint, registry `ennor`).

## Features

None.

## Dependencies

| Crate | Why |
|---|---|
| `quench-serde` | TOML (de)serialisation and `Value` |
| `quench-core` | `Result`, `Context`, `bail!` |
| `quench-client` (feature `blocking`) | blocking HTTP client for the token exchange and login |
| `quench-cli` | `term::prompt_password` |
| `quench-test` (dev) | harness |

## Where to go next

* [API reference](api.md)
* [Secret file](secret-file.md): where the file lives, its format and how it is written.
* [Usage](usage.md)
* The server side: `POST /api/v1/token`, `POST /api/v1/auth/login`, `POST /api/v1/me/tokens` in the Gatehouse API ([api](../../services/gatehouse/api.md)).
* [Testing](testing.md)

## Minimal example

```rust
use gatehouse_auth::{access_token, bearer, load};
use std::path::Path;

fn token_header() -> quench_core::Result<Option<String>> {
    let Some(credentials) = load("anvil", Path::new("."))? else { return Ok(None) };
    let token = access_token(&credentials)?;     // POST {gatehouse_url}/api/v1/token
    Ok(Some(bearer(&token)))                      // "Bearer <token>"
}
```
