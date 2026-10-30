# Lectern

Serves the documentation books held in a Warehouse files storage to anyone: no login, no redirect to Gatehouse.
The service signs in to Warehouse itself (the `lectern-warehouse` OAuth client), keeps the newest release of
every book in memory and answers `GET`s.

```text
WAREHOUSE_URL=https://localhost:6443/warehouse \
WAREHOUSE_CLIENT_SECRET=... GATEHOUSE_URL=https://localhost:5443/gatehouse \
BASE_PATH=/lectern lectern-service
```

Books are released with `anvil docs release`. See [the Lectern pages](../../../docs/services/lectern/index.md) of the
forge book for the routes, the environment and how a book's links are made to work under its own path.
