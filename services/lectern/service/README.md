# Lectern
Public preview of the documentation books held in a Warehouse files storage.

## What it is
`lectern-service` serves the documentation books that `anvil docs release` puts in a Warehouse files storage, to anyone: no login, no redirect to Gatehouse. It signs in to Warehouse itself (the `lectern-warehouse` OAuth client), keeps the newest release of every book in memory and answers `GET`s.

## Use cases
- Publish the estate's documentation without an account.
- Serve several books, each under its own path.

## Example
```sh
WAREHOUSE_URL=https://localhost:6443/warehouse \
WAREHOUSE_CLIENT_SECRET=... GATEHOUSE_URL=https://localhost:5443/gatehouse \
BASE_PATH=/lectern lectern-service
```

Books are released with `anvil docs release`; the Lectern pages of the forge book cover the routes, the environment and how a book's links are made to work under its own path.

## Documentation
Full documentation: [services/lectern](../../../docs/services/lectern/index.md)
