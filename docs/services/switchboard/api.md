---
title: API
parent: Switchboard
grand_parent: Services
nav_order: 1
---

The HTTP API of Switchboard 0.6.2, rendered from [openapi.yaml](openapi.yaml) (OpenAPI 3.1.0). vLLM instance manager and model registry. Everything under /api/v1/{gpu,vllm,models} is behind the realm Auth middleware (bearer or session cookie, audience switchboard); launch, stop and delete are additionally checked against the grants switchboard:launch, switchboard:stop and switchboard:delete-model. Error bodies are plain-text translation keys (api_error_*), not JSON.

Paths below are relative to the base path (`BASE_PATH`, server variable `base`).

## gpu

GPU memory.

### `GET /api/v1/gpu/status`

Video memory of the machine's GPU. Operation id `gpuStatus`.

Reads `/sys/class/drm/card<N>/device/mem_info_vram_{total,used}`; among several cards the one with the most memory wins (ties by name). Values are GiB rounded to two decimals.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`GpuInfo`](#gpuinfo) | Read from sysfs on every call. All zeros and an empty name when no card reports video memory (no error status). |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
{
  "name": "card1",
  "total_gb": 32.0,
  "used_gb": 1.2,
  "free_gb": 30.8
}
```

## vllm

vLLM instances and capabilities.

### `GET /api/v1/vllm/capabilities`

What is switched on and what the caller may do. Operation id `vllmCapabilities`.

The `can_*` fields are true when authentication is off, or when the token holds `switchboard:<action>` (service name from SERVICE_NAME) or a wildcard role. The caller needs no grant to ask.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Capabilities`](#capabilities) | Feature flags, the caller's launch/stop/delete grants and the namespace. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
{
  "models_dashboard": true,
  "vllm_management": true,
  "can_launch": true,
  "can_stop": true,
  "can_delete_model": false,
  "namespace": "native"
}
```

### `GET /api/v1/vllm/list`

List vLLM instances. Operation id `listInstancesCanonical`.

No grant is checked: any identity that passes Auth may list.

Authentication: bearer token or session cookie.

Permission: `any authenticated identity`.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`VllmInstance`](#vllminstance) | Instances as the active engine sees them (native: processes whose command line contains `vllm serve`, plus instances being terminated; kubernetes: managed pods; mock: in-memory). |
| 500 | text/plain | string | The engine failed to list. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
[
  {
    "id": "pid-48213",
    "namespace": "native",
    "model": "/mnt/dev/huggingface/hub/models--Qwen--Qwen3-8B/snapshots/abc",
    "host": "0.0.0.0",
    "port": 8000,
    "quantization": null,
    "dtype": null,
    "limit_mm_per_prompt": null,
    "max_model_len": 16384,
    "gpu_memory_utilization": 0.9,
    "enable_prefix_caching": true,
    "enable_tool_calling": false,
    "task": null,
    "device": null,
    "started_at": "2026-10-29T10:00:00.123Z",
    "status": "starting",
    "log_path": "dist/vllm_logs/20261029T100000-...-8000-pid-48213.log",
    "last_error": null
  }
]
```

Example `500`:

```text
api_error_vllm_list_failed
```

### `GET /api/v1/vllm/instances`

List vLLM instances (alias of /api/v1/vllm/list). Operation id `listInstances`.

No grant is checked: any identity that passes Auth may list.

Authentication: bearer token or session cookie.

Permission: `any authenticated identity`.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`VllmInstance`](#vllminstance) | Instances as the active engine sees them (native: processes whose command line contains `vllm serve`, plus instances being terminated; kubernetes: managed pods; mock: in-memory). |
| 500 | text/plain | string | The engine failed to list. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
[
  {
    "id": "pid-48213",
    "namespace": "native",
    "model": "/mnt/dev/huggingface/hub/models--Qwen--Qwen3-8B/snapshots/abc",
    "host": "0.0.0.0",
    "port": 8000,
    "quantization": null,
    "dtype": null,
    "limit_mm_per_prompt": null,
    "max_model_len": 16384,
    "gpu_memory_utilization": 0.9,
    "enable_prefix_caching": true,
    "enable_tool_calling": false,
    "task": null,
    "device": null,
    "started_at": "2026-10-29T10:00:00.123Z",
    "status": "starting",
    "log_path": "dist/vllm_logs/20261029T100000-...-8000-pid-48213.log",
    "last_error": null
  }
]
```

Example `500`:

```text
api_error_vllm_list_failed
```

### `POST /api/v1/vllm/instances`

Launch a vLLM instance. Operation id `launchInstance`.

The engine error text is logged, not returned. Native: the first free port at or above `port` is used (so the reported `port` can differ); Kubernetes: `host` in the request is ignored, the container binds 0.0.0.0 and the reported host is `<pod>.<namespace>.svc.cluster.local`. See launching.md.

Authentication: bearer token or session cookie.

Permission: `switchboard:launch`.

Request body (`application/json`), required: [`LaunchRequest`](#launchrequest)

| Field | Type | Required | Description |
|---|---|---|---|
| model | string | yes |  |
| host | string | yes |  |
| port | integer `uint16` | yes |  |
| namespace | string or null | no | Kubernetes namespace; default the engine's. Ignored natively. |
| quantization | string or null | no | Passed as --quantization. |
| dtype | string or null | no | Passed as --dtype. |
| limit_mm_per_prompt | string or null | no | Passed as --limit-mm-per-prompt, e.g. {"image": 4}. |
| max_num_seqs | integer or null | no | --max-num-seqs. Not offered by the browser form. |
| max_model_len | integer or null | no |  |
| gpu_memory_utilization | number or null | no | --gpu-memory-utilization, formatted with two decimals; not passed for device cpu. |
| enable_prefix_caching | boolean | yes | Required in the JSON body. |
| enable_tool_calling | boolean | no | --enable-auto-tool-choice --tool-call-parser hermes. Default `false`. |
| task | string or null | no | embed or embedding: --runner pooling --convert embed; classify: --runner pooling --convert classify; generate: --runner generate; anything else: --runner <value>. |
| device | string or null | no | `cpu` selects the CPU path; empty, gpu, auto and default pass nothing; any other value is passed as --device. |

```json
{
  "model": "/mnt/dev/huggingface/hub/models--Qwen--Qwen3-8B/snapshots/abc",
  "host": "0.0.0.0",
  "port": 8000,
  "max_model_len": 16384,
  "gpu_memory_utilization": 0.9,
  "enable_prefix_caching": true,
  "enable_tool_calling": false
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 202 | application/json | [`VllmInstance`](#vllminstance) | Accepted: the instance exists with status `starting`. |
| 400 | text/plain | string | Blank `model`. |
| 403 |  |  | The caller lacks `switchboard:launch`. Empty body. |
| 500 | text/plain | string | The engine failed (native: the process exited within 1.2 s, the port search failed or `vllm` could not be spawned; kubernetes: pod or service creation failed). |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `202`:

```json
{
  "id": "pid-48213",
  "namespace": "native",
  "model": "/mnt/dev/huggingface/hub/models--Qwen--Qwen3-8B/snapshots/abc",
  "host": "0.0.0.0",
  "port": 8000,
  "quantization": null,
  "dtype": null,
  "limit_mm_per_prompt": null,
  "max_model_len": 16384,
  "gpu_memory_utilization": 0.9,
  "enable_prefix_caching": true,
  "enable_tool_calling": false,
  "task": null,
  "device": null,
  "started_at": "2026-10-29T10:00:00.123Z",
  "status": "starting",
  "log_path": "dist/vllm_logs/20261029T100000-...-8000-pid-48213.log",
  "last_error": null
}
```

Example `400`:

```text
api_error_model_name_empty
```

Example `500`:

```text
api_error_vllm_launch_failed
```

### `POST /api/v1/vllm/launch`

Launch a vLLM instance from an HTML form. Operation id `launchInstanceForm`.

Legacy server-rendered form variant, kept in the code; the browser client uses POST /api/v1/vllm/instances. Form fields: model (required), host (default 0.0.0.0), port (default 8000), namespace, quantization, dtype, limit_mm_per_prompt, max_model_len, gpu_memory_utilization (default 0.90, none for device cpu), enable_prefix_caching or prefix_caching, enable_tool_calling, task, device. Blank strings are treated as absent. The model name is not checked for blank.

Authentication: bearer token or session cookie.

Permission: `switchboard:launch`.

Request body (`application/x-www-form-urlencoded`), required: [`LaunchForm`](#launchform)

| Field | Type | Required | Description |
|---|---|---|---|
| model | string | yes |  |
| host | string | no |  |
| port | string | no |  |
| namespace | string | no |  |
| quantization | string | no |  |
| dtype | string | no |  |
| limit_mm_per_prompt | string | no |  |
| max_model_len | string | no |  |
| gpu_memory_utilization | string | no |  |
| task | string | no |  |
| device | string | no |  |
| enable_prefix_caching | boolean | no |  |
| prefix_caching | boolean | no |  |
| enable_tool_calling | boolean | no |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 202 | text/html | string | HTML fragment `<div id="launch-modal" class="modal launch-modal"></div>`. |
| 403 |  |  | The caller lacks `switchboard:launch`. Empty body. |
| 500 | text/plain | string | Engine failure. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `500`:

```text
api_error_vllm_launch_failed
```

### `DELETE /api/v1/vllm/instances/{id}`

Stop a vLLM instance. Operation id `stopInstance`.

Native: sends SIGTERM (`kill -TERM <pid>`) and marks the record `terminating` for 15 seconds. Kubernetes: deletes the pod (its service goes with it). A failure whose text contains "not found" (any case) is a 404.

Authentication: bearer token or session cookie.

Permission: `switchboard:stop`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Native: `pid-<pid>`; Kubernetes: `pod:<namespace>:<name>` (or the older `pod-<name>`); mock: `mock-<millis>`. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | HTML fragment `<div id="confirm-stop-instance-modal" class="estimates-modal"></div>`. |
| 403 |  |  | The caller lacks `switchboard:stop`. Empty body. |
| 404 | text/plain | string | No such instance. |
| 500 | text/plain | string | Stopping failed. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `404`:

```text
api_error_instance_not_found
```

Example `500`:

```text
api_error_vllm_stop_failed
```

## models

Model registry.

### `POST /api/v1/models/list`

List the discovered models. Operation id `listModels`.

Every field is optional. `source` defaults to `hf`; `all` keeps every source. The GPU's current free memory is used by the `vram_*` sorts.

Authentication: bearer token or session cookie.

Permission: `any authenticated identity`.

Request body (`application/json`), required: [`ModelFilters`](#modelfilters)

| Field | Type | Required | Description |
|---|---|---|---|
| source | string | no | `hf` (default), `gguf` or `all`. Alias key `type`. Compared case-insensitively with the stored source (HF, GGUF). |
| search | string | no | Case-insensitive substring of the name. |
| sort | string (`name_asc`, `name_desc`, `params_asc`, `params_desc`, `vram_asc`, `vram_desc`) | no |  |
| quant | string | no | ALL (or empty) for all; otherwise a quantisation name (FP16, BF16, FP8, INT8, AWQ, GPTQ, Q80 or Q8_0, Q6K or Q6_K, ...). |
| context | integer or string | no | Minimum context: number or numeric string; 0 or absent for all. |
| vllm_only | boolean | no |  |

```json
{
  "source": "hf",
  "search": "qwen",
  "sort": "name_asc",
  "quant": "ALL",
  "context": "0",
  "vllm_only": false
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`Model`](#model) | Models from the cache (database), filtered and sorted in memory. Unknown sort values keep the stored order. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

### `GET /api/v1/models/running`

Models currently served by instances. Operation id `runningModels`.

Not a `switchboard:*` grant: the caller must hold a wildcard role, or authentication must be off. This is the route Sage polls with its client-credentials token. `endpoint` is `http://<host>:<port>` of the instance record.

Authentication: bearer token or session cookie.

Permission: `wildcard role (admin or service)`.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`RunningModel`](#runningmodel) | One entry per instance. |
| 403 |  |  | Not an admin. Empty body. |
| 500 | text/plain | string | Listing failed. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
[
  {
    "id": "pid-48213",
    "model": "Qwen/Qwen3-8B",
    "endpoint": "http://0.0.0.0:8000",
    "status": "running"
  }
]
```

Example `500`:

```text
api_error_vllm_list_failed
```

### `POST /api/v1/models/delete`

Delete a model from the drive. Operation id `deleteModel`.

Removes the directory (`remove_dir_all`) or file at `path`. The check is `Path::starts_with` against each root, component-wise; `..` segments in the path are not resolved.

Authentication: bearer token or session cookie.

Permission: `switchboard:delete-model`.

Request body (`application/json`), required: [`DeleteModelRequest`](#deletemodelrequest)

| Field | Type | Required | Description |
|---|---|---|---|
| path | string | yes | The model's `path` as listed. |

```json
{
  "path": "/mnt/dev/huggingface/hub/models--Qwen--Qwen3-8B"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 |  |  | Deleted from disk and from the cache. Empty body. |
| 403 | text/plain | string | Missing grant (empty body) or a path outside HF_ROOTS and GGUF_ROOTS (text `api_error_invalid_model_path`). |
| 404 | text/plain | string | The path does not exist. |
| 500 | text/plain | string | Removal failed. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `404`:

```text
api_error_model_not_found
```

Example `500`:

```text
api_error_model_delete_failed
```

### `POST /api/v1/models/delete-form`

Delete a model, form variant. Operation id `deleteModelForm`.

Legacy form variant (field `path`); same checks as POST /api/v1/models/delete.

Authentication: bearer token or session cookie.

Permission: `switchboard:delete-model`.

Request body (`application/x-www-form-urlencoded`), required: [`DeleteModelRequest`](#deletemodelrequest)

| Field | Type | Required | Description |
|---|---|---|---|
| path | string | yes | The model's `path` as listed. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | HTML fragment `<div id="confirm-delete-modal" class="estimates-modal"></div>` with header `HX-Trigger: models-refresh`. |
| 403 |  |  | The caller lacks `switchboard:delete-model`. Empty body. |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `404`:

```text
api_error_model_not_found
```

Example `500`:

```text
api_error_model_delete_failed
```

## ui

Browser pages, assets and SSO routes.

### `GET /ui`

Redirect to the home page, or to login without a session. Operation id `uiRoot`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/`

Same as /ui. Operation id `uiRootSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/home`

Client page, home screen. Operation id `uiHome`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/home/`

Same as /ui/home. Operation id `uiHomeSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/models/dashboard`

Client page, models registry. Operation id `uiModels`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/models/dashboard/`

Same as /ui/models/dashboard. Operation id `uiModelsSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/vllm/manage`

Client page, vLLM management. Operation id `uiVllm`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/vllm/manage/`

Same as /ui/vllm/manage. Operation id `uiVllmSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/assets/quench.js`

The quench-ui loader script. Operation id `uiAssetScript`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | The file (cache-control: no-cache). |

### `GET /ui/assets/app.css`

Shell stylesheet followed by the client stylesheet. Operation id `uiAssetStyle`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/css | string | The file (cache-control: no-cache). |

### `GET /ui/assets/shell.js`

The forge-shell script. Operation id `uiAssetShell`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | The file (cache-control: no-cache). |

### `GET /ui/assets/favicon.png`

The service icon. Operation id `uiAssetFavicon`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | image/png | string `binary` | The file (cache-control: no-cache). |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/assets/app.wasm`

The browser client module. Operation id `uiAssetModule`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/wasm | string `binary` | The file (cache-control: no-cache). |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/login`

Start sign-in (SSO redirect to Gatehouse). Operation id `uiLogin`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home (refresh cookie accepted, new cookies set) or to Gatehouse's /api/v1/authorize; sets the cookie forge_authorize_state. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/login/`

Same as /ui/login. Operation id `uiLoginSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | See /ui/login. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/auth/callback`

Complete the authorization-code exchange. Operation id `uiAuthCallback`.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| code | query | no | string |  |
| state | query | no | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home with session cookies, or to {base}/ui/login on any failure. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/logout`

Realm logout through Gatehouse. Operation id `uiLogout`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To Gatehouse's /ui/logout with a redirect back to {base}/ui/login. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/status`

Session status for the page shell. Operation id `uiStatus`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | Always 200. |

### `POST /ui/refresh`

Exchange the refresh cookie for new cookies. Operation id `uiRefresh`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | New session and refresh cookies set. |
| 401 |  |  | No refresh cookie, or Gatehouse refused it. Empty body. |

Example `200`:

```json
{
  "authenticated": true,
  "username": null,
  "roles": []
}
```

## Schemas

### GpuInfo

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes | Card directory name, e.g. card1. |
| total_gb | number | yes |  |
| used_gb | number | yes |  |
| free_gb | number | yes |  |

### Capabilities

| Field | Type | Required | Description |
|---|---|---|---|
| models_dashboard | boolean | yes | FEATURE_MODELS_DASHBOARD_ENABLED. |
| vllm_management | boolean | yes | FEATURE_VLLM_MANAGEMENT_ENABLED. |
| can_launch | boolean | yes |  |
| can_stop | boolean | yes |  |
| can_delete_model | boolean | yes |  |
| namespace | string | yes | `native`, `mock`, or the Kubernetes namespace (VLLM_K8S_NAMESPACE, else the pod's, else `default`). |

### VllmInstance

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| namespace | string | yes |  |
| model | string | yes | Path or name given to `vllm serve`. |
| host | string | yes |  |
| port | integer `uint16` | yes |  |
| quantization | string or null | yes |  |
| dtype | string or null | yes |  |
| limit_mm_per_prompt | string or null | yes |  |
| max_model_len | integer or null | yes |  |
| gpu_memory_utilization | number or null | yes |  |
| enable_prefix_caching | boolean | yes |  |
| enable_tool_calling | boolean | yes |  |
| task | string or null | yes |  |
| device | string or null | yes |  |
| started_at | string | yes | RFC 3339. |
| status | string | yes | Native: starting, running, terminating. Kubernetes: starting, running, terminating, completed, interrupted, or the raw pod phase (Failed, Unknown). Mock: running. |
| log_path | string or null | yes |  |
| last_error | string or null | yes |  |

### LaunchRequest

| Field | Type | Required | Description |
|---|---|---|---|
| model | string | yes |  |
| host | string | yes |  |
| port | integer `uint16` | yes |  |
| namespace | string or null | no | Kubernetes namespace; default the engine's. Ignored natively. |
| quantization | string or null | no | Passed as --quantization. |
| dtype | string or null | no | Passed as --dtype. |
| limit_mm_per_prompt | string or null | no | Passed as --limit-mm-per-prompt, e.g. {"image": 4}. |
| max_num_seqs | integer or null | no | --max-num-seqs. Not offered by the browser form. |
| max_model_len | integer or null | no |  |
| gpu_memory_utilization | number or null | no | --gpu-memory-utilization, formatted with two decimals; not passed for device cpu. |
| enable_prefix_caching | boolean | yes | Required in the JSON body. |
| enable_tool_calling | boolean | no | --enable-auto-tool-choice --tool-call-parser hermes. Default `false`. |
| task | string or null | no | embed or embedding: --runner pooling --convert embed; classify: --runner pooling --convert classify; generate: --runner generate; anything else: --runner <value>. |
| device | string or null | no | `cpu` selects the CPU path; empty, gpu, auto and default pass nothing; any other value is passed as --device. |

### LaunchForm

| Field | Type | Required | Description |
|---|---|---|---|
| model | string | yes |  |
| host | string | no |  |
| port | string | no |  |
| namespace | string | no |  |
| quantization | string | no |  |
| dtype | string | no |  |
| limit_mm_per_prompt | string | no |  |
| max_model_len | string | no |  |
| gpu_memory_utilization | string | no |  |
| task | string | no |  |
| device | string | no |  |
| enable_prefix_caching | boolean | no |  |
| prefix_caching | boolean | no |  |
| enable_tool_calling | boolean | no |  |

### ModelFilters

| Field | Type | Required | Description |
|---|---|---|---|
| source | string | no | `hf` (default), `gguf` or `all`. Alias key `type`. Compared case-insensitively with the stored source (HF, GGUF). |
| search | string | no | Case-insensitive substring of the name. |
| sort | string (`name_asc`, `name_desc`, `params_asc`, `params_desc`, `vram_asc`, `vram_desc`) | no |  |
| quant | string | no | ALL (or empty) for all; otherwise a quantisation name (FP16, BF16, FP8, INT8, AWQ, GPTQ, Q80 or Q8_0, Q6K or Q6_K, ...). |
| context | integer or string | no | Minimum context: number or numeric string; 0 or absent for all. |
| vllm_only | boolean | no |  |

### Model

| Field | Type | Required | Description |
|---|---|---|---|
| source | string (`HF`, `GGUF`) | yes |  |
| name | string | yes |  |
| path | string | yes |  |
| architecture | string or null | yes |  |
| vllm_supported | boolean | yes |  |
| quant | string (`ALL`, `FP16`, `BF16`, `FP8`, `INT8`, `Q80`, `Q6K`, `Q5KM`, `Q50`, `Q4KM`, `Q40`, `Q3KM`, `Q2K`, `AWQ`, `GPTQ`) | yes |  |
| context | integer (`512`, `1024`, `2048`, `4096`, `8192`, `16384`, `32768`, `65536`, `131072`) or string (`ALL`) | yes | A number from 512 to 131072 in powers of two, or the string "ALL" (the model's own maximum, treated as 131072 in comparisons). |
| layers | integer | yes |  |
| hidden_size | integer | yes |  |
| params_billion | number | yes |  |
| estimates | array of [`ModelEstimate`](#modelestimate) | yes |  |

### ModelEstimate

| Field | Type | Required | Description |
|---|---|---|---|
| quant | string (`ALL`, `FP16`, `BF16`, `FP8`, `INT8`, `Q80`, `Q6K`, `Q5KM`, `Q50`, `Q4KM`, `Q40`, `Q3KM`, `Q2K`, `AWQ`, `GPTQ`) | yes |  |
| context | integer (`512`, `1024`, `2048`, `4096`, `8192`, `16384`, `32768`, `65536`, `131072`) or string (`ALL`) | yes | A number from 512 to 131072 in powers of two, or the string "ALL" (the model's own maximum, treated as 131072 in comparisons). |
| weights_gb | number | yes |  |
| kv_gb | number | yes |  |
| total_gb | number | yes | (weights + kv + 3.0) * 1.15. |

### RunningModel

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| model | string | yes |  |
| endpoint | string | yes |  |
| status | string | yes |  |

### DeleteModelRequest

| Field | Type | Required | Description |
|---|---|---|---|
| path | string | yes | The model's `path` as listed. |

### Error

| Field | Type | Required | Description |
|---|---|---|---|
| error | string | yes |  |

### AuthStatus

| Field | Type | Required | Description |
|---|---|---|---|
| authenticated | boolean | yes |  |
| username | string or null | yes |  |
| roles | array of string | yes |  |

