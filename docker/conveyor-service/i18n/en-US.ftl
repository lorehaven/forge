# required
header_label = Conveyor
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Language
theme_label = Theme

# ── Shell ────────────────────────────────────────────────────────────────────

ui_header_home = Conveyor
ui_home_button = Home
ui_logout = Log out

# ── Home ─────────────────────────────────────────────────────────────────────

ui_home_title = Pipelines
ui_home_subtitle = Every build, from the commit that asked for it.

ui_repos_title = Repositories
ui_repos_empty = No repositories are registered yet.

ui_runs_title = Recent runs
ui_runs_empty = Nothing has run yet.

# ── Runs ─────────────────────────────────────────────────────────────────────

ui_col_status = Status
ui_col_repository = Repository
ui_col_ref = Ref
ui_col_commit = Commit
ui_col_trigger = Trigger
ui_col_when = When
ui_col_provider = Provider
ui_col_branch = Default branch
ui_col_state = State
ui_repo_enabled = Enabled
ui_repo_disabled = Disabled
ui_run_not_found = No such run.
ui_run_reason = Why it ended
ui_meta_trigger = Trigger
ui_meta_queued = Queued
ui_meta_duration = Duration
ui_meta_attempt = Attempt
ui_artifacts_title = Artifacts
ui_log_loading = Loading…

# ── Scan ─────────────────────────────────────────────────────────────────────

ui_scan_subtitle = Lint, unused dependencies and known vulnerabilities, from the most recent run.
ui_scan_lint_title = Lint
ui_scan_machete_title = Unused dependencies
ui_scan_audit_title = Vulnerabilities
ui_scan_no_runs = This repository has not run yet.
ui_scan_no_checks = The most recent run did not run lint, machete or audit. Add `anvil lint`, `anvil machete` or `anvil audit` as steps in .conveyor.toml to see them here.
ui_scan_repo_not_found = No such repository.

# ── Status ───────────────────────────────────────────────────────────────────

ui_status_queued = Queued
ui_status_running = Running
ui_status_success = Passed
ui_status_failed = Failed
ui_status_cancelled = Cancelled
ui_status_skipped = Skipped
