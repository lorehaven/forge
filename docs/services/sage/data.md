---
title: Data
parent: Sage
grand_parent: Services
nav_order: 3
---

Schema `sage`, owned by the foundry module `sage` 0.2.0 (requires `auth@^0.2`, `quench-core@^0.1`, `pgvector@^0.1`; variables `embedding_dim = "1024"` and `auth_schema = "auth"`; default schema `sage`). The service creates nothing at start. Migrations in order (both `since = "0.2.0"`, author `sage-service`): `0001-workspace`, `0002-files-and-rag`. The service accesses the tables by `DB_SCHEMA` + name through `quench-db` repositories and raw SQL. Redis holds only realm sessions. In memory: pending turn requests and the metrics, cost and rate-limit counters.

All timestamps are `TEXT` (RFC 3339), not `TIMESTAMPTZ`.

## Tables

`projects`: `id TEXT PK`, `name TEXT NOT NULL`, `owner TEXT NOT NULL REFERENCES auth.users(username) ON DELETE CASCADE`, `created_at`, `updated_at`.

`conversations`: `id TEXT PK`, `title TEXT NOT NULL`, `owner TEXT NOT NULL` (no foreign key), `active_message_id TEXT` (no foreign key: the tip), `project_id TEXT REFERENCES projects ON DELETE CASCADE`, `updated_at`.

`messages`: `id TEXT PK`, `conversation_id TEXT NOT NULL REFERENCES conversations ON DELETE CASCADE`, `parent_id TEXT REFERENCES messages ON DELETE CASCADE`, `role TEXT NOT NULL` (`user` or `assistant`), `content TEXT NOT NULL`, `created_at`.

`files`: `id TEXT PK`, `owner TEXT NOT NULL REFERENCES auth.users ON DELETE CASCADE`, `file_name`, `mime_type`, `file_size BIGINT`, `conversation_id` (FK conversations, cascade), `project_id` (FK projects, cascade), `message_id` (FK messages, cascade), `status TEXT NOT NULL`, `error_message TEXT`, `created_at`, `updated_at`; CHECK `num_nonnulls(conversation_id, project_id) = 1`. Indexes on `conversation_id`, `project_id`, `owner`, `message_id`.

`file_blobs`: `file_id TEXT PK REFERENCES files ON DELETE CASCADE`, `data BYTEA NOT NULL`.

`file_chunks`: `id TEXT PK`, `file_id TEXT NOT NULL REFERENCES files ON DELETE CASCADE`, `chunk_index INTEGER NOT NULL`, `content TEXT NOT NULL`, `embedding vector(1024)`, `embedding_model TEXT`, `metadata JSONB`, `created_at`; UNIQUE `(file_id, chunk_index)`; index on `file_id`; HNSW index `file_chunks_embedding_idx ... USING hnsw (embedding vector_cosine_ops)`.

`rag_contexts`: `id TEXT PK`, `message_id TEXT NOT NULL REFERENCES messages ON DELETE CASCADE`, `file_id TEXT REFERENCES files ON DELETE SET NULL`, `file_name TEXT NOT NULL`, `chunk_index INTEGER`, `detail TEXT`, `similarity DOUBLE PRECISION`, `source TEXT NOT NULL` (`auto` injected chunk, `tool` from `file_search`, `web` a grounded web source with `file_id` null, `file_name` = `[n] domain`, `detail` = URL), `created_at`; index on `message_id`.

## The message tree

A conversation's messages form a tree through `parent_id`. `conversations.active_message_id` is the tip of the branch on screen. A new message is a child of the tip. An alternative (regenerate, edit) is another child of the same parent, so a message's `siblings` are the children of its parent in creation order. `GET /api/v1/conversations/{id}` walks from the tip to the root with a recursive query. Switching to a sibling moves the tip to the newest descendant reached by always taking the newest child. The history sent to the model is the same walk.

## Messages and files

A file staged in the composer has `message_id` null; the turn links the ids in `file_ids` (owned by the user, in the conversation) to the new user message. Images of earlier messages are loaded again for follow-up turns.
