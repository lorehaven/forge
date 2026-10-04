# Sage Service

Sage is Forge's AI workspace/chat service: a `quench-http` application that gives users a browser-based chat UI (and a matching JSON API) backed by vLLM models served through [Switchboard](./switchboard-service.md). It manages conversations organized into projects, supports tool-calling (web search, calculator, file operations, shell commands, code execution), and lets users upload files that are extracted, chunked, embedded into pgvector, and retrieved as RAG context to ground chat answers. Sage owns the "keep my configured models warm" responsibility too: at startup and on a 10s loop it asks Switchboard to launch whatever models are declared in `SAGE_DEFAULT_MODELS`, and it blocks its own home page behind an "initializing" screen until they're all running.

## Features

- **Conversation tree chat** — conversations are a tree of messages (`parent_id`/`active_message_id`), not a flat log, so branching/regenerating a reply is a first-class operation. Chat responses stream over SSE (`text/event-stream`), both from the JSON API (`POST /api/v1/chat`) and the HTMX-driven UI.
- **Projects** — conversations can be grouped under a project; files uploaded to a project are visible to every conversation in it, not just one.
- **File upload + RAG** — users attach files to a conversation or project; non-image files are extracted to text, chunked, embedded (via a running embedding-task vLLM instance), and stored in `pgvector`. Chat automatically augments the system prompt with a file list plus the top-K most relevant chunks for the current message (cosine similarity, `SAGE_RAG_*` env vars tune top-k/threshold/auto-inject). A `file_search` tool also lets the model query files on demand instead of relying solely on auto-injection.
- **Image attachments** — images (png/jpg/jpeg/webp/gif) skip the text/RAG pipeline entirely and are sent to vision models as message content parts instead (bounded by `SAGE_MAX_IMAGES_PER_REQUEST`). The whole feature is gated by `SAGE_IMAGES_ENABLED` (default true): set it to false when `SAGE_DEFAULT_MODELS` points at a text-only model — uploads are rejected as an unsupported type, the composer's file picker (`upload_accept_attribute`) stops offering image extensions, and `cap_images` strips images from every outgoing request regardless of conversation history.
- **Tool calling** — the model's output is scanned for `<tool_call>`/`<toolcall>` XML-wrapped JSON (Qwen-style; two JSON shapes are accepted) and dispatched through a `ToolRegistry`. Tools: `web_search` (DuckDuckGo/Brave/SearXNG/SerpAPI providers), `web_fetch`, `calculator`, `file_ops`, `file_search`, `file_list`, `command` (shell), `code_executor`. Registry enforces per-tool timeouts, retry-with-backoff on transient errors, rate limiting, and audit logging; some tools (`command`, `file_ops`) require explicit confirmation.
- **Capability profiles** — `web_assistant`, `code_assistant`, `cli_agent` profiles gate which tools are enabled and their timeouts (`SAGE_CAPABILITY_PROFILE`), so a deployment can offer web-search-only vs. full shell access.
- **Default-model lifecycle management** — a background monitor task launches configured default models one at a time (serialized so they don't contend for GPU memory), retries up to 3 times per model, and (optionally, `SAGE_STOP_MODELS_ON_SHUTDOWN`) gracefully stops the ones it itself launched on shutdown — careful not to kill an instance a newer rolling-update replica already relaunched.
- **Observability** — per-profile/per-tool metrics (`/api/v1/chat/metrics`), per-user/per-profile cost tracking (`/api/v1/chat/costs`), and audit logging of tool executions and restricted-tool attempts.

## Architecture

- **`routers/chat.rs` / `routers/ui/chat.rs`** — JSON chat API and the HTMX chat UI; both stream tokens from vLLM (via Switchboard-discovered instances) as SSE.
- **`routers/files.rs` / `routers/ui/pages/files.rs`** — file upload (multipart, 25 MB default cap, 50 files/scope default cap), download, reprocessing, chunk listing, and deletion. Both JSON API and UI share `create_uploaded_file`.
- **`files/`** — the extraction → chunking → embedding pipeline:
  - `extractor.rs`: MIME-aware text extraction (PDF via `pdf-extract`, CSV, Markdown via `pulldown-cmark`, HTML via `scraper`, plain text/code by extension) into metadata-tagged `Segment`s (heading, page, language).
  - `chunker.rs`: splits segments into ~512-token chunks (`SAGE_CHUNK_SIZE_TOKENS`) with configurable overlap, preferring paragraph boundaries.
  - `embedder.rs`: finds a running vLLM instance serving the configured embedding model (`SAGE_EMBEDDING_MODEL`, task `embed`) and calls its `/v1/embeddings` endpoint in batches, storing vectors as pgvector literals.
  - `rag.rs`: cosine-similarity search over `file_chunks` scoped to a conversation/project, and system-prompt augmentation with relevant excerpts plus source attribution (`rag_contexts` table).
  - `pipeline.rs`: orchestrates the above per-file, spawned as a background tokio task after upload; tracks file status (`uploaded` → `processing` → `ready`/`failed`).
- **`tools/`** — `ToolRegistry`, `CapabilityProfile`s, per-tool executors, and `parser.rs` (extracts/strips tool-call JSON from model output, tolerant of malformed closing tags and nested braces).
- **`clients/`** — `SwitchboardClient` (OAuth client-credentials call to Switchboard's `/api/v1/vllm/instances` etc., with retry + circuit breaker) and `VllmClient` (talks directly to a vLLM instance's OpenAI-compatible endpoints for chat streaming and embeddings).
- **`startup/`** — `state.rs` builds `AppState` (its pieces handed individually to the DI container in `main.rs`); `default_models.rs` is the model-launch monitor described above; `validate.rs` checks Switchboard connectivity and search-provider config at boot.
- **`domain/`** — `Conversation`, `Project`, `File`, `FileChunk` models plus a `TokenCounter` used by the chunker/context builder.
- **`observability/`** — metrics collector, cost tracker, audit logger.

## API routes / UI pages

All routes below sit under the service's base path (`/sage` in local dev). JSON API (`Auth` + `RequireWrite` on all writes):

- `POST /api/v1/chat` — streamed chat completion (SSE) against a chosen vLLM instance.
- `GET /api/v1/chat/capabilities`, `/metrics`, `/metrics/{profile}`, `/costs`, `/costs/user/{user_id}`, `/context-status/{profile}`.
- `POST /api/v1/files`, `GET /api/v1/files`, `GET /api/v1/files/{id}`, `GET /api/v1/files/{id}/download`, `POST /api/v1/files/{id}/reprocess`, `GET /api/v1/files/{id}/chunks`, `DELETE /api/v1/files/{id}`.

UI (`/ui`, HTMX-rendered, auth via Gatehouse SSO delegation — Sage itself has no login form):

- `/ui/home` — chat UI (conversation tree, composer, file chips).
- `/ui/initializing` — shown until every configured default model reaches `running`; polls status and redirects back to home.
- `/ui/projects/*`, `/ui/files/*`, `/ui/chat/*` — project management, file upload/attachment, and message-send fragments.
- `/ui/login`, `/ui/auth/callback`, `/ui/logout`, `/ui/status` — delegate to Gatehouse; no local credential handling.

## Requirements

- **PostgreSQL with the `vector` extension (pgvector)** — required for conversations, projects, files (`file_blobs` stores uploads as `BYTEA`), `file_chunks` (with an HNSW cosine index), and `rag_contexts`. Non-Postgres (`Db::InMemory`) mode exists for tests but returns "not implemented" for anything file/RAG-related.
- **Switchboard** — Sage has no model-serving code of its own; it discovers and calls vLLM instances entirely through Switchboard's API, authenticating via OAuth client-credentials against Gatehouse.
- **A running embedding-task vLLM instance** — needed for file processing and RAG search to actually produce vectors; if none is running, chunks are stored without embeddings and a later reprocess can fill them in.
- **Gatehouse** — JWT/session auth and SSO delegation for both the API and UI.

## Configuration

Key environment variables (see `docker/sage-service/.env`):

- `SWITCHBOARD_URL`, `CLIENT_SECRET_SAGE_SWITCHBOARD`, `GATEHOUSE_URL` — service-to-service auth toward Switchboard.
- `SAGE_CAPABILITY_PROFILE` — `web_assistant` (default), `code_assistant`, or `cli_agent`.
- `SAGE_DEFAULT_MODELS` — JSON array of models to keep launched (name, GPU utilization, context length, quantization, dtype, tool-calling, task, and `device`); the monitor loop reconciles against this on every tick. Set `"device": "cpu"` on an entry to have switchboard launch that model on CPU (`vllm serve --device cpu`) instead of the GPU — useful for keeping a small chat model resident without spending VRAM; requires a CPU-capable vLLM and rules out FP8 checkpoints.
- `SAGE_SUPPORTED_MODELS` — glob patterns a model name must match to be eligible for default-model launch.
- `SAGE_STOP_MODELS_ON_SHUTDOWN` — gracefully stop owned default models on shutdown (default false).
- `SAGE_EMBEDDING_MODEL`, `SAGE_EMBEDDING_DIMENSION`, `SAGE_EMBEDDING_BATCH_SIZE` — must match the `file_chunks.embedding vector(N)` column dimension.
- `SAGE_FILE_MAX_SIZE_MB` (25), `SAGE_MAX_FILES_PER_SCOPE` (50), `SAGE_CHUNK_SIZE_TOKENS` (512), `SAGE_CHUNK_OVERLAP_TOKENS` (50).
- `SAGE_RAG_AUTO_INJECT`, `SAGE_RAG_TOP_K`, `SAGE_RAG_SIMILARITY_THRESHOLD`, `SAGE_RAG_MAX_CONTEXT_CHARS` — tune automatic context injection.
- `SAGE_GROUNDED` — default `false`. When `true` (and the active profile has `web_search`), Sage retrieves in code instead of leaving it to the model: a constrained-JSON planner turns the latest message into up to `SAGE_GROUNDED_MAX_QUERIES` (3) search queries or decides none is needed, the search runs, the top `SAGE_GROUNDED_FETCH_PAGES` (3) pages are fetched, the best `SAGE_GROUNDED_PASSAGES` (6) passages (capped at `SAGE_GROUNDED_MAX_CHARS`, 6000) are picked by embedding similarity, and they are injected into the system prompt as numbered `[n]` sources the model must answer from and cite. The whole step is bounded by `SAGE_GROUNDED_TIMEOUT_SECS` (25); on failure the prompt tells the model to say it found no sources rather than answer from memory. Native tool calls are not sent to vLLM in this mode. Aimed at small models; see `src/grounding/mod.rs`.
- Web search in grounded mode falls back across providers: if the chosen one errors or returns nothing usable, it tries the providers in `SAGE_SEARCH_FALLBACK` (default `searxng,duckduckgo`; only those that are registered). Metered providers (`brave`, `serpapi`) are never used as a silent fallback: choose them as the provider, or add them to `SAGE_SEARCH_FALLBACK` deliberately. DuckDuckGo's HTML endpoint is scraped and throttles automated traffic, so prefer a self-hosted SearXNG (`SEARCH_PROVIDER=searxng`, `SEARXNG_INSTANCE_URL`; the homecloud `forge/searxng` overlay deploys one) or a Brave key.
- `SAGE_GROUNDED_VERIFY` — default `true` (grounded mode only). After the answer is written, the model fact-checks it against the sources with a constrained-JSON call; unsupported or contradicted claims are listed in a "⚠" note appended to the answer, and `[n]` markers that point at no source are removed. `SAGE_GROUNDED_VERIFIER_MODEL` (default empty = the answering model) runs the check on another running model, e.g. a different family; `SAGE_GROUNDED_VERIFY_TIMEOUT_SECS` (20) bounds it, and a failed or slow check leaves the answer untouched.
- Grounded answers are hardened against hostile pages: each source is passed as a quoted `<<<SOURCE n …>>> … <<<END n>>>` block (look-alike markers inside page text are neutralized) with explicit rules that source text is data, never instructions, and that the reply language follows the user's message; links in the answer that appear in neither the sources nor the question are replaced with `[link removed]`. The fact-check labels each flagged claim `contradicted`, `core` or `detail` and only the first two produce a ⚠ note, so harmless extra background no longer triggers warnings. If an answer states something but cites no source, one extra call (`SAGE_GROUNDED_CITE_REPAIR`, default `true`, bounded by `SAGE_GROUNDED_VERIFY_TIMEOUT_SECS`) rewrites it with citations; refusals are left alone and a failed repair keeps the original.
- **No sources.** When the search finds nothing usable (or times out), Sage does not refuse: the model answers briefly from its own knowledge, told not to state contact details, quotes, exact statistics or links, and the answer is stamped with a notice ("ℹ No sources were found for this question. This answer is generated from the model's own knowledge and has not been verified.") in the language of the question (en/pl/de/es/fr). The API reports it as `"unsourced": true`. Questions that need no search (small talk, rewriting, code, arithmetic, summaries of pasted text) are ordinary generation and are not stamped.
- **Refusals don't hand over someone else's data.** When an answer says it could not find something, sentences with an email address or phone number are removed, and (if the answer opens with the refusal) so are sentences with numbers of three or more digits that the question did not contain. Answers that are not refusals are never touched.
- `POST /api/v1/chat` also takes optional `history` (`[{"role","content"}]`, oldest first), `search_provider` (`duckduckgo`, `searxng`, `brave`, `serpapi`) and `evidence` (`[{"url","text"}]`: fixed source passages; planning, search and fetching are skipped so models can be compared on identical evidence; an empty list means "there are no sources"). In grounded mode it returns the whole answer in one event, then `{"grounded": true, "sources": [...], "search_failed": bool, "timings": {"retrieve_s","generate_s","verify_s"}}`. `evals/run_eval.py` uses it; see `evals/README.md`.
- `SAGE_IMAGES_ENABLED` — master switch for image attachments (default true); see Features above.
- `SAGE_MAX_IMAGES_PER_REQUEST`, `SAGE_IMAGE_TOKEN_ESTIMATE` — vision-model attachment limits.
- `SEARCH_PROVIDER`, `BRAVE_SEARCH_API_KEY`, `SERPAPI_API_KEY`, `SEARXNG_INSTANCE_URL` — web search tool provider selection.
- `DATABASE_URL`, `DB_SCHEMA` (`sage`), `DB_POOL_MAX_SIZE`, `DB_MIGRATION_TABLE`, `DB_RECREATE`.

In local dev (`foreman.toml`), Sage runs on port 8443 under base path `/sage`, and declares `needs = ["gatehouse", "switchboard"]` — foreman starts Gatehouse first (it owns the auth realm), then Switchboard, then Sage (which checks Switchboard's health at startup). `FORGE_SKIP_MODELS=1` sets `SAGE_DEFAULT_MODELS=[]` to skip GPU-hungry model launches when just bringing the estate up.

## Testing

Unit tests live under `tests/unit/`, one file per module (`tools_parser_tests.rs`, `files_chunker_tests.rs`, `files_embedder_tests.rs`, `clients_vllm_tests.rs`, `startup_default_models_tests.rs`, etc.), aggregated through `tests/unit.rs`. Run the suite the same way as the rest of the workspace, e.g. `anvil test -p sage-service` or `foreman test sage`.

[Home](../README.md)
