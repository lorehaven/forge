# Quench Starter

Server bootstrap crate shared by Forge's Actix-based HTTP services — wraps TLS/plain HTTP setup, base-path scoping, health/readiness state, request logging and correlation IDs, database bootstrap via `quench-db`, and a small UI/routing layer built on `quench-web` behind one `serve()` entry point.

See [docs/libs/quench-starter.md](../../docs/libs/quench-starter.md) for full documentation.
