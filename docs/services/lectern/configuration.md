---
title: Configuration
parent: Lectern
grand_parent: Services
nav_order: 2
---

Lectern reads its environment at start. Only `WAREHOUSE_URL` is required.

| Variable | Default | Meaning |
|---|---|---|
| `WAREHOUSE_URL` | none (required) | Where Warehouse is, for example `https://ennor.ddns.net/warehouse` or the cluster-internal address. A trailing slash is dropped. |
| `GATEHOUSE_URL` | none | Where Gatehouse is. With `WAREHOUSE_CLIENT_SECRET` it makes the service sign in to Warehouse; the token comes from `<GATEHOUSE_URL>/api/v1/token`. |
| `WAREHOUSE_CLIENT_ID` | `lectern-warehouse` | The OAuth client the service signs in as. |
| `WAREHOUSE_CLIENT_SECRET` | none | Its secret. Without it (or without `GATEHOUSE_URL`) requests carry no token, which only a Warehouse with authentication off answers; a warning is logged at start. |
| `LECTERN_STORAGE` | `docs` | The Warehouse files storage the books are in. |
| `LECTERN_REFRESH_SECS` | `60` | How often the storage is looked at. A value that is not a positive number is ignored. |
| `BASE_PATH` | `/` | The path the service is mounted at (`/lectern` behind the estate's ingress). |
| `WAREHOUSE_TLS_VERIFY` | `true` | `false` accepts any certificate, for Warehouse or Gatehouse on a self-signed one (local estates). |
| `SERVER_ADDR`, `SERVER_HTTP_REDIRECT_ADDR`, `SERVER_CERT_PATH`, `SERVER_KEY_PATH` | `0.0.0.0:443`, `0.0.0.0:80`, `cert.pem`, `key.pem` | As for every service: HTTPS when the certificate and key are found, otherwise plain HTTP on `SERVER_ADDR`. See [Common service runtime](../common-runtime.md). |
| `RUST_LOG`, `LOG_DIR`, `LOG_FILE_PREFIX` | | Logging, as for every service. |

## What the client needs

The `lectern-warehouse` client has to exist in Gatehouse (`services/gatehouse/service/config/clients.toml`) with `secret_env = "CLIENT_SECRET_LECTERN_WAREHOUSE"`, and Gatehouse needs that variable in its environment. Its token is for the `warehouse` audience only. The token's rights are those every `client_credentials` client gets (the `service` role), which is what lets it read the storage; Lectern never writes.

The storage itself is a Warehouse static storage: `FILE_STORAGES` on Warehouse lists `docs=<directory>` (in the estate: `artifacts=/artifacts;books=/books;docs=/docs`).
