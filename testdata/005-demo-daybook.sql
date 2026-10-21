-- Demo inbox tasks and lists for local development, loaded by the `seed` task
-- in foreman.toml - never applied outside `foreman`. See 001-demo-users.sql
-- for the general approach and 002-demo-workbench.sql for the schema guard.
--
-- Fixed ids keep reruns idempotent via ON CONFLICT. Rows belong to users from
-- 001-demo-users.sql; Daybook scopes everything to the token's `sub`, so each
-- user sees only their own.

DO $$
BEGIN
    IF to_regclass('daybook.tasks') IS NULL THEN
        RAISE NOTICE 'daybook schema not installed, skipping demo daybook data';
        RETURN;
    END IF;

    INSERT INTO daybook.projects (id, owner, name, sort_order) VALUES
        ('seed-db-project-home', 'alice', 'Home', 0),
        ('seed-db-project-work', 'alice', 'Work', 1),
        ('seed-db-project-reading', 'bob', 'Reading list', 0)
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO daybook.tasks
        (id, owner, project_id, content, description, is_completed, sort_order, due_at)
    VALUES
        ('seed-db-task-1', 'alice', NULL, 'Pay rent', NULL, FALSE, 0, NOW() + INTERVAL '3 days'),
        ('seed-db-task-2', 'alice', NULL, 'Book dentist', 'Cleaning, any morning', FALSE, 1, NULL),
        ('seed-db-task-3', 'alice', 'seed-db-project-home', 'Replace hallway bulb', NULL, FALSE, 0, NULL),
        ('seed-db-task-4', 'alice', 'seed-db-project-home', 'Fix the squeaky door', NULL, TRUE, 1, NULL),
        ('seed-db-task-5', 'alice', 'seed-db-project-work', 'Review the release notes', NULL, FALSE, 0, NOW() + INTERVAL '1 day'),
        ('seed-db-task-6', 'bob', NULL, 'Renew passport', NULL, FALSE, 0, NULL),
        ('seed-db-task-7', 'bob', 'seed-db-project-reading', 'Finish the Rust async book', NULL, FALSE, 0, NULL)
    ON CONFLICT (id) DO NOTHING;
END
$$;
