---
title: Files and retrieval
parent: Sage
grand_parent: Services
nav_order: 8
---

Sources: `service/src/routers/files.rs`, `service/src/files/*`. A file belongs to exactly one scope, a conversation or a project, and to its owner.

## Upload

`POST /api/v1/files` (multipart: `file`, and exactly one of `conversation_id`, `project_id`) and `POST /api/v1/files/staged` (the composer's variant: conversation scope, creates a missing conversation with a blank title). Rules, in the order the code checks them:

1. Scope: exactly one of conversation or project, else `400` `api_error_file_scope_required` (staged: missing `conversation_id` is `400` `api_error_missing_conversation_id`).
2. A file name is required (`api_error_missing_file_name`) and its lower-cased extension must be in the allowed list (`api_error_unsupported_file_type`); the stored MIME type comes from the extension, not from the client.
3. Size: more than `SAGE_FILE_MAX_SIZE_MB` (25) MiB is `413`; zero bytes is `400` `api_error_file_empty`.
4. The target conversation or project must exist and belong to the caller (`404` / `403`).
5. At most `SAGE_MAX_FILES_PER_SCOPE` (50) files for the scope (`422` `api_error_file_limit_reached`). The count is `WHERE conversation_id = $1 OR project_id = $2` over the `files` table.
6. The row and its bytes are stored in one transaction (`files`, `file_blobs`); an in-memory database answers `501` `api_error_postgres_required`.
7. Images are `ready` at once. Everything else starts as `uploaded` and a background task runs the pipeline.

Allowed extensions (`ALLOWED_UPLOAD_TYPES`): images `png`, `jpg`, `jpeg`, `webp`, `gif` (only while `SAGE_IMAGES_ENABLED` is true); documents `pdf`, `txt`, `log`, `ini`, `conf`, `cfg`, `csv`, `md`, `html`, `htm`; data `json`, `yaml`, `yml`, `toml`, `xml`; source `rs`, `py`, `js`, `mjs`, `jsx`, `ts`, `tsx`, `java`, `kt`, `kts`, `go`, `c`, `h`, `cpp`, `cc`, `cxx`, `hpp`, `cs`, `rb`, `php`, `swift`, `sh`, `bash`, `zsh`, `fish`, `sql`, `css`, `scss`. `GET /api/v1/models` returns the same list as `accept`.

## Pipeline

`process_file` (spawned after an upload and by `reprocess`): status `processing`; load the bytes; extract segments by MIME type; chunk; delete old chunks; insert chunks; embed; status `ready`. Any error sets status `failed` with `error_message`. Status values: `uploaded`, `processing`, `ready`, `failed`.

* Extraction (`extractor.rs`): `application/pdf` (per page), `text/csv`, `text/markdown` (by heading), `text/html` (text of the page), and plain text for every `text/*` type and `application/json|yaml|toml|xml|sql`; source files carry a `language` label. Empty segments are dropped; no text at all fails with "No text content could be extracted from the file". Other MIME types fail with `Unsupported MIME type for extraction`.
* Chunking: segments are split into chunks of about `SAGE_CHUNK_SIZE_TOKENS` (512) tokens with `SAGE_CHUNK_OVERLAP_TOKENS` (50) overlap, preferring paragraph boundaries; a chunk never spans two segments and keeps its segment's metadata (`heading`, `page`, `language`).
* Embedding: the instance must be one that Switchboard lists with `model == SAGE_EMBEDDING_MODEL` (default `Qwen/Qwen3-Embedding-0.6B`) and status exactly `running`; batches of `SAGE_EMBEDDING_BATCH_SIZE` (32) go to `POST http://<host>:<port>/v1/embeddings`. Every vector must have `SAGE_EMBEDDING_DIMENSION` (1024) entries or the file fails with `embedding dimension mismatch`; the column is `vector(1024)` (foundry variable `embedding_dim`). With an empty `SAGE_EMBEDDING_MODEL`, no running instance or an in-memory database the chunks are stored without vectors (a warning) and the file is still `ready`; `reprocess` fills them later. Keep the embedding model in `SAGE_DEFAULT_MODELS` with `task: embed`.
* `POST /api/v1/files/{id}/reprocess`: `409` while processing, `422` for images; otherwise starts again and answers `202`.
* Deleting a file removes its blob and chunks (cascade) and detaches its rag context rows (`ON DELETE SET NULL`).

## Visibility

A conversation sees its own files plus, when it belongs to a project, the project's files and the files of every conversation of that project (`visible_files_for_conversation`). A project lists only its own files. `GET /api/v1/files?conversation_id=` returns the first set. Staged files (`message_id` null) become attached to a message when the turn is sent with `file_ids`; a message's attachments are returned in `GET /api/v1/conversations/{id}`.

## Retrieval

* Auto-inject (`SAGE_RAG_AUTO_INJECT`, default true): for each turn, if the conversation can see any ready non-image file, the system prompt gets a section "UPLOADED FILES" listing the names and telling the model to use `file_search`; then the message is embedded (wrapped in `SAGE_EMBEDDING_QUERY_INSTRUCTION`, default "Given a search query, retrieve relevant passages from the uploaded documents"; an empty value embeds the query as is) and the `SAGE_RAG_TOP_K` (4) nearest chunks (cosine, `1 - (embedding <=> query)`, only `ready` files, only chunks with vectors) with similarity at least `SAGE_RAG_SIMILARITY_THRESHOLD` (0.35) are appended under "RELEVANT FILE EXCERPTS" as `[file · detail · similarity 0.81]` plus the text, until `SAGE_RAG_MAX_CONTEXT_CHARS` (2000) are used. The chunks used are stored in `rag_contexts` (source `auto`) so the answer shows its sources; a search failure leaves the list alone.
* The tools `file_search` (semantic search, source `tool`) and `file_list` give the model the same on demand ([Tools](tools.md)).

## Images

With `SAGE_IMAGES_ENABLED` (default true) images are accepted and sent to vision models as content parts of the message they are attached to (as `data:` URIs), also in later turns of the conversation. At most `SAGE_MAX_IMAGES_PER_REQUEST` (2) images are sent per request, newest first, across the current message and the history; this must not exceed the instance's `--limit-mm-per-prompt` (`limit_mm` of the default model). Each image counts `SAGE_IMAGE_TOKEN_ESTIMATE` (1024) tokens against the prompt budget. With the switch off no image is accepted or sent. `download` serves images `inline`, other files as `attachment`.
