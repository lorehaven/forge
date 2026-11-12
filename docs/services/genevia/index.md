---
title: Genevia
parent: Services
nav_order: 8
---

Genevia keeps family trees. A GEDCOM file is imported into Postgres; a tree is browsed as a list (Ahnentafel table by generation), a pedigree or a radial fan, and people, couples and relatives can be edited. A wedding is one record, so a change made from either spouse shows for both.

## Crates

| Crate | Path | Role |
|---|---|---|
| `genevia-core` | `services/genevia/core` | GEDCOM parser, tree model, ancestor and fan geometry, and the wire types (`wire`). No I/O. |
| `genevia-app` | `services/genevia/app` | Screens (`quench-ui`); every call goes through `api` over a `Host` the page installs. |
| `genevia-web` | `services/genevia/web` | `wasm32` module: `fetch` host, the service prefix from the page's address. |
| `genevia` | `services/genevia/service` | The server: store, API, SSO and page routes. Binary `genevia-service`. |

The foundry catalog module `genevia` (schema `genevia`) owns the tables.

## Facts

* Foreman: HTTPS 17443, redirect 17080, `BASE_PATH=/genevia`; the client is at `/genevia/ui`, a tree at `/genevia/ui/t/<tree>/<list|pedigree|fan>[/<person>]`. Gatehouse client `genevia`, secret `CLIENT_SECRET_GENEVIA`.
* Tests: `GENEVIA_TEST_DATABASE_URL` (module installed with foundry) enables the API tests against Postgres.

## API (`/api/v1`, behind `Auth`)

| Route | Needs |
|---|---|
| `GET /me`, `GET /trees` (only the trees the caller may open) | any grant |
| `POST /trees` (multipart `file`; a file of the same name replaces that tree) | blanket `write`, or `write` on that tree id |
| `GET /trees/{t}/ancestry?root=&generations=`, `/search?q=`, `/people/{x}` | `read` on the tree |
| `GET/PUT/DELETE /trees/{t}/people/{x}[/edit]`, `POST /people/{x}/relatives`, `GET/PUT/DELETE /families/{f}[/edit]` | `write` on the tree |

## Permissions

`genevia:read` and `genevia:write` cover every tree; a grant can name one tree, `genevia:tree:<id>:read|write` (the id is the import's file name, e.g. `walus`). `write` includes read. The client shows edit controls only where the API says the caller may change the tree. Trees hold names and dates of living people, so only the `editor` template carries a grant.

## Known gap

Ctrl + wheel zoom of the pedigree and fan needs a `quench-ui` release with wheel events; the zoom buttons and dragging work.
