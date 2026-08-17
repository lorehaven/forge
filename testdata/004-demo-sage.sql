-- Demo projects, conversations and messages for local development, loaded by
-- the `seed` task in foreman.toml - never applied outside `foreman`. See
-- 001-demo-users.sql for the general approach and 002-demo-workbench.sql for
-- the schema guard.
--
-- Files, embeddings and RAG contexts are deliberately not seeded: a fake
-- 1024-dim embedding retrieves nothing meaningful, so it would look like data
-- without being useful as any.
--
-- Sage stores its timestamps as `Utc::now().to_rfc3339()` text, not
-- TIMESTAMPTZ (see the migration's `created_at TEXT` columns) - literals
-- below are plain RFC 3339 strings for the same reason.

DO $$
BEGIN
    IF to_regclass('sage.projects') IS NULL THEN
        RAISE NOTICE 'sage schema not installed, skipping demo sage data';
        RETURN;
    END IF;

    INSERT INTO sage.projects (id, name, owner, created_at, updated_at) VALUES
        ('seed-sg-project-research', 'Product Research', 'alice',
         '2026-08-10T09:00:00+00:00', '2026-08-10T09:00:00+00:00'),
        ('seed-sg-project-runbooks', 'Runbook Drafts', 'dave',
         '2026-08-12T11:30:00+00:00', '2026-08-12T11:30:00+00:00')
    ON CONFLICT (id) DO NOTHING;

    -- active_message_id has no foreign key of its own (see the migration), so
    -- it can point at a message inserted below without ordering the two
    -- statements the other way round.
    INSERT INTO sage.conversations (id, title, owner, active_message_id, project_id, updated_at) VALUES
        ('seed-sg-conv-1', 'Competitive analysis outline', 'alice', 'seed-sg-msg-1-b',
         'seed-sg-project-research', '2026-08-10T09:05:00+00:00'),
        ('seed-sg-conv-2', 'Quick question about pgvector', 'bob', 'seed-sg-msg-2-b',
         NULL, '2026-08-14T16:20:00+00:00')
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO sage.messages (id, conversation_id, parent_id, role, content, created_at) VALUES
        ('seed-sg-msg-1-a', 'seed-sg-conv-1', NULL, 'user',
         'Can you outline how our top three competitors structure their onboarding flow?',
         '2026-08-10T09:00:30+00:00'),
        ('seed-sg-msg-1-b', 'seed-sg-conv-1', 'seed-sg-msg-1-a', 'assistant',
         'Here''s a first pass: all three front-load account setup before showing any real data, ' ||
         'and two of them use a progress checklist that stays pinned until every step is done.',
         '2026-08-10T09:05:00+00:00'),

        ('seed-sg-msg-2-a', 'seed-sg-conv-2', NULL, 'user',
         'Does pgvector support cosine similarity out of the box?',
         '2026-08-14T16:18:00+00:00'),
        ('seed-sg-msg-2-b', 'seed-sg-conv-2', 'seed-sg-msg-2-a', 'assistant',
         'Yes - the vector_cosine_ops operator class, used with an HNSW or IVFFlat index. ' ||
         'This estate''s own file_chunks table already indexes its embeddings that way.',
         '2026-08-14T16:20:00+00:00')
    ON CONFLICT (id) DO NOTHING;
END $$;
