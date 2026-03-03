# Warehouse Service

Warehouse storage service exposing API endpoints for crate, file, and docker registry operations.

## Run

```bash
cargo run -p warehouse-service
```

## Docker API

Warehouse implements Docker Registry HTTP API v2 endpoints under `/v2`, plus token issuance at `/token`.

### Auth Flow

1. Client requests `/v2/...`.
2. Service responds with `401` and `WWW-Authenticate: Bearer realm="<REGISTRY_REALM>",service="<REGISTRY_SERVICE>"` when token is missing/invalid.
3. Client requests `GET /token?service=<REGISTRY_SERVICE>&scope=repository:<repo>:pull,push` with Basic auth (or anonymous when auth is disabled).
4. Client retries `/v2/...` with `Authorization: Bearer <jwt>`.

Key environment variables:

- `JWT_SECRET` (required)
- `REGISTRY_AUTH_ENABLED` (`true|false`, default `false`)
- `REGISTRY_USERNAME`, `REGISTRY_PASSWORD` (required when auth is enabled)
- `REGISTRY_SERVICE` (default `warehouse`)
- `REGISTRY_REALM` (default `https://localhost:8698/token`)
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
