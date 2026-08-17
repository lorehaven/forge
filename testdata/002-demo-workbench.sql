-- Demo projects, issues, labels and comments for local development, loaded
-- by the `seed` task in foreman.toml - never applied outside `foreman`.
--
-- Ids are fixed, readable strings rather than the usual `Uuid::new_v4()`
-- (see domain::project::create etc.) so this file stays legible and reruns
-- are idempotent via ON CONFLICT. "forge-demo" specifically has to be fixed:
-- 001-demo-users.sql grants carol a resource-scoped permission naming that
-- exact project id.
--
-- Wrapped in a guarded block because the `seed` task always runs, even for a
-- `foreman start <service>` that never installed workbench's schema (e.g.
-- `foreman start sage`) - skip quietly rather than failing the whole start.

DO $$
BEGIN
    IF to_regclass('workbench.projects') IS NULL THEN
        RAISE NOTICE 'workbench schema not installed, skipping demo workbench data';
        RETURN;
    END IF;

    INSERT INTO workbench.projects (id, key, name, description) VALUES
        ('forge-demo', 'DEMO', 'Demo Project',
         'Sample project for exploring Workbench locally.'),
        ('forge-atlas', 'ATLAS', 'Atlas Platform',
         'Second sample project, used to test cross-project permission scoping.')
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO workbench.labels (id, project_id, name, color) VALUES
        ('seed-label-demo-frontend', 'forge-demo', 'frontend', '#4f8ef7'),
        ('seed-label-demo-backend', 'forge-demo', 'backend', '#22b8a1'),
        ('seed-label-demo-urgent', 'forge-demo', 'urgent', '#e74c3c'),
        ('seed-label-atlas-ci', 'forge-atlas', 'ci', '#2ecc71'),
        ('seed-label-atlas-infra', 'forge-atlas', 'infra', '#f39c12')
    ON CONFLICT (id) DO NOTHING;

    -- DEMO-1..5
    INSERT INTO workbench.issues
        (id, project_id, parent_id, seq, kind, title, description, status, priority, assignee, reporter)
    VALUES
        ('seed-issue-demo-1', 'forge-demo', NULL, 1, 'story',
         'Design the onboarding flow',
         'First-run experience for new workspace members.',
         'in-progress', 'high', 'carol', 'alice'),

        ('seed-issue-demo-2', 'forge-demo', NULL, 2, 'task',
         'Wire up the settings page', NULL,
         'todo', 'medium', 'bob', 'alice'),

        ('seed-issue-demo-3', 'forge-demo', NULL, 3, 'bug',
         'Session expires too early on mobile',
         'Reported by a beta tester; happens within 5 minutes of login.',
         'blocked', 'high', 'alice', 'bob'),

        ('seed-issue-demo-4', 'forge-demo', 'seed-issue-demo-1', 4, 'task',
         'Write onboarding docs', NULL,
         'done', 'low', 'carol', 'carol'),

        ('seed-issue-demo-5', 'forge-demo', NULL, 5, 'bug',
         'Typo in password reset email', NULL,
         'rejected', 'low', NULL, 'dave'),

        -- ATLAS-1..4
        ('seed-issue-atlas-1', 'forge-atlas', NULL, 1, 'story',
         'Migrate build pipeline to new runners', NULL,
         'todo', 'medium', 'dave', 'dave'),

        ('seed-issue-atlas-2', 'forge-atlas', NULL, 2, 'task',
         'Add retry logic to the deploy job', NULL,
         'in-progress', 'high', 'alice', 'dave'),

        ('seed-issue-atlas-3', 'forge-atlas', NULL, 3, 'bug',
         'Flaky integration test on CI', NULL,
         'blocked', 'medium', NULL, 'alice'),

        ('seed-issue-atlas-4', 'forge-atlas', NULL, 4, 'task',
         'Document the release checklist', NULL,
         'done', 'low', 'bob', 'bob')
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO workbench.issue_labels (issue_id, label_id) VALUES
        ('seed-issue-demo-1', 'seed-label-demo-frontend'),
        ('seed-issue-demo-1', 'seed-label-demo-urgent'),
        ('seed-issue-demo-3', 'seed-label-demo-backend'),
        ('seed-issue-demo-3', 'seed-label-demo-urgent'),
        ('seed-issue-atlas-2', 'seed-label-atlas-ci'),
        ('seed-issue-atlas-2', 'seed-label-atlas-infra')
    ON CONFLICT DO NOTHING;

    INSERT INTO workbench.comments (id, issue_id, author, body) VALUES
        ('seed-comment-demo-1-a', 'seed-issue-demo-1', 'bob',
         'Left some notes on the Figma file - see the #onboarding channel.'),
        ('seed-comment-demo-1-b', 'seed-issue-demo-1', 'carol',
         'Updated the flow based on your notes, ready for another look.'),
        ('seed-comment-demo-3-a', 'seed-issue-demo-3', 'alice',
         'Can reproduce on iOS Safari, digging into the token refresh path.'),
        ('seed-comment-atlas-2-a', 'seed-issue-atlas-2', 'dave',
         'Retries are in, let''s watch the next few nightly runs before closing.')
    ON CONFLICT (id) DO NOTHING;
END $$;
