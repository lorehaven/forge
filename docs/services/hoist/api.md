---
title: API
parent: Hoist
grand_parent: Services
nav_order: 1
---

The catalogue API of `hoist-service`, three read-only routes. The machine-readable description is [openapi.yaml](openapi.yaml). Routes are relative to `BASE_PATH` (`/hoist` in the estate, `/` when unset).

All routes need `Authorization: Bearer <access token>` from Gatehouse. Any valid identity is accepted; no permission is checked (`x-permission` is not used). Errors are `{"error": "<text>"}`.

## Tag: catalogue

### GET /v1/apps

Browse or search, a page at a time. Operation `list_apps`.

| Query | Type | Default | Meaning |
|---|---|---|---|
| `query` | string | none | Substring of label or package name, case-insensitive. `%`, `_` and `\` are literal. Blank is ignored. |
| `category` | string | none | Exact category. Empty is ignored. |
| `min_votes` | integer | none | `rating_count >= min_votes`. |
| `sort` | `downloads` \| `rating` \| `alpha` | `downloads` | `downloads DESC NULLS LAST, label`; `rating_avg DESC NULLS LAST, rating_count DESC NULLS LAST, label`; `LOWER(label), package_name`. |
| `offset` | integer | `0` | Negative is read as 0. |
| `limit` | integer | `30` | Clamped to 1..100. |

Response `200`: `AppList`, with the effective `offset` and `limit` echoed.

```json
{
  "total": 1, "offset": 0, "limit": 30,
  "items": [{
    "package_name": "org.example.app", "label": "Example", "summary": "An example",
    "description": null, "icon_url": null, "category": "Tools",
    "version_code": 12, "version_name": "1.2", "size_bytes": 1048576,
    "download_url": "https://f-droid.org/repo/org.example.app_12.apk",
    "sha256": "00...00", "md5": null, "install_source": "fdroid",
    "rating_avg": 4.3, "rating_count": 120, "downloads": 50000, "malware_rank": null
  }]
}
```

Errors: `400 invalid sort: <value>`; `401`; `500` (SQL error text); `503 the catalogue needs Postgres` (in-memory database).

### GET /v1/categories

Each category with its app count, biggest first (ties by lowercase name). Apps with a null or empty category are not counted. Operation `list_categories`.

Response `200`: `{"items": [{"name": "Tools", "count": 420}]}`. Errors: `401`, `500`, `503`.

### GET /v1/apps/{package_name}

One app by Android application id. Operation `get_app`. Response `200`: `App`. Errors: `401`; `404 not found`; `500`; `503`.

## Schemas

### App

| Field | Type | Notes |
|---|---|---|
| `package_name` | string | |
| `label` | string | |
| `summary`, `description`, `icon_url`, `category` | string or null | |
| `version_code` | integer | |
| `version_name` | string | |
| `size_bytes` | integer | |
| `download_url` | string | The store's own address; the file is not served here. |
| `sha256` | string or null | F-Droid and IzzyOnDroid builds; an Aptoide build may have none. |
| `md5` | string or null | |
| `install_source` | `fdroid` \| `izzyondroid` \| `aptoide` | Which source won the install slot. |
| `rating_avg` | number or null | Aptoide's. |
| `rating_count`, `downloads` | integer or null | Aptoide's. |
| `malware_rank` | string or null | Aptoide's verdict for the winning listing; null otherwise. |

All fields are always present (nulls are serialized, not omitted).

### AppList

`total` (integer, rows matching the filters), `offset`, `limit` (integers), `items` (array of `App`).

## Routes not in the API

Health probes (`/health/...`) are registered by `quench-starter`, not by this service, and are open. No `/ui` routes and no static files exist.

*Unverified:* the exact probe paths; see the Quench book (`quench-starter`).
