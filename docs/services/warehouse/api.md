---
title: API
parent: Warehouse
grand_parent: Services
nav_order: 1
---

Every HTTP route of `warehouse-service`, rendered from [openapi.yaml](openapi.yaml).

## Conventions

* **Two mount points.** `/v2/*` and `/token` (Docker Registry protocol) are at the **server root**; `main.rs`'s `RootOrBasePath` sends any path starting with `/v2/`, equal to `/v2` or equal to `/token` to the Docker router. Everything else is under `BASE_PATH` (deployments use `/warehouse`). A request for `<BASE_PATH>/v2...` or `<BASE_PATH>/token...` is answered `404` (`RegistryRootOnly`).
* **Authentication by area.** Docker: its own bearer token (`GET /token`, Basic auth with realm credentials). Crates, files, artifacts, rivets, `/admin`: `Auth` (realm bearer token or session cookie; the token's audience must include `warehouse`) and, on those prefixes, `RequireWrite`: every non-GET/HEAD/OPTIONS request needs the blanket `warehouse:write` action (a wildcard role passes). Reading therefore needs only a valid identity for this service; there is no separate `read` check anywhere. The files API makes its own decisions per storage (see below). With `SERVICE_AUTH_ENABLED` false the realm checks are bypassed everywhere. `x-permission` marks the action each operation needs.
* **Feature flags.** `FEATURE_FILES_ENABLED`, `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`) and `FEATURE_RIVETS_ENABLED` switch the files, artifact and rivet APIs on (default off; disabled means `404`). `FEATURE_DOCKER_ENABLED` and `FEATURE_CRATES_ENABLED` only change what `/ui/api/overview` reports: the Docker and crates routes are always served.
* **Error bodies.** Docker: `{"errors":[{"code","message","detail":{}}]}`. Crates: `{"errors":[{"detail"}]}`. Everything else: `{"error": "..."}`. A request the framework cannot read (invalid JSON, missing required query field, invalid number in a path) is `400` with a plain-text body.
* **Framework routes** (outside `openapi.yaml`): `GET /health`, `/health/live`, `/health/ready`, `/swagger-ui`, `/swagger-ui/`, `GET <base>` and `<base>/` (302 to `<base>/ui`), `GET /` (302 to `<base>/ui` when a base path is set). They are mounted under `BASE_PATH` like all non-Docker routes.
* **Docker blob routes and `HEAD`.** Four `HEAD` routes (`/v2/`, `/v2/{name}/blobs/{digest}`, `/v2/{name}/manifests/{reference}`, `/api/v1/files/{storage}/file`) are registered by hand because no `#[head]` macro exists. Route patterns for a repository name are `{name:.+}` (`{name:.*}` for starting an upload), written `{name}` in the OpenAPI paths.

The machine-readable description is [openapi.yaml](openapi.yaml) (OpenAPI 3.1.0, version 0.8.2). Each section below is generated from it.

## crates

Cargo registry: publish, download, yank, owners, search (JSON API under `/api/v1/crates`).

### PUT `/api/v1/crates/new`

Publish a crate

Cargo's publish wire format: `u32 LE json length`, the metadata JSON (`name`, `vers`, `deps[]`, `features`, optional `features2`, `links`, `rust_version`), `u32 LE tarball length`, the `.crate` tarball. Streams the tarball to `<CRATES_STORAGE_PATH>/<name>/<version>/<name>-<version>.crate`, computes SHA-256 and appends one JSON line to the sparse index file. The handler itself requires an `Authorization` header (401 without one) besides the middleware.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/octet-stream`, required)

Schema: string (binary)

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Published. | `application/json` [CratePublishResponse](#schema-cratepublishresponse) |
| 400 | Short or truncated payload, bad metadata JSON, interrupted body. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 401 | No Authorization header (or middleware refusal). | `application/json` [CargoErrors](#schema-cargoerrors) |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 409 | Version already published. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 422 | Invalid crate name or version. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 500 | Storage failure. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `200` response:

```json
{
  "warnings": {
    "invalid_categories": [],
    "invalid_badges": [],
    "other": []
  }
}
```

Example `400` response:

```json
{
  "errors": [
    {
      "detail": "payload too short"
    }
  ]
}
```

Example `401` response:

```json
{
  "errors": [
    {
      "detail": "missing authorization token"
    }
  ]
}
```

Example `409` response:

```json
{
  "errors": [
    {
      "detail": "this version has already been published"
    }
  ]
}
```

Example `422` response:

```json
{
  "errors": [
    {
      "detail": "invalid crate name"
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "detail": "failed to write crate chunk"
    }
  ]
}
```

### GET `/api/v1/crates/{name}/{version}/download`

Download a `.crate`

Auth: writes need `warehouse:write` (Auth + RequireWrite); reads need nothing unless `CRATES_REQUIRE_AUTH_FOR_READS=true`, then any valid realm identity.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Crate name: 1 to 64 of ASCII letters, digits, `-`, `_`. |
| `version` | path | string | yes | Version string: 1 to 64 of letters, digits, `.`, `-`, `+`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The tarball (`application/octet-stream`, `Content-Disposition: attachment; filename="name-version.crate"`). | `application/octet-stream` string (binary) |
| 404 | Unknown crate or version, or invalid name. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `404` response:

```json
{
  "errors": [
    {
      "detail": "crate or version not found"
    }
  ]
}
```

### DELETE `/api/v1/crates/{name}/{version}/yank`

Yank a version

Sets `yanked: true` in the index line; the tarball stays.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Crate name: 1 to 64 of ASCII letters, digits, `-`, `_`. |
| `version` | path | string | yes | Version string: 1 to 64 of letters, digits, `.`, `-`, `+`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Yanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | No such crate/version. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 500 | Index write failed. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "detail": "crate or version not found"
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "detail": "failed to write index file: ..."
    }
  ]
}
```

### PUT `/api/v1/crates/{name}/{version}/unyank`

Unyank a version

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Crate name: 1 to 64 of ASCII letters, digits, `-`, `_`. |
| `version` | path | string | yes | Version string: 1 to 64 of letters, digits, `.`, `-`, `+`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Unyanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | No such crate/version. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 500 | Index write failed. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "detail": "crate or version not found"
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "detail": "failed to write index file: ..."
    }
  ]
}
```

### GET `/api/v1/crates/{name}/owners`

List owners

Auth: writes need `warehouse:write` (Auth + RequireWrite); reads need nothing unless `CRATES_REQUIRE_AUTH_FOR_READS=true`, then any valid realm identity. Names are lower-cased.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Crate name: 1 to 64 of ASCII letters, digits, `-`, `_`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Owners from `<name>/owners.json` (empty when none). | `application/json` [CrateOwners](#schema-crateowners) |
| 404 | Crate not published. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `200` response:

```json
{
  "users": [
    {
      "id": 1,
      "login": "alice"
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "detail": "crate not found"
    }
  ]
}
```

### PUT `/api/v1/crates/{name}/owners`

Add owners

Adds logins (blank and case-insensitive duplicates skipped); ids are assigned in sequence. Owners are informational: no route checks them.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Crate name: 1 to 64 of ASCII letters, digits, `-`, `_`. |

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `users` | array of string | yes | Logins; must not be empty. |

```json
{
  "users": [
    "alice"
  ]
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Saved. | `application/json` [Ok](#schema-ok) |
| 400 | Empty `users`. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Crate not published. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 500 | Not saved. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `400` response:

```json
{
  "errors": [
    {
      "detail": "users list must not be empty"
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "detail": "crate not found"
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "detail": "failed to save owners: ..."
    }
  ]
}
```

### DELETE `/api/v1/crates/{name}/owners`

Remove owners

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Crate name: 1 to 64 of ASCII letters, digits, `-`, `_`. |

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `users` | array of string | yes | Logins; must not be empty. |

```json
{
  "users": [
    "alice"
  ]
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Saved. | `application/json` [Ok](#schema-ok) |
| 400 | Empty `users`. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Crate not published. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 500 | Not saved. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `400` response:

```json
{
  "errors": [
    {
      "detail": "users list must not be empty"
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "detail": "crate not found"
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "detail": "failed to save owners: ..."
    }
  ]
}
```

### GET `/api/v1/crates`

Search crates

Auth: writes need `warehouse:write` (Auth + RequireWrite); reads need nothing unless `CRATES_REQUIRE_AUTH_FOR_READS=true`, then any valid realm identity. Substring match on directory names (lower-cased); `description` is always null.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `q` | query | string | yes | Search text; blank gives 400. |
| `per_page` | query | integer | no | Clamped to 1..100. |
| `page` | query | integer | no | From 1. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | A page. | `application/json` [CrateSearchResponse](#schema-cratesearchresponse) |
| 400 | Blank query. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `200` response:

```json
{
  "crates": [
    {
      "name": "serde",
      "max_version": "1.0.0",
      "description": null
    }
  ],
  "meta": {
    "total": 1
  }
}
```

Example `400` response:

```json
{
  "errors": [
    {
      "detail": "search query must not be empty"
    }
  ]
}
```

## crates-index

Cargo sparse index served at `/index`.

### GET `/index/{path}`

Sparse index

Auth: writes need `warehouse:write` (Auth + RequireWrite); reads need nothing unless `CRATES_REQUIRE_AUTH_FOR_READS=true`, then any valid realm identity. `config.json` returns the registry configuration; any other path is `<prefix>/<name>` per the crates.io layout (`1/a`, `2/ab`, `3/a/abc`, `ab/cd/abcd...`); the body is the crate's index file, one JSON line per version. `ETag` is `sha256:<hex>` of the body; a matching `If-None-Match` gets 304.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `path` | path | string | yes | `config.json`, or `<prefix>/<crate name>`; the route pattern is `{path:.*}`. |
| `If-None-Match` | header | string | no |  |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | `config.json` as `application/json`, or the index file as `text/plain; charset=utf-8` with `ETag` and `Cache-Control: no-cache`. | `application/json` [CratesIndexConfig](#schema-cratesindexconfig); `text/plain` string |
| 304 | Not modified. |  |
| 404 | Unknown path, wrong prefix, or no index file (empty body). |  |

Example `200` response:

```json
{
  "dl": "https://ennor.ddns.net/warehouse/api/v1/crates/{crate}/{version}/download",
  "api": "https://ennor.ddns.net/warehouse"
}
```

## docker

Docker Registry HTTP API v2 and its token endpoint. Served at the **server root** (`/v2`, `/token`), outside `BASE_PATH`.

### GET `/v2/`

Registry check

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Reachable. Header `Docker-Distribution-API-Version: registry/2.0`. |  |
| 401 | Authentication required. | `application/json` [DockerErrors](#schema-dockererrors) |
| 429 | Too many failed authentications from this client. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `401` response:

```json
{
  "errors": [
    {
      "code": "UNAUTHORIZED",
      "message": "authentication required",
      "detail": {}
    }
  ]
}
```

Example `429` response:

```json
{
  "errors": [
    {
      "code": "DENIED",
      "message": "too many authentication attempts",
      "detail": {}
    }
  ]
}
```

### HEAD `/v2/`

Registry check (HEAD)

Same as `GET /v2/`.

**Authentication:** `dockerBearer`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Reachable. |  |
| 401 | Authentication required. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `401` response:

```json
{
  "errors": [
    {
      "code": "UNAUTHORIZED",
      "message": "authentication required",
      "detail": {}
    }
  ]
}
```

### GET `/v2/_catalog`

List repositories

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `n` | query | integer | no | Page size, default 100. |
| `last` | query | string | no | Previous page's last repository. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | A page; when full, `Link: </v2/_catalog?n=N&last=X>; rel="next"`. | `application/json` [DockerCatalog](#schema-dockercatalog) |
| 401 | Authentication required. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `200` response:

```json
{
  "repositories": [
    "forge/ci/rust-builder"
  ]
}
```

Example `401` response:

```json
{
  "errors": [
    {
      "code": "UNAUTHORIZED",
      "message": "authentication required",
      "detail": {}
    }
  ]
}
```

### GET `/v2/{name}/tags/list`

List tags

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `n` | query | integer | no | Page size, default 100. |
| `last` | query | string | no | Previous page's last tag. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | A page (`Link` rel=next when full). | `application/json` [DockerTagList](#schema-dockertaglist) |
| 400 | Invalid repository name. | `application/json` [DockerErrors](#schema-dockererrors) |
| 401 | Authentication required. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | No such repository. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `200` response:

```json
{
  "name": "forge/x",
  "tags": [
    "latest"
  ]
}
```

Example `400` response:

```json
{
  "errors": [
    {
      "code": "NAME_UNKNOWN",
      "message": "invalid repository name",
      "detail": {}
    }
  ]
}
```

Example `401` response:

```json
{
  "errors": [
    {
      "code": "UNAUTHORIZED",
      "message": "authentication required",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "NAME_UNKNOWN",
      "message": "repository name not known to registry",
      "detail": {}
    }
  ]
}
```

### POST `/v2/{name}/blobs/uploads/`

Start a blob upload

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked. With `mount` (a digest) and `from` both present and the blob already stored, answers 201 without an upload (blobs are global, not per repository).

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `mount` | query | string | no | Digest to mount. |
| `from` | query | string | no | Source repository (only its presence is checked). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 202 | Upload started. Headers `Location: /v2/<name>/blobs/uploads/<uuid>`, `Docker-Upload-UUID`, `Range: 0-0`. |  |
| 201 | Mounted. Headers `Location`, `Docker-Content-Digest`. |  |
| 400 | Invalid name or digest. | `application/json` [DockerErrors](#schema-dockererrors) |
| 401 | Authentication required. | `application/json` [DockerErrors](#schema-dockererrors) |
| 429 | More than `MAX_CONCURRENT_UPLOADS` upload requests in flight. | `application/json` [DockerErrors](#schema-dockererrors) |
| 500 | Could not create the upload file. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "NAME_UNKNOWN",
      "message": "invalid repository name",
      "detail": {}
    }
  ]
}
```

Example `401` response:

```json
{
  "errors": [
    {
      "code": "UNAUTHORIZED",
      "message": "authentication required",
      "detail": {}
    }
  ]
}
```

Example `429` response:

```json
{
  "errors": [
    {
      "code": "DENIED",
      "message": "too many concurrent upload requests",
      "detail": {}
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "internal server error",
      "detail": {}
    }
  ]
}
```

### GET `/v2/{name}/blobs/uploads/{uuid}`

Upload status

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `uuid` | path | string | yes | Upload id from the start response. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Headers `Location`, `Docker-Upload-UUID`, `Range: 0-<size-1>` (`0-0` when empty), `Content-Length: 0`. |  |
| 400 | Invalid name. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | No such upload. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "NAME_UNKNOWN",
      "message": "invalid repository name",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "BLOB_UNKNOWN",
      "message": "blob upload unknown to registry",
      "detail": {}
    }
  ]
}
```

### PATCH `/v2/{name}/blobs/uploads/{uuid}`

Upload a chunk

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked. Appends the body (streamed) to the upload; the total is capped by `MAX_DOCKER_BLOB_BYTES`.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `uuid` | path | string | yes | Upload id from the start response. |

**Request body** (`application/octet-stream`, required)

Schema: string (binary)

**Responses**

| Status | Description | Body |
|---|---|---|
| 202 | Appended. Headers `Range: 0-<new size-1>`, `Docker-Upload-UUID`, `Location`. |  |
| 400 | Invalid name, interrupted body, or an empty chunk. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | No such upload. | `application/json` [DockerErrors](#schema-dockererrors) |
| 413 | Exceeds the blob ceiling. | `application/json` [DockerErrors](#schema-dockererrors) |
| 429 | Concurrent upload cap. | `application/json` [DockerErrors](#schema-dockererrors) |
| 500 | Write failure. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "empty upload chunk",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "BLOB_UNKNOWN",
      "message": "blob upload unknown to registry",
      "detail": {}
    }
  ]
}
```

Example `413` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "blob exceeds the maximum allowed size",
      "detail": {}
    }
  ]
}
```

Example `429` response:

```json
{
  "errors": [
    {
      "code": "DENIED",
      "message": "too many concurrent upload requests",
      "detail": {}
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "internal server error",
      "detail": {}
    }
  ]
}
```

### PUT `/v2/{name}/blobs/uploads/{uuid}`

Complete a blob upload

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked. Appends the body (may be empty), hashes the whole upload from disk and, if it equals `digest`, moves it to `blobs/sha256/<hex>`. If that blob already exists the upload is discarded and 201 is returned.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `uuid` | path | string | yes | Upload id from the start response. |
| `digest` | query | string | yes | Expected `sha256:<hex>`. |

**Request body** (`application/octet-stream`, optional)

Schema: string (binary)

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Stored. Headers `Location: /v2/<name>/blobs/<digest>`, `Docker-Content-Digest`. |  |
| 400 | Invalid digest/name, interrupted body, or digest mismatch (`BLOB_UNKNOWN`, `digest invalid`). | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | No such upload. | `application/json` [DockerErrors](#schema-dockererrors) |
| 413 | Exceeds the blob ceiling. | `application/json` [DockerErrors](#schema-dockererrors) |
| 429 | Concurrent upload cap. | `application/json` [DockerErrors](#schema-dockererrors) |
| 500 | Write failure. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "BLOB_UNKNOWN",
      "message": "digest invalid",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "BLOB_UNKNOWN",
      "message": "blob upload unknown to registry",
      "detail": {}
    }
  ]
}
```

Example `413` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "blob exceeds the maximum allowed size",
      "detail": {}
    }
  ]
}
```

Example `429` response:

```json
{
  "errors": [
    {
      "code": "DENIED",
      "message": "too many concurrent upload requests",
      "detail": {}
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "internal server error",
      "detail": {}
    }
  ]
}
```

### DELETE `/v2/{name}/blobs/uploads/{uuid}`

Cancel a blob upload

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `uuid` | path | string | yes | Upload id from the start response. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Cancelled. |  |
| 400 | Invalid name. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | No such upload. | `application/json` [DockerErrors](#schema-dockererrors) |
| 500 | Could not delete. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "NAME_UNKNOWN",
      "message": "invalid repository name",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "BLOB_UNKNOWN",
      "message": "blob upload unknown to registry",
      "detail": {}
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "internal server error",
      "detail": {}
    }
  ]
}
```

### GET `/v2/{name}/blobs/{digest}`

Fetch a blob

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked. The repository name is ignored (blobs are global). Supports `Range: bytes=a-b` (206) and, when `ENABLE_REDIRECT=true`, answers 307 to `<BLOB_REDIRECT_BASE>/blobs/sha256/<hex>`.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `digest` | path | string | yes | `sha256:` plus 64 hex digits. |
| `Range` | header | string | no |  |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Streamed. Headers `Content-Type: application/octet-stream`, `Content-Length`, `Accept-Ranges: bytes`, `Docker-Content-Digest`. | `application/octet-stream` string (binary) |
| 206 | Partial content (`Content-Range`). |  |
| 307 | Redirect to the backend (`ENABLE_REDIRECT`). |  |
| 400 | Invalid digest. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | Unknown blob. | `application/json` [DockerErrors](#schema-dockererrors) |
| 416 | Bad range. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "invalid digest",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "BLOB_UNKNOWN",
      "message": "blob unknown to registry",
      "detail": {}
    }
  ]
}
```

Example `416` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "requested range not satisfiable",
      "detail": {}
    }
  ]
}
```

### HEAD `/v2/{name}/blobs/{digest}`

Blob exists

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `digest` | path | string | yes | `sha256:` plus 64 hex digits. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Headers `Docker-Content-Digest`, `Content-Length`, `Accept-Ranges`. |  |
| 400 | Invalid digest. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | Unknown blob. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "invalid digest",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "BLOB_UNKNOWN",
      "message": "blob unknown to registry",
      "detail": {}
    }
  ]
}
```

### PUT `/v2/{name}/manifests/{reference}`

Put a manifest

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked. Accepts `application/vnd.docker.distribution.manifest.v2+json`, `...manifest.list.v2+json`, `application/vnd.oci.image.manifest.v1+json`, `...image.index.v1+json` (415 otherwise; a missing Content-Type is accepted). `schemaVersion` must be 2. Descriptors with a zero or missing `size` get it filled from the stored blob/manifest (400 `manifest references missing content` when absent). The manifest is stored under its SHA-256; a tag reference writes `tags/<tag>`.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `reference` | path | string | yes | A tag (one path component) or a digest. |

**Request body** (`application/vnd.docker.distribution.manifest.v2+json`, required)

Schema: object

**Request body** (`application/vnd.oci.image.manifest.v1+json`, required)

Schema: object

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Stored. Headers `Location`, `Docker-Content-Digest`. |  |
| 400 | Invalid payload/schema/descriptor, name or reference. | `application/json` [DockerErrors](#schema-dockererrors) |
| 415 | Unsupported media type. | `application/json` [DockerErrors](#schema-dockererrors) |
| 500 | Write failure. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "invalid manifest payload",
      "detail": {}
    }
  ]
}
```

Example `415` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "manifest media type unsupported",
      "detail": {}
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "internal server error",
      "detail": {}
    }
  ]
}
```

### GET `/v2/{name}/manifests/{reference}`

Get a manifest

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `reference` | path | string | yes | A tag (one path component) or a digest. |
| `Accept` | header | string | no |  |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The manifest with `Content-Type` of its media type and `Docker-Content-Digest`. | `application/vnd.docker.distribution.manifest.v2+json` object |
| 400 | Invalid name or reference. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | Unknown manifest. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "NAME_UNKNOWN",
      "message": "invalid repository name",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "MANIFEST_UNKNOWN",
      "message": "manifest unknown",
      "detail": {}
    }
  ]
}
```

### HEAD `/v2/{name}/manifests/{reference}`

Manifest exists

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `reference` | path | string | yes | A tag (one path component) or a digest. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Headers `Content-Type`, `Docker-Content-Digest`, `Content-Length`. |  |
| 404 | Unknown manifest. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `404` response:

```json
{
  "errors": [
    {
      "code": "MANIFEST_UNKNOWN",
      "message": "manifest unknown",
      "detail": {}
    }
  ]
}
```

### DELETE `/v2/{name}/manifests/{reference}`

Delete a manifest

Unless `SERVICE_AUTH_ENABLED` is false, a request without a valid token gets 401 with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"`; the token's `scope` must contain `repository:<name>:pull` (GET/HEAD) or `:push` (POST/PATCH/PUT/DELETE) for the repository in the path (`*` as repository or action matches all). `/v2/` and `/v2/_catalog` are not scope-checked. Only by digest; also removes every tag file of the repository that points at it. Blobs stay until GC.

**Authentication:** `dockerBearer`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Repository name; may contain `/`. The route pattern is `{name:.+}` (`{name:.*}` for the upload start). Must not contain `\`, `.`/`..` components or be rooted. |
| `reference` | path | string | yes | A `sha256:` digest. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 202 | Deleted. |  |
| 400 | Invalid name. | `application/json` [DockerErrors](#schema-dockererrors) |
| 404 | Unknown manifest. | `application/json` [DockerErrors](#schema-dockererrors) |
| 405 | Reference is not a digest. | `application/json` [DockerErrors](#schema-dockererrors) |
| 500 | Delete failed. | `application/json` [DockerErrors](#schema-dockererrors) |

Example `400` response:

```json
{
  "errors": [
    {
      "code": "NAME_UNKNOWN",
      "message": "invalid repository name",
      "detail": {}
    }
  ]
}
```

Example `404` response:

```json
{
  "errors": [
    {
      "code": "MANIFEST_UNKNOWN",
      "message": "manifest unknown",
      "detail": {}
    }
  ]
}
```

Example `405` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "manifest deletion requires a digest reference",
      "detail": {}
    }
  ]
}
```

Example `500` response:

```json
{
  "errors": [
    {
      "code": "UNSUPPORTED",
      "message": "internal server error",
      "detail": {}
    }
  ]
}
```

### GET `/token`

Issue a registry token

Basic-auth with realm credentials (or anonymous when `SERVICE_AUTH_ENABLED` is false). `service` must equal `SERVICE_NAME`. The token (10 minutes) carries `scope` exactly as requested (default `docker`): **the requested scope is not checked against the user's grants** (see the report).

**Authentication:** `dockerBasic`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `service` | query | string | yes | Must equal this service's name. |
| `scope` | query | string | no | `repository:<name>:pull,push`, space separated for several. |
| `account` | query | string | no | Ignored. |
| `client_id` | query | string | no | Ignored. |
| `offline_token` | query | boolean | no | Ignored. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Token. | `application/json` [DockerToken](#schema-dockertoken) |
| 400 | `service` mismatch (empty body). |  |
| 401 | Bad or missing credentials; `WWW-Authenticate: Basic realm="registry"`. |  |

Example `200` response:

```json
{
  "token": "eyJ...",
  "expires_in": 600,
  "issued_at": "2026-10-30T08:00:00Z"
}
```

## files

Named file storages: static (operator-configured directories) and dynamic (database-backed, owned, quota-limited).

### GET `/api/v1/files`

List storages

Auth: `Auth` middleware (any valid realm identity with audience `warehouse`) plus the per-storage check below. **Static** storage: read open to that identity, write needs `warehouse:write`. **Dynamic** storage: owner, wildcard role, or `warehouse:storage:<name>:<action>`; the blanket `warehouse:read`/`write` does not apply. Everything answers 404 `file storage is not enabled` unless `FEATURE_FILES_ENABLED`.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Static storages by name; dynamic ones with owner, quota, usage, sync flag, filtered to those the caller may read. | `application/json` array of [StorageSummary](#schema-storagesummary) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Files are not enabled. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
[
  {
    "name": "media"
  },
  {
    "name": "phone",
    "owner": "alice",
    "quota_bytes": 10737418240,
    "used_bytes": 1024,
    "sync_enabled": true
  }
]
```

Example `404` response:

```json
{
  "error": "file storage is not enabled"
}
```

### POST `/api/v1/files`

Provision a dynamic storage

Needs the blanket `warehouse:write` (checked in the handler, not by `RequireWrite`).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Letters, digits, `-`, `_`. |
| `owner` | string | yes | An existing realm user. |
| `max_file_bytes` | integer or null (int64) | no |  |
| `quota_bytes` | integer or null (int64) | no | Default `DEFAULT_STORAGE_QUOTA_BYTES` (10 GiB). |
| `sync_enabled` | boolean | no |  Default `false`. |

```json
{
  "name": "phone",
  "owner": "alice",
  "quota_bytes": 10737418240,
  "sync_enabled": true
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Created. | `application/json` [DynamicStorage](#schema-dynamicstorage) |
| 400 | Invalid name or unknown owner. | `application/json` [Error](#schema-error) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | Lacks write. | `application/json` [Error](#schema-error) |
| 404 | Files are not enabled. | `application/json` [Error](#schema-error) |
| 409 | Name taken by a static or dynamic storage. | `application/json` [Error](#schema-error) |
| 500 | Not created. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "no such user to own this storage"
}
```

Example `403` response:

```json
{
  "error": "write access to warehouse is required to provision a storage"
}
```

Example `404` response:

```json
{
  "error": "file storage is not enabled"
}
```

Example `409` response:

```json
{
  "error": "a storage with that name already exists"
}
```

Example `500` response:

```json
{
  "error": "could not create the storage"
}
```

### GET `/api/v1/files/{storage}`

List a storage

Auth: `Auth` middleware (any valid realm identity with audience `warehouse`) plus the per-storage check below. **Static** storage: read open to that identity, write needs `warehouse:write`. **Dynamic** storage: owner, wildcard role, or `warehouse:storage:<name>:<action>`; the blanket `warehouse:read`/`write` does not apply. Everything answers 404 `file storage is not enabled` unless `FEATURE_FILES_ENABLED`. Static: a shallow directory walk of `prefix`; dynamic: flat prefix match on stored paths. Page size `n` default 500, max 2000; `Link: <...>; rel="next"` when more follow.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |
| `prefix` | query | string | no | Subtree (default root). |
| `n` | query | integer | no | Page size (1..2000, default 500). |
| `last` | query | string | no | Previous page's last item (exclusive); a stale cursor restarts. |
| `desc` | query | boolean | no | Newest/last first. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Entries. | `application/json` [FileListing](#schema-filelisting) |
| 400 | Empty path pieces. | `application/json` [Error](#schema-error) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | No read access, or a prefix outside the storage. | `application/json` [Error](#schema-error) |
| 404 | No such storage or directory. | `application/json` [Error](#schema-error) |
| 500 | Listing failed. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
{
  "storage": "media",
  "prefix": "",
  "entries": [
    {
      "name": "a.txt",
      "path": "a.txt",
      "kind": "file",
      "size": 12
    }
  ]
}
```

Example `400` response:

```json
{
  "error": "path is required"
}
```

Example `403` response:

```json
{
  "error": "read access to this storage is required"
}
```

Example `404` response:

```json
{
  "error": "no such directory"
}
```

Example `500` response:

```json
{
  "error": "listing failed"
}
```

### PATCH `/api/v1/files/{storage}`

Reconfigure a dynamic storage

Needs blanket `warehouse:write`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `max_file_bytes` | integer or null (int64) | no |  |
| `clear_max_file_bytes` | boolean | no | True clears the override (JSON cannot tell absent from null). Default `false`. |
| `quota_bytes` | integer or null (int64) | no |  |
| `sync_enabled` | boolean or null | no |  |

```json
{
  "quota_bytes": 21474836480,
  "sync_enabled": true
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Updated. | `application/json` [DynamicStorage](#schema-dynamicstorage) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | Lacks write. | `application/json` [Error](#schema-error) |
| 404 | No such dynamic storage. | `application/json` [Error](#schema-error) |
| 500 | Not saved. | `application/json` [Error](#schema-error) |

Example `403` response:

```json
{
  "error": "write access to warehouse is required to reconfigure a storage"
}
```

Example `404` response:

```json
{
  "error": "no such dynamic storage"
}
```

Example `500` response:

```json
{
  "error": "could not update the storage"
}
```

### DELETE `/api/v1/files/{storage}`

Delete a dynamic storage

Needs blanket `warehouse:write`. Deletes every file first (releasing blobs), then the storage. Requires `DYNAMIC_STORAGE_ROOT`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Deleted. |  |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | Lacks write. | `application/json` [Error](#schema-error) |
| 404 | No such dynamic storage. | `application/json` [Error](#schema-error) |
| 500 | No `DYNAMIC_STORAGE_ROOT`, or delete failed. | `application/json` [Error](#schema-error) |

Example `403` response:

```json
{
  "error": "write access to warehouse is required to delete a storage"
}
```

Example `404` response:

```json
{
  "error": "no such dynamic storage"
}
```

Example `500` response:

```json
{
  "error": "this deployment has no DYNAMIC_STORAGE_ROOT configured"
}
```

### GET `/api/v1/files/{storage}/sync`

Change feed of a dynamic storage

Auth: `Auth` middleware (any valid realm identity with audience `warehouse`) plus the per-storage check below. **Static** storage: read open to that identity, write needs `warehouse:write`. **Dynamic** storage: owner, wildcard role, or `warehouse:storage:<name>:<action>`; the blanket `warehouse:read`/`write` does not apply. Everything answers 404 `file storage is not enabled` unless `FEATURE_FILES_ENABLED`.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |
| `since` | query | integer (int64) | no | Cursor: entries with `id` greater than this. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Entries after the cursor. | `application/json` array of [SyncLogEntry](#schema-synclogentry) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | No read access. | `application/json` [Error](#schema-error) |
| 404 | No such storage. | `application/json` [Error](#schema-error) |
| 409 | Static storage, or sync not enabled. | `application/json` [Error](#schema-error) |
| 500 | Read failed. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
[
  {
    "id": 7,
    "path": "photos/a.jpg",
    "op": "put",
    "sha256": "ab..",
    "size": 1024,
    "at": "2026-10-30T08:00:00Z"
  }
]
```

Example `403` response:

```json
{
  "error": "read access to this storage is required"
}
```

Example `404` response:

```json
{
  "error": "no file storage named `x`"
}
```

Example `409` response:

```json
{
  "error": "sync is not enabled for this storage"
}
```

Example `500` response:

```json
{
  "error": "could not read the sync log"
}
```

### GET `/api/v1/files/{storage}/file`

Download a file

Auth: `Auth` middleware (any valid realm identity with audience `warehouse`) plus the per-storage check below. **Static** storage: read open to that identity, write needs `warehouse:write`. **Dynamic** storage: owner, wildcard role, or `warehouse:storage:<name>:<action>`; the blanket `warehouse:read`/`write` does not apply. Everything answers 404 `file storage is not enabled` unless `FEATURE_FILES_ENABLED`. Streamed with `Content-Length`; no range support. `disposition=inline` serves it in place with a content type guessed from the extension.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |
| `path` | query | string | yes | Path inside the storage: relative, no `..`, no control bytes; empty gives 400, other violations 403. |
| `disposition` | query | string (`inline`) | no | `inline` for in-place rendering (default: attachment). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The file (`Content-Type`, `Content-Length`, `Content-Disposition`). | `application/octet-stream` string (binary) |
| 400 | Empty path. | `application/json` [Error](#schema-error) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | No read access, or path escapes. | `application/json` [Error](#schema-error) |
| 404 | No such file/storage. | `application/json` [Error](#schema-error) |
| 500 | Lookup failed. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "path is required"
}
```

Example `403` response:

```json
{
  "error": "path resolves outside the storage"
}
```

Example `404` response:

```json
{
  "error": "no such file"
}
```

Example `500` response:

```json
{
  "error": "lookup failed"
}
```

### HEAD `/api/v1/files/{storage}/file`

File exists

Auth: `Auth` middleware (any valid realm identity with audience `warehouse`) plus the per-storage check below. **Static** storage: read open to that identity, write needs `warehouse:write`. **Dynamic** storage: owner, wildcard role, or `warehouse:storage:<name>:<action>`; the blanket `warehouse:read`/`write` does not apply. Everything answers 404 `file storage is not enabled` unless `FEATURE_FILES_ENABLED`. Same checks as GET; the answer has `Content-Type: application/octet-stream` and `Content-Length` and no body.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |
| `path` | query | string | yes | Path inside the storage: relative, no `..`, no control bytes; empty gives 400, other violations 403. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Exists. |  |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | No read access. | `application/json` [Error](#schema-error) |
| 404 | No such file. | `application/json` [Error](#schema-error) |

Example `403` response:

```json
{
  "error": "read access to this storage is required"
}
```

Example `404` response:

```json
{
  "error": "no such file"
}
```

### PUT `/api/v1/files/{storage}/file`

Upload a file

Auth: `Auth` middleware (any valid realm identity with audience `warehouse`) plus the per-storage check below. **Static** storage: read open to that identity, write needs `warehouse:write`. **Dynamic** storage: owner, wildcard role, or `warehouse:storage:<name>:<action>`; the blanket `warehouse:read`/`write` does not apply. Everything answers 404 `file storage is not enabled` unless `FEATURE_FILES_ENABLED`. The raw body is streamed to disk (static: a staging file renamed over the target; dynamic: content-addressed blob plus a row). Limit: `MAX_FILE_BYTES` (static) or the storage's `max_file_bytes`; dynamic uploads also count against `quota_bytes`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write (static) or warehouse:storage:<name>:write (dynamic)`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |
| `path` | query | string | yes | Path inside the storage: relative, no `..`, no control bytes; empty gives 400, other violations 403. |

**Request body** (`application/octet-stream`, required)

Schema: string (binary)

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Created. | `application/json` [Stored](#schema-stored) |
| 200 | Replaced an existing file. | `application/json` [Stored](#schema-stored) |
| 400 | Empty path or interrupted upload. | `application/json` [Error](#schema-error) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | No write access, or path escapes. | `application/json` [Error](#schema-error) |
| 404 | No such storage. | `application/json` [Error](#schema-error) |
| 409 | A directory is at that path. | `application/json` [Error](#schema-error) |
| 413 | Over the size limit. | `application/json` [Error](#schema-error) |
| 507 | Quota exceeded. | `application/json` [Error](#schema-error) |
| 500 | Storage failure. | `application/json` [Error](#schema-error) |

Example `201` response:

```json
{
  "path": "a/b.txt",
  "size": 5,
  "digest": "sha256:2cf24dba..."
}
```

Example `400` response:

```json
{
  "error": "the upload was interrupted"
}
```

Example `403` response:

```json
{
  "error": "write access to this storage is required"
}
```

Example `404` response:

```json
{
  "error": "no file storage named `x`"
}
```

Example `409` response:

```json
{
  "error": "a directory already exists at that path"
}
```

Example `413` response:

```json
{
  "error": "file exceeds the 1073741824-byte limit"
}
```

Example `507` response:

```json
{
  "error": "storage quota exceeded"
}
```

Example `500` response:

```json
{
  "error": "could not store the file"
}
```

### DELETE `/api/v1/files/{storage}/file`

Delete a file

Auth: `Auth` middleware (any valid realm identity with audience `warehouse`) plus the per-storage check below. **Static** storage: read open to that identity, write needs `warehouse:write`. **Dynamic** storage: owner, wildcard role, or `warehouse:storage:<name>:<action>`; the blanket `warehouse:read`/`write` does not apply. Everything answers 404 `file storage is not enabled` unless `FEATURE_FILES_ENABLED`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write (static) or warehouse:storage:<name>:write (dynamic)`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | path | string | yes | Storage name: letters, digits, `-`, `_`. |
| `path` | query | string | yes | Path inside the storage: relative, no `..`, no control bytes; empty gives 400, other violations 403. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Deleted. |  |
| 400 | Empty path. | `application/json` [Error](#schema-error) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | No write access. | `application/json` [Error](#schema-error) |
| 404 | No such file. | `application/json` [Error](#schema-error) |
| 500 | Delete failed. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "path is required"
}
```

Example `403` response:

```json
{
  "error": "write access to this storage is required"
}
```

Example `404` response:

```json
{
  "error": "no such file"
}
```

Example `500` response:

```json
{
  "error": "could not delete the file"
}
```

## artifacts

Multi-platform artifact registry addressed by `{program}/{platform}/{version_code}`.

### PUT `/api/v1/artifacts/{program}/{platform}/{version_code}`

Publish a build

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). The raw body is streamed to a staging file (limited to `MAX_FILE_BYTES`, default 1 GiB), SHA-256 computed, identity checked, and moved to `<ARTIFACT_STORAGE_PATH>/<program>/<platform>/<version_code>/<filename>`. Android: package name and `versionCode` are read from the APK's `AndroidManifest.xml` and must match the path (422); `format` is `apk`. Other platforms: `?format=` is required.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `program` | path | string | yes | Reverse-DNS id: dot-separated segments of `[A-Za-z_][A-Za-z0-9_]*`, at most 255 characters. |
| `platform` | path | string | yes | `android`, `linux`, `windows` or `macos`. |
| `version_code` | path | string | yes | Integer build number, or `latest` on the metadata and download routes. |
| `format` | query | string | no | Required off Android: a token of letters, digits, `.`, `-`, `_` (lower-cased). |
| `arch` | query | string | no | Optional; ignored for Android. |
| `version_name` | query | string | no | Optional; default the version code. |
| `label` | query | string | no | Optional. |
| `filename` | query | string | no | Optional served name: one path component; default `<program>-<version_code>.<format>`. |

**Request body** (`application/octet-stream`, required)

Schema: string (binary)

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Published. | `application/json` [ArtifactView](#schema-artifactview) |
| 400 | Interrupted upload. | `application/json` [Error](#schema-error) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 409 | The same program/platform/format/version already exists. | `application/json` [Error](#schema-error) |
| 413 | Over `MAX_FILE_BYTES` (default 1 GiB). | `application/json` [Error](#schema-error) |
| 422 | Unknown platform, bad program/format/filename, bad or mismatching APK. | `application/json` [Error](#schema-error) |
| 500 | Storage failure. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "the upload was interrupted"
}
```

Example `409` response:

```json
{
  "error": "this version has already been published"
}
```

Example `413` response:

```json
{
  "error": "artifact exceeds the 1073741824-byte limit"
}
```

Example `422` response:

```json
{
  "error": "format is required for non-android artifacts"
}
```

Example `500` response:

```json
{
  "error": "could not store the file"
}
```

### GET `/api/v1/artifacts/{program}/{platform}/{version_code}`

One build's row

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). `version_code=latest` gives the highest non-yanked code (optionally within `format`).

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `program` | path | string | yes | Reverse-DNS id: dot-separated segments of `[A-Za-z_][A-Za-z0-9_]*`, at most 255 characters. |
| `platform` | path | string | yes | `android`, `linux`, `windows` or `macos`. |
| `version_code` | path | string | yes | Integer build number, or `latest` on the metadata and download routes. |
| `format` | query | string | no | The package format when a version has several (`deb`, `rpm`, `pkg.tar.zst`...). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The build. | `application/json` [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Not found, or disabled. | `application/json` [Error](#schema-error) |
| 409 | Several formats; name one. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "program or version not found"
}
```

Example `409` response:

```json
{
  "error": "this version has several formats (deb, rpm): pass ?format="
}
```

### GET `/api/v1/artifacts/{program}/{platform}/{version_code}/download`

Download a build

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). `latest` accepted. Whole file read into memory; `Content-Disposition: attachment; filename="<filename>"`; `Content-Type`: `application/vnd.android.package-archive` (apk), `application/x-apple-diskimage` (dmg), otherwise `application/octet-stream`.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `program` | path | string | yes | Reverse-DNS id: dot-separated segments of `[A-Za-z_][A-Za-z0-9_]*`, at most 255 characters. |
| `platform` | path | string | yes | `android`, `linux`, `windows` or `macos`. |
| `version_code` | path | string | yes | Integer build number, or `latest` on the metadata and download routes. |
| `format` | query | string | no | The package format when a version has several (`deb`, `rpm`, `pkg.tar.zst`...). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The bytes. | `application/octet-stream` string (binary) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |
| 409 | Several formats. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "program or version not found"
}
```

Example `409` response:

```json
{
  "error": "this version has several formats (deb, rpm): pass ?format="
}
```

### GET `/api/v1/artifacts/{program}/{platform}`

Versions on one platform

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). Newest `version_code` first; an unknown platform or invalid program gives `[]`.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `program` | path | string | yes | Reverse-DNS id: dot-separated segments of `[A-Za-z_][A-Za-z0-9_]*`, at most 255 characters. |
| `platform` | path | string | yes | `android`, `linux`, `windows` or `macos`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Builds. | `application/json` array of [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Disabled. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "artifact storage is not enabled"
}
```

### GET `/api/v1/artifacts/{program}`

Every build of a program

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). Sorted by platform, then newest first.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `program` | path | string | yes | Reverse-DNS id: dot-separated segments of `[A-Za-z_][A-Za-z0-9_]*`, at most 255 characters. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Builds. | `application/json` array of [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Disabled. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "artifact storage is not enabled"
}
```

### GET `/api/v1/artifacts`

Catalog

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). The latest non-yanked build per `(program, platform, format)`; an unknown `platform` gives `[]`.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `platform` | query | string | no | Filter. |
| `format` | query | string | no | Filter. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Latest builds. | `application/json` array of [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Disabled. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "artifact storage is not enabled"
}
```

### DELETE `/api/v1/artifacts/{program}/{platform}/{version_code}/yank`

Yank a version

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). Every format of the version unless `format` is given.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `program` | path | string | yes | Reverse-DNS id: dot-separated segments of `[A-Za-z_][A-Za-z0-9_]*`, at most 255 characters. |
| `platform` | path | string | yes | `android`, `linux`, `windows` or `macos`. |
| `version_code` | path | string | yes | Integer build number, or `latest` on the metadata and download routes. |
| `format` | query | string | no | The package format when a version has several (`deb`, `rpm`, `pkg.tar.zst`...). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Yanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |
| 500 | Update failed. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `404` response:

```json
{
  "error": "program or version not found"
}
```

Example `500` response:

```json
{
  "error": "..."
}
```

### PUT `/api/v1/artifacts/{program}/{platform}/{version_code}/unyank`

Unyank a version

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `program` | path | string | yes | Reverse-DNS id: dot-separated segments of `[A-Za-z_][A-Za-z0-9_]*`, at most 255 characters. |
| `platform` | path | string | yes | `android`, `linux`, `windows` or `macos`. |
| `version_code` | path | string | yes | Integer build number, or `latest` on the metadata and download routes. |
| `format` | query | string | no | The package format when a version has several (`deb`, `rpm`, `pkg.tar.zst`...). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Unyanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |
| 500 | Update failed. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `404` response:

```json
{
  "error": "program or version not found"
}
```

Example `500` response:

```json
{
  "error": "..."
}
```

## apk

Deprecated `android`-only alias of the artifact API.

### PUT `/api/v1/apk/{package}/{version_code}`

Publish an APK

Deprecated alias of the artifact API for `platform=android`. Every call is logged at warn and answered with `Deprecation: true` and `Link: <.../api/v1/artifacts>; rel="successor-version"` (plus `Sunset` when `APK_ALIAS_SUNSET` is set); with `FEATURE_APK_ALIAS_ENABLED=false` every call is `410 Gone`. Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `package` | path | string | yes | The Android package name (the artifact `program`). |
| `version_code` | path | string | yes | Build number. |
| `version_name` | query | string | no | Ignored for Android (read from the manifest). |
| `label` | query | string | no | Ignored. |
| `filename` | query | string | no | Optional served name. |

**Request body** (`application/vnd.android.package-archive`, required)

Schema: string (binary)

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Published. | `application/json` [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 409 | Exists. | `application/json` [Error](#schema-error) |
| 410 | Alias switched off. | `application/json` [CargoErrors](#schema-cargoerrors) |
| 422 | Bad or mismatching APK. | `application/json` [Error](#schema-error) |

Example `409` response:

```json
{
  "error": "this version has already been published"
}
```

Example `410` response:

```json
{
  "errors": [
    {
      "detail": "/api/v1/apk has been removed; use /warehouse/api/v1/artifacts"
    }
  ]
}
```

Example `422` response:

```json
{
  "error": "manifest declares ..."
}
```

### GET `/api/v1/apk/{package}/{version_code}`

One APK's row

Deprecated alias of the artifact API for `platform=android`. Every call is logged at warn and answered with `Deprecation: true` and `Link: <.../api/v1/artifacts>; rel="successor-version"` (plus `Sunset` when `APK_ALIAS_SUNSET` is set); with `FEATURE_APK_ALIAS_ENABLED=false` every call is `410 Gone`. Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). `latest` accepted.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `package` | path | string | yes | The Android package name (the artifact `program`). |
| `version_code` | path | string | yes | Build number or `latest`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The build. | `application/json` [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |
| 410 | Alias off. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `404` response:

```json
{
  "error": "package or version not found"
}
```

Example `410` response:

```json
{
  "errors": [
    {
      "detail": "..."
    }
  ]
}
```

### GET `/api/v1/apk/{package}/{version_code}/download`

Download an APK

Deprecated alias of the artifact API for `platform=android`. Every call is logged at warn and answered with `Deprecation: true` and `Link: <.../api/v1/artifacts>; rel="successor-version"` (plus `Sunset` when `APK_ALIAS_SUNSET` is set); with `FEATURE_APK_ALIAS_ENABLED=false` every call is `410 Gone`. Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). `latest` accepted.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `package` | path | string | yes | The Android package name (the artifact `program`). |
| `version_code` | path | string | yes | Build number or `latest`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The APK. | `application/vnd.android.package-archive` string (binary) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |
| 410 | Alias off. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `404` response:

```json
{
  "error": "package or version not found"
}
```

Example `410` response:

```json
{
  "errors": [
    {
      "detail": "..."
    }
  ]
}
```

### GET `/api/v1/apk/{package}`

Versions of an APK

Deprecated alias of the artifact API for `platform=android`. Every call is logged at warn and answered with `Deprecation: true` and `Link: <.../api/v1/artifacts>; rel="successor-version"` (plus `Sunset` when `APK_ALIAS_SUNSET` is set); with `FEATURE_APK_ALIAS_ENABLED=false` every call is `410 Gone`. Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`).

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `package` | path | string | yes | The Android package name (the artifact `program`). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Newest first. | `application/json` array of [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 410 | Alias off. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `410` response:

```json
{
  "errors": [
    {
      "detail": "..."
    }
  ]
}
```

### GET `/api/v1/apk`

Android catalog

Deprecated alias of the artifact API for `platform=android`. Every call is logged at warn and answered with `Deprecation: true` and `Link: <.../api/v1/artifacts>; rel="successor-version"` (plus `Sunset` when `APK_ALIAS_SUNSET` is set); with `FEATURE_APK_ALIAS_ENABLED=false` every call is `410 Gone`. Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`). Latest non-yanked per package, sorted by package.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Latest builds. | `application/json` array of [ArtifactView](#schema-artifactview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 410 | Alias off. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `410` response:

```json
{
  "errors": [
    {
      "detail": "..."
    }
  ]
}
```

### DELETE `/api/v1/apk/{package}/{version_code}/yank`

Yank an APK version

Deprecated alias of the artifact API for `platform=android`. Every call is logged at warn and answered with `Deprecation: true` and `Link: <.../api/v1/artifacts>; rel="successor-version"` (plus `Sunset` when `APK_ALIAS_SUNSET` is set); with `FEATURE_APK_ALIAS_ENABLED=false` every call is `410 Gone`. Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `package` | path | string | yes | The Android package name (the artifact `program`). |
| `version_code` | path | string | yes | Build number. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Yanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |
| 410 | Alias off. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `404` response:

```json
{
  "error": "program or version not found"
}
```

Example `410` response:

```json
{
  "errors": [
    {
      "detail": "..."
    }
  ]
}
```

### PUT `/api/v1/apk/{package}/{version_code}/unyank`

Unyank an APK version

Deprecated alias of the artifact API for `platform=android`. Every call is logged at warn and answered with `Deprecation: true` and `Link: <.../api/v1/artifacts>; rel="successor-version"` (plus `Sunset` when `APK_ALIAS_SUNSET` is set); with `FEATURE_APK_ALIAS_ENABLED=false` every call is `410 Gone`. Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` is needed for PUT/DELETE. Answers 404 `artifact storage is not enabled` unless `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `package` | path | string | yes | The Android package name (the artifact `program`). |
| `version_code` | path | string | yes | Build number. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Unyanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |
| 410 | Alias off. | `application/json` [CargoErrors](#schema-cargoerrors) |

Example `404` response:

```json
{
  "error": "program or version not found"
}
```

Example `410` response:

```json
{
  "errors": [
    {
      "detail": "..."
    }
  ]
}
```

## rivets

`.rivet` package registry (whole Riveter overlays, versioned).

### PUT `/api/v1/rivets/{name}/{version}`

Publish a package

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` publishes or yanks. Answers 404 `rivet storage is not enabled` unless `FEATURE_RIVETS_ENABLED` (read on every call). The body (the `.rivet`, `tar.zst`) is streamed to a staging file, limited to `RIVET_MAX_BYTES` (default 64 MiB), then fully validated with `rivet_package::Package::read` (default limits); its `rivet.toml` name and version must equal the path. The file is hard-linked into place (no clobbering) and the row inserted.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Package name: a DNS-1123 label. |
| `version` | path | string | yes | Semver, optionally with build metadata. |

**Request body** (`application/zstd`, required)

Schema: string (binary)

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Published. | `application/json` [RivetView](#schema-rivetview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Rivets disabled. | `application/json` [Error](#schema-error) |
| 409 | Exists. | `application/json` [Error](#schema-error) |
| 413 | Over `RIVET_MAX_BYTES`. | `application/json` [Error](#schema-error) |
| 422 | Invalid name/version, invalid archive, or manifest mismatch. | `application/json` [Error](#schema-error) |
| 500 | Storage failure. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "rivet storage is not enabled"
}
```

Example `409` response:

```json
{
  "error": "this package version has already been published"
}
```

Example `413` response:

```json
{
  "error": "package exceeds the 67108864-byte limit"
}
```

Example `422` response:

```json
{
  "error": "the archive declares `a` version 1.0.0, which does not match the published path `b/1.0.0`"
}
```

Example `500` response:

```json
{
  "error": "could not store the package"
}
```

### GET `/api/v1/rivets/{name}/{version}`

One version's row

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` publishes or yanks. Answers 404 `rivet storage is not enabled` unless `FEATURE_RIVETS_ENABLED` (read on every call).

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Package name: a DNS-1123 label. |
| `version` | path | string | yes | Semver with optional build metadata (`0.4.0+b123`), or `latest` on GET routes. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The row. | `application/json` [RivetView](#schema-rivetview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "package or version not found"
}
```

### GET `/api/v1/rivets/{name}/{version}/download`

Download a package

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` publishes or yanks. Answers 404 `rivet storage is not enabled` unless `FEATURE_RIVETS_ENABLED` (read on every call). Headers `Content-Type: application/zstd`, `x-rivet-sha256`, a `Content-Disposition` with the file name. A yanked exact version still downloads.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Package name: a DNS-1123 label. |
| `version` | path | string | yes | Semver with optional build metadata (`0.4.0+b123`), or `latest` on GET routes. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The archive. | `application/zstd` string (binary) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "package or version not found"
}
```

### GET `/api/v1/rivets/{name}`

Versions of a package

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` publishes or yanks. Answers 404 `rivet storage is not enabled` unless `FEATURE_RIVETS_ENABLED` (read on every call). Newest first by semver; an invalid name gives `[]`.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Package name: a DNS-1123 label. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Rows. | `application/json` array of [RivetView](#schema-rivetview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Rivets disabled. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "rivet storage is not enabled"
}
```

### GET `/api/v1/rivets`

Catalog

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` publishes or yanks. Answers 404 `rivet storage is not enabled` unless `FEATURE_RIVETS_ENABLED` (read on every call). The latest non-yanked version of each package.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Rows. | `application/json` array of [RivetView](#schema-rivetview) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 404 | Rivets disabled. | `application/json` [Error](#schema-error) |

Example `404` response:

```json
{
  "error": "rivet storage is not enabled"
}
```

### DELETE `/api/v1/rivets/{name}/{version}/yank`

Yank a version

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` publishes or yanks. Answers 404 `rivet storage is not enabled` unless `FEATURE_RIVETS_ENABLED` (read on every call).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Package name: a DNS-1123 label. |
| `version` | path | string | yes | Exact version. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Yanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `404` response:

```json
{
  "error": "package or version not found"
}
```

### PUT `/api/v1/rivets/{name}/{version}/unyank`

Unyank a version

Auth: `Auth` + `RequireWrite`: any valid realm identity reads, `warehouse:write` publishes or yanks. Answers 404 `rivet storage is not enabled` unless `FEATURE_RIVETS_ENABLED` (read on every call).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Package name: a DNS-1123 label. |
| `version` | path | string | yes | Exact version. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Unyanked. | `application/json` [Ok](#schema-ok) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 404 | Not found. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
{
  "ok": true
}
```

Example `404` response:

```json
{
  "error": "package or version not found"
}
```

## admin

Garbage collection of the crates and Docker storages.

### POST `/admin/crates/gc`

Garbage-collect the crates storage

Deletes tarballs of yanked versions (so a later unyank cannot restore them) and of versions missing from the index, repairs index entries that point at missing tarballs, removes orphan `owners.json` and empty directories. `Auth` + `RequireWrite`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | What was done. | `application/json` [GcCratesReport](#schema-gccratesreport) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 500 | GC failed (empty body). |  |

Example `200` response:

```json
{
  "deleted_crates": 2,
  "kept_crates": 10,
  "removed_index_entries": 1,
  "deleted_owner_files": 0,
  "removed_empty_dirs": 3
}
```

### POST `/admin/docker/gc`

Garbage-collect the Docker storage

Deletes blob files that no stored manifest references (following manifest lists to their children), then empty directories. Manifests are never deleted (including ones no tag points at); `deleted` and `kept` count blobs. `Auth` + `RequireWrite`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `warehouse:write`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | What was done. | `application/json` [GcDockerReport](#schema-gcdockerreport) |
| 401 | Missing/invalid token or no session (empty body or the framework's text). |  |
| 403 | `RequireWrite`: the token lacks the blanket `warehouse:write` action (empty body). |  |
| 500 | GC failed (empty body). |  |

Example `200` response:

```json
{
  "deleted": 5,
  "kept": 120
}
```

## ui-api

JSON the browser client reads and writes (`/ui/api/...`), behind the realm session cookie.

### GET `/ui/api/overview`

Who is asking and what is on

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Overview. | `application/json` [Overview](#schema-overview) |
| 401 | No session. | `application/json` [Error](#schema-error) |

Example `200` response:

```json
{
  "username": "alice",
  "can_manage": true,
  "features": {
    "docker": true,
    "crates": true,
    "files": true,
    "artifacts": true,
    "rivets": true
  }
}
```

Example `401` response:

```json
{
  "error": "sign in"
}
```

### GET `/ui/api/docker/repositories`

Repositories

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Names. | `application/json` array of string |
| 401 | No session. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

### GET `/ui/api/docker/tags`

Tags of a repository

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `repo` | query | string | yes | Repository name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Tags with digests, newest first. | `application/json` array of [DockerTag](#schema-dockertag) |
| 400 | Invalid name. | `application/json` [Error](#schema-error) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 404 | No such repository. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "api_error_invalid_repository"
}
```

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `404` response:

```json
{
  "error": "api_error_invalid_repository"
}
```

### POST `/ui/api/docker/delete-image`

Delete an image

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. **Any session may delete** (no `warehouse:write` check).

**Authentication:** `sessionCookie`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `repository` | string | yes |  |
| `digest` | string | yes | `sha256:<64 hex>` |

```json
{
  "repository": "forge/x",
  "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Deleted (manifest and every tag pointing at it). |  |
| 400 | Bad digest/repository. | `application/json` [Error](#schema-error) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 404 | Unknown manifest. | `application/json` [Error](#schema-error) |
| 500 | Failed. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "api_error_digest_required"
}
```

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `404` response:

```json
{
  "error": "api_error_manifest_unknown"
}
```

Example `500` response:

```json
{
  "error": "api_error_internal"
}
```

### GET `/ui/api/crates`

Crate names

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Names. | `application/json` array of string |
| 401 | No session. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

### GET `/ui/api/crates/{name}/versions`

Versions of a crate

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Crate name: 1 to 64 of ASCII letters, digits, `-`, `_`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Oldest first (index order). | `application/json` array of [CrateVersion](#schema-crateversion) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 404 | Not in the index. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `404` response:

```json
{
  "error": "api_error_crate_version_not_found"
}
```

### POST `/ui/api/crates/yank`

Yank a crate version

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. **Any session may** (no `warehouse:write` check).

**Authentication:** `sessionCookie`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |

```json
{
  "name": "serde",
  "version": "1.0.0"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Done. |  |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 404 | No such version. | `application/json` [Error](#schema-error) |
| 500 | Failed. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `404` response:

```json
{
  "error": "api_error_crate_version_not_found"
}
```

Example `500` response:

```json
{
  "error": "api_error_internal"
}
```

### POST `/ui/api/crates/unyank`

Unyank a crate version

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. **Any session may** (no `warehouse:write` check).

**Authentication:** `sessionCookie`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |

```json
{
  "name": "serde",
  "version": "1.0.0"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Done. |  |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 404 | No such version. | `application/json` [Error](#schema-error) |
| 500 | Failed. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `404` response:

```json
{
  "error": "api_error_crate_version_not_found"
}
```

Example `500` response:

```json
{
  "error": "api_error_internal"
}
```

### GET `/ui/api/artifacts`

All artifact versions

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. `[]` when artifacts are disabled.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Entries. | `application/json` array of [ArtifactEntry](#schema-artifactentry) |
| 401 | No session. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

### POST `/ui/api/artifacts/yank`

Yank an artifact version

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. Applies to every format of the version.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `program` | string | yes |  |
| `platform` | string | yes |  |
| `version_code` | integer (int64) | yes |  |

```json
{
  "program": "dev.lorehaven.pedlar",
  "platform": "linux",
  "version_code": 42
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Done. | `application/json` [Ok](#schema-ok) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Disabled or unknown. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_artifacts_disabled"
}
```

### POST `/ui/api/artifacts/unyank`

Unyank an artifact version

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. Applies to every format of the version.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `program` | string | yes |  |
| `platform` | string | yes |  |
| `version_code` | integer (int64) | yes |  |

```json
{
  "program": "dev.lorehaven.pedlar",
  "platform": "linux",
  "version_code": 42
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Done. | `application/json` [Ok](#schema-ok) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Disabled or unknown. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_artifacts_disabled"
}
```

### GET `/ui/api/rivets`

All rivet versions

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. `[]` when rivets are disabled.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Entries. | `application/json` array of [RivetEntry](#schema-rivetentry) |
| 401 | No session. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

### POST `/ui/api/rivets/yank`

Yank a rivet version

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |

```json
{
  "name": "forge",
  "version": "0.4.0"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Done. | `application/json` [Ok](#schema-ok) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Disabled or unknown. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_rivets_disabled"
}
```

### POST `/ui/api/rivets/unyank`

Unyank a rivet version

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |

```json
{
  "name": "forge",
  "version": "0.4.0"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Done. | `application/json` [Ok](#schema-ok) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Disabled or unknown. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_rivets_disabled"
}
```

### GET `/ui/api/files/storages`

Storages

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. Dynamic storages are listed to every session (no per-storage filter).

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Fixed and dynamic storages. | `application/json` [UiStorages](#schema-uistorages) |
| 401 | No session. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

### POST `/ui/api/files/storages`

Create a dynamic storage

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `owner` | string | yes |  |
| `quota_bytes` | integer or null (int64) | no |  |
| `max_file_bytes` | integer or null (int64) | no |  |
| `sync_enabled` | boolean | yes |  |

```json
{
  "name": "phone",
  "owner": "alice",
  "sync_enabled": true
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Created. | `application/json` [DynamicStorage](#schema-dynamicstorage) |
| 400 | Invalid name, owner, quota or limit. | `application/json` [Error](#schema-error) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Files disabled. | `application/json` [Error](#schema-error) |
| 409 | Name clash. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "api_error_invalid_storage_name"
}
```

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_files_disabled"
}
```

Example `409` response:

```json
{
  "error": "api_error_storage_exists"
}
```

### POST `/ui/api/files/storages/{name}/edit`

Edit a dynamic storage

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Storage name. |

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `quota_bytes` | integer or null (int64) | no | Null keeps the quota. |
| `max_file_bytes` | integer or null (int64) | no | Null keeps the limit unless `clear_max_file`. |
| `clear_max_file` | boolean | yes |  |
| `sync_enabled` | boolean | yes |  |

```json
{
  "quota_bytes": 21474836480,
  "clear_max_file": false,
  "sync_enabled": true
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Updated. | `application/json` [DynamicStorage](#schema-dynamicstorage) |
| 400 | Invalid quota or limit. | `application/json` [Error](#schema-error) |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Files disabled, or no such storage. | `application/json` [Error](#schema-error) |

Example `400` response:

```json
{
  "error": "api_error_invalid_quota"
}
```

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_storage_not_found"
}
```

### POST `/ui/api/files/storages/{name}/delete`

Delete a dynamic storage

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | Storage name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Deleted. |  |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Files disabled, or no such storage. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_storage_not_found"
}
```

### GET `/ui/api/files/browse`

Files of a storage

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted. A static storage is open to any session; a dynamic one to its owner, a wildcard role or `warehouse:storage:<name>:read` (otherwise `access: denied` with no files).

**Authentication:** `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `storage` | query | string | yes | Storage name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Listing. | `application/json` [Browse](#schema-browse) |
| 401 | No session. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

### POST `/ui/api/files/delete-file`

Delete a file

Behind the realm session cookie (`401 {"error":"sign in"}` without one). Viewing needs any session; a change needs `warehouse:write` or a wildcard role (`403 {"error":"api_error_forbidden"}`) unless noted.

**Authentication:** `sessionCookie`. **Permission:** `warehouse:write`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `storage` | string | yes |  |
| `path` | string | yes |  |

```json
{
  "storage": "media",
  "path": "a/b.txt"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Deleted. |  |
| 401 | No session. | `application/json` [Error](#schema-error) |
| 403 | Lacks `warehouse:write`. | `application/json` [Error](#schema-error) |
| 404 | Files disabled or no such file. | `application/json` [Error](#schema-error) |

Example `401` response:

```json
{
  "error": "sign in"
}
```

Example `403` response:

```json
{
  "error": "api_error_forbidden"
}
```

Example `404` response:

```json
{
  "error": "api_error_files_disabled"
}
```

## ui

Pages, redirects, sign-in handover and the client's static files (`/ui/...`).

### GET `/ui`

Entry point

Without a session 302 to the login page, otherwise 302 to `/ui/home`.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To `/ui/home` or `/ui/login`. |  |

### GET `/ui/`

Entry point (trailing slash)

Same as `/ui`.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To `/ui/home` or `/ui/login`. |  |

### GET `/ui/home`

Home page

Needs a session; without one: 302 to the login page. The client draws the screen from the `/ui/api` calls.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/home/`

Home page (trailing slash)

Needs a session; without one: 302 to the login page. The client draws the screen from the `/ui/api` calls.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/docker`

Redirect /ui/docker

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/docker/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/docker/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/docker/`

Redirect /ui/docker/

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/docker/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/docker/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/crates`

Redirect /ui/crates

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/crates/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/crates/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/crates/`

Redirect /ui/crates/

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/crates/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/crates/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/files`

Redirect /ui/files

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/files/storages`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/files/storages`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/files/`

Redirect /ui/files/

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/files/storages`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/files/storages`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/artifacts`

Redirect /ui/artifacts

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/artifacts/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/artifacts/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/artifacts/`

Redirect /ui/artifacts/

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/artifacts/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/artifacts/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/apk`

Redirect /ui/apk

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/artifacts/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/artifacts/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/apk/`

Redirect /ui/apk/

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/artifacts/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/artifacts/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/rivets`

Redirect /ui/rivets

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/rivets/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/rivets/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/rivets/`

Redirect /ui/rivets/

Needs a session (otherwise 302 to the login page); then a permanent redirect (308) to `/ui/rivets/catalog`.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To `/ui/rivets/catalog`. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/docker/tags/{repository}`

Old tag-page link

Needs a session; 308 to `/ui/docker/catalog?repo=<repository>`. The route pattern is `{repository:.+}`.

**Authentication:** `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `repository` | path | string | yes | Repository name (may contain `/`). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 308 | To the catalog. Headers: `Location`. |  |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/docker/catalog`

Docker catalog page

Needs a session; without one: 302 to the login page. The address keeps `?repo=`, `?tag=`, `?storage=`, `?path=` for the client to read.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/docker/catalog/`

Docker catalog page (trailing slash)

Same as without the slash.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/crates/catalog`

Crates catalog page

Needs a session; without one: 302 to the login page. The address keeps `?repo=`, `?tag=`, `?storage=`, `?path=` for the client to read.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/crates/catalog/`

Crates catalog page (trailing slash)

Same as without the slash.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/files/storages`

Storages page

Needs a session; without one: 302 to the login page. The address keeps `?repo=`, `?tag=`, `?storage=`, `?path=` for the client to read.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/files/storages/`

Storages page (trailing slash)

Same as without the slash.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/files/browse`

File browser page

Needs a session; without one: 302 to the login page. The address keeps `?repo=`, `?tag=`, `?storage=`, `?path=` for the client to read.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/files/browse/`

File browser page (trailing slash)

Same as without the slash.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/artifacts/catalog`

Artifacts page

Needs a session; without one: 302 to the login page. The address keeps `?repo=`, `?tag=`, `?storage=`, `?path=` for the client to read.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/artifacts/catalog/`

Artifacts page (trailing slash)

Same as without the slash.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/rivets/catalog`

Rivets page

Needs a session; without one: 302 to the login page. The address keeps `?repo=`, `?tag=`, `?storage=`, `?path=` for the client to read.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/rivets/catalog/`

Rivets page (trailing slash)

Same as without the slash.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page. | `text/html` string |
| 302 | No session: to `<base>/ui/login`. Headers: `Location`. |  |

### GET `/ui/login`

Hand the browser to Gatehouse

A still-good refresh cookie is renewed in place (302 to `<base>/ui/home`, new cookies). Otherwise a 302 to Gatehouse's `/api/v1/authorize` (PKCE S256, `client_id` = `GATEHOUSE_CLIENT_ID`, `redirect_uri` = this service's `/ui/auth/callback`) with the state cookie `forge_authorize_state`. `503 gatehouse is not configured` (text) when `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID` or `GATEHOUSE_CLIENT_SECRET` is missing.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To Gatehouse, or home after a silent renewal. |  |
| 503 | Gatehouse not configured. |  |

### GET `/ui/login/`

Hand the browser to Gatehouse (trailing slash)

Same as `/ui/login`.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To Gatehouse. |  |
| 503 | Gatehouse not configured. |  |

### GET `/ui/auth/callback`

Authorization-code callback

Receives `code` and `state` from Gatehouse, checks the state against the `forge_authorize_state` cookie, exchanges the code at Gatehouse's `/api/v1/token` (`GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`, the PKCE verifier from the cookie) and sets the session and refresh cookies. Any failure (missing or corrupt state, state mismatch, no code, unreachable or refusing Gatehouse) is a 302 to `<base>/ui/login` with the state cookie cleared.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `code` | query | string | no | From Gatehouse. |
| `state` | query | string | no | Must match the state cookie. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Success: to the saved destination (`<base>/ui/home`) with cookies set. Failure: to `<base>/ui/login`. |  |
| 503 | `GATEHOUSE_URL` not set. |  |

### GET `/ui/logout`

Sign out

Delegates to Gatehouse's logout.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To Gatehouse. |  |

### GET `/ui/status`

Session status for the page watcher

Always 200 JSON `{authenticated, username, roles}` (`quench-auth`'s `auth_status`).

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Status. | `application/json` object |

### POST `/ui/refresh`

Renew the session

Exchanges the refresh cookie through Gatehouse (`sso_client::refresh`) and sets new cookies.

**Authentication:** `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Renewed; cookies set. |  |
| 401 | No or bad refresh cookie. |  |

### GET `/ui/assets/quench.js`

The wasm loader script

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The wasm loader script | `text/javascript` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/app.css`

The stylesheet

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The stylesheet | `text/css` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/shell.js`

forge-shell's script

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | forge-shell's script | `text/javascript` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/favicon.png`

The service icon

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The service icon | `image/png` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/app.wasm`

The browser client module (404 when not installed)

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The browser client module (404 when not installed) | `application/wasm` string (binary) |
| 404 | Not available. |  |

## Schemas

### Schema Error

<a id="schema-error"></a>

`{"error": "..."}` (artifacts, rivets, files, UI API; the UI API uses `api_error_*` keys).

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | string | yes |  |

### Schema CargoErrors

<a id="schema-cargoerrors"></a>

Cargo registry error body.

| Field | Type | Required | Description |
|---|---|---|---|
| `errors` | array of object | yes | Cargo's error list. |

### Schema DockerErrors

<a id="schema-dockererrors"></a>

Docker Registry error body.

| Field | Type | Required | Description |
|---|---|---|---|
| `errors` | array of object | yes |  |

### Schema Ok

<a id="schema-ok"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `ok` | boolean | yes |  |

### Schema ArtifactView

<a id="schema-artifactview"></a>

A published build. The three SDK/permission fields are Android-only (null/empty elsewhere).

| Field | Type | Required | Description |
|---|---|---|---|
| `program` | string | yes | Reverse-DNS application id. |
| `platform` | string (`android`, `linux`, `windows`, `macos`) | yes |  |
| `arch` | string or null | yes | `x86_64`, `aarch64`, `universal`...; null for Android. |
| `format` | string | yes | `apk` for Android; the client's `?format=` token otherwise. |
| `version_code` | integer (int64) | yes |  |
| `version_name` | string | yes |  |
| `filename` | string | yes |  |
| `size_bytes` | integer (int64) | yes |  |
| `sha256` | string | yes |  |
| `label` | string or null | yes |  |
| `min_sdk_version` | integer or null | yes |  |
| `target_sdk_version` | integer or null | yes |  |
| `permissions` | array of string | yes |  |
| `uploaded_by` | string | yes |  |
| `yanked` | boolean | yes |  |
| `created_at` | string (date-time) | yes |  |

### Schema RivetView

<a id="schema-rivetview"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |
| `description` | string or null | yes |  |
| `namespace` | string or null | yes |  |
| `filename` | string | yes | `<name>-<version>.rivet` |
| `size_bytes` | integer (int64) | yes |  |
| `sha256` | string | yes |  |
| `manifest` | object | yes | The archive's own `rivet.toml`, parsed. |
| `uploaded_by` | string | yes |  |
| `yanked` | boolean | yes |  |
| `created_at` | string (date-time) | yes |  |

### Schema CratePublishResponse

<a id="schema-cratepublishresponse"></a>

Always empty lists.

| Field | Type | Required | Description |
|---|---|---|---|
| `warnings` | object | yes |  |

### Schema CratesIndexConfig

<a id="schema-cratesindexconfig"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `dl` | string | yes | `<REGISTRY_BASE_URL><base>/api/v1/crates/{crate}/{version}/download` |
| `api` | string | yes | `<REGISTRY_BASE_URL><base>` |
| `auth-required` | boolean | no | Present (true) only when `CRATES_REQUIRE_AUTH_FOR_READS` is on. |

### Schema CrateSearchResponse

<a id="schema-cratesearchresponse"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `crates` | array of object | yes |  |
| `meta` | object | yes |  |

### Schema CrateOwners

<a id="schema-crateowners"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `users` | array of object | yes |  |

### Schema CrateOwnersRequest

<a id="schema-crateownersrequest"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `users` | array of string | yes | Logins; must not be empty. |

### Schema DockerToken

<a id="schema-dockertoken"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `token` | string | yes |  |
| `expires_in` | integer | yes | Always 600. |
| `issued_at` | string (date-time) | yes |  |

### Schema DockerCatalog

<a id="schema-dockercatalog"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `repositories` | array of string | yes |  |

### Schema DockerTagList

<a id="schema-dockertaglist"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `tags` | array of string | yes |  |

### Schema StorageSummary

<a id="schema-storagesummary"></a>

Static storages carry only `name`; the other fields are present for dynamic ones.

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `owner` | string | no |  |
| `quota_bytes` | integer (int64) | no |  |
| `used_bytes` | integer (int64) | no |  |
| `sync_enabled` | boolean | no |  |

### Schema DynamicStorage

<a id="schema-dynamicstorage"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `owner` | string | yes |  |
| `max_file_bytes` | integer or null (int64) | yes |  |
| `quota_bytes` | integer (int64) | yes |  |
| `used_bytes` | integer (int64) | yes |  |
| `sync_enabled` | boolean | yes |  |
| `created_at` | string (date-time) | yes |  |

### Schema CreateStorage

<a id="schema-createstorage"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Letters, digits, `-`, `_`. |
| `owner` | string | yes | An existing realm user. |
| `max_file_bytes` | integer or null (int64) | no |  |
| `quota_bytes` | integer or null (int64) | no | Default `DEFAULT_STORAGE_QUOTA_BYTES` (10 GiB). |
| `sync_enabled` | boolean | no |  Default `false`. |

### Schema PatchStorage

<a id="schema-patchstorage"></a>

Only the given fields change.

| Field | Type | Required | Description |
|---|---|---|---|
| `max_file_bytes` | integer or null (int64) | no |  |
| `clear_max_file_bytes` | boolean | no | True clears the override (JSON cannot tell absent from null). Default `false`. |
| `quota_bytes` | integer or null (int64) | no |  |
| `sync_enabled` | boolean or null | no |  |

### Schema FileEntry

<a id="schema-fileentry"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `path` | string | yes |  |
| `kind` | string (`file`, `directory`) | yes |  |
| `size` | integer (int64) | no | Files only. |

### Schema FileListing

<a id="schema-filelisting"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `storage` | string | yes |  |
| `prefix` | string | yes |  |
| `entries` | array of [FileEntry](#schema-fileentry) | yes |  |

### Schema Stored

<a id="schema-stored"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |
| `size` | integer (int64) | yes |  |
| `digest` | string | yes | `sha256:<hex>` |

### Schema SyncLogEntry

<a id="schema-synclogentry"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | integer (int64) | yes | The cursor. |
| `path` | string | yes |  |
| `op` | string | yes |  |
| `sha256` | string or null | yes |  |
| `size` | integer or null (int64) | yes |  |
| `at` | string (date-time) | yes |  |

### Schema GcDockerReport

<a id="schema-gcdockerreport"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `deleted` | integer | yes |  |
| `kept` | integer | yes |  |

### Schema GcCratesReport

<a id="schema-gccratesreport"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `deleted_crates` | integer | yes |  |
| `kept_crates` | integer | yes |  |
| `removed_index_entries` | integer | yes |  |
| `deleted_owner_files` | integer | yes |  |
| `removed_empty_dirs` | integer | yes |  |

### Schema Overview

<a id="schema-overview"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `can_manage` | boolean | yes | Holds `warehouse:write` or a wildcard role. |
| `features` | object | yes |  |

### Schema DockerTag

<a id="schema-dockertag"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `tag` | string | yes |  |
| `digest` | string | yes |  |
| `media_type` | string or null | no |  |
| `size_bytes` | integer or null | no |  |

### Schema ImageRef

<a id="schema-imageref"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `repository` | string | yes |  |
| `digest` | string | yes | `sha256:<64 hex>` |

### Schema CrateRef

<a id="schema-crateref"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |

### Schema CrateVersion

<a id="schema-crateversion"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `vers` | string | yes |  |
| `yanked` | boolean | yes |  |
| `cksum` | string | yes |  |
| `rust_version` | string or null | no |  |
| `links` | string or null | no |  |
| `features` | array of string | no |  |
| `deps` | array of object | no |  |

### Schema ArtifactEntry

<a id="schema-artifactentry"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `program` | string | yes |  |
| `platform` | string | yes |  |
| `format` | string | yes |  |
| `version_code` | integer (int64) | yes |  |
| `version_name` | string | yes |  |
| `filename` | string | yes |  |
| `size_bytes` | integer (int64) | yes |  |
| `sha256` | string | yes |  |
| `uploaded_by` | string | yes |  |
| `yanked` | boolean | yes |  |
| `arch` | string or null | no |  |
| `label` | string or null | no |  |
| `min_sdk_version` | integer or null | no |  |
| `target_sdk_version` | integer or null | no |  |
| `permissions` | array of string | no |  |

### Schema ArtifactRef

<a id="schema-artifactref"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `program` | string | yes |  |
| `platform` | string | yes |  |
| `version_code` | integer (int64) | yes |  |

### Schema RivetEntry

<a id="schema-rivetentry"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |
| `size_bytes` | integer (int64) | yes |  |
| `sha256` | string | yes |  |
| `uploaded_by` | string | yes |  |
| `yanked` | boolean | yes |  |
| `published` | string | yes | `2026-10-29 10:00 UTC` |
| `description` | string or null | no |  |
| `namespace` | string or null | no |  |
| `requires_riveter` | string or null | no |  |
| `requires_packages` | array of string | no |  |
| `meta` | array of object | no |  |

### Schema RivetRef

<a id="schema-rivetref"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `version` | string | yes |  |

### Schema UiStorages

<a id="schema-uistorages"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `fixed` | array of object | yes |  |
| `dynamic` | array of object | yes |  |

### Schema UiNewStorage

<a id="schema-uinewstorage"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `owner` | string | yes |  |
| `quota_bytes` | integer or null (int64) | no |  |
| `max_file_bytes` | integer or null (int64) | no |  |
| `sync_enabled` | boolean | yes |  |

### Schema UiStorageChanges

<a id="schema-uistoragechanges"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `quota_bytes` | integer or null (int64) | no | Null keeps the quota. |
| `max_file_bytes` | integer or null (int64) | no | Null keeps the limit unless `clear_max_file`. |
| `clear_max_file` | boolean | yes |  |
| `sync_enabled` | boolean | yes |  |

### Schema FileRef

<a id="schema-fileref"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `storage` | string | yes |  |
| `path` | string | yes |  |

### Schema Browse

<a id="schema-browse"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `storage` | string | yes |  |
| `access` | string (`readable`, `denied`, `missing`) | yes |  |
| `files` | array of object | yes |  |
| `truncated` | boolean | yes | More files than the listing cap (50 000 for a static storage; a dynamic one is read in pages of 2000). |
