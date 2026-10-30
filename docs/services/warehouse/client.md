---
title: Client
parent: Warehouse
grand_parent: Services
nav_order: 5
---

The browser client is the `warehouse-web` wasm module (screens from `warehouse-app`, model from `warehouse-core`) served by `forge-web` from `web/app.wasm`. The app does no I/O: it asks its host for data and every change (`Request`) and is told what came (`Msg`); the host maps requests to the management API (`/ui/api/...`) with the session cookie.

## Crates

| Crate | Content |
|---|---|
| `warehouse-core` | the JSON model (`Overview`, `Features`, `DockerTag`, `ImageRef`, `CrateVersion`, `CrateDep`, `CrateRef`, `ArtifactEntry`, `ArtifactRef`, `RivetEntry`, `MetaPair`, `RivetRef`, `StaticStorage`, `DynamicStorage`, `Storages`, `NewStorage`, `StorageChanges`, `FileRef`, `Browse`, `BrowseFile`), form drafts (`NewStorageDraft`, `EditStorageDraft`, `DraftError`: GiB and MiB text to bytes), `fmt` helpers and the registry/file trees |
| `warehouse-app` | `warehouse(requests, setup) -> Warehouse { view, deliver }`, `Request`, `Msg`, `Setup { base, language, login_url, history }`, `LANGUAGES`, `STYLE`, the screens |
| `warehouse-web` | `app()`, `stylesheet()`, `Client<T: Transport>`, `base_of` |

## Screens

Routes (all `{base}/ui/...`, each also with a trailing slash): `home` (also `/ui` and `/ui/`), `docker/catalog`, `crates/catalog`, `files/storages`, `files/browse`, `artifacts/catalog`, `rivets/catalog`; anything else is "not found". The service serves the page for each of these paths to a signed-in browser (the `/ui/docker`, `/ui/crates`, `/ui/files`, `/ui/artifacts`, `/ui/apk`, `/ui/rivets` entry paths redirect permanently to them). The address keeps `?repo=`, `?tag=`, `?storage=`, `?path=` for the client.

| Screen | Shows | Management controls (need `can_manage`) |
|---|---|---|
| home | cards for the registries that are on (from `features`) | |
| docker catalog | repositories, tags with digest, media type, size | delete image (`POST /ui/api/docker/delete-image`; available to any session) |
| crates catalog | crates, versions oldest first with checksum, `rust_version`, features and dependencies | yank / unyank (any session) |
| storages | static storages (name, root) and dynamic ones (owner, quota, used, max file, sync, created) | create, edit (quota, max file, clear max file, sync), delete |
| browse | the files of a storage as a tree (or "denied" / "missing"), up to the listing cap | delete a file |
| artifacts catalog | programs, platforms, formats, versions, sha256, size, uploader, Android SDK range and permissions | yank / unyank a version (all formats) |
| rivets catalog | packages and versions in semver order, requirements, `[meta]`, digest, a `riveter install` line | yank / unyank |

The side panel lists home plus one item per enabled registry (icons `layers`, `box`, `folder`, `archive`, `database`); the top bar is `forge-shell`'s (brand Warehouse, icon `grid`, `dashboard: true`, `account: true`, languages `en-US de-DE es-ES fr-FR pl-PL`).

## Requests between app and host

| `Request` | Host call | Answer |
|---|---|---|
| `Startup` | `GET /ui/api/overview` | `Overview` |
| `DockerRepos`, `DockerTags(repo)`, `DeleteImage` | `GET /ui/api/docker/repositories`, `GET /ui/api/docker/tags?repo=`, `POST /ui/api/docker/delete-image` | `DockerRepos`, `DockerTags`, `ImageDeleted` |
| `Crates`, `CrateVersions(name)`, `SetCrateYank` | `GET /ui/api/crates`, `GET /ui/api/crates/{name}/versions`, `POST /ui/api/crates/yank|unyank` | `Crates`, `CrateVersions` |
| `Artifacts`, `SetArtifactYank` | `GET /ui/api/artifacts`, `POST /ui/api/artifacts/yank|unyank` | `Artifacts` |
| `Rivets`, `SetRivetYank` | `GET /ui/api/rivets`, `POST /ui/api/rivets/yank|unyank` | `Rivets` |
| `Storages`, `CreateStorage`, `EditStorage`, `DeleteStorage` | `GET /ui/api/files/storages`, `POST .../storages`, `POST .../storages/{name}/edit`, `POST .../storages/{name}/delete` | `Storages`, `StorageSaved`, `StorageDeleted` |
| `Browse(storage)`, `DeleteFile` | `GET /ui/api/files/browse?storage=`, `POST /ui/api/files/delete-file` | `Browse`, `FileDeleted` |
| `Language(code)` | none | none |

Failure handling: `401` is `SignedOut` (the app shows a sign-in link to `login_url`, a full page load of `/ui/login`); a read that returns `403` becomes `Failed("this account may not see Warehouse: ask for access in gatehouse")`; a change that fails becomes `Rejected(<the service's error key or "the service answered N">)`, translated when it is an `api_error_*` key; a transport error is `Failed("cannot reach the service: ...")`. Names are percent-encoded per path segment.

## Internationalisation and styling

Translations: `service/i18n/<locale>.ftl` for `en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL`, compiled into the module. Keys include `ui_nav_docker`, `ui_nav_crates`, `ui_nav_files`, `ui_nav_artifacts`, `ui_nav_rivets`, `ui_home_button` and the `api_error_*` keys listed in the [API](api.md#ui-api) (`api_error_forbidden`, `api_error_internal`, `api_error_invalid_repository`, `api_error_digest_required`, `api_error_invalid_digest`, `api_error_manifest_unknown`, `api_error_crate_version_not_found`, `api_error_artifacts_disabled`, `api_error_rivets_disabled`, `api_error_files_disabled`, `api_error_invalid_storage_name`, `api_error_storage_owner_required`, `api_error_storage_name_static_clash`, `api_error_invalid_quota`, `api_error_invalid_max_file`, `api_error_storage_exists`, `api_error_storage_owner_unknown`, `api_error_storage_not_found`).

Styling: the app's own classes use the prefix `wh-` (`wh-main`, `wh-content`, `wh-current`, `wh-page`, `wh-section`, `wh-panel`, `wh-split`, `wh-tree`, `wh-tags`, `wh-link`, `wh-tag`, `wh-badge`, `wh-ok`, `wh-warn`, `wh-meta-list`, `wh-btn`, `wh-primary`, `wh-danger`, `wh-form`, ...; full list in `warehouse-app/src/style.css`) on top of `forge-shell`'s `fui-*` vocabulary (see `forge-shell` in [Libraries](../../libs/index.md)). The side panel items carry `data-nav` (`home`, `docker`, `crates`, `files`, `artifacts`, `rivets`).

The page is served by `forge-web` with `Cache-Control: no-store`, a per-response script nonce and a strict content security policy; the assets (`/ui/assets/quench.js`, `app.css`, `shell.js`, `favicon.png`, `app.wasm`) are anonymous.
