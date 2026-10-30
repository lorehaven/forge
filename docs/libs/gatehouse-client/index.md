---
title: gatehouse-client
parent: Libraries
has_children: true
permalink: /libs/gatehouse-client/
---

`gatehouse-client` (library `gatehouse_client`) is the blocking client session for apps that sign a person in against Gatehouse (the Forklift-style native apps): sign in with a username and password, keep the rotating refresh token safe, hand out an access token with life left, and make bearer requests that renew the token and retry once when the server refuses it. All calls are plain blocking calls; run them on a worker thread.

## Facts

| | |
|---|---|
| Version | 0.1.0 (`publish = false`) |
| Edition | 2024 (workspace) |
| License | MIT |
| Test harness | `harness = false`, `quench-test`; tests use a stand-in TCP server |
| Unsafe | none |
| Used by | `services/forklift/net`, `services/hoist/net`, `services/daybook/net` (feature `client`) |

## Install

Not published. In the workspace: `gatehouse-client = { workspace = true }` (`path = "libs/gatehouse-client"`).

## Features

None.

## Dependencies

| Crate | Why |
|---|---|
| `quench-client` (feature `blocking`) | `blocking::{Client, Response}`, `transport::Error` |
| `quench-serde` | `Deserialize`, `Value`, `json!`, `json::from_str` |
| `quench-test` (dev) | harness |

## Where to go next

* [API reference](api.md)
* [Usage](usage.md): login, resume, shared sessions, tracing.
* [Testing](testing.md)

## Minimal example

```rust
use gatehouse_client::{login, Session};

let mut session: Session = login("https://host", "alice", "secret")?
    .keeping_tokens(|refresh| { /* persist `refresh` */ let _ = refresh; });
let response = session.get("/warehouse/api/v1/files", &[], false)?;
println!("{}", response.status());
# Ok::<(), gatehouse_client::Error>(())
```

Note the base: `login` appends `/gatehouse/api/v1/auth/login` to the base you pass (the `/gatehouse` base path is fixed in the crate), and `Session::get` appends the path you give to the same base, so a request to another service names its base path in `path`.
