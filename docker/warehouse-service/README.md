# Warehouse Service

Warehouse storage service exposing API endpoints for crate, file, and docker registry operations.

## Run

```bash
cargo run -p warehouse-service
```

## Files API

Plain file storage, for the things neither registry fits. A build output is not
a crate and not an image: it has a name somebody chose, it is fetched back
whole, and nothing resolves it by version. Conveyor's artifacts are the first
caller.

Everything is under `{BASE_PATH}/api/v1/files` and behind the realm's auth —
a bearer token or the realm cookie, verified against gatehouse's JWKS like any
other relying party. Turned on with `FEATURE_FILES_ENABLED`.

Upload and delete need the realm's `warehouse:write` permission (or a wildcard
role); download and list need only `warehouse:read`. Enforced by `RequireWrite`
(`quench-auth`), which 403s a write-shaped request when the token's scope has
no matching grant.

The crates registry (`/api/v1/crates`) and admin endpoints (`/admin`) are
**not** behind this: they have no real authentication yet, so there is no
identity to build a permission on top of. Fixing that is separate, unstarted
work.

### Storages

A *storage* is a name bound to a directory:

```bash
FILE_STORAGES=artifacts=/storage/artifacts;media=/mnt/media
```

Callers name the storage, never the directory, so the host's layout is not
something a client can learn or come to depend on. A name that is not
configured is a `404` — there is no implicit creation, because a typo would
otherwise start a new pile of files nobody is watching. A malformed entry is
dropped with a warning rather than taken as fatal: losing one storage beats
refusing to start and taking the crates and docker registries down with it.

Names may use letters, digits, `-` and `_`. A duplicate name keeps the first
binding, so which directory is served does not depend on the order of a
semicolon-separated string.

### Endpoints

- `GET /api/v1/files` — the configured storages, names only
- `GET /api/v1/files/{storage}?prefix=<p>` — one directory, shallow
- `PUT /api/v1/files/{storage}/file?path=<p>` — store a file; the body *is* the file
- `GET /api/v1/files/{storage}/file?path=<p>` — fetch it back
- `HEAD /api/v1/files/{storage}/file?path=<p>` — existence and size
- `DELETE /api/v1/files/{storage}/file?path=<p>` — remove one file

`PUT` answers `201` for a new file and `200` for a replacement, with the size
and SHA-256 of what was stored:

```console
$ curl -u service:… -X PUT --data-binary @thing.tar.gz \
      "$WAREHOUSE/api/v1/files/artifacts/file?path=conveyor/run-1/thing.tar.gz"
{"path":"conveyor/run-1/thing.tar.gz","size":4096,"digest":"sha256:d1cc…"}
```

Uploads are streamed to disk rather than buffered, so a large artifact costs a
file descriptor and a 64KB buffer rather than its own size in memory, and
`MAX_FILE_BYTES` (defaulting to `MAX_REQUEST_BODY_BYTES`, 1GiB) is enforced as
the bytes arrive. Each one is written beside its target and renamed into place,
so a dropped connection leaves the previous version intact rather than a
truncated file that looks complete. Downloads stream back the same way.

Listing is shallow and one directory at a time. A recursive walk of a storage
holding every artifact of every run would be a request whose cost grows with
history; ask for the subtree instead.

### Paths

The `path` parameter is the only caller-controlled part of where a file lands,
and it is the whole attack surface of this API. Two independent checks:

- **Lexically**, `..` is *refused* rather than normalised away, along with
  absolute paths, empty paths, and NUL or control bytes. Collapsing `a/../../b`
  is only correct if you are also right about what `a` is — and if `a` is a
  symlink, the lexical answer and the filesystem's differ. Refusing the
  component means there is no arithmetic to get wrong, and no legitimate caller
  needs it. A name that merely *looks* alarming (`..thing`, `a..b`) is fine:
  `..` is a path component, not a substring.
- **Against the filesystem**, the resolved path is checked to be inside the
  storage root once every symlink has been followed. A symlink planted in a
  storage cannot be used to read or write outside it — including through its
  parent directory, for a file that does not exist yet.

Both are `403`. A listing applies the same rule: a symlink that stays inside
the storage is listed and served, one that leaves is neither.

## Docker API

Warehouse implements Docker Registry HTTP API v2 endpoints under `/v2`, plus token issuance at `/token`.

### Auth Flow

1. Client requests `/v2/...`.
2. Service responds with `401` and `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"` when token is missing/invalid.
3. Client requests `GET /token?service=<SERVICE_NAME>&scope=repository:<repo>:pull,push` with Basic auth (or anonymous when auth is disabled).
4. Client retries `/v2/...` with `Authorization: Bearer <jwt>`.

Key environment variables:

- `DOCKER_TOKEN_SECRET` (required when auth is enabled) — signs and verifies
  these registry tokens. Local to warehouse and independent of the realm's
  Ed25519 keys: the registry protocol speaks its own Basic-then-Bearer
  exchange that gatehouse has no part in, so there is nothing to share.
- `SERVICE_AUTH_ENABLED` (`true|false`, default `false`)
- `SERVICE_USERNAME`, `SERVICE_PASSWORD` (required when auth is enabled)
- `SERVICE_NAME` (default `warehouse`)
- `SERVICE_REALM` (default `https://localhost:8698/token`)
- `STORAGE_PATH` (default `./storage/docker`)

### Endpoint Summary

Registry:

- `GET /v2/` and `HEAD /v2/`: registry availability check
- `GET /v2/_catalog?n=<n>&last=<repo>`: paginated repository list
- `GET /v2/{name}/tags/list?n=<n>&last=<tag>`: paginated tags for repository

Blob upload/download:

- `POST /v2/{name}/blobs/uploads/`: start upload (or cross-repo mount via `mount` + `from`)
- `GET /v2/{name}/blobs/uploads/{uuid}`: upload status
- `PATCH /v2/{name}/blobs/uploads/{uuid}`: upload chunk
- `PUT /v2/{name}/blobs/uploads/{uuid}?digest=sha256:...`: complete upload
- `DELETE /v2/{name}/blobs/uploads/{uuid}`: cancel upload
- `HEAD /v2/{name}/blobs/{digest}`: check blob exists
- `GET /v2/{name}/blobs/{digest}`: retrieve blob (supports `Range`)

Manifests:

- `PUT /v2/{name}/manifests/{reference}`: upload manifest (Docker/OCI media types)
- `GET /v2/{name}/manifests/{reference}`: fetch manifest by tag or digest
- `HEAD /v2/{name}/manifests/{reference}`: check manifest exists
- `DELETE /v2/{name}/manifests/{reference}`: delete manifest (digest reference required)

Token:

- `GET /token?service=<service>&scope=<scope>`: issue JWT token for Docker Bearer auth

### OpenAPI / Swagger

- Swagger UI: `/swagger-ui/index.html`
- OpenAPI JSON: `/api-doc/openapi.json`
