---
title: Genevia
parent: Services
nav_order: 16
---

Genevia keeps family trees. A GEDCOM file is imported into Postgres; a tree is browsed as a list (Ahnentafel table by generation), a pedigree or a radial fan, and people, couples and relatives can be edited. A tree can be renamed and deleted from the list of trees. A wedding is one record, so a change made from either spouse shows for both.

## Crates

| Crate | Path | Role |
|---|---|---|
| `genevia-core` | `services/genevia/core` | GEDCOM parser, tree model, ancestor and fan geometry, and the wire types (`wire`). No I/O. |
| `genevia-app` | `services/genevia/app` | Screens (`quench-ui`); every call goes through `api` over a `Host` the page installs. |
| `genevia-web` | `services/genevia/web` | `wasm32` module: `fetch` host, the service prefix from the page's address. |
| `genevia` | `services/genevia/service` | The server: store, API, SSO and page routes. Binary `genevia-service`. |

The foundry catalog module `genevia` (schema `genevia`) owns the tables.

## Facts

* Foreman: HTTPS 17443, redirect 17080, `BASE_PATH=/genevia`; the client is at `/genevia/ui/home` (`/genevia/ui` redirects there), a tree at `/genevia/ui/t/<tree>/<list|pedigree|fan>[/<person>]`. Gatehouse client `genevia`, secret `CLIENT_SECRET_GENEVIA`.
* Tests: `GENEVIA_TEST_DATABASE_URL` (module installed with foundry) enables the API tests against Postgres.

## API (`/api/v1`, behind `Auth`)

| Route | Needs |
|---|---|
| `GET /me`, `GET /trees` (only the trees the caller may open) | any grant |
| `POST /trees` (multipart `file`; a file of the same name replaces that tree, keeping its name) | blanket `write`, or `write` on that tree id |
| `PUT /trees/{t}` (`{"name": ...}`; the id stays), `DELETE /trees/{t}` | `manage` on the tree |
| `GET /trees/{t}/ancestry?root=&generations=`, `/search?q=`, `/people/{x}` | `read` on the tree |
| `GET/PUT/DELETE /trees/{t}/people/{x}[/edit]`, `POST /people/{x}/relatives`, `GET/PUT/DELETE /families/{f}[/edit]` | `write` on the tree |

## Permissions

`genevia:read`, `genevia:write` and `genevia:manage` cover every tree; a grant can name one tree, `genevia:tree:<id>:read|write|manage` (the id is the import's file name, e.g. `walus`). `write` includes read; `manage` (rename and delete) includes write. Gatehouse's permission editor lists the trees by asking this service (`GENEVIA_API_URL`). Deleting a tree leaves the grants naming it, so importing the same file name again gives their holders the new tree. The client shows edit controls only where the API says the caller may change the tree. Trees hold names and dates of living people, so only the `editor` template carries a grant.

## Known gap

Ctrl + wheel zoom of the pedigree and fan needs a `quench-ui` release with wheel events; the zoom buttons and dragging work.
