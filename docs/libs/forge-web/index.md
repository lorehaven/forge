---
title: forge-web
parent: Libraries
has_children: true
permalink: /libs/forge-web/
---

`forge-web` (library `forge_web`) serves a service's browser client: the HTML page that starts a `quench-ui` wasm module, and the five files that page loads (loader script, stylesheet, wasm module, shell script, icon). A service declares four to six small routes and hands each to a `Client`. The page carries a strict content security policy with a per-response nonce.

## Facts

| | |
|---|---|
| Version | 0.1.0 (`publish = false`) |
| Edition | 2024 (workspace) |
| License | MIT |
| Lints | workspace lints |
| Test harness | `harness = false`, `quench-test` (`tests/unit.rs`) |
| Unsafe | none |

## Install

Not published. In the workspace: `forge-web = { workspace = true }`. Used by the `service` crates of gatehouse, warehouse, gantry, workbench, sage, daybook (and others) in their `ui/pages/client.rs`.

## Features

None.

## Dependencies

| Crate | Why |
|---|---|
| `forge-shell` | `STYLE` and `SCRIPT` for a framed client |
| `quench-core` | `env::get_or` for `WEB_DIR` and `GATEHOUSE_URL` |
| `quench-fullstack` | `Page`, `render_document`, `new_nonce`, `content_security_policy` |
| `quench-http` | `Response`, `StatusCode` |
| `quench-starter` | `common::routes::with_base_path` |
| `quench-ui` | `View`, `html::escape_attr` |
| `quench-ui-web` | `QUENCH_JS` loader script |
| `quench-test` (dev) | harness |

## Where to go next

* [API reference](api.md)
* [Usage](usage.md): wiring the routes, building the wasm, environment.
* [Testing](testing.md)
* The consumers: for example the gatehouse `client` page, see the `gatehouse` service documentation.

## Minimal example

```rust
static CLIENT: forge_web::Client = forge_web::Client::new("Gauge", gauge_web::stylesheet);

#[get("/ui/home")]
async fn home(_who: ActorOrRedirect) -> Response { CLIENT.page() }   // the service's own session check stays
#[get("/ui/assets/quench.js")]
async fn script() -> Response { CLIENT.script() }
#[get("/ui/assets/app.css")]
async fn style() -> Response { CLIENT.style() }
#[get("/ui/assets/app.wasm")]
async fn module() -> Response { CLIENT.module() }
```
