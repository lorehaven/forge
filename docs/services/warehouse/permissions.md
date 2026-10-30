---
title: Permissions
parent: Warehouse
grand_parent: Services
nav_order: 6
---

Warehouse's catalog entry in Gatehouse (`config/permissions.toml`): actions `read`, `write`; `resource_types = ["storage"]`. Templates: `viewer` grants `warehouse:read`, `editor` grants `read` and `write`.

## What is actually enforced

The code checks **only the `write` action** (and a wildcard role) for the blanket grant, plus audience membership, plus the per-storage rule for dynamic storages. `warehouse:read` is never tested by a handler: a token has `warehouse` in its audience when its holder has any grant on warehouse (or a wildcard role), and that is all reading needs.

| Area | Read | Write |
|---|---|---|
| Crates | nothing; or, with `CRATES_REQUIRE_AUTH_FOR_READS=true`, any valid realm identity (audience `warehouse`) | `warehouse:write` (publish, yank, unyank, owner changes) |
| Docker | a registry token whose scope allows `pull` for the repository | a registry token whose scope allows `push` |
| Files, static storage | any valid realm identity | `warehouse:write` |
| Files, dynamic storage | owner, wildcard role, or `warehouse:storage:<name>:read` | owner, wildcard role, or `warehouse:storage:<name>:write` |
| Files, administration (`POST/PATCH/DELETE /api/v1/files...`) | | `warehouse:write` |
| Artifacts, rivets | any valid realm identity | `warehouse:write` |
| `/admin/*` | | `warehouse:write` |
| Management API (`/ui/api`) | any session | `warehouse:write` for artifact/rivet yank, storage create/edit/delete, file delete; **no check** for crate yank/unyank and Docker image delete |

`Claims::can("warehouse", "write")` is true for a token whose `scope` lists `warehouse:write` or a wildcard role (`admin`, `service`). `RequireWrite` checks the action against the service name of this process (`SERVICE_NAME`, `warehouse` in all deployments).

The management API's storage list (`GET /ui/api/files/storages`) is not filtered per storage: every session sees the names, owners, quotas and usage of all dynamic storages and the root directories of the static ones.

## Dynamic storage rule (`routers/files/authz.rs`)

`can_on_storage(claims, config, storage, action)`: true when authentication is disabled; false without claims; else true when the token has a wildcard role, or `storage.owner == claims.sub`, or `claims.can("warehouse", "storage:<name>:<action>")` with `action` `read` or `write`. A grant is written `warehouse:storage:<name>:read` in Gatehouse. Note `Claims::can` compares the action string exactly, so there is no implication from `write` to `read`: a writer who should also read needs both entries. The blanket `warehouse:read`/`write` does **not** apply to dynamic storages. Provisioning (`POST/PATCH/DELETE /api/v1/files...`) uses `has_blanket`: `warehouse:write` only.

Static storages: `authorize` returns true for any action other than `write`, and for `write` requires the blanket grant. Reading is still behind `Auth` (a valid token).

## Path rules (files API)

Applies to the `path` query of `GET|HEAD|PUT|DELETE /api/v1/files/{storage}/file` and the `prefix` of listings.

* Storage names: non-empty, ASCII letters, digits, `-`, `_` (`valid_storage_name`).
* `relative(path)`: empty or blank gives `400 path is required`; any byte below `0x20` or `0x7f` gives `403 path contains invalid characters`; components are walked: normal parts kept, `.` skipped, `..` gives `403 path must not contain `..``, a root or drive prefix gives `403 path must be relative to the storage`; a result with no components is `400`. `..` is rejected, never normalised.
* Static storages additionally pass `confined`: the target (or its nearest existing ancestor) is canonicalised, following every symlink, and must stay under the canonical storage root, else `403 path resolves outside the storage` (`prefix resolves outside the storage` for listings).
* Dynamic storages: the same lexical rules; the path is a database key (`storage_files.path`), never a filesystem path.
* Other validators: crate names 1 to 64 of `[A-Za-z0-9_-]`; crate versions 1 to 64 of `[A-Za-z0-9.+-]`; artifact programs reverse-DNS segments `[A-Za-z_][A-Za-z0-9_]*` up to 255 characters; artifact filenames one component up to 255 characters without `/`, `\`, NUL or control characters; rivet names DNS-1123 labels, versions semver; Docker repository names: non-empty, no `\`, all path components normal (no `.`, `..`, root); Docker tags: one normal component; digests exactly `sha256:` plus 64 hex digits.

## Docker authorisation in detail

1. Client calls `/v2/...` without a token: `401` with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`.
2. Client calls `GET /token?service=<SERVICE_NAME>&scope=repository:<repo>:pull,push` with Basic credentials; the realm user must exist and the password match (`UserDb::validate`). The issued token's `scope` is the request narrowed to the user's grants: `pull` needs `warehouse:read` or `warehouse:write`, every other action (`push`, `delete`, `*`) needs `warehouse:write`; entries left with no action are dropped (a user with no warehouse grant gets an empty scope and can reach nothing). With auth off the scope is as requested. No scope at all gives `docker`.
3. `WarehouseAuth` checks, for `/v2/<repo>/(blobs|manifests|tags/list)...`, that some scope entry has type `repository`, repository equal to the path's (or `*`) and an action list containing the needed action (`pull` for GET/HEAD, `push` for POST/PATCH/PUT/DELETE) or `*`. `/v2/`, `/v2/_catalog` and paths with no repository are not scope-checked.
4. Failed authentications are counted per client (first `X-Forwarded-For` entry, else `unknown`): after `MAX_AUTH_FAILURES_PER_MINUTE` (default 30) within `AUTH_FAILURE_WINDOW_SECONDS` (default 60), answer `429` (`DENIED`, `too many authentication attempts`); a success clears the count. In-process only.

Consequence: a realm user can pull or push only if their warehouse grants allow it. `GET /v2/_catalog` is still not scope-checked.

## Cargo clients

Cargo sends the configured token verbatim as `Authorization`; store it as `Bearer <realm access token>` (`cargo login --registry <name>`). With `auth-required` advertised (reads protected) Cargo sends it on every request. A token from Gatehouse's `client_credentials` or `app_token` grant works (audience `warehouse`).
