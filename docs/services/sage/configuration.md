---
title: Configuration
parent: Sage
grand_parent: Services
nav_order: 2
---

Platform variables (`BASE_PATH`, `SERVER_*`, `SERVICE_NAME`, `SERVICE_AUTH_ENABLED`, `GATEHOUSE_*`, `AUTH_*`, `REDIS_URL`, `RUST_LOG`, `LOG_*`, `DATABASE_URL`, `ALLOW_IN_MEMORY_DB`, `DB_POOL_MAX_SIZE`) are in [Common service runtime](../common-runtime.md). Variables marked "SAGE_ or plain" are read through `ConfigLoader::new("SAGE")`: `SAGE_<X>` first, then `<X>`. The service has no config file besides the system prompt, and no `FEATURE_*` flags.

## Service and dependencies

| Variable | Default | Where | Meaning |
|---|---|---|---|
| `SWITCHBOARD_URL` | `http://switchboard-service:8080` | `clients/switchboard.rs`, `startup/mod.rs` | Switchboard origin including its base path. Readiness waits on `{SWITCHBOARD_URL}/health/ready`. |
| `GATEHOUSE_URL` | none | `clients/switchboard.rs` | Required here too: the token endpoint for the Switchboard client is `{GATEHOUSE_URL}/api/v1/token`. Startup panics if it or the secret is empty. |
| `CLIENT_SECRET_SAGE_SWITCHBOARD` | none | `clients/switchboard.rs` | Secret of the client `sage-switchboard` (client credentials). Required (panic). Names only. |
| `SWITCHBOARD_TLS_VERIFY` | `true` | `clients/switchboard.rs` | Verify Switchboard's and Gatehouse's certificates for that client. |
| `VLLM_TLS_VERIFY` | `true` | `clients/vllm.rs` | The vLLM calls use plain `http://` URLs; the flag configures the shared HTTP client. |
| `SKIP_SWITCHBOARD_CHECK` | unset | `startup/validate.rs` | `true` skips the connectivity check at start. |
| `DB_SCHEMA` | `sage` | `main.rs` and every query | `main` sets it to `sage` when unset. Interpolated into SQL unchecked. |
| `WEB_DIR` | `web` | `forge-web` | Directory of `app.wasm`. |

## Models and chat

SAGE_ or plain: see [Models](models.md) for the semantics.

| Variable | Default | Meaning |
|---|---|---|
| `SAGE_DEFAULT_MODELS` | `[{"name": "qwen2.5-coder:7b"}]` | Models to keep running. |
| `SAGE_SUPPORTED_MODELS` | `qwen*, *-instruct, llama*` | Glob patterns of models that are launched, waited for and offered. |
| `SAGE_STOP_MODELS_ON_SHUTDOWN` | `false` | Stop the default models this process launched on shutdown. |
| `SAGE_CAPABILITY_PROFILE` | `web_assistant` | `web_assistant`, `code_assistant`, `cli_agent`. |
| `SAGE_SYSTEM_PROMPT_PATH` | `config/system_prompt.txt` | System prompt file; a tool section is appended. |
| `SAGE_SEARCH_PROVIDER` / `SEARCH_PROVIDER` | `duckduckgo` | Default provider (see the note in Models). |
| `BRAVE_SEARCH_API_KEY` | none | Enables `brave`. (The startup check looks for `BRAVE_API_KEY` and warns when it is missing: see the report.) |
| `SERPAPI_API_KEY` | none | Enables `serpapi`. |
| `SEARXNG_INSTANCE_URL` | `https://searxng.be` | SearXNG endpoint. |
| `FILE_OPS_BASE_PATH` | `.` | Root of the `file_ops` tool. |

## Grounding

`SAGE_GROUNDED`, `SAGE_GROUNDED_MAX_QUERIES`, `SAGE_GROUNDED_FETCH_PAGES`, `SAGE_GROUNDED_PASSAGES`, `SAGE_GROUNDED_MAX_CHARS`, `SAGE_GROUNDED_TIMEOUT_SECS`, `SAGE_GROUNDED_VERIFY`, `SAGE_GROUNDED_VERIFIER_MODEL`, `SAGE_GROUNDED_VERIFY_TIMEOUT_SECS`, `SAGE_GROUNDED_CITE_REPAIR`, `SAGE_SEARCH_FALLBACK`: table in [Grounding](grounding.md). Read at each request, not cached.

## Files and retrieval

| Variable | Default | Meaning |
|---|---|---|
| `SAGE_FILE_MAX_SIZE_MB` | 25 | Upload limit in MiB. |
| `SAGE_MAX_FILES_PER_SCOPE` | 50 | Files per conversation or project. |
| `SAGE_CHUNK_SIZE_TOKENS` | 512 | Chunk size. |
| `SAGE_CHUNK_OVERLAP_TOKENS` | 50 | Overlap. |
| `SAGE_EMBEDDING_MODEL` | `Qwen/Qwen3-Embedding-0.6B` | Embedding model name as Switchboard lists it; empty disables embedding. |
| `SAGE_EMBEDDING_BATCH_SIZE` | 32 | Texts per embeddings call. |
| `SAGE_EMBEDDING_DIMENSION` | 1024 | Expected vector length; must equal the column's `vector(N)`. |
| `SAGE_EMBEDDING_QUERY_INSTRUCTION` | "Given a search query, retrieve relevant passages from the uploaded documents" | Wraps a query before embedding; empty embeds verbatim. |
| `SAGE_RAG_AUTO_INJECT` | `true` | Inject excerpts into the prompt. |
| `SAGE_RAG_TOP_K` | 4 | Chunks fetched. |
| `SAGE_RAG_SIMILARITY_THRESHOLD` | 0.35 | Minimum cosine similarity. |
| `SAGE_RAG_MAX_CONTEXT_CHARS` | 2000 | Excerpt budget. |
| `SAGE_IMAGES_ENABLED` | `true` | Accept and send images. Parsed as a boolean (`true`/`false`); other text means true. |
| `SAGE_MAX_IMAGES_PER_REQUEST` | 2 | Images per model request. |
| `SAGE_IMAGE_TOKEN_ESTIMATE` | 1024 | Prompt-budget cost per image. |

Numbers that fail to parse fall back to the default. Under foreman, `FORGE_SKIP_MODELS` in foreman's environment sets `SAGE_DEFAULT_MODELS=[]`.

## Secrets

`CLIENT_SECRET_SAGE_SWITCHBOARD`, `GATEHOUSE_CLIENT_SECRET`, `BRAVE_SEARCH_API_KEY`, `SERPAPI_API_KEY`. Names only.
