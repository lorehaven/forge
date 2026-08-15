# required
header_label = Conveyor
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Language
theme_label = Theme

# ── Shell ────────────────────────────────────────────────────────────────────

ui_header_home = Conveyor
ui_home_button = Home
ui_nav_credentials = Credentials
ui_logout = Log out

# ── Home ─────────────────────────────────────────────────────────────────────

ui_home_title = Pipelines
ui_home_subtitle = Every build, from the commit that asked for it.

ui_repos_title = Repositories
ui_repos_empty = No repositories are registered yet.

ui_runs_title = Recent runs
ui_runs_empty = Nothing has run yet.
ui_runs_view_all = View all pipelines

ui_project_subtitle = Repositories and pipelines under this node.
ui_project_not_found = No such project.

# ── Pipelines ────────────────────────────────────────────────────────────────

ui_pipelines_title = All pipelines
ui_pipelines_subtitle = Every run, newest first.
ui_pager_prev = Previous
ui_pager_next = Next
ui_pager_page = Page {$page} of {$total}

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
ui_col_checks = Checks
ui_col_actions = Actions
ui_repo_enabled = Enabled
ui_repo_disabled = Disabled
ui_repo_run_now = Run now
ui_chip_not_run = Not run yet
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
ui_scan_back = Back to overview
ui_scan_clean = Nothing found.

# ── Status ───────────────────────────────────────────────────────────────────

ui_status_queued = Queued
ui_status_running = Running
ui_status_success = Passed
ui_status_failed = Failed
ui_status_cancelled = Cancelled
ui_status_skipped = Skipped

# ── Repositories admin ──────────────────────────────────────────────────────

ui_repos_manage = Manage repositories
ui_repos_back = Back to repositories
ui_repos_back_home = Back to home
ui_repos_add_title = Add a repository
ui_repos_owner = Owner
ui_repos_name = Name
ui_repos_clone_url = Clone URL
ui_repos_branch = Default branch
ui_repos_project = Project
ui_repos_provider = Provider
ui_repos_create = Register repository
ui_repos_save = Save changes
ui_repos_edit = Edit
ui_repos_view_only = You do not have write access to this repository's project; these fields are read-only.
ui_repos_delete_title = Danger zone
ui_repos_delete_hint = Removing a repository deletes its run history. This cannot be undone.
ui_repos_delete = Delete repository
ui_repos_not_found = No such repository.
ui_repos_ok_created = Repository registered.
ui_repos_ok_saved = Changes saved.
ui_repos_ok_deleted = Repository deleted.
ui_repos_err_owner_name_empty = Owner and name are required.
ui_repos_err_project_required = Choose a project for this repository.
ui_repos_err_bad_clone_url = That clone URL is not valid.
ui_repos_err_forbidden = You do not have write access to that project.
ui_repos_err_not_found = No such repository.
ui_repos_err_write_failed = That could not be saved. It may already exist.

# ── Credentials ──────────────────────────────────────────────────────────────

ui_credentials_title = Credentials
ui_credentials_empty = No credentials visible to you.
