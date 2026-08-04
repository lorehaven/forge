# Quench Starter

`quench-starter` (crate `quench_starter`) is the server bootstrap crate shared by Forge's Actix-based HTTP services. It wraps the boilerplate every service repeats — TLS/plain HTTP setup with an HTTPS redirect server, base-path scoping, health/readiness state, request logging and correlation IDs, database bootstrap via `quench-db`, and a small UI/routing layer built on `quench-web` — behind one `serve()` entry point. Its own README is a short pointer to this page. It is a direct dependency of `conveyor-service`, `switchboard-service`, `sage-service`, `warehouse-service` and `gatehouse-service` (all under `docker/`).

## Public API / Key Types

Everything below is reachable through `quench_starter::prelude::*` unless noted.

- `actix::serve(root_module, scoped_module, db, init)` — the main entry point. Installs the rustls crypto provider, resolves `BASE_PATH`, builds a `DbWrapper` (unless one is passed in), starts a `HealthState`, wires `FilteredLogger` middleware and the health/swagger/UI routes, then binds either HTTPS + an HTTP→HTTPS redirect server (when `SERVER_CERT_PATH`/`SERVER_KEY_PATH` resolve) or plain HTTP.
- `ScopedModule` trait — implemented by a service to register its own scope of routes via `register(&self, scope: Scope) -> Scope<...>`.
- `DbWrapper` (`actix::domain::db`) — `DbWrapper::init_env()` / `DbWrapper::init(url)` connect through `quench_db::Db::connect`. An empty URL only succeeds when `ALLOW_IN_MEMORY_DB=true`; otherwise the process panics rather than starting against a database it isn't configured for.
- `HealthState` (`routers::health`) — `live()`, `is_live()`, `is_ready()`, `mark_ready()`; backs the `/health`, `/health/live`, `/health/ready` routes mounted by `routers::health::scope()`.
- `routers::metrics::scope()` — a second, simpler `/metrics`, `/health`, `/health/ready`, `/health/live` set (the metrics text is currently a placeholder gauge, not real Prometheus output).
- `routers::swagger` — redirects `/swagger-ui` and `/swagger-ui/` to `/swagger-ui/index.html` under the base path.
- `routers::ui` — `ui_path`, `ui_asset_path`, `ui_login_redirect` / `ui_login_redirect_for` (htmx-aware, sends `HX-Redirect` for `HX-Request` calls), `serve_assets`, and a re-export of `quench_auth::actix::routers::ui::is_ui_authenticated`. `routers::ui::pages::home::handle_home` and `service_card` help build an authenticated home page.
- `middleware::CorrelationIdMiddleware` — reads or generates `X-Correlation-ID`, stores it on the request and echoes it on the response; `get_correlation_id`, `inject_correlation_id_header` are the matching helpers. `middleware::logger::FilteredLogger` logs method/path/status, skipping paths under `LOG_SKIP_PREFIXES` unless the response was an error.
- `common::routes` — `normalize_base_path(raw)`, `with_base_path(path)`.
- `common::wait` — `gatehouse_health_url()`, `wait_for_services(service_name, urls)` (polls each URL until it answers 2xx).
- `logging::init()` — loads `.env` (root, then `docker/<CARGO_BIN_NAME>/.env`), initializes `tracing_subscriber` from `RUST_LOG` (default `info`).
- `metrics::RequestMetrics` / `MetricsSnapshot` / `TimedBlock` — counters for inter-service HTTP calls, with `MetricsSnapshot::to_prometheus(name)` rendering real Prometheus text.
- `resilience::RetryConfig` / `retry_with_backoff` — exponential backoff retry helper; `CircuitBreaker` — closed/open/half-open breaker; `format_error_message` — turns a raw error string into a user-facing message based on keyword sniffing (timeout, connection, rate limit, unauthorized, not found).

Note: `src/tracing.rs` (OpenTelemetry/Jaeger setup, gated behind a `jaeger` feature that isn't declared in `Cargo.toml`) exists in the source tree but is **not** declared as a module in `lib.rs`, so it is not compiled or reachable — treat it as dead code rather than part of the API.

## Configuration

Read via `envmnt`/`std::env`, all optional unless stated:

- `BASE_PATH` — path prefix every route is scoped under (default `/`).
- `SERVER_ADDR` (default `0.0.0.0:443`), `SERVER_HTTP_REDIRECT_ADDR` (default `0.0.0.0:80`).
- `SERVER_CERT_PATH` / `SERVER_KEY_PATH` — PEM cert/key; when both load successfully, `serve` runs HTTPS plus an HTTP redirect server, otherwise it falls back to plain HTTP.
- `SERVICE_NAME` — used in startup log lines and, via `common::wait`, service identification.
- `DATABASE_URL` / `POSTGRES_URL` — passed to `quench_db::Db::connect`; `ALLOW_IN_MEMORY_DB` must be `true` to allow an empty URL.
- `GATEHOUSE_URL` — base URL `gatehouse_health_url()` appends `/health/ready` to.
- `LOG_SKIP_PREFIXES` — comma-separated path prefixes `FilteredLogger` won't log on success.
- `RUST_LOG` — verbosity passed to `tracing_subscriber` by `logging::init()`.

## Testing

`libs/quench-starter/tests/unit.rs` wires unit tests under `tests/unit/`: `actix_routers_health_tests.rs`, `actix_routers_ui_tests.rs`, `metrics_tests.rs`, `resilience_tests.rs`.

## Usage example

```rust
use quench_starter::prelude::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();

    serve(
        || actix_web::web::scope(""),      // root_module
        || actix_web::web::scope(""),      // scoped_module
        None,                               // db: build from DATABASE_URL
        async {},                           // init: readiness work
    )
    .await
}
```

[Home](../README.md)
