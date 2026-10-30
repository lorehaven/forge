---
title: Data
parent: Switchboard
grand_parent: Services
nav_order: 3
---

Schema `switchboard`, owned by the foundry module `switchboard` 0.2.0 (requires `auth@^0.2`, `quench-core@^0.1`; default schema `switchboard`). One migration, `0001-model-cache` (since 0.2.0, author `switchboard-service`), written with the `createTable` change kind:

`switchboard.vllm_architectures`

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT NOT NULL | primary key; an architecture class name |

`switchboard.model_cache`

| Column | Type | Notes |
|---|---|---|
| `path` | TEXT NOT NULL | primary key; model directory or `.gguf` file |
| `data` | JSONB NOT NULL | the serialised `Model` (see [API](api.md)) |
| `updated_at` | TIMESTAMPTZ NOT NULL | |

The service accesses them through `quench-db` repositories (`switchboard.vllm_architectures`, `switchboard.model_cache` are hard-coded table names). Instances are not stored: native instances are derived from `/proc`, Kubernetes ones from the cluster. Redis holds only realm sessions. Files: the native vLLM logs under `VLLM_LOG_DIR`.

The service crate's own `service/migrations/` directory (00001 to 00003: users, model cache, auth sessions) is a leftover from before foundry; the service does not run it and the catalog does not contain its `users` and `sessions` tables.
