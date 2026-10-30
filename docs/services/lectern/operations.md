---
title: Operations
parent: Lectern
grand_parent: Services
nav_order: 3
---

## Running

Under foreman: `foreman start lectern` (it starts Gatehouse and Warehouse first). The service listens on `https://localhost:15443/lectern/`. To have something to show, release a book to a Warehouse started with `FILE_STORAGES=docs=<directory>`: `anvil docs release forge`.

## Health

| Route | Answer |
|---|---|
| `GET /health` and `/health/live` | `200 OK` as soon as the process runs. |
| `GET /health/ready` | `200 {"status":"ready"}` once the storage has been listed successfully at least once (whether or not it held a book); `503 {"status":"not_ready"}` before that. |

All three are under `BASE_PATH`, like every other route.

## Log lines

| Line | Meaning |
|---|---|
| `serving forge 2026-10-30` | A release was loaded. |
| `forge is gone from the storage` | The directory is no longer there; the book was dropped. |
| `could not update forge: <reason>` | A release could not be read (missing archive, checksum mismatch, damaged archive, empty archive). The previous release keeps being served. |
| `could not list the books: <reason>` | Warehouse or Gatehouse was unreachable or refused (a `403` means the client lacks the rights to read the storage). Nothing changes; the next look is one interval later. |

## Failure modes

* **Warehouse down at start:** the service starts, answers `/health` and an empty list of books, and loads them when Warehouse is back. Pods should wait on `/health/ready`.
* **A page shows without styles:** the book was built with a `baseurl` that is not the path Lectern serves it at. Release it without a `baseurl`, or with the right one.
* **A new release does not show:** it appears within `LECTERN_REFRESH_SECS` of `latest.json` being written; releases write the archive first and `latest.json` last, so a half-written release is never picked up.
* **Memory:** every book is held unpacked. The forge book is 224 files, about 15 MB.

## Deploying

The cluster overlay is `homecloud/overlays/forge/lectern/` (deployment, service, ingress, config map, secret), listed in `overlay.yaml`, with the values in `values.yaml` (`LECTERN_*`). The pod runs the image `forge/lectern/lectern`, reads `docs` through the cluster address of Warehouse (`LECTERN_WAREHOUSE_URL`) and signs in to it as the `lectern-warehouse` client. Public address: `https://ennor.ddns.net/lectern/`, with no authentication in front of it.

What has to be in place, in this order:

1. The Warehouse has the `docs` storage (`FILE_STORAGES` names `docs=/docs`, with the volume mounted).
2. Gatehouse knows the client: `lectern-warehouse` in its `clients-configmap`, with its secret `CLIENT_SECRET_LECTERN_WAREHOUSE` in Gatehouse's secret, and the catalog entry `[services.lectern]` in its `permissions-configmap` (the card on the dashboard) with `LECTERN_UI_URL`. The same secret value goes into `lectern-secret` as `WAREHOUSE_CLIENT_SECRET`. Both come from `CLIENT_SECRET_LECTERN_WAREHOUSE` in `secrets.yaml`, which therefore has to be given a value (any random string) before the overlay renders.
3. A Gatehouse release that knows `public` services. An older one does not read the `public` key, so the card would only show to people holding a grant on Lectern, which nobody can.
4. The image is in the registry.
5. At least one book is released (`anvil docs release`); until then the front page says no documentation has been published yet.
