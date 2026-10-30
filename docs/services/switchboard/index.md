---
title: Switchboard
parent: Services
nav_order: 10
has_children: true
---

Switchboard is the only service of the estate that starts and stops vLLM. It discovers model files on the drive (HuggingFace directories and GGUF files), estimates how much GPU memory each would take at each quantisation and context length, and launches, lists and stops `vllm serve` instances through one of three engines: a local process (`native`), Kubernetes pods (`kubernetes`) or an in-memory stand-in (`mock`). It has a small browser client (registry and instance management). It does not proxy requests to the instances: clients (Sage) talk to the instance's own `host:port`.

## Crates

| Crate | Path | Version | Role |
|---|---|---|---|
| `switchboard-core` | `services/switchboard/core` | 0.1.0 | Wire model (`Gpu`, `Capabilities`, `Model`, `Estimate`, `Instance`, `ModelFilters`, `LaunchBody`), form drafts (`FilterDraft`, `LaunchDraft`) and the fit arithmetic (`fit.rs`). No I/O. |
| `switchboard-app` | `services/switchboard/app` | 0.1.0 | Screens and the `Request`/`Msg` protocol. |
| `switchboard-web` | `services/switchboard/web` | 0.1.0 | `wasm32` module and `Client::execute`. |
| `switchboard-service` | `services/switchboard/service` | 0.6.2 | The server and the engines. Binary `switchboard-service`. |

The foundry module `switchboard` 0.2.0 (schema `switchboard`) owns two tables.

## Facts

* Foreman: HTTPS 7443, `BASE_PATH=/switchboard`, `needs = ["gatehouse"]`; sage needs switchboard. A `pre_stop` hook asks Switchboard (as the `sage-switchboard` client) to stop its instances because they are not foreman's children.
* Dependencies: Postgres (required), Gatehouse, Redis for sessions (optional), the `vllm` executable on `PATH` (native), a Kubernetes API (kubernetes), `/sys/class/drm` with AMD `mem_info_vram_*` files for GPU memory.
* Authentication: `Auth` over `/api/v1/gpu`, `/api/v1/vllm`, `/api/v1/models`. Per-action grants `switchboard:launch`, `switchboard:stop`, `switchboard:delete-model`; `GET /api/v1/models/running` needs a wildcard role. See [Permissions](permissions.md).
* The registry and management screens are enabled by `FEATURE_MODELS_DASHBOARD_ENABLED` and `FEATURE_VLLM_MANAGEMENT_ENABLED` (both default off). They hide the client's entries; the API routes stay reachable.

## Pages

| Page | Content |
|---|---|
| [API](api.md), [openapi.yaml](openapi.yaml) | 11 API operations and 19 browser routes. |
| [Permissions](permissions.md) | Which route checks what. |
| [Launching](launching.md) | Engines, the launch request to vLLM arguments, ports, logs, stopping, restorer and reaper. |
| [Instances](instances.md) | Instance ids and the status values per engine. |
| [Models and GPU sizing](gpu-sizing.md) | Discovery, estimates, fit checks, GPU memory. |
| [Configuration](configuration.md) | Environment variables. |
| [Data](data.md) | Tables. |
| [Operations](operations.md) | Running, health, failure modes. |
| [Client](client.md) | Screens, requests, i18n keys. |

Shared behaviour: [Common service runtime](../common-runtime.md).
