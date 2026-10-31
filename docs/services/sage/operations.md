---
title: Operations
parent: Sage
grand_parent: Services
nav_order: 4
---

## Run locally

`foreman start sage` (`foreman.toml`): starts Gatehouse, Switchboard and Sage (port 8443, `/sage`), runs foundry for `sage` and its dependencies, builds the browser module. The service environment gets `GATEHOUSE_CLIENT_SECRET` and `CLIENT_SECRET_SAGE_SWITCHBOARD`. With `FORGE_SKIP_MODELS` set in foreman's environment, `SAGE_DEFAULT_MODELS=[]` and no GPU is touched. The first screen is the initializing screen until every supported default model is `running`. See [Running the estate locally](../../guides/running-locally.md). `service/run-debug.sh` runs the binary with debug logging.

Never start vLLM or Switchboard's native engine for a documentation or test run: use `VLLM_MANAGEMENT_MODE=mock` on Switchboard, which makes launches instant fakes.

## Cluster

Image `sage` (`.anvil/docker.toml`: `RESOURCE_DIR=web`, `WASM_PACKAGE=sage-web`). Needs Postgres with the `sage`, `auth` and `pgvector` modules installed, Gatehouse and Switchboard reachable, `config/system_prompt.txt` (shipped in `config`). Readiness: `/health/ready` after Switchboard and Gatehouse answer `/health/ready`; then the monitor starts and the startup validation runs (its failure is only logged). Rolling updates: `SAGE_STOP_MODELS_ON_SHUTDOWN` stops only what this process launched. Deployment manifests are not in this repository.

## Failure modes

| Symptom | Cause |
|---|---|
| Panic `Missing required environment variables: GATEHOUSE_URL=..., CLIENT_SECRET_SAGE_SWITCHBOARD=...` | the Switchboard client cannot be built. |
| Stuck on the initializing screen | a supported default model is not `running` (the card shows Queued, Starting, Failed or Connecting); `GET /api/v1/models` shows each state. A model that is not supported by `SAGE_SUPPORTED_MODELS` is not waited for. |
| `failure` event `api_error_switchboard_unavailable` | Switchboard unreachable or its circuit breaker is open (5 failures, 60 s). |
| `failure` event `api_error_instance_not_found` | the chosen instance is gone; the client refreshes the picker. |
| `failure` event `api_error_embedding_model_chat` | an embedding instance was chosen. |
| `failure` with a vLLM error text | the vLLM call failed (its circuit breaker opens after 3 failures). |
| `404` on the stream | the pending turn is unknown: the service restarted between POST and GET, or the id was already used. |
| A file is `ready` but has no vectors | no running instance of `SAGE_EMBEDDING_MODEL` when it was processed (logged as a warning); start the model and `reprocess`. `failed` comes from extraction errors, a dimension mismatch or a database error. |
| `501 api_error_postgres_required` | an in-memory database (`ALLOW_IN_MEMORY_DB`). |
| Everything `401` | token audience is not `sage`, session inactive. |

Limits: 25 MiB per file, 50 files per scope, prompt budgets from the instance's `max_model_len`, grounding timeout 25 s. Pending turn requests are held in memory without expiry until their turn finishes.

## Tests

`cargo test -p sage-core -p sage-app -p sage-web -p sage`. Unit tests in `service/tests/unit/` (routers, grounding, clients, files, tools, startup), `core/tests/unit`, `app/tests/screens.rs`, `web/tests/unit.rs`. Integration tests need Postgres with the module installed and `SAGE_TEST_DATABASE_URL`. The notes `service/docs/TESTING_GUIDE.md` and `QUICK_TEST_REFERENCE.md` describe older manual test flows; their `/ui/chat/*` HTMX requests no longer exist.
