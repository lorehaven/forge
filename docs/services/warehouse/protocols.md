---
title: Protocols
parent: Warehouse
grand_parent: Services
nav_order: 7
---

How standard clients talk to Warehouse. Hostnames are the deployment's (`ennor.ddns.net`); replace for a local run (`https://localhost:6443`).

## Cargo (sparse registry)

`~/.cargo/config.toml`:

```toml
[registries.ennor]
index = "sparse+https://ennor.ddns.net/warehouse/index/"
credential-provider = "cargo:token"
```

```bash
cargo login --registry ennor      # paste:  Bearer <realm access token>
cargo publish --registry ennor
```

* `GET /index/config.json` returns `{"dl": "<REGISTRY_BASE_URL><base>/api/v1/crates/{crate}/{version}/download", "api": "<REGISTRY_BASE_URL><base>"}` and `"auth-required": true` when `CRATES_REQUIRE_AUTH_FOR_READS` is on. `REGISTRY_BASE_URL` defaults to `https://localhost`.
* Index files are at the crates.io layout: `1/a`, `2/ab`, `3/a/abc`, `ab/cd/abcd...` (names lower-cased). One JSON line per version: `name`, `vers`, `deps` (`name`, `req`, `features`, `optional`, `default_features`, `target`, `kind`, `registry`?, `package`?), `cksum` (SHA-256 of the tarball), `features`, `features2`?, `yanked`, `links`?, `rust_version`?, `v: 1`.
* Publish (`PUT /api/v1/crates/new`) carries Cargo's framing: `u32 LE` length of the metadata JSON, the JSON, `u32 LE` length of the `.crate`, the tarball. A version can be published once (`409`); there is no overwrite.
* Yank: `DELETE /api/v1/crates/{name}/{version}/yank`, unyank `PUT .../unyank`; both rewrite the index line's `yanked`.
* Owners (`GET|PUT|DELETE /api/v1/crates/{name}/owners`) are stored in `<name>/owners.json` and are informational.
* Search (`GET /api/v1/crates?q=`) scans the storage directory.

On disk (`CRATES_STORAGE_PATH`): `<name>/<version>/<name>-<version>.crate`, `<name>/owners.json`, `index/<prefix>/<name>`.

## Docker / OCI

```bash
docker login ennor.ddns.net
docker push ennor.ddns.net/forge/rust-builder:latest
```

Flow: `GET /v2/` (401 with `WWW-Authenticate: Bearer realm="https://ennor.ddns.net/token",service="warehouse"`), `GET /token?service=warehouse&scope=repository:forge/rust-builder:pull,push` with Basic credentials, then `POST /v2/<name>/blobs/uploads/` (202, `Location`), `PATCH` chunks and/or `PUT ...?digest=sha256:...` (201), `PUT /v2/<name>/manifests/<tag>` (201). Pulls use `GET`/`HEAD` on manifests and blobs (`Range` supported on blobs). `DELETE /v2/<name>/manifests/<digest>` removes a manifest and its tags; `POST /admin/docker/gc` removes blobs no manifest references.

Layout under `STORAGE_PATH`: `blobs/sha256/<hex>`, `manifests/sha256/<hex>`, `<name>/tags/<tag>` (file holding the manifest digest), `<name>/_uploads/<uuid>` (in-flight upload). Blobs and manifests are shared by all repositories; a blob GET ignores the repository in the path. Manifest media types stored and negotiated: Docker manifest v2, manifest list v2, OCI manifest v1, OCI index v1. Limits: `MAX_DOCKER_BLOB_BYTES` (default 32 GiB, enforced as bytes arrive, `413`), `MAX_CONCURRENT_UPLOADS` (default 32; `429`).

## Files

```bash
TOKEN=...   # a realm access token
curl -X PUT -H "Authorization: Bearer $TOKEN" --data-binary @report.pdf \
  "https://ennor.ddns.net/warehouse/api/v1/files/books/file?path=reports/report.pdf"
# 201 {"path":"reports/report.pdf","size":12345,"digest":"sha256:..."}
curl -H "Authorization: Bearer $TOKEN" "https://ennor.ddns.net/warehouse/api/v1/files/books?prefix=reports/&n=100"
```

Static storages come from `FILE_STORAGES=name=path;name=path`. Dynamic storages: create with `POST /api/v1/files {"name","owner","quota_bytes"?,"max_file_bytes"?,"sync_enabled"?}`. A dynamic upload is hashed, stored once under `<DYNAMIC_STORAGE_ROOT>/.blobs/<aa>/<bb>/<sha256>` (reference counted), and a row `storage_files(storage, path)` points at it. Quota accounting charges the full logical size even for a deduplicated upload. With `sync_enabled`, every put and delete is appended to `storage_sync_log`; `GET /api/v1/files/{storage}/sync?since=<id>` returns entries with `id > since`.

## Artifacts

```bash
# Android: identity comes from the APK
curl -X PUT -H "Authorization: Bearer $TOKEN" --data-binary @app.apk \
  https://ennor.ddns.net/warehouse/api/v1/artifacts/dev.lorehaven.pedlar/android/42
# Linux: format is required
curl -X PUT -H "Authorization: Bearer $TOKEN" --data-binary @pedlar.deb \
  "https://ennor.ddns.net/warehouse/api/v1/artifacts/dev.lorehaven.pedlar/linux/42?format=deb&arch=x86_64&version_name=1.4.2"
curl -OJ -H "Authorization: Bearer $TOKEN" \
  "https://ennor.ddns.net/warehouse/api/v1/artifacts/dev.lorehaven.pedlar/linux/latest/download?format=deb"
```

Identity: `(program, platform, format, version_code)` is unique; Android's `format` is always `apk`. A second publish of the same identity is `409`. A version in several formats needs `?format=` on metadata, download and (to touch one format) yank; without it metadata/download answer `409` listing the formats and yank/unyank apply to all formats. `latest` is the highest non-yanked `version_code` (within `format` if given). Files: `<ARTIFACT_STORAGE_PATH>/<program>/<platform>/<version_code>/<filename>`. The Android manifest decoder reads `package`, `versionCode`, `versionName`, `uses-sdk` min/target, `application` label (unless a resource reference) and `uses-permission` names; errors: `not a valid zip archive`, `no AndroidManifest.xml entry in the archive`, `AndroidManifest.xml could not be decoded: ...`, `manifest has no `package` attribute`, `manifest has no `versionCode` attribute, or it is not an integer` (all `422`).

## Rivets

```bash
riveter --env forge pack            # builds forge-0.4.0.rivet
curl -X PUT -H "Authorization: Bearer $TOKEN" --data-binary @forge-0.4.0+b123.rivet \
  "https://ennor.ddns.net/warehouse/api/v1/rivets/forge/0.4.0+b123"
curl -H "Authorization: Bearer $TOKEN" https://ennor.ddns.net/warehouse/api/v1/rivets/forge/latest
```

(The `+` in a URL path is literal; in a query string it would be a space.) The archive format is documented under `rivet-package` in [Libraries](../../libs/rivet-package/format.md). Publishing fully validates the archive and requires its manifest name and version to equal the path; `latest` is chosen by semver precedence among non-yanked versions (build metadata breaks ties as the semver library orders it, so `b9` sorts after `b10`). A yanked exact version still downloads. Files: `<RIVET_STORAGE_PATH>/<name>/<version>/<name>-<version>.rivet`; staging files `*.part.<pid>.<n>` are removed on failure. Table `rivet_packages` has no foreign key on `uploaded_by` because the usual publisher is a pipeline whose subject is an OAuth client id.
