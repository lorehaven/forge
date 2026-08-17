-- Demo repos, runs, jobs, steps, logs and one artifact for local development,
-- loaded by the `seed` task in foreman.toml - never applied outside `foreman`.
-- See 001-demo-users.sql for the general approach (fixed ids, ON CONFLICT,
-- password/account context) and 002-demo-workbench.sql for the schema guard.
--
-- Secrets and credentials are deliberately not seeded here: both store
-- ChaCha20-Poly1305 ciphertext sealed with conveyor's own encryption key, and
-- there is no way to fabricate a valid sealed value outside the app itself.

DO $$
BEGIN
    IF to_regclass('conveyor.repos') IS NULL THEN
        RAISE NOTICE 'conveyor schema not installed, skipping demo conveyor data';
        RETURN;
    END IF;

    INSERT INTO conveyor.projects (id, name, parent_id) VALUES
        ('seed-cv-project-root', 'lorehaven', NULL),
        ('seed-cv-project-forge', 'forge', 'seed-cv-project-root'),
        ('seed-cv-project-quench', 'quench', 'seed-cv-project-root')
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO conveyor.repos
        (id, provider, owner, name, clone_url, default_branch, registered_by, enabled, project_id)
    VALUES
        ('seed-cv-repo-forge', 'github', 'lorehaven', 'forge',
         'https://github.com/lorehaven/forge.git', 'master', 'dave', TRUE,
         'seed-cv-project-forge'),
        ('seed-cv-repo-quench', 'generic', 'lorehaven', 'quench',
         'https://git.internal/lorehaven/quench.git', 'main', 'alice', TRUE,
         'seed-cv-project-quench')
    ON CONFLICT (id) DO NOTHING;

    -- forge: a clean push build, and a pull request still in flight.
    INSERT INTO conveyor.runs
        (id, repo_id, trigger, git_ref, sha, message, status,
         queued_at, started_at, finished_at, claimed_by, claimed_at, attempt)
    VALUES
        ('seed-cv-run-forge-1', 'seed-cv-repo-forge', 'push', 'refs/heads/master',
         'a1b2c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3', 'format+lint', 'success',
         '2026-08-15T09:00:00+00', '2026-08-15T09:00:05+00', '2026-08-15T09:04:32+00',
         NULL, NULL, 1),
        ('seed-cv-run-forge-2', 'seed-cv-repo-forge', 'pull_request', 'refs/heads/feature/seed-data',
         'b2c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3e4', 'add testdata for docker services', 'running',
         '2026-08-17T08:50:00+00', '2026-08-17T08:50:04+00', NULL,
         'worker-1', '2026-08-17T08:52:00+00', 1)
    ON CONFLICT (id) DO NOTHING;

    -- quench: a manual run whose test stage failed.
    INSERT INTO conveyor.runs
        (id, repo_id, trigger, git_ref, sha, message, status,
         queued_at, started_at, finished_at, attempt, error)
    VALUES
        ('seed-cv-run-quench-1', 'seed-cv-repo-quench', 'manual', 'refs/heads/main',
         'c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3e4f5', 'bump dependency versions', 'failed',
         '2026-08-16T14:00:00+00', '2026-08-16T14:00:03+00', '2026-08-16T14:03:47+00',
         1, 'pipeline: stage ''test'' failed')
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO conveyor.jobs
        (id, run_id, stage, name, needs, status, exit_code, started_at, finished_at, error)
    VALUES
        ('seed-cv-job-f1-build', 'seed-cv-run-forge-1', 'build', 'build', '[]', 'success', 0,
         '2026-08-15T09:00:05+00', '2026-08-15T09:02:10+00', NULL),
        ('seed-cv-job-f1-test', 'seed-cv-run-forge-1', 'test', 'test', '["build"]', 'success', 0,
         '2026-08-15T09:02:10+00', '2026-08-15T09:04:32+00', NULL),

        ('seed-cv-job-f2-build', 'seed-cv-run-forge-2', 'build', 'build', '[]', 'success', 0,
         '2026-08-17T08:50:04+00', '2026-08-17T08:51:40+00', NULL),
        ('seed-cv-job-f2-test', 'seed-cv-run-forge-2', 'test', 'test', '["build"]', 'running', NULL,
         '2026-08-17T08:51:40+00', NULL, NULL),

        ('seed-cv-job-q1-build', 'seed-cv-run-quench-1', 'build', 'build', '[]', 'success', 0,
         '2026-08-16T14:00:03+00', '2026-08-16T14:01:15+00', NULL),
        ('seed-cv-job-q1-test', 'seed-cv-run-quench-1', 'test', 'test', '["build"]', 'failed', 1,
         '2026-08-16T14:01:15+00', '2026-08-16T14:03:47+00', '2 tests failed')
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO conveyor.steps (id, job_id, ordinal, kind, command, status, exit_code, started_at, finished_at)
    VALUES
        ('seed-cv-step-f1-build-1', 'seed-cv-job-f1-build', 1, 'shell', 'cargo build --release', 'success', 0,
         '2026-08-15T09:00:05+00', '2026-08-15T09:02:10+00'),
        ('seed-cv-step-f1-test-1', 'seed-cv-job-f1-test', 1, 'shell', 'cargo test --workspace', 'success', 0,
         '2026-08-15T09:02:10+00', '2026-08-15T09:04:32+00'),

        ('seed-cv-step-f2-build-1', 'seed-cv-job-f2-build', 1, 'shell', 'cargo build --release', 'success', 0,
         '2026-08-17T08:50:04+00', '2026-08-17T08:51:40+00'),
        ('seed-cv-step-f2-test-1', 'seed-cv-job-f2-test', 1, 'shell', 'cargo test --workspace', 'running', NULL,
         '2026-08-17T08:51:40+00', NULL),

        ('seed-cv-step-q1-build-1', 'seed-cv-job-q1-build', 1, 'shell', 'cargo build --release', 'success', 0,
         '2026-08-16T14:00:03+00', '2026-08-16T14:01:15+00'),
        ('seed-cv-step-q1-test-1', 'seed-cv-job-q1-test', 1, 'shell', 'cargo test --workspace', 'failed', 1,
         '2026-08-16T14:01:15+00', '2026-08-16T14:03:47+00')
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO conveyor.logs (job_id, seq, stream, chunk) VALUES
        ('seed-cv-job-f1-build', 1, 'stdout', E'   Compiling forge v0.1.0\n'),
        ('seed-cv-job-f1-build', 2, 'stdout', E'    Finished release [optimized] target(s) in 42.10s\n'),
        ('seed-cv-job-f1-test', 1, 'stdout', E'running 128 tests\n'),
        ('seed-cv-job-f1-test', 2, 'stdout', E'test result: ok. 128 passed; 0 failed\n'),
        ('seed-cv-job-q1-test', 1, 'stdout', E'running 64 tests\n'),
        ('seed-cv-job-q1-test', 2, 'stderr', E'test result: FAILED. 62 passed; 2 failed\n')
    ON CONFLICT (job_id, seq) DO NOTHING;

    INSERT INTO conveyor.artifacts (id, run_id, job_id, kind, name, version, uri, digest) VALUES
        ('seed-cv-artifact-forge', 'seed-cv-run-forge-1', 'seed-cv-job-f1-build',
         'container-image', 'forge/gatehouse-service', '0.2.6',
         'registry.local/forge/gatehouse-service:0.2.6',
         'sha256:9f2b1c4d5e6f7089a1b2c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3e4f5a6b7')
    ON CONFLICT (id) DO NOTHING;
END $$;
